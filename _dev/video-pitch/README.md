# Video pitch: ArchieOS for the Nebius hackathon

The pitch video is cut outside the repository, on the director's brief storyboard (2:48):
real captures of ArchieOS, the 3D shots with the real desktop on their screens, the receipts
and the Nemotron call as graphics, a narrator, captions and a score. The cut, its pieces and
the DaVinci timeline live with the pitch's other assets, not in git (they are hundreds of MB).

What stays here is how the first, headless take of the demo was made, for reference:

- `tools/take.sh`: one take on Xvfb: the stage, the shell, Firefox, the dock clicks, the
  drawing in Photopea, and ffmpeg.
- `tools/build_video.py`, `tools/finish.py`: how that take was cut, voiced and captioned.
- `narrator.en.txt`, `narrator.es.txt`, `take-draw.log`, `take-events.log`: its script and
  what it recorded.

The video that take produced was removed: it did not follow the storyboard, and its captions
were burned into every shot.

To record the real demo on the node: `display/session/nebius-demo.sh` with
`ARCHIE_UI_LANG=en LISTEN_LANGUAGE=en` (docs/DEMO-NEBIUS.md).
