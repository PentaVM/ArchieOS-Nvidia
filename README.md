<div align="center">

**English** · [Español](README.es.md) · [Português](README.pt-BR.md) · [Français](README.fr.md) · [中文](README.zh-CN.md)

<br>

<img src="docs/readme/boot.gif" width="760" alt="The ArchieOS start-up animation: the Archie mark draws itself and lights up">

<br>
<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-logo-dark.png">
  <img src="display/shell/brand/archieos-logo-light.png" width="360" alt="ArchieOS">
</picture>

### A personal AI that lives inside the operating system — and asks before it touches anything.

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-20d79d?style=flat-square)](LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-45e3f5?style=flat-square)
![C++ · Qt 6](https://img.shields.io/badge/display-C%2B%2B%20%C2%B7%20Qt%206-9a75ff?style=flat-square)
![NVIDIA Nemotron](https://img.shields.io/badge/NVIDIA-Nemotron-76b900?style=flat-square)
![Nebius Token Factory](https://img.shields.io/badge/Nebius-Token%20Factory-ffca70?style=flat-square)

</div>

---

## What is ArchieOS?

ArchieOS is an operating system with an assistant, **Archie**, built in. You speak to it in
your own words, in English or Spanish. It answers out loud, opens your applications and uses
them the way a person would: it moves its own pointer, clicks, types and draws.

What makes it different is **who is in charge**. The AI model never touches your computer
directly. It can only *suggest* an action. The operating system decides whether that action is
allowed, lets it happen once, checks it again at the last moment, and keeps a receipt. The
moment you move your mouse, you are back in control.

<div align="center">
<img src="docs/readme/voice.svg" width="900" alt="You say: Archie, draw the Mona Lisa. Nemotron proposes draw_picture. Policy allows it. Archie draws in Photopea.">
</div>

## See it

**"Archie, draw the Mona Lisa."** Archie opens Photopea in the browser, picks up the brush and
draws the painting line by line with its own pointer — sepia first, then red chalk, the way
Leonardo worked — and says what it is doing as it goes.

<div align="center">
<img src="docs/readme/mona-lisa-timelapse.gif" width="760" alt="Time-lapse: Archie draws the Mona Lisa in Photopea, stroke by stroke">
</div>

**The desktop.** One window for everything: the orb that shows what the machine is doing, the
dock (Vault, Forge, Sentry, World, Stage, Settings) and whatever Archie opens on top. It follows
your light or dark theme.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/desktop-dark.jpg">
  <img src="docs/readme/desktop-light.jpg" width="760" alt="The ArchieOS desktop: the orb, the top bar and the dock">
</picture>
</div>

**World.** The Earth lit by the real sun, under the real night sky, with a quiet search bar
(a place or "lat, lon"), layers, a ruler and Archie's console.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/world-dark.jpg">
  <img src="docs/readme/world-light.jpg" width="760" alt="World: the globe, lit by the sun, with the constellations behind it">
</picture>
</div>

## How it keeps you in control

<div align="center">
<img src="docs/readme/authority.svg" width="900" alt="The model proposes, Cerebra decides, Policy grants a one-operation lease, the Stage checks it again and acts, and a receipt comes back.">
</div>

1. **The model proposes.** Nemotron (on Nebius) or a local model turns what you said into one
   suggested action, such as `draw_picture("Mona Lisa")` or `open_app("Sentry")`.
2. **Cerebra decides.** ArchieOS's own reasoning layer checks the request makes sense — or asks
   you a question first.
3. **Policy authorises.** A plain text file (`apps.toml`) lists which applications Archie may
   open and what it may do in each. Allowed actions get a sealed lease that is good for one
   operation; everything else is refused.
4. **The Stage acts.** The Stage is the window system Archie works in. It checks the lease and
   the target window once more, then does exactly that one thing, and returns a receipt.
5. **You win, always.** Touch the mouse or keyboard and the screen says **HUMAN CONTROL**;
   Archie stops and waits. After a few quiet seconds it may continue.

## What you can do today

| | |
| --- | --- |
| 🎙️ **Talk** | Speech in, speech out, on your machine. NVIDIA Nemotron 3.5 streaming speech recognition, Nemotron on Nebius for the answer, and a local voice (Piper on CPU, Magpie on GPU). Archie replies in the language you used. |
| 🖱️ **Act** | Open apps, click, type, scroll, drag and draw — always through the Stage and its policy. Archie finds buttons through accessibility first and only looks at the screen when it has to. |
| 🎨 **Draw** | "Draw the Mona Lisa" (and other pictures it knows) in Photopea, at a human pace you can follow on video. |
| 🌍 **Explore** | World: the globe, real sunlight, real stars, places by name or coordinates. |
| 🛡️ **Watch** | Sentry: what is on your network and what is talking to whom. |
| 👁️ **See** | IRIS, Archie's eyes: tell it what to look for, it returns where it is. Faces are always blacked out — it never identifies people. |
| 🎬 **Edit video** | Timelines of clips, titles, music and fades rendered by MLT (Kdenlive's engine), without a window. |
| 🧩 **Connect** | Archie's tools for coding agents over MCP. |

## Try it

ArchieOS runs as a Linux system: on a PC, in a virtual machine, or under **WSL2 on Windows**.

**On Windows (WSL2).** Build the image and install it:

```bash
sudo ./scripts/wsl/build-rootfs.sh ./dist
```

```powershell
wsl --install --from-file .\dist\ArchieOS.wsl
```

Then follow [first run](docs/FIRST-RUN.md). The [WSL build guide](docs/WSL-BUILD.md) and the
[security boundary](docs/SECURITY-BOUNDARY.md) explain what the image contains and what it keeps
apart.

**The Mona Lisa demo.** Inside ArchieOS (or any Linux with the pieces built), one command opens
the desktop, the browser and Archie listening in English and Spanish:

```bash
display/session/nebius-demo.sh
```

What to build first, the options and a suggested script are in
[`docs/DEMO-NEBIUS.md`](docs/DEMO-NEBIUS.md).

<details>
<summary><b>Build from source</b></summary>

<br>

The control plane is Rust (1.88+); the desktop and the Stage are C++ with Qt 6.

```bash
# Archie's voice, hands and tools
cd core
cargo build --release -p archie-voice --features speech
cargo test --workspace --locked

# The Stage (Wayland compositor) and the desktop
cmake -S display/stage  -B build/stage   -G Ninja && cmake --build build/stage
cmake -S display/native -B build/display -G Ninja && cmake --build build/display
```

`--features speech` needs NeMo-Speech.cpp (`NEMO_SPEECH_PREFIX`) and PulseAudio;
`scripts/models/install-nemotron-speech.sh` fetches the pinned recognition model, and
`scripts/models/install-piper-runtime.sh` / `install-piper-voices.sh` the local voices.

Talk to Archie with the Stage, using a Nebius Token Factory key:

```bash
NEBIUS_TOKEN_FACTORY_KEY=... display/stage/run-archie.sh public
```

The answering model is replaceable: NVIDIA NIM, Nebius Token Factory or any local
OpenAI-compatible server. Keys are read at request time and never written to the repository or
the image. `scripts/check.sh` runs the checks worth running before a push.

</details>

## Under the hood

```text
 microphone ─► Nemotron 3.5 ASR ─► floor ─► Nemotron (Nebius) ─► voice (Piper / Magpie)
                                               │ proposes
 camera ─► IRIS ─► scene + memory ─────────────┤
                                               ▼
                                 Cerebra ─► Policy ─► Stage ─► applications
                                               │
                         TFOG signals ─► archie-displayd ─► desktop (Orb · Forge · World)
```

- **Rust** for authority, policy, the scheduler, the model router, memory and state
  (PostgreSQL/TimescaleDB), Cerebra and the Vault.
- **C++ / Qt Quick 3D** for the desktop, the Orb, World, Cortex, Sentry and the Stage.
- Identity, memory, policy and authority live in ArchieOS; language and vision models are
  replaceable resources it calls.

## Status

This is an alpha, built for the NVIDIA × Nebius hackathon. Honestly:

- **Works and tested:** speech to speech in English and Spanish, the Stage and its policy,
  opening and using apps by voice, the Mona Lisa drawing, World, the desktop in light and dark,
  IRIS detection, video rendering, the WSL2 image.
- **In progress:** IRIS Beta 2 training; Cortex steering the desktop's view by command; IRIS
  proposing actions beyond clicks; the CUDA build of the Python-free IRIS runtime.
- **Known limits:** the drawing runs at a person's pace (minutes, not seconds); in the ArchISO
  image, apps open through the Stage rather than from Forge; CI runs locally while GitHub
  Actions is paused.

## Read more

[Architecture](ARCHITECTURE.md) · [Archie](ARCHIE.md) · [Hackathon provenance](HACKATHON.md) ·
[Stage](docs/STAGE.md) · [Speech](docs/SPEECH-NEMOTRON.md) · [IRIS](docs/IRIS.md) ·
[IRIS runtime](docs/IRIS-RUNTIME.md) · [Security](docs/SECURITY.md) ·
[World](docs/WORLD-EXPLORATION.md) · [Orb rendering](docs/TFOG-ORB-RENDERING.md)

## Credits

- **PentaVM** — architecture, the Rust and C++ implementation, the control plane, IRIS, the voice
  stack and the native desktop.
- **Xavi** ([`ByXavi270`](https://github.com/ByXavi270) / XaviCottier) — the design: the
  desktop, the neural sphere, the logo and its start-up animation.
- **Santiago Rearte** (Sheriff) — the Nebius Token Factory and Nemotron integration, and testing
  on his own WSL.
- Built on NVIDIA Nemotron, NeMo and NIM, Nebius Token Factory, Piper, MLT, Qt and Wayland.

## License

Apache License 2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE). Apache rather than MIT for its
patent grant: this repository contains device control, physical actuation and inference-routing
work. Data providers keep their own terms; `providers/` records the attribution each one
requires, and ArchieOS refuses to show data whose attribution is missing.

<div align="center">

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-icon-256.png">
  <img src="display/shell/brand/archieos-icon-light-256.png" width="56" alt="">
</picture>

**Made with love ♥**

*by PentaVM, Xavi and Sheriff*

</div>
