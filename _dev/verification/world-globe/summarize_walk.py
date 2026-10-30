#!/usr/bin/env python3
"""Retain measured camera/stream metadata, never provider pixel payloads."""
import argparse
import json
import math
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('report', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
out = []
for line in args.report.read_text().splitlines():
    row = json.loads(line)
    if 'state' not in row:
        continue
    state = row['state']
    eye = state['eye']
    radius = math.sqrt(sum(eye[k] ** 2 for k in ('x', 'y', 'z')))
    stream = state['stream']
    metadata = {k: stream.get(k) for k in (
        'generation', 'source_id', 'source', 'state', 'min_zoom', 'max_zoom',
        'desired_tiles', 'desired_count', 'loaded_count', 'fallback_count',
        'pending_count', 'pending_tiles', 'usage', 'provider', 'signals', 'errors')}
    out.append(dict(label=row.get('label', row.get('shot', row['kind'])),
                    input=row['kind'], selected_area=state['selectedArea'],
                    altitude_m=(radius / 20 - 1) * 6371008.8,
                    latitude=math.degrees(math.asin(eye['y'] / radius)),
                    longitude=math.degrees(math.atan2(eye['x'], eye['z'])),
                    stream=metadata))
serialized = json.dumps(out, indent=2)
assert 'data_base64' not in serialized and '"token"' not in serialized
assert all(not row['selected_area'] for row in out), 'Unexpected AOI camera'
args.output.write_text(serialized + '\n')
print(json.dumps([dict(label=x['label'], altitude_m=round(x['altitude_m'], 1),
                      lod=x['stream']['max_zoom'], state=x['stream']['state'],
                      loaded=x['stream']['loaded_count'],
                      pending=x['stream']['pending_count'],
                      requests=x['stream']['usage'].get('requests_sent'))
                  for x in out], indent=2))
