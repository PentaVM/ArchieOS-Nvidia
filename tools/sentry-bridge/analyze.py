"""Local finite pcapng -> per-interface Sentry metadata; no listening socket."""
import argparse
from collections import Counter
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import queue
import select
import signal
import subprocess
import threading
import time
import uuid


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temp = path.with_name(path.name + '.' + uuid.uuid4().hex + '.partial')
    with temp.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write('\n')
    temp.replace(path)


def interface_key(session, interface_id, name, description):
    stable = bool(name or description)
    identity = [name, description] if stable else [session, interface_id]
    return hashlib.sha256(json.dumps(identity).encode()).hexdigest()[:24], stable


def normalize_interface_id(raw, classic_single_interface):
    if not raw and classic_single_interface:
        raw = '0'
    if not raw.isdecimal() or int(raw) > 65535:
        raise ValueError('capture lacks a bounded interface identity')
    return raw


def observation_identity(interface_id, name, description, component):
    if component:
        guid = str(uuid.UUID(component['adapter_guid']))
        if not 1 <= int(component['id']) <= 65535:
            raise ValueError('invalid filtered component')
        return interface_id, str(component['adapter_name']), 'windows-pktmon:' + guid
    # PktMon's placeholder is not a stable NIC identity.
    if name.lower() == 'unknown':
        name = ''
    return interface_id, name, description


class WanSummary:
    """Per observation point; intentionally never sums duplicate NIC paths."""
    def __init__(self, local_addresses, on_link_prefixes=()):
        self.local = {ipaddress.ip_address(a.split('%')[0]) for a in local_addresses}
        self.on_link = [ipaddress.ip_network(p, strict=False) for p in on_link_prefixes]
        self.outbound = self.inbound = self.frames = 0
        self.peers = Counter()
        self.truncated = False

    def observe(self, row):
        try:
            source = ipaddress.ip_address((row.get('ip.src') or row.get('ipv6.src') or '').split(',')[0])
            destination = ipaddress.ip_address((row.get('ip.dst') or row.get('ipv6.dst') or '').split(',')[0])
            size = int(row['frame.len'])
            if not 0 <= size <= 2**24:
                raise ValueError('frame size')
        except (ValueError, KeyError):
            return
        peer = None
        remote_destination = not any(destination in net for net in self.on_link)
        remote_source = not any(source in net for net in self.on_link)
        if source in self.local and destination.is_global and not destination.is_multicast and remote_destination:
            self.outbound += size
            peer = str(destination)
        elif destination in self.local and source.is_global and not source.is_multicast and remote_source:
            self.inbound += size
            peer = str(source)
        if peer is not None:
            self.frames += 1
            if peer in self.peers or len(self.peers) < 128:
                self.peers[peer] += size
            else:
                self.truncated = True

    def result(self):
        return {'wan_frames':self.frames, 'outbound_bytes':self.outbound,
                'inbound_bytes':self.inbound, 'peers':dict(self.peers.most_common()),
                'peer_tracking_truncated':self.truncated, 'gateway_wan_observed':False,
                'scope':'host public-address traffic excluding known on-link prefixes; includes NAT-forwarded host workloads',
                'wan_path_verified':False,
                'counts_are_per_observation_point':True}


def _terminate(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=3)


def _write(process, data, deadline):
    fd = process.stdin.fileno()
    view = memoryview(data)
    while view:
        if time.monotonic() > deadline:
            raise TimeoutError('metadata window deadline')
        if process.poll() is not None:
            raise RuntimeError('Sentry metadata ingester exited early')
        _, writable, _ = select.select([], [fd], [], 1)
        if writable:
            try:
                view = view[os.write(fd, view):]
            except BlockingIOError:
                pass


