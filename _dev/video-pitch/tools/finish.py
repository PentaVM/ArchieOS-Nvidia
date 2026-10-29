#!/usr/bin/env python3
"""Final pass: the narrator's captions as ASS at 1920x1080 (top, two lines at most), audio 48 kHz."""
import re, subprocess
P = '/tmp/claude-0/-home-user/4934f298-af32-52e3-99bd-bc587ce668b6/scratchpad/pitch'
W, OUT = f'{P}/work', f'{P}/out'
def t(s):
    h, m, rest = s.split(':'); sec, ms = rest.split(',')
    return int(h) * 3600 + int(m) * 60 + int(sec) + int(ms) / 1000
def a(x):
    h, r = divmod(x, 3600); m, s = divmod(r, 60)
    return f'{int(h)}:{int(m):02d}:{s:05.2f}'
blocks = open(f'{W}/narrator.srt').read().strip().split('\n\n')
events = []
for b in blocks:
    lines = b.split('\n')
    start, end = [t(x.strip()) for x in lines[1].split('-->')]
    events.append(f'Dialogue: 0,{a(start)},{a(end)},Narrator,,0,0,0,,' + ' '.join(lines[2:]))
ass = f"""[Script Info]
ScriptType: v4.00+
PlayResX: 1920
PlayResY: 1080
WrapStyle: 0
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Narrator,Inter,40,&H00FFFFFF,&H00FFFFFF,&H38120B0B,&H38120B0B,0,0,0,0,100,100,0,0,3,14,0,8,260,260,70,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
""" + '\n'.join(events) + '\n'
open(f'{W}/narrator.ass', 'w').write(ass)
subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-i', f'{W}/video.mp4', '-i', f'{W}/audio.wav',
                '-vf', f'ass={W}/narrator.ass', '-c:v', 'libx264', '-preset', 'medium', '-crf', '19', '-pix_fmt', 'yuv420p',
                '-c:a', 'aac', '-ar', '48000', '-b:a', '192k', '-movflags', '+faststart', '-shortest',
                f'{OUT}/archieos-nebius-demo.mp4'], check=True)
subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-i', f'{W}/video.mp4', '-i', f'{W}/audio.wav',
                '-c:v', 'copy', '-c:a', 'aac', '-ar', '48000', '-b:a', '192k', '-shortest',
                f'{OUT}/archieos-nebius-demo.clean.mp4'], check=True)
print('ok')
