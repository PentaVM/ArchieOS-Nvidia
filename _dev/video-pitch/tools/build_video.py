#!/usr/bin/env python3
"""Cuts the recorded take into the English pitch video.

take/raw.mkv + take/events.log + take/draw.log -> pitch/out/archieos-nebius-demo.mp4,
with Piper voices (Archie: en_US-ryan-high, narrator: en_US-lessac-high), the narrator's
captions burned in at the top (Archie's own subtitles are already on screen, drawn by the
Stage), and SRT files beside it.
"""
import json, os, re, subprocess, wave

S = '/tmp/claude-0/-home-user/4934f298-af32-52e3-99bd-bc587ce668b6/scratchpad'
TAKE, P = f'{S}/take', f'{S}/pitch'
OUT, WORK = f'{P}/out', f'{P}/work'
BRAND = '/home/user/wt/t3/display/shell/brand'
FONT = '/usr/share/fonts/opentype/inter/Inter-SemiBold.otf'
FONT_R = '/usr/share/fonts/opentype/inter/Inter-Regular.otf'
os.makedirs(OUT, exist_ok=True); os.makedirs(WORK, exist_ok=True)
FPS = 30

def run(*cmd):
    subprocess.run(cmd, check=True)

def tts(text, voice, path):
    if not os.path.exists(path):
        subprocess.run([f'{S}/tts/bin/python', '-m', 'piper', '-m', f'{S}/voices/{voice}.onnx', '-f', path],
                       input=text.encode(), check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    with wave.open(path) as w:
        return w.getnframes() / w.getframerate()

# --- the take
events = {}
for line in open(f'{TAKE}/events.log'):
    t, name = line.split(None, 1)
    events[name.strip().split()[0]] = float(t)
lines = []
for line in open(f'{TAKE}/draw.log'):
    m = re.match(r'\s*([\d.]+)s\s+(.*)', line)
    if m:
        lines.append((float(m.group(1)) + events['draw'], m.group(2).strip()))
end = events['end']
narr = dict(l.strip().split('|', 1) for l in open(f'{P}/narrator.txt') if '|' in l)

# --- voices
archie = [(t, text, f'{WORK}/a{i:02d}.wav') for i, (t, text) in enumerate(lines)]
alen = [tts(text, 'en_US-ryan-high', p) for _, text, p in archie]
nlen = {k: tts(v, 'en_US-lessac-high', f'{WORK}/n_{k}.wav') for k, v in narr.items()}

# --- the plan: (kind, args, out_seconds), audio cues (time in output, wav), captions
segments, cues, caps = [], [], []
now = 0.0
def cue(at, wav, text=None, who=None, dur=None):
    cues.append((at, wav))
    if text:
        caps.append((at, at + dur, text, who))

# 1. Intro: the boot animation, then the title.
intro = max(6.0, nlen['intro'] + 1.5)
segments.append(('intro', None, intro)); cue(1.0, f'{WORK}/n_intro.wav', narr['intro'], 'n', nlen['intro']); now += intro

# 2. The desktop, Sentry and Forge, in real time.
d0, d1 = events['desktop'], events['draw']
desk = max(d1 - d0, nlen['desktop'] + nlen['apps'] + 2.0)
segments.append(('clip', (d0, d1, (d1 - d0) / desk), desk))
cue(now + 0.4, f'{WORK}/n_desktop.wav', narr['desktop'], 'n', nlen['desktop'])
apps_at = now + max(events['sentry'] - d0, 0.4 + nlen['desktop'] + 0.5)
cue(apps_at, f'{WORK}/n_apps.wav', narr['apps'], 'n', nlen['apps'])
now += desk

# 3. The request, on a held frame.
ask = nlen['ask'] + 1.6
segments.append(('ask', d1, ask)); cue(now + 0.5, f'{WORK}/n_ask.wav', narr['ask'], 'n', nlen['ask']); now += ask

# 4. The drawing: each of Archie's lines at its own pace, the long stretches between them
#    sped up, the narrator in three of those stretches.
long_gaps = [i for i in range(len(archie) - 1) if archie[i + 1][0] - archie[i][0] > 30]
slots = dict(zip(long_gaps, ['fast', 'authority', 'stack']))
for i, (t, text, wav) in enumerate(archie):
    t_next = archie[i + 1][0] if i + 1 < len(archie) else events['drawn'] + 2.0
    raw = max(t_next - t, 0.05)
    o = alen[i] + 0.5
    cue(now, wav)
    k = slots.get(i)
    if k:
        cue(now + o + 0.3, f'{WORK}/n_{k}.wav', narr[k], 'n', nlen[k])
        o += 0.3 + nlen[k] + 0.6
    elif raw > o:
        o = min(raw, max(o, 4.0))
    segments.append(('clip', (t, t_next, raw / o), o))
    now += o

# 5. The finished picture, then the end card.
hold = max(end - (events['drawn'] + 2.0), nlen['outro'] + 1.0)
segments.append(('clip', (events['drawn'] + 2.0, end, (end - events['drawn'] - 2.0) / hold), hold))
cue(now + 0.5, f'{WORK}/n_outro.wav', narr['outro'], 'n', nlen['outro']); now += hold
segments.append(('end', None, 6.0)); now += 6.0
total = now

# --- render each segment at 1920x1080@30
def esc(t):
    return t.replace('\\', '\\\\').replace(':', '\\:').replace("'", "’").replace('%', '\\%')

parts = []
for n, (kind, a, dur) in enumerate(segments):
    out = f'{WORK}/seg{n:03d}.mp4'
    parts.append(out)
    enc = ['-r', str(FPS), '-c:v', 'libx264', '-preset', 'medium', '-crf', '19', '-pix_fmt', 'yuv420p', '-an', out]
    if kind == 'intro':
        # ArchieOS's own boot animation (90 full frames, ending on the logo), held, with
        # the line that says what this is.
        run('ffmpeg', '-loglevel', 'error', '-y', '-framerate', '30', '-start_number', '1', '-i', f'{BRAND}/boot/f%03d.jpg',
            '-vf', f"tpad=stop_mode=clone:stop_duration={dur},"
                   f"drawtext=fontfile={FONT_R}:text='{esc('An operating system you talk to  ·  Nebius hackathon demo')}':fontsize=34:fontcolor=0xc8d6e4:x=(w-tw)/2:y=h-130:alpha='if(lt(t,3.0),0,min(1,(t-3.0)/0.6))',"
                   f"fade=t=in:st=0:d=0.4",
            '-t', str(dur), *enc)
    elif kind == 'end':
        run('ffmpeg', '-loglevel', 'error', '-y', '-loop', '1', '-i', f'{BRAND}/archieos-app-512.png',
            '-f', 'lavfi', '-i', f'color=c=0x0b1220:s=1920x1080:d={dur}',
            '-filter_complex',
            f"[0]scale=300:-1[l];[1][l]overlay=(W-w)/2:200:shortest=1,"
            f"drawtext=fontfile={FONT}:text='{esc('You say it. Archie does it. You stay in charge.')}':fontsize=46:fontcolor=white:x=(w-tw)/2:y=600,"
            f"drawtext=fontfile={FONT_R}:text='{esc('Speech: NVIDIA Nemotron ASR  ·  Reasoning: Nemotron on Nebius  ·  Drawing: Photopea, by Archie')}':fontsize=28:fontcolor=0xb8c6d8:x=(w-tw)/2:y=690,"
            f"drawtext=fontfile={FONT_R}:text='{esc('Design: Xavi  ·  Native implementation: PentaVM')}':fontsize=28:fontcolor=0x8496ad:x=(w-tw)/2:y=745,"
            f"drawtext=fontfile={FONT_R}:text='{esc('Recorded on a headless rehearsal machine (software rendering). The drawing is real; long stretches are sped up.')}':fontsize=22:fontcolor=0x66788f:x=(w-tw)/2:y=h-90,"
            f"fade=t=in:st=0:d=0.5,fade=t=out:st={dur-0.8}:d=0.8",
            '-t', str(dur), *enc)
    elif kind == 'ask':
        run('ffmpeg', '-loglevel', 'error', '-y', '-ss', str(a), '-i', f'{TAKE}/raw.mkv', '-frames:v', '1', f'{WORK}/ask.png')
        run('ffmpeg', '-loglevel', 'error', '-y', '-loop', '1', '-i', f'{WORK}/ask.png',
            '-vf', f"drawbox=x=0:y=0:w=iw:h=ih:color=black@0.35:t=fill,"
                   f"drawtext=fontfile={FONT}:text='{esc('“Archie, draw the Mona Lisa.”')}':fontsize=72:fontcolor=white:x=(w-tw)/2:y=(h-th)/2:"
                   f"box=1:boxcolor=0x0b1220@0.75:boxborderw=36",
            '-t', str(dur), *enc)
    else:
        t0, t1, speed = a
        badge = ''
        if speed > 1.5:
            badge = (f",drawtext=fontfile={FONT}:text='{esc(f'×{round(speed)}')}':fontsize=34:fontcolor=white:"
                     f"x=w-tw-48:y=96:box=1:boxcolor=0x0b1220@0.7:boxborderw=14")
        run('ffmpeg', '-loglevel', 'error', '-y', '-ss', f'{t0:.3f}', '-t', f'{max(t1 - t0, 0.07):.3f}', '-i', f'{TAKE}/raw.mkv',
            '-vf', f"setpts=(PTS-STARTPTS)/{speed:.5f},fps={FPS},tpad=stop_mode=clone:stop_duration={dur}{badge}",
            '-t', f'{dur:.3f}', *enc)

with open(f'{WORK}/parts.txt', 'w') as f:
    f.writelines(f"file '{p}'\n" for p in parts)
run('ffmpeg', '-loglevel', 'error', '-y', '-f', 'concat', '-safe', '0', '-i', f'{WORK}/parts.txt', '-c', 'copy', f'{WORK}/video.mp4')

# --- the soundtrack: every cue at its time
inputs, chains = [], []
for i, (at, wav) in enumerate(cues):
    inputs += ['-i', wav]
    ms = int(at * 1000)
    chains.append(f'[{i}]aresample=48000,adelay={ms}|{ms}[c{i}]')
mix = ''.join(f'[c{i}]' for i in range(len(cues)))
run('ffmpeg', '-loglevel', 'error', '-y', *inputs, '-filter_complex',
    ';'.join(chains) + f';{mix}amix=inputs={len(cues)}:normalize=0,apad,atrim=0:{total:.3f},loudnorm=I=-16:TP=-1.5[a]',
    '-map', '[a]', '-ac', '2', f'{WORK}/audio.wav')

# --- captions: the narrator at the top, burned in; SRTs of everything beside the video
def ts(x):
    h, r = divmod(x, 3600); m, s = divmod(r, 60)
    return f'{int(h):02d}:{int(m):02d}:{int(s):02d},{int(round((s % 1) * 1000)):03d}'
def srt(entries, path):
    with open(path, 'w') as f:
        for n, (a, b, t) in enumerate(entries, 1):
            f.write(f'{n}\n{ts(a)} --> {ts(b)}\n{t}\n\n')
narr_caps = [(a, b + 0.3, t) for a, b, t, w in caps if w == 'n']
srt(narr_caps, f'{WORK}/narrator.srt')
# Archie's lines, at the times they are heard in the cut.
arch_caps = []
ai = 0
for at, wav in cues:
    m = re.search(r'/a(\d+)\.wav$', wav)
    if m:
        k = int(m.group(1)); arch_caps.append((at, at + alen[k] + 0.2, 'Archie: ' + archie[k][1]))
srt(sorted(narr_caps + arch_caps), f'{OUT}/archieos-nebius-demo.en.srt')
style = ('FontName=Inter,FontSize=15,PrimaryColour=&H00FFFFFF,BackColour=&H99201208,BorderStyle=3,'
         'Outline=8,Shadow=0,Alignment=8,MarginV=40,MarginL=160,MarginR=160')
run('ffmpeg', '-loglevel', 'error', '-y', '-i', f'{WORK}/video.mp4', '-i', f'{WORK}/audio.wav',
    '-vf', f"subtitles={WORK}/narrator.srt:force_style='{style}'",
    '-c:v', 'libx264', '-preset', 'medium', '-crf', '19', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-b:a', '192k',
    '-movflags', '+faststart', '-shortest', f'{OUT}/archieos-nebius-demo.mp4')
# Clean versions for re-editing: no burned captions, and the voices alone.
run('ffmpeg', '-loglevel', 'error', '-y', '-i', f'{WORK}/video.mp4', '-i', f'{WORK}/audio.wav',
    '-c:v', 'copy', '-c:a', 'aac', '-b:a', '192k', '-shortest', f'{OUT}/archieos-nebius-demo.clean.mp4')
run('ffmpeg', '-loglevel', 'error', '-y', '-i', f'{WORK}/audio.wav', f'{OUT}/archieos-nebius-demo.voices.wav')
json.dump({'total_seconds': round(total, 1), 'segments': [(k, d) for k, _, d in segments]}, open(f'{WORK}/plan.json', 'w'))
print(f'done: {total:.1f}s')
