# Xavi's desktop sphere, native — 2026-10-20

Design: Xavi (`ByXavi270` / `XaviCottier`). Native implementation: PentaVM.

## What was different

PR41 ported Xavi's presets, materials and physics, but the desktop still did not look like
his sphere (`docs/gui/screenshots/29-esfera-holograma-frio.png`):

- His sphere drew **ArchieOS's own files** (`ARCHIEOS-NVIDIA · 2499 ELEMENTOS · 3 GB`,
  `display/visor`: a recorded scan of the repository). The native orb draws TFOG entities
  (at most 420) and is empty without signals (`33-native-orb-fixture.png`).
- His "holograma frío" is a **CSS filter over the finished canvas** (`LiveOrb.qml` at
  `1177407`: `hue-rotate(168deg) saturate(1.7) contrast(1.15) brightness(1.05)`): warm
  filaments become cyan, blue file beads become orange, cool rings become pink.
- Rings with ticks, a projector beam, a bright core, scan lines and mono labels came from
  Lumenfield 0449, whose source (`web/`, `sphere-layer.ts`) is **not** in the delivered
  `codigo fuente esfera neuronal.rar` (that is Sentry-Graph Astra, 2026-10-09). Those
  parts are rebuilt from the screenshots, not ported line by line.

## Result

- Rust `archie-lumen-design::content`: bounded, read-only listing of one folder (2 500
  nodes, 30 files per folder, breadth first; hidden entries and build output skipped;
  links never followed; nothing opened or written) and the exact `buildField` "regions"
  layout of `field.js`, serialised as the scene `LumenField::accept` already reads.
  C ABI `design_content_scene`.
- `LumenField`: setting `fieldSource` (`tfog` | `content`); the content listing runs off
  the GUI thread every 5 s or when its bounds change. Root from `--content-root`,
  `ARCHIE_CONTENT_ROOT`, the repository the display was built from, `/srv/archieos`.
- Renderer (`archie::visual::lumen`): `filament` (long C-shaped strands) and `holo_frame`
  (two tilted orbit rings with ticks, dotted rim, projector rings and beam, core beam).
  QML: glow core and haze, mono labels for the top-level folders and the root label.
- `HologramGrade.qml` + `shaders/hologram_grade.frag`: the CSS filter matrices in order,
  a light bloom (8 or 16 taps, by render budget), scan lines and a red/cyan split, over
  the finished View3D, with ACES tone mapping as Three.js used.
- Presets `holograma` (cold, Xavi's desktop) and `holograma-calido` (vivid, warm). The
  shell's desktop sphere opens on `holograma`; `LiveOrb` keeps its values until the person
  changes one, as for the polar profiles. `archie visual preset holograma` works.

## Verification

- Rust `archie-lumen-design`: 25 passing (4 new: classification, skip/determinism, bounds,
  FFI). `display/visual`: 19 suites passing (filament and frame cases added). Native
  CTest: 4 passing (new `contentSphereListsItsFolderWithoutTheObserver`). Shell QML: 77.
- Real frames with `--render-check` under WSLg/Mesa llvmpipe (software; Zink failed):
  `35-esfera-nativa-neural.png` (Esfera neuronal, preset `holograma`; la variante cálida se retiró).
  `ARCHIE_RENDER_CHECK_MS` lengthens the wait so a slow software renderer can settle.

## Limits

Not pixel parity: Lumenfield 0449's source was not delivered, so its filament curves,
ring geometry and label layout are reconstructions. Not measured on the RTX 3070 or any
real GPU; frame cost of 2 500 nodes × 18-segment filaments is untested there. Under the
grade the scene's MSAA and built-in glow are off and vignette/dithering are on, so the
light of thousands of additive lines sums in HDR and is tone mapped once (without the
post chain each line is tone mapped alone and the centre clips white).
