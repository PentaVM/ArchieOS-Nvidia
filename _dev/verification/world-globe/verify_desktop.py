#!/usr/bin/env python3
"""Check the launched desktop viewer; save metadata only, with no capability."""
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import sys

base = Path.home() / '.local/share/archie-world-demo'
run = max(base.glob('run-*'), key=lambda p: p.stat().st_mtime)
summary = json.loads((run / 'summary.json').read_text())
for pid in summary['daemon_pids']:
    os.kill(pid, 0)
assert summary['startup_aoi_jobs'] == 0
assert summary['source'] == 'google_satellite'
with socket.create_connection(('127.0.0.1', summary['native_port']), timeout=5) as peer:
    peer.sendall(b'{"action":"list"}\n')
    catalog = json.loads(peer.makefile('rb').readline())
assert 'world' in catalog['scenes']
cap = json.loads((Path(summary['runtime']) / 'navigation.json').read_text())
host, port = cap['address'].rsplit(':', 1)
with socket.create_connection((host, int(port)), timeout=5) as peer:
    peer.sendall((json.dumps(dict(token=cap['token'], session_id=cap['session_id'],
                  command=dict(action='status', known_keys=[]))) + '\n').encode())
    reply = json.loads(peer.makefile('rb').readline(8 * 1024 * 1024))
assert reply['ok']
frame = reply['result']
active_navigation = '--allow-active-navigation' in sys.argv[2:]
if not active_navigation:
    assert frame['source_id'] == 'google_satellite'
    assert frame['state'] == 'ready' and frame['pending_tiles'] == 0
if not active_navigation:
    assert frame['tiles'], 'No original Google tiles available'
pixel_hashes = {hashlib.sha256(base64.b64decode(t['data_base64'])).digest()
                for t in frame['tiles']}
files = [p for p in (run / 'cache').rglob('*') if p.is_file()]
if frame['source_id'] == 'google_satellite':
    assert not any(hashlib.sha256(p.read_bytes()).digest() in pixel_hashes for p in files)
result = dict(run=str(run), native_control='responsive', startup_aoi_jobs=0,
              active_navigation_allowed=active_navigation,
              owner_renewal_enabled=summary.get('renew_navigation', False),
              photogrammetry_admitted=summary['navigation']['request'].get('photogrammetry', False),
              source=frame['source_id'], state=frame['state'],
              min_zoom=frame['min_zoom'], max_zoom=frame['max_zoom'],
              pending_tiles=frame['pending_tiles'], usage=frame['usage'],
              original_tile_dimensions=sorted({(t['width'], t['height']) for t in frame['tiles']}),
              sampled_pixel_hashes_absent_from_disk=True if frame['source_id']=='google_satellite' and pixel_hashes else None,
              executable_sha256=hashlib.sha256((base / 'archie-display').read_bytes()).hexdigest())
Path(sys.argv[1]).write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
