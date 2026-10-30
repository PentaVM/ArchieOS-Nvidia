# Softer light theme, official app icons and visible atmosphere

The user found the previous light canvas too bright, icon contrast inadequate,
and the light sphere atmosphere effectively missing. Continue the authorized
native screenshot-driven iterations on the same Nvidia repository/worktree.

Plan:
- [x] Darken the neutral substrate behind the 60% white veil and tint the 75%
  surfaces blue-gray. Keep body ink opaque and composited contrast >=4.5:1.
- [x] Keep app logos opaque, inside a contrasting icon well. Prefer installed
  assets; package original official logos for the known application suggestions.
  Add reproducible source/hash metadata and a download/verification script.
  Downloading an icon must not install or launch an app or make it available.
- [x] Replace the barely visible light sphere wash with an actual view-dependent
  atmospheric rim, soft external halo and internal volume. Preserve real native
  node positions, bounded labels/markers, motion and all authority boundaries.
- [x] Build, capture Orb/Forge and both themes in the real native renderer;
  inspect and adjust before accepting. Rerun affected contracts and review.
- [x] Save local change and actual screenshots; no model/runtime claims.

The original icon artwork must not be recolored or replaced with generated
approximations. Source attribution remains with each upstream project. Existing
untracked codebase-LOC reports are unrelated and must remain untouched.

Evidence: D:\ArchieOS-Theme-Refinement-20261028.

## Implementation and visual acceptance

The neutral light substrate is now #50657e under the retained 60% white veil,
yielding an opaque canvas around #bac2cc. The 75% surfaces use blue-gray rather
than pure white. Body ink remains opaque; reading surfaces remain resolved over
the desktop. Shared theme tests exercise text contrast >=4.5:1 after alpha
compositing and dock glyph contrast under highlights.

Forge uses contrasting pale icon wells and fully opaque original artwork in both
themes. All nine known suggestions now have official upstream fallbacks: Blender,
Firefox Browser, FreeCAD, GIMP, Inkscape, Kdenlive, KiCad, LibreOffice and OBS Studio.
The installed desktop icon wins when it resolves. App availability, operability,
launching, installation and Policy do not derive from icon presence. The shell's
suggestion rows also load the same app.icon image URLs; app.iconSource is only
provenance metadata.

The light neural sphere now has two view-dependent premultiplied materials:
internal volume with a stronger atmospheric rim, and an external halo at 1.08R.
World-space normals and the real camera position determine coverage. The dark
sphere material, native graph positions, camera and bounded marker/label paths
remain intact. This is an appearance change, not synthetic runtime state.

Visual iterations: first capture checked the softened canvas and atmospheric
volume; the subsequent build checked all nine logos together in Forge, then dark
regressions and the embedded shell list. All images below are actual native Qt
window captures, inspected after rendering:

- orb-light-atmosphere-1.png: visible teal volume, rim and halo with opaque labels.
- orb-dark-check.png: dark rendering regression check.
- forge-official-light.png and forge-official-dark.png: nine recognizable original
  logos, their full colors preserved, inside distinct icon wells.
- shell-forge/003-forge-shell-light.png and 005-forge-shell-dark.png: actual shell
  Forge rows. report.jsonl records bar.theme changing the shared theme with Forge
  still open: one click, zero without effect.

Captures show disconnected backend/bridge state honestly. They do not verify
Linux application launching, WSL, IRIS autonomy, model throughput or frame rate.
Windows emits an initial View3D camera-assignment warning and a legacy fps=0
diagnostic; no performance conclusion is drawn from these screenshots.

## Asset provenance and transport review

display/native/assets/app-icons/manifest.json pins source commits, URLs, exact
SHA256 hashes and byte counts. README.md and thirteen license/source documents
preserve upstream attribution and distinguish repository license defaults from
icon-specific notices. .gitattributes preserves original bytes across checkout.
Upstream license/source notices retain their original whitespace; their scoped
-whitespace attribute excludes only those unchanged vendor files from Git's
whitespace lint, without relaxing checks on implementation code.

scripts/fetch-app-icons.py defaults to offline verification. --fetch downloads
only pinned HTTPS resources, checks size/hash and static SVG/PNG structure, then
replaces files atomically. Source and license files are bundled for offline use.

Independent review found that initial urllib redirects were checked only after
the next request. Fixed before acceptance: the custom redirect handler validates
each destination before following it; HTTPS, same host, standard port and pinned
commit are required. Tests reject downgrade, cross-host, localhost, nonstandard
ports and unpinned redirects without real network traffic. --fetch and --verify
are mutually exclusive. The reviewer confirmed closure without further blockers.

The final --fetch succeeded against the real sources with the strict handler;
all nine image hashes and source/license documents remained unchanged. Both TLS
verification paths retain certificate validation.

## Verification and integration

- Native build: success; build-official-icons.log. Existing Rust/MinGW .drectve
  warnings remain nonfatal.
- CTest: 39/39 passed, including image decode, installed-icon priority and missing
  known-icon fallback contracts; native-tests.txt.
- Shell QML: 108/108 passed, including composited contrast and theme interaction;
  shell-tests-final.txt.
- Native QML: 24/24 passed; native-qml-tests-final.txt.
- Python downloader: 6/6 passed; final real fetch exit 0, icons-final-fetch.txt.
- All native capture/walk processes exited 0. git diff --check passed.

An initial final-QML invocation used an incorrectly quoted PowerShell logging
argument and exited before running tests. Corrected the argument, then reran both
suites fully; the passing counts above are from those complete runs.

Changes remain on codex/light-iris-cinema as a local commit. The branch also holds
prior integration work; publishing or merging its full diff is outside this
correction. Existing unrelated codebase-LOC reports remain untouched.
