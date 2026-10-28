#!/usr/bin/env python3
"""Mechanical render-ray validation; Google buffers stay exclusively in RAM."""
import base64
import json
import math
from pathlib import Path
import socket
import struct
import sys

report=Path(sys.argv[1])
state=next(r['state'] for r in reversed([json.loads(s) for s in report.read_text().splitlines()]) if 'state' in r)
cap=json.loads(Path(sys.argv[2]).read_text());host,port=cap['address'].rsplit(':',1)
known=[];known_tiles=[];meshes=[]
for _ in range(8):
    with socket.create_connection((host,int(port)),timeout=15) as peer:
        peer.sendall((json.dumps(dict(token=cap['token'],session_id=cap['session_id'],command=dict(action='status',known_keys=known_tiles,known_mesh_keys=known)))+'\n').encode())
        frame=json.loads(peer.makefile('rb').readline(8388608))['result']
    assert frame['generation']==state['stream']['generation']
    known_tiles+= [t['key'] for t in frame['tiles'] if t['key'] not in known_tiles]
    meshes+=frame['meshes'];known+=[m['key'] for m in frame['meshes']]
    if set(frame['earth']['active_keys'])<=set(known):break

def sub(a,b):return tuple(x-y for x,y in zip(a,b))
def dot(a,b):return sum(x*y for x,y in zip(a,b))
def cross(a,b):return a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]
eye=tuple(state['eye'][k] for k in ('x','y','z'));target=tuple(state['target'][k] for k in ('x','y','z'))
direction=sub(target,eye);length=math.sqrt(dot(direction,direction));direction=tuple(v/length for v in direction)
nearest=math.inf;hits=0
for mesh in meshes:
    origin=mesh['origin']
    vertices=[tuple(v[i]+origin[i] for i in range(3)) for v in struct.iter_unpack('<8f',base64.b64decode(mesh['vertices_base64']))]
    for indices in struct.iter_unpack('<3I',base64.b64decode(mesh['indices_base64'])):
        a,b,c=(vertices[i] for i in indices);ab=sub(b,a);ac=sub(c,a);p=cross(direction,ac);det=dot(ab,p)
        if abs(det)<1e-22:continue
        tvec=sub(eye,a);u=dot(tvec,p)/det
        if u<0 or u>1:continue
        q=cross(tvec,ab);v=dot(direction,q)/det
        if v<0 or u+v>1:continue
        distance=dot(ac,q)/det
        if distance>0:hits+=1;nearest=min(nearest,distance)
result=dict(meshes_checked=len(meshes),center_ray_triangle_hits=hits,
    nearest_distance_m=None if not math.isfinite(nearest) else nearest*6371008.8/20,
    native_render=state.get('render'),source_state=frame['earth']['state'])
Path(sys.argv[3]).write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
