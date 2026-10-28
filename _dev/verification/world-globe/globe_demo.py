#!/usr/bin/env python3
"""Open the continuous globe with bounded owner-session streaming.

No AOI is downloaded at startup. Native camera movements request visible tiles.
Only metadata is saved here; Google pixels stay in daemon/renderer memory.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
from datetime import datetime, timedelta, timezone


def maintain_navigation(current, read_status, renew, now_ms):
    """Only the opted-in owner launcher renews its own expired admission.

    A stop, changed session, exhausted budget before expiry or control error
    cannot be converted into an automatic new session.
    """
    if current is None or now_ms < current['expires_at_ms']:
        return current
    status = read_status(current['id'])
    if status['id'] != current['id'] or status['state'] != 'expired':
        return None
    return renew()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, required=True)
    parser.add_argument('--run-dir', type=Path, required=True)
    parser.add_argument('--native-bin', type=Path)
    parser.add_argument('--source', default='google_satellite')
    parser.add_argument('--world-port', type=int, default=17667)
    parser.add_argument('--display-port', type=int, default=17666)
    parser.add_argument('--native-port', type=int, default=17668)
    parser.add_argument('--keep', action='store_true')
    parser.add_argument('--renew-navigation', action='store_true',
                        help='Owner desktop: renew expired hourly sessions while its window remains open')
    parser.add_argument('--session-seconds', type=int, default=3600, choices=range(1, 3601),
                        metavar='1..3600')
    args = parser.parse_args()
    args.run_dir.mkdir(parents=True, exist_ok=False)
    runtime = args.run_dir / 'runtime'
    environment = dict(os.environ, ARCHIE_WORLD_RUNTIME_DIR=str(runtime),
                       ARCHIE_WORLDD_PORT=str(args.world_port))
    processes, logs = [], []
    # Keep dated NASA products pinned throughout this window, including renewals.
    imagery_date = (datetime.now(timezone.utc).date() - timedelta(days=3)).isoformat()

    def command(arguments):
        response = subprocess.run([str(args.bin_dir / 'archie'), 'world', *arguments],
                                  env=environment, capture_output=True, text=True, timeout=20)
        if response.returncode:
            raise RuntimeError('Owner navigation command failed; session was not renewed')
        return json.loads(response.stdout)['navigation']

    def navigate():
        navigation = command(['navigate', '--domain', 'personal', '--json',
                              '--duration-seconds', str(args.session_seconds),
                              '--imagery-date', imagery_date])
        with (args.run_dir / 'navigation-history.jsonl').open('a', encoding='utf-8') as history:
            history.write(json.dumps(navigation) + '\n')
        return navigation

    def start(name, arguments, executable=None):
        log = (args.run_dir / (name + '.log')).open('w', encoding='utf-8')
        logs.append(log)
        child = subprocess.Popen([str(executable or args.bin_dir / name), *arguments],
                    env=environment, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                    start_new_session=True)
        processes.append(child)
        return child

    try:
        start('archie-worldd', ['--bind', '127.0.0.1:' + str(args.world_port),
              '--enable-exploration', '--domain', 'personal',
              '--exploration-runtime', str(runtime),
              '--exploration-cache', str(args.run_dir / 'cache')])
        deadline = time.monotonic() + 20
        while not (runtime / 'control.json').exists():
            if time.monotonic() > deadline or any(p.poll() is not None for p in processes):
                raise RuntimeError('Private daemon did not start; inspect retained logs')
            time.sleep(0.1)
        start('archie-displayd', ['--bind', '127.0.0.1:' + str(args.display_port),
              '--orb-socket', str(runtime / 'orb.sock'), '--world-poll-ms', '60000'])
        active_navigation = navigate()
        accepted = dict(navigation=active_navigation)
        (args.run_dir / 'accepted.json').write_text(json.dumps(accepted, indent=2), encoding='utf-8')
        summary = dict(runtime=str(runtime), navigation=accepted['navigation'],
                       world_port=args.world_port, display_port=args.display_port,
                       native_port=args.native_port, daemon_pids=[p.pid for p in processes],
                       startup_aoi_jobs=0, source=args.source)
        summary['renew_navigation'] = bool(args.renew_navigation and args.native_bin)
        (args.run_dir / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
        print(json.dumps(summary, indent=2), flush=True)
        if args.native_bin:
            native = start('archie-display', ['--scene', 'world', '--displayd-port', str(args.display_port),
                          '--control-port', str(args.native_port), '--imagery-source', args.source],
                          executable=args.native_bin)
            while native.poll() is None:
                if any(p.poll() is not None for p in processes if p is not native):
                    raise RuntimeError('A demo daemon exited; inspect retained logs')
                if args.renew_navigation:
                    try:
                        active_navigation = maintain_navigation(
                            active_navigation,
                            lambda session_id: command(['navigation-status', session_id,
                                                        '--domain', 'personal', '--json']),
                            navigate, int(time.time() * 1000))
                    except (RuntimeError, subprocess.TimeoutExpired):
                        active_navigation = None
                        print('Owner renewal stopped: session could not be verified', flush=True)
                time.sleep(0.5)
        elif args.keep:
            while all(p.poll() is None for p in processes):
                time.sleep(1)
    finally:
        for child in processes:
            child.terminate()
        for child in processes:
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
        for log in logs:
            log.close()


if __name__ == '__main__':
    main()
