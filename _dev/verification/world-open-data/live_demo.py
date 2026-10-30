#!/usr/bin/env python3
"""Owner-session real-provider demo. Does not save proprietary imagery pixels.

Run in WSL with --bin-dir pointing to freshly built Cargo binaries. Creates a
new run directory and only stops the child daemons it started. --keep leaves
this foreground session running until Ctrl+C, for interactive native testing.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, required=True)
    parser.add_argument('--run-dir', type=Path, required=True)
    parser.add_argument('--keep', action='store_true')
    parser.add_argument('--native-bin', type=Path,
                        help='Open the native World window and stop this demo when it closes')
    parser.add_argument('--native-imagery-source', default='sentinel2',
                        choices=['sentinel2', 'blue_marble', 'google_satellite'])
    parser.add_argument('--signal-updates', type=int, default=2)
    args = parser.parse_args()
    args.run_dir.mkdir(parents=True, exist_ok=False)
    runtime = args.run_dir / 'runtime'
    environment = dict(os.environ, ARCHIE_WORLD_RUNTIME_DIR=str(runtime),
                       ARCHIE_WORLDD_PORT='17667')
    children = []
    logs = []

    def cli(*arguments):
        response = subprocess.run([str(args.bin_dir / 'archie'), 'world', *arguments],
                                  env=environment, capture_output=True, text=True,
                                  timeout=20)
        if response.returncode:
            raise RuntimeError(response.stderr[:500])
        return json.loads(response.stdout)

    def save(name, value):
        # The display-only Google pixels remain only in the daemon/renderer RAM.
        value = copy.deepcopy(value)
        def strip(node):
            if isinstance(node, dict):
                node.pop('data_base64', None)
                for child in node.values():
                    strip(child)
            elif isinstance(node, list):
                for child in node:
                    strip(child)
        strip(value)
        (args.run_dir / name).write_text(json.dumps(value, indent=2), encoding='utf-8')

    def start(name, arguments, executable=None):
        log = (args.run_dir / (name + '.log')).open('w', encoding='utf-8')
        logs.append(log)
        process = subprocess.Popen([str(executable or args.bin_dir / name), *arguments],
                                   env=environment, stdout=log, stderr=log,
                                   stdin=subprocess.DEVNULL, start_new_session=True)
        children.append(process)
        return process

    try:
        start('archie-worldd', ['--bind', '127.0.0.1:17667', '--enable-exploration',
              '--domain', 'personal', '--exploration-runtime', str(runtime),
              '--exploration-cache', str(args.run_dir / 'cache')])
        deadline = time.monotonic() + 20
        while not (runtime / 'projection.json').exists():
            if any(child.poll() is not None for child in children) or time.monotonic() > deadline:
                raise RuntimeError('Private world endpoint did not start; see preserved logs')
            time.sleep(0.1)
        start('archie-displayd', ['--bind', '127.0.0.1:17666',
              '--orb-socket', str(runtime / 'orb.sock'), '--world-poll-ms', '60000'])
        region = ['--latitude', '37.62', '--longitude=-122.38', '--radius-m', '15000',
                  '--zoom', '11', '--layers', 'terrain,imagery,signals',
                  '--imagery-sources', 'sentinel2,blue-marble,google-satellite',
                  '--imagery-date', '2026-10-20', '--signal-updates', str(args.signal_updates),
                  '--signal-interval-seconds', '60', '--max-tiles', '64',
                  '--max-bytes', '67108864', '--domain', 'personal', '--json']
        save('plan.json', cli('plan', *region))
        accepted = cli('explore', *region)
        save('accepted.json', accepted)
        job_id = accepted['job']['id']
        deadline = time.monotonic() + 160
        job = None
        while time.monotonic() < deadline:
            status = cli('status', '--domain', 'personal', '--json')
            job = next(item for item in status['jobs'] if item['id'] == job_id)
            preview = job.get('preview') or {}
            sources = preview.get('imagery', [])
            ready = any(source.get('source_id') == 'sentinel2' and source.get('tiles') for source in sources)
            if ready and (preview.get('signals') or {}).get('sources'):
                break
            if job['state'] in ('failed', 'cancelled'):
                raise RuntimeError('Live job failed: ' + str(job.get('error')))
            time.sleep(0.5)
        else:
            raise RuntimeError('Live imagery/signals did not become ready; files preserved')
        token = json.loads((runtime / 'projection.json').read_text())['token']
        request = urllib.request.Request('http://127.0.0.1:17666/api/exploration',
                                         headers={'Authorization': 'Bearer ' + token})
        with urllib.request.urlopen(request, timeout=5) as response:
            projected = json.load(response)
        try:
            urllib.request.urlopen('http://127.0.0.1:17666/api/exploration', timeout=5)
        except urllib.error.HTTPError as response:
            anonymous_status = response.code
        else:
            raise RuntimeError('Anonymous exploration unexpectedly allowed')
        save('status-redacted.json', status)
        save('projection-redacted.json', projected)
        summary = dict(job_id=job_id, state=job['state'], runtime=str(runtime),
                       world_port=17667, displayd_port=17666, anonymous_status=anonymous_status,
                       downloaded_payload_bytes=job['bytes_received'], stored_bytes=job['stored_bytes'],
                       source_metadata=[{k: v for k, v in source.items() if k != 'tiles'} for source in sources],
                       signal_points=len(preview.get('signals', {}).get('points', [])),
                       signal_sources=preview.get('signals', {}).get('sources', []),
                       daemon_pids=[child.pid for child in children],
                       proprietary_pixels_saved=False, gpu_or_live_voice_verified=False)
        save('summary.json', summary)
        print(json.dumps(summary, indent=2), flush=True)
        if args.native_bin:
            native = start('archie-display', ['--scene', 'world', '--displayd-port', '17666',
                           '--control-port', '17668', '--imagery-source', args.native_imagery_source],
                           executable=args.native_bin)
            while native.poll() is None:
                if any(child.poll() is not None for child in children if child is not native):
                    raise RuntimeError('A demo daemon exited; inspect its preserved log')
                time.sleep(0.5)
            return
        if args.keep:
            while all(child.poll() is None for child in children):
                time.sleep(1)
            raise RuntimeError('A demo daemon exited; inspect its preserved log')
    finally:
        for child in children:
            child.terminate()
        for child in children:
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
        for log in logs:
            log.close()


if __name__ == '__main__':
    main()
