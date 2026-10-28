# Native World verification — 2026-10-25

Repository remote checked before editing: `https://github.com/PentaVM/ArchieOS-Nvidia.git`.
Worktree: `C:\Users\Penta\.codex\worktrees\archieos-nvidia-latest\ArchieOS`.
Display changes only; existing uncommitted feature work was preserved. No Core authority or Claude/Xavi GUI files were edited.

## Root causes and corrections

- `Texture.source` supports file/qrc URLs, whereas the bounded memory imagery uses `image://world/`. The production `WorldImageryMaterial` now loads through an `Image` in `sourceItem`. The older gray diagnostic never requested provider images; the rebuilt path did.
- GeoFabric's UV convention has north at v=0. Qt's mesh convention rendered each tile upside down. An actual Qt render test with red north / blue south failed, then passed with `flipV`.
- Duplicating measured terrain and an image surface with a half-metre offset caused black triangle z-fighting at globe-scale float precision. Imagery now **is** the measured surface within accepted tile coverage. A per-source geometric partition retains the original terrain only outside that coverage. No inflated elevation, hidden-terrain diagnostic, or placeholder mesh remains. Tests verify exact measured vertices, uncovered coordinates, and preservation of triangle area without holes/duplicate coverage.
- Transparent source pixels are composed over neutral terrain on that same surface. They do not become black imagery or sky holes. The HUD explains this neutral material. An actual partially transparent Qt image fixture failed before the correction and now passes.
- Local map rendering uses linear tone mapping (sRGB output), without the cinematic vignette. A gray-128 input now renders gray-128. `TonemapModeNone` was explicitly rejected by the fidelity test because it produced linear gray-55. Global globe/orb styling remains separately controlled.
- Later aircraft markers could intercept clicks on overlapping AOI navigation beacons. A production MouseArea QTest reproduced this, and AOI navigation now has pointer/render priority. The final native click tests select the older capture correctly.
- UTF-8 AOI punctuation is preserved. Signal source/aggregate counts identify lower bounds, partial coverage, and omitted observations. Dated UTC snapshot timestamps remain visible rather than presenting a completed aircraft capture as live.

## Evidence and commands

Host: WSL distro `ArchieOS-Operator-RTX3070-Test`, user `archie`, Qt 6.12, actual WSLg/llvmpipe renderer. This verifies real Qt rendering and native pointer delivery, **not RTX hardware, model accuracy, latency, or deployment**.

```sh
cmake --build /tmp/archie-world-native-check -j2
ctest --test-dir /tmp/archie-world-native-check --output-on-failure
ctest --test-dir /tmp/archie-world-visual-check --output-on-failure
node display/native/tests/test_exploration.js
```

Results: **8/8 native CTest**, **19/19 visual CTest**, Node helper contracts pass. `git diff --check -- display` passes (line-ending notices only). Logs are stored beside this file. The new native texture test exercises the production material, memory provider, custom MeshGeometry, real render surface, north/south orientation, alpha-gap composition, and midtone fidelity.

Actual runtime: `/tmp/archie-world-open-data-20261025-v3/runtime`; trusted display projection port 17666, isolated visual control port 17668.

```sh
ARCHIE_WORLD_RUNTIME_DIR=/tmp/archie-world-open-data-20261025-v3/runtime \
  /tmp/archie-world-native-check/archie-display --scene world \
  --displayd-port 17666 --control-port 17668 --imagery-source sentinel2 \
  --ui-walk /tmp/archie-world-old-aoi-walk.json \
  --ui-walk-out /tmp/archie-world-sentinel2-walk-delivery
```

The equivalent run with `--imagery-source blue_marble` writes `/tmp/archie-world-blue-marble-walk-fixed`.

The script begins at the newer AOI `e0776578fddffc74b7af7703ced11e1a` (37.69, -122.37), then sends a real pointer click to `world.pick.aoi:766fc39a847d540c013ffbff14e0fb0d` (37.62, -122.38; radius 15 km). Both final reports show `selectedArea` changing to that older ID and `selectedPoint` becoming its AOI marker, with 1 click / 0 no-effect. NASA pixel change: 63.1; Blue Marble: 81.3.

Visually inspected final artifacts:

- `native-sentinel2-20261020.png`: dated NASA HLS Sentinel-2, coherent north-oriented measured surface, neutral alpha gaps, source attribution and snapshot times, older area selected. The source date is cloudy; its bounded preview is approximately 242 m/pixel despite approximately 60 m/pixel source imagery.
- `native-blue-marble.png`: real NASA Blue Marble static imagery, approximately 484 m/pixel; same measured surface and successful older-area pointer selection.
- `native-latest-aoi.png`: state before the older-area click.
- `native-older-aoi-pointer-report.jsonl`, `native-blue-marble-pointer-report.jsonl`, and the saved steps contain the actual selection evidence.

Only Sentinel-2 and Blue Marble were captured. Google remained an online-only in-memory preview; no proprietary pixels were exported or logged. Existing capture guards remain active.

## Main display files changed in this debugging pass

`qml/WorldImageryMaterial.qml`, `qml/Main.qml`, `qml/WorldBeacons.qml`, `qml/WorldHud.qml`, `qml/Exploration.js`, `src/WorldVisuals.cpp`, `src/WorldImageProvider.h`, `src/UiWalk.h`, `src/main.cpp`, `CMakeLists.txt`, and focused native tests. The final executable is `/tmp/archie-world-native-check/archie-display`; its QML/assets are embedded. Parent integration owns persistent-copy startup verification, repository integration, commits, and pushes.