def analyze(capture, config, netwatch, state, out, session):
    os.umask(0o077)
    if capture.stat().st_size > 512 * 1024 * 1024:
        raise ValueError('capture exceeds the bridge analysis bound')
    with capture.open('rb') as stream:
        classic_single_interface = stream.read(4).hex() in {'d4c3b2a1','a1b2c3d4','4d3cb2a1','a1b23c4d'}
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    if state.stat().st_uid != os.getuid() or state.is_symlink():
        raise ValueError('state directory must be owned by this user, without a symlink')
    os.chmod(state, 0o700)
    settings = json.loads(config.read_text(encoding='utf-8-sig'))
    if len(settings['local_addresses']) > 1024:
        raise ValueError('too many host addresses')
    schema = json.loads(subprocess.check_output([str(netwatch),'capture-fields'], timeout=10))
    fields = schema.get('fields', [])
    if schema.get('format') != 'archie-sentry-fields-v1' or not 1 <= len(fields) <= 128:
        raise ValueError('unsupported Sentry metadata catalogue')
    forbidden = ('request.arg', 'authorization', 'password', 'payload', 'data.data')
    if any(not isinstance(f,str) or any(s in f for s in forbidden) for f in fields):
        raise ValueError('catalogue contains a payload/credential field')
    prefix = ['frame.interface_id','frame.interface_name','frame.interface_description']
    command = ['tshark','-r',str(capture),'-n','-l','-Q','-T','fields',
               '-E','separator=/t','-E','occurrence=a','-E','aggregator=,','-E','escape=y']
    for field in prefix + fields:
        command += ['-e', field]
    deadline = time.monotonic() + 180
    points, all_children = {}, []
    result = {'format':'archie-sentry-host-window-v1','session':session,
              'capture_file_sha256':hashlib.file_digest(capture.open('rb'),'sha256').hexdigest(),
              'catalogue_sha256':hashlib.sha256(json.dumps(fields).encode()).hexdigest(),
              'gateway_wan_observed':False, 'packet_payloads_retained':False,
              'cross_interface_totals':'not summed: duplicate observations may exist',
              'loopback_coverage':'not guaranteed by NIC capture', 'interfaces':[]}
    tshark_errors = state / 'tshark.log'
    files = []
    try:
        errors = tshark_errors.open('wb'); files.append(errors)
        tshark = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors, start_new_session=True)
        all_children.append(tshark)
        lines = queue.Queue(maxsize=256)

        def reader():
            try:
                while True:
                    line = tshark.stdout.readline(65537)
                    if not line:
                        break
                    lines.put(line)
            finally:
                lines.put(None)

        threading.Thread(target=reader, daemon=True).start()
        while True:
            if time.monotonic() > deadline:
                raise TimeoutError('TShark metadata window deadline')
            try:
                raw = lines.get(timeout=1)
            except queue.Empty:
                continue
            if raw is None:
                break
            if len(raw) > 65536:
                raise ValueError('oversized metadata row')
            cells = raw.decode('utf-8').rstrip('\r\n').split('\t')
            if len(cells) != len(prefix) + len(fields):
                raise ValueError('TShark catalogue/row mismatch')
            interface_id, name, description = cells[:3]
            interface_id = normalize_interface_id(interface_id, classic_single_interface)
            interface_id, name, description = observation_identity(interface_id, name, description, settings.get('pktmon_component_filter'))
            key, stable = interface_key(session, interface_id, name, description)
            identity = (interface_id, name, description)
            if identity not in points:
                if len(points) >= 64:
                    raise ValueError('too many observation points')
                if any(p['key'] == key for p in points.values()):
                    raise ValueError('ambiguous repeated interface label; refusing shared audit state')
                directory = state/'interfaces'/key
                if not directory.exists() and (state/'interfaces').exists() and len(list((state/'interfaces').iterdir())) >= 64:
                    raise ValueError('interface-state storage bound reached; inspect capture identity')
                directory.mkdir(parents=True, exist_ok=True, mode=0o700)
                stdout = (directory/'ingest.stdout').open('wb')
                stderr = (directory/'ingest.stderr').open('wb')
                files += [stdout, stderr]
                args = [str(netwatch),'ingest-fields','--state-dir',str(directory),'--learn','600']
                if settings.get('gateway'):
                    ipaddress.ip_address(settings['gateway'])
                    args += ['--gateway', settings['gateway']]
                child = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=stdout,
                                         stderr=stderr, start_new_session=True)
                os.set_blocking(child.stdin.fileno(), False)
                all_children.append(child)
                points[identity] = {'child':child,'directory':directory,'key':key,
                                    'stable':stable,'summary':WanSummary(settings['local_addresses'], settings.get('on_link_prefixes', []))}
            point = points[identity]
            metadata = cells[3:]
            point['summary'].observe(dict(zip(fields, metadata)))
            _write(point['child'], ('\t'.join(metadata)+'\n').encode(), deadline)
        if tshark.wait(timeout=max(1,deadline-time.monotonic())) != 0:
            raise RuntimeError('TShark failed; see bounded private diagnostic')
        for identity, point in points.items():
            child = point['child']
            child.stdin.close()
            if child.wait(timeout=max(1,deadline-time.monotonic())) != 0:
                raise RuntimeError('Sentry rejected metadata; no healthy receipt')
            report = subprocess.check_output([str(netwatch),'report','--state-dir',str(point['directory'])], timeout=15)
            report = json.loads(report)
            result['interfaces'].append({'capture_interface_id':identity[0], 'name':identity[1],
                'description':identity[2], 'stable_identity':point['stable'],
                'pktmon_component_filter':settings.get('pktmon_component_filter'),
                'state_key':point['key'], 'wan':point['summary'].result(),
                'alerts':report['alerts'], 'profiles':report['profiles']})
        result['status'] = 'analyzed'
        result['frames'] = sum(p['summary'].frames for p in points.values())
        result['frames_scope'] = 'WAN observations across interfaces, not unique host packets'
        atomic_json(out, result)
        atomic_json(state/'latest.json', result)
        return result
    finally:
        for child in reversed(all_children):
            _terminate(child)
        for stream in files:
            stream.close()
        if tshark_errors.exists() and tshark_errors.stat().st_size > 65536:
            with tshark_errors.open('r+b') as stream:
                stream.truncate(65536)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    for argument in ('capture','config','netwatch','state','out'):
        parser.add_argument('--'+argument, type=Path, required=True)
    parser.add_argument('--session', required=True)
    args = parser.parse_args()
    if not args.session.isalnum() or len(args.session) > 64:
        raise SystemExit('invalid session identifier')
    result = analyze(args.capture, args.config, args.netwatch, args.state, args.out, args.session)
    print(json.dumps({'status':result['status'],'observation_points':len(result['interfaces'])}))
