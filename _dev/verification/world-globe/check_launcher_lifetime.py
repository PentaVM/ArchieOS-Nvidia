#!/usr/bin/env python3
"""Real CLI/daemon/native expiry+stop test using five-second owner sessions."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

repository = Path(__file__).resolve().parents[3]
binary = repository / 'core/target/linux-validation/debug'
run = Path('/tmp') / ('archie-world-lifetime-' + str(time.time_ns()))
log = run.with_suffix('.log').open('w')
helper = subprocess.Popen(['python3', str(Path(__file__).with_name('globe_demo.py')),
    '--bin-dir', str(binary), '--run-dir', str(run),
    '--native-bin', '/tmp/archie-world-native-check/archie-display',
    '--source', 'blue_marble', '--world-port', '18667', '--display-port', '18666',
    '--native-port', '18668', '--renew-navigation', '--session-seconds', '5'],
    stdout=log, stderr=log)


def history():
    path = run / 'navigation-history.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


try:
    deadline = time.monotonic() + 30
    while len(history()) < 2:
        assert helper.poll() is None, 'Launcher exited before renewal'
        assert time.monotonic() < deadline, 'Expired owner admission was not renewed'
        time.sleep(.1)
    entries = history()
    initial, renewed = entries[-2:]
    assert initial['id'] != renewed['id'] and initial['request'] == renewed['request']
    assert renewed['request']['imagery_date'] is not None, 'Date must stay pinned'
    environment = dict(os.environ, ARCHIE_WORLD_RUNTIME_DIR=str(run / 'runtime'))
    stopped = subprocess.run([str(binary / 'archie'), 'world', 'navigation-stop',
        renewed['id'], '--domain', 'personal', '--json'],
        env=environment, capture_output=True, text=True, timeout=10, check=True)
    assert json.loads(stopped.stdout)['navigation']['state'] == 'stopped'
    count = len(entries)
    time.sleep(7)
    assert len(history()) == count, 'Explicit stop was automatically overridden'
    assert helper.poll() is None, 'Stopping downloads closed the viewer'
    result = dict(real_cli_daemon_native=True, expiry_seconds=5,
        expiry_renewed=True, identical_request_and_date=True,
        explicit_stop_preserved_after_expiry=True, viewer_still_running=True)
    Path(__file__).with_name('launcher-lifetime.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result, indent=2))
finally:
    # Close only this fixture's native child; its helper owns daemon cleanup.
    for process in Path('/proc').iterdir():
        if not process.name.isdigit():
            continue
        try:
            argv = (process / 'cmdline').read_bytes().split(b'\0')
            env = (process / 'environ').read_bytes().split(b'\0')
            if (argv[0] == b'/tmp/archie-world-native-check/archie-display'
                    and ('ARCHIE_WORLD_RUNTIME_DIR=' + str(run / 'runtime')).encode() in env):
                os.kill(int(process.name), signal.SIGTERM)
        except (OSError, IndexError):
            pass
    try:
        helper.wait(timeout=10)
    except subprocess.TimeoutExpired:
        helper.terminate()
    log.close()
