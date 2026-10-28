#!/usr/bin/env python3
"""Verify a running live_demo.py capture without saving proprietary pixels."""
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
    parser.add_argument('--run-dir', type=Path, required=True)
    parser.add_argument('--bin-dir', type=Path, required=True)
    parser.add_argument('--create-second', action='store_true')
    args = parser.parse_args()
    summary = json.loads((args.run_dir / 'summary.json').read_text())
    token = json.loads((args.run_dir / 'runtime/projection.json').read_text())['token']
    environment = dict(os.environ, ARCHIE_WORLD_RUNTIME_DIR=str(args.run_dir / 'runtime'))

    def cli(*arguments):
        result = subprocess.run([str(args.bin_dir / 'archie'), 'world', *arguments],
                                env=environment, capture_output=True, text=True, timeout=20)
        if result.returncode:
            raise RuntimeError(result.stderr[:500])
        return json.loads(result.stdout)

    def view(path, authenticated=True, method='GET'):
        request = urllib.request.Request('http://127.0.0.1:17666' + path, method=method,
                    headers={'Authorization': 'Bearer ' + token} if authenticated else {})
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                return response.status, json.load(response)
        except urllib.error.HTTPError as response:
            return response.code, None

    def save(name, value):
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

    first_id = summary['job_id']
    second_id = None
    if args.create_second:
        accepted = cli('explore', '--latitude', '37.69', '--longitude=-122.37',
                       '--radius-m', '15000', '--zoom', '11',
                       '--layers', 'terrain,imagery,signals',
                       '--imagery-sources', 'sentinel2,blue-marble',
                       '--imagery-date', '2026-10-20', '--signal-updates', '1',
                       '--max-tiles', '64', '--max-bytes', '67108864',
                       '--domain', 'personal', '--json')
        save('second-accepted.json', accepted)
        second_id = accepted['job']['id']
        deadline = time.monotonic() + 160
        while time.monotonic() < deadline:
            state = cli('status', '--domain', 'personal', '--json')
            job = next(job for job in state['jobs'] if job['id'] == second_id)
            if job['state'] == 'complete':
                break
            if job['state'] in ('failed', 'cancelled'):
                raise RuntimeError('Second capture failed; inspect owner status')
            time.sleep(0.5)
        else:
            raise RuntimeError('Second capture did not finish before bounded timeout')

    selected_path = '/api/exploration/jobs/' + first_id
    assert view(selected_path, authenticated=False)[0] == 403
    assert view(selected_path, method='POST')[0] == 405
    assert view('/api/exploration/jobs/' + '0' * 32)[0] == 404
    code, selected = view(selected_path)
    assert code == 200 and selected['selected_job_id'] == first_id
    first = next(job for job in selected['jobs'] if job['id'] == first_id)
    assert first['preview'] and first['state'] == 'complete'
    assert all(not job.get('preview') for job in selected['jobs'] if job['id'] != first_id)
    before = [(job['id'], job['bytes_received'], job['cache_hits']) for job in selected['jobs']]
    for _ in range(3):
        assert view(selected_path)[0] == 200
    _, after = view(selected_path)
    assert before == [(job['id'], job['bytes_received'], job['cache_hits']) for job in after['jobs']]
    google = next(source for source in first['preview']['imagery'] if source['source_id'] == 'google_satellite')
    assert google['state'] == 'ready' and google['tiles']
    manifests = list((args.run_dir / 'cache/personal').glob('job-*/manifest.json'))
    # Every persistent Google imagery entry must omit its pixel-bearing tiles.
    google_persistent_entries = 0
    def check(node):
        nonlocal google_persistent_entries
        if isinstance(node, dict):
            if node.get('source_id') == 'google_satellite' and 'tiles' in node:
                google_persistent_entries += 1
                assert not node['tiles'], 'Google preview persisted in manifest'
            for child in node.values():
                check(child)
        elif isinstance(node, list):
            for child in node:
                check(child)
    # Runtime filenames are implementation generated; inspect job JSON manifests only.
    if not manifests:
        manifests = list((args.run_dir / 'cache/personal').glob('job-*.json'))
    assert manifests, 'No persistent job manifest found'
    for manifest in manifests:
        check(json.loads(manifest.read_text()))
    assert google_persistent_entries, 'No persisted Google metadata inspected'
    signals = first['preview'].get('signals', {})
    proof = dict(first_job_id=first_id, second_job_id=second_id,
                 final_state=first['state'], signal_updates_done=first['signal_updates_done'],
                 final_signal_points=len(signals.get('points', [])),
                 final_signal_sources=signals.get('sources', []),
                 selected_route_anonymous_status=403, selected_route_post_status=405,
                 missing_capture_status=404, selected_reads_change_downloads=False,
                 google_ram_tiles=len(google['tiles']),
                 google_persistent_entries_without_pixels=google_persistent_entries)
    save('final-proof.json', proof)
    save('final-selected-redacted.json', after)
    print(json.dumps(proof, indent=2))


if __name__ == '__main__':
    main()
