#!/usr/bin/env python3
"""Cold/warm admitted streaming timings. Source buffers stay only in RAM."""
import argparse
import json
import math
import os
from pathlib import Path
import signal
import socket
import subprocess
import time

parser=argparse.ArgumentParser()
parser.add_argument('--bin-dir',type=Path,required=True)
parser.add_argument('--run-dir',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
helper=Path(__file__).with_name('globe_demo.py')
with args.output.with_suffix('.log').open('w') as log:
    process=subprocess.Popen(['python3',str(helper),'--bin-dir',str(args.bin_dir),
        '--run-dir',str(args.run_dir),'--world-port','19667','--display-port','19666',
        '--native-port','19668','--keep'],stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+25
        descriptor=args.run_dir/'runtime/navigation.json'
        while not descriptor.exists():
            assert process.poll() is None and time.monotonic()<deadline
            time.sleep(.05)
        cap=json.loads(descriptor.read_text());host,port=cap['address'].rsplit(':',1)
        known_tiles=[];known_meshes=[]
        def query(command):
            command.update(known_keys=known_tiles,known_mesh_keys=known_meshes)
            start=time.monotonic()
            with socket.create_connection((host,int(port)),timeout=20) as peer:
                peer.sendall((json.dumps(dict(token=cap['token'],session_id=cap['session_id'],command=command))+'\n').encode())
                reply=json.loads(peer.makefile('rb').readline(8388608))
            assert reply['ok'],reply.get('error')
            frame=reply['result']
            for tile in frame['tiles']:
                if tile['key'] not in known_tiles:known_tiles.append(tile['key'])
            for mesh in frame.get('meshes',[]):
                if mesh['key'] not in known_meshes:known_meshes.append(mesh['key'])
            del known_tiles[:-256];del known_meshes[:-64]
            return frame,(time.monotonic()-start)*1000
        results=[]
        for generation,offset in [(1,0.),(2,.0004)]:
            lat=math.radians(40.7128);lon=math.radians(-74.006+offset);r=6371008.8
            up=(math.cos(lat)*math.sin(lon),math.sin(lat),math.cos(lat)*math.cos(lon))
            target=[v*r for v in up];eye=[v*(r+200.) for v in up]
            start=time.monotonic();first=None;latencies=[];last=None
            frame,latency=query(dict(action='viewport',generation=generation,source='google_satellite',
                viewport=dict(eye=eye,target=target,width=1550,height=1016,fov_degrees=45)))
            while time.monotonic()-start<100:
                latencies.append(latency);earth=frame.get('earth',{})
                if frame.get('meshes') and first is None:first=time.monotonic()-start
                last=frame
                if earth.get('state') in ('ready','no_coverage','partial') and not earth.get('more_meshes'):
                    break
                time.sleep(.2);frame,latency=query(dict(action='status'))
            earth=last.get('earth',{})
            result=dict(kind='cold' if generation==1 else 'warm_pan',first_mesh_s=first,
                completed_s=time.monotonic()-start,state=earth.get('state'),error=earth.get('error'),
                active_meshes=earth.get('active_meshes'),level=earth.get('maximum_level'),
                requests=last.get('usage',{}).get('requests_sent'),bytes=last.get('usage',{}).get('bytes_received'),
                response_ms_max=max(latencies),response_ms_median=sorted(latencies)[len(latencies)//2])
            results.append(result);print(json.dumps(result),flush=True)
        args.output.write_text(json.dumps(results,indent=2)+'\n')
    finally:
        summary=args.run_dir/'summary.json'
        if summary.exists():
            info=json.loads(summary.read_text());pid=info['daemon_pids'][0]
            command=Path('/proc',str(pid),'cmdline').read_bytes()
            assert str(args.run_dir/'runtime').encode() in command and b'archie-worldd' in command
            os.kill(pid,signal.SIGTERM)
        try:process.wait(timeout=10)
        except subprocess.TimeoutExpired:process.terminate()
