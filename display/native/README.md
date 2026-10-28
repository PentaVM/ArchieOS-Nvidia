# Observatory controls

The native Observatory opens on Orb. Its scene switcher is ordered Orb, Forge, World.
`--scene orb|forge|world` still selects a specific opening view.

Archie's live voice exposes two visual tools: `observatory_scene` and `orb_preset`. The same
controls are on the CLI: `archie visual scene <name>`, `archie visual preset <name>` and
`archie visual list`. Both are clients of `archie_orb::control`, which sends one JSON line to
the native display on `127.0.0.1:7668` and reads one line back (`ok` or `error: <reason>`).

The display is the only place the names live. `list` answers with its scenes (`orb`, `forge`,
`world`, from `DisplayControl`) and its presets: every `presets/*.json` file is embedded in the
binary and is a preset named after its file. `field` (Regiones vivas) is the original look,
with no overrides. Applying a preset opens Orb. A preset may only set keys that exist in the
renderer's defaults, with values that pass the same checks as the panel (type, `#rrggbb`
colours, known layout, numbers inside their range, not clamped): one bad key refuses the whole
file and the reply says which. Broken preset files are reported when the display starts. HUD
labels are the `labels` setting, so a preset can hide them. Local overlay toggles use
the same presentation settings and do not change OS authority.

PR41 also preserves its local `cinematic`, `overlay` and shell `panel` controls. The
eight `polar-*` profiles from Xavi's delivered source are embedded by the Rust
`archie-lumen-design` static library and join the same validated renderer catalog.
Qt/QML, GPU shaders and World remain the native host. See
[`source-port.md`](../../_dev/verification/xavi/source-port.md) for source attribution,
CPU validation and the explicit visual/hardware fidelity limits.

`holograma` and `holograma-calido` are Xavi's desktop sphere: the content sphere lists a
folder read-only (Rust, `fieldSource: content`) and draws it with his hologram grade,
rings, projector and labels. `--preset holograma --content-root <dir>` opens it; see
[`content-sphere.md`](../../_dev/verification/xavi/content-sphere.md).

Build with Cargo plus the existing Qt6/CMake dependencies; CMake builds/links the
Rust static library automatically. Optional CPU shader checks use Mesa llvmpipe:

```sh
cmake -S display/native -B build/native -G Ninja -DARCHIE_NATIVE_BUILD_TESTS=ON -DARCHIE_NATIVE_BUILD_SHADER_TESTS=ON
cmake --build build/native --parallel 2
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software ctest --test-dir build/native --output-on-failure
```

The shader test explicitly selects/verifies software OpenGL itself. It is a synthetic
rendering test, not a benchmark. ISO/VBox keep their existing launch contract. For a
WSL Operator image, use that build as `ARCHIE_NATIVE_BUILD_DIR=build/native` when
packaging; `archie-design --preview` is the explicit synthetic QML preview.

The listener changes only the local view. It cannot run commands, access Stage,
or write to displayd. If the Observatory is closed, Archie reports that the local
window did not respond.

Forge includes a CLI tile that starts `foot` when a graphical Wayland or X11
session and `foot` are present. The EGLFS image has no window manager, so Forge
shows its existing launch limitation there.


World reads `GET http://127.0.0.1:7666/api/exploration` through `ArchieBridge.exploration`.
The final hop requires the separate read-only `projection.json` bearer credential. Native
code reads only that owner-private regular file (up to 512 bytes, schema 1, 64 hex characters),
refusing symlinks/reparse points, public Unix permissions and Windows ACLs granting another
principal access. It never reads `control.json` and never exposes the token to QML/logs.
Runtime path precedence matches Rust: `ARCHIE_WORLD_RUNTIME_DIR`, `XDG_RUNTIME_DIR/archie-worldd`,
`LOCALAPPDATA/ArchieOS/worldd/runtime`, then `HOME/.cache/archie-worldd/runtime`. No configured
private location or invalid credential produces unavailable; no anonymous request is sent.
`WorldHud` reuses the shell's actual `Theme`, `Panel`, `Chip` and `SourceChip` resources,
including light/dark mode, typography, domain marks and motion preferences. Design credit:
Xavi (`ByXavi270` / `XaviCottier`); native implementation: PentaVM. No shell source is copied.
A shell launch creates a hidden native World host in the same QML engine; `world` is only
advertised after that host exists. `--shell --scene world` or the normal visual scene command
opens it. Escape/close returns to the shell. The hidden host does not override shell Orb
activity or reset its theme.

The HUD shows daemon job states, personal/work domains, tile/cache/byte counters, provider
rate limits and provider/local quota provenance. The system inspector reads snapshot status,
models, nodes, raw metric registries and kernel availability. Effective end-to-end inference
TPS is shown only when Core's measured inference availability flag is set; generation-only
TPS and GPU utilization remain unavailable. A new job ID applies its AOI camera plan once.
Missing `camera.elevation_m` uses a labelled
provisional reference datum. Its first measured terrain-ready elevation may correct the same
job once while the camera remains automatic; manual takeover blocks this correction. Measured
negative elevations remain negative. Subsequent progress polls and focus changes do not restart
the plan or change a job, quota, download or Core policy. Rendering may interpolate locally.

Only bounded decoded `preview` values are rendered: north-to-south terrain grids up to 33x33,
uniform WebMercator rows (north to south), up to 128 building rings (512 vertices each), measured building-height walls and outlines when
height is unknown. Geographic metres use the same Earth-fixed model frame as the globe and
AOI circle. The local camera frames the actual AOI surface, preserving the global orbit clamps
outside exploration. Unknown elevation samples produce holes; they are never replaced with
invented terrain. Building ground level uses sampled terrain when available, otherwise a
clearly labelled sea-level datum. Small display offsets/stroke widths only avoid z-fighting.
Artifact paths remain metadata: the renderer never opens downloaded paths. The HUD marks
`projection_truncated` and reports `preview.omitted_buildings`, keeping bounded view contents
distinct from full job counters. High-latitude grid placement and nearest elevation lookup
both use the same WebMercator row convention as decoded source tiles and exported artifacts.

This is a bounded geometry preview, not full-resolution terrain or photoreal imagery.
Downloaded imagery is not yet textured; building walls have no inferred roof/height, holes
are not modelled and antimeridian-crossing terrain bounds are not supported. At close AOI scale
(eye-to-target distance <= 0.5 scene units) with actual terrain triangles, the local geometry
preview suppresses the global Earth basemap and atmosphere, including the ocean surface. This
lets source-negative elevations remain visible without lifting or clamping them to sea level.
The HUD reports the sample elevation range and source camera elevation; this view is terrain,
not an ocean/bathymetry simulation. At wider framing the normal Earth context remains.
Actual visual fidelity, near-plane depth precision,
GPU behavior and the live desktop-to-World transition have not been visually verified.
No hardware, deployment, model accuracy or latency claim follows from these CPU/QML checks.

Focused checks (no visible native window):

```sh
node display/native/tests/test_exploration.js
cmake -S display/visual -B build/visual -DARCHIE_VISUAL_BUILD_TESTS=ON
cmake --build build/visual --target test_camera test_observatory --parallel 2
ctest --test-dir build/visual -R 'test_camera|test_observatory' --output-on-failure
cmake -S display/native -B build/native -DARCHIE_NATIVE_BUILD_TESTS=ON
cmake --build build/native --target test_world_hud test_exploration_preview test_exploration_credential --parallel 1
QT_QPA_PLATFORM=offscreen ctest --test-dir build/native -R 'world_hud_contracts|exploration_preview_contracts|exploration_credential_contracts' --output-on-failure
```
