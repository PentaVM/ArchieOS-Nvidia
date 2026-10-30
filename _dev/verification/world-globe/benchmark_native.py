#!/usr/bin/env python3
"""Run real native camera QA, preserving operational telemetry only."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time

parser=argparse.ArgumentParser()
parser.add_argument('--bin-dir',type=Path,required=True)
parser.add_argument('--native-bin',type=Path,required=True)
parser.add_argument('--run-dir',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
root=Path(__file__).parent
with args.output.with_suffix('.log').open('w') as helper_log:
    helper=subprocess.Popen(['python3',str(root/'globe_demo.py'),'--bin-dir',str(args.bin_dir),
        '--run-dir',str(args.run_dir),'--world-port','18667','--display-port','18666',
        '--native-port','18668','--keep'],stdout=helper_log,stderr=helper_log)
    try:
        descriptor=args.run_dir/'runtime/navigation.json';deadline=time.monotonic()+25
        while not descriptor.exists():
            assert helper.poll() is None and time.monotonic()<deadline
            time.sleep(.05)
        environment=dict(os.environ,ARCHIE_WORLD_RUNTIME_DIR=str(args.run_dir/'runtime'),QT_QPA_PLATFORM='xcb',LIBGL_ALWAYS_SOFTWARE='1',QSG_INFO='1')
        environment['QSG_RHI_BACKEND']='opengl'
        log_path=args.output.with_name(args.output.stem+'-renderer.log')
        with log_path.open('w') as log:
            run=subprocess.Popen(['xvfb-run','-a','-s','-screen 0 1920x1080x24 -nolisten tcp',
                str(args.native_bin),'--scene','world','--displayd-port','18666','--control-port','18668',
                '--imagery-source','google_satellite','--ui-walk',str(root/'earth-render-walk.json'),
                '--ui-walk-out',str(args.run_dir/'walk')],env=environment,stdout=log,stderr=log,start_new_session=True)
            try:
                returncode=run.wait(timeout=130)
            finally:
                try:os.killpg(run.pid,signal.SIGTERM)
                except ProcessLookupError:pass
        assert returncode==0,returncode
        log=log_path.read_text()
        subprocess.run(['python3',str(root/'summarize_earth.py'),str(args.run_dir/'walk/report.jsonl'),str(args.output)],check=True)
        states=json.loads(args.output.read_text())
        for state in states[1:]:
            assert state['loaded_meshes']>0 and state['triangles']>0,state
            assert state['earth']['state']=='ready',state
            assert state['renderer']['fps']>0,state
        args.output.with_name(args.output.stem+'-graphics.json').write_text(json.dumps(dict(
            graphics_api='OpenGL',software_requested=True,hardware_acceleration_verified=False,native_exit=returncode,
            source_payloads_saved=False),indent=2)+'\n')
    finally:
        summary=args.run_dir/'summary.json'
        if summary.exists():
            info=json.loads(summary.read_text());pid=info['daemon_pids'][0]
            command=Path('/proc',str(pid),'cmdline').read_bytes()
            assert str(args.run_dir/'runtime').encode() in command and b'archie-worldd' in command
            os.kill(pid,signal.SIGTERM)
        try:helper.wait(timeout=10)
        except subprocess.TimeoutExpired:helper.terminate()
