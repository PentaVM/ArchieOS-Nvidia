# Video pitch: ArchieOS for the Nebius hackathon (English)

`archieos-nebius-demo.mp4`: 2:49, 1920×1080, English throughout: the GUI
(`ARCHIE_UI_LANG=en`), Archie's narration, the narrator and the captions.

What is in it:

1. ArchieOS's boot animation and the line "An operating system you talk to".
2. The desktop in real time: the orb, then Sentry and Forge opened from the dock with
   sealed clicks through the Stage.
3. The request: "Archie, draw the Mona Lisa."
4. Archie drawing it in Photopea (photopea.com, real Firefox) with its own pointer: 463
   lines, 41 commands, 0 refused, 321 s. Each of Archie's lines plays at its own pace; the
   long stretches between them are sped up (the ×N badge says by how much).
5. The finished drawing and the end card: speech is NVIDIA Nemotron ASR, reasoning is
   Nemotron on Nebius; design by Xavi, native implementation by PentaVM.

How it was made, so nothing in it is mistaken for more than it is:

- Recorded on a headless rehearsal container (Xvfb, software rendering), not on the RTX
  3070 node: the stage, the shell and the drawing are the real binaries of this branch.
- There is no microphone in that container. The request is a title card, and the drawing
  starts from `archie-voice draw "the Mona Lisa"`, the same drawing the conversation's
  `draw_picture` starts. Archie's lines are its own narration (`take-draw.log`), spoken by
  Piper (en_US-ryan-high); the narrator is Piper en_US-lessac-high reading
  `narrator.en.txt`.
- The end card says so in one line.

Files:

- `archieos-nebius-demo.en.srt`: every line (narrator and Archie) with its time in the cut.
- `narrator.en.txt`, `narrator.es.txt`: the narrator's script, in English and Spanish.
- `take-draw.log`, `take-events.log`: what the take recorded, with times.
- `tools/take.sh`: the take (stage, shell, Firefox, the dock clicks, the drawing, ffmpeg).
- `tools/build_video.py`, `tools/finish.py`: the cut, the voices and the captions.

To record the real one on the node, `display/session/nebius-demo.sh` with
`ARCHIE_UI_LANG=en LISTEN_LANGUAGE=en` (docs/DEMO-NEBIUS.md).
