#!/usr/bin/env python3
"""Save operational proof only: never mesh/image payloads or capabilities."""
import json
import math
from pathlib import Path
import sys

rows=[]
for line in Path(sys.argv[1]).read_text().splitlines():
    row=json.loads(line)
    if row.get('kind')!='state':continue
    state=row['state'];stream=state['stream'];eye=state['eye'];target=state['target']
    radius=math.sqrt(sum(v*v for v in target.values()))
    earth=dict(stream.get('earth',{}));earth.pop('active_keys',None)
    rows.append(dict(label=row['label'],earth=earth,
        latitude=None if radius==0 else math.degrees(math.asin(target['y']/radius)),
        longitude=math.degrees(math.atan2(target['x'],target['z'])),
        altitude_m=(math.sqrt(sum(v*v for v in eye.values()))-20)*6371008.8/20,
        loaded_meshes=stream.get('earth_loaded_count'),triangles=stream.get('earth_triangle_count'),
        decoded_mesh_bytes=stream.get('earth_memory_bytes'),usage=stream.get('usage'),
        renderer=state.get('render'),selected_area=state['selectedArea']))
encoded=json.dumps(rows,indent=2)
assert all(not x['selected_area'] for x in rows)
assert not any(s in encoded for s in ('vertices_base64','texture_base64','data_base64','"token"'))
Path(sys.argv[2]).write_text(encoded+'\n')
print(json.dumps([dict(label=x['label'],lat=x['latitude'],lon=x['longitude'],
    level=x['earth'].get('maximum_level'),meshes=x['loaded_meshes'],triangles=x['triangles'],
    state=x['earth'].get('state'),renderer=x['renderer']) for x in rows],indent=2))
