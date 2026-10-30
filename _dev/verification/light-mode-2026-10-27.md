# Light sphere: native rendering verification, 2026-10-27

Repository remote checked: `https://github.com/PentaVM/ArchieOS-Nvidia.git`.
Design: Xavi (`ByXavi270` / `XaviCottier`). Native implementation: PentaVM.

The desktop and native Observatory now use the existing shell light tokens by
default. Neural geometry has a separate light ink shader path; the original dark
fragment programs and additive blend behavior remain intact. Upper-left file
snapshot diagnostics moved behind **Detalles**. Model rows use the existing
read-only activity projection; absent sources display **Sin telemetría**, and
absent measured TPS remains `—`.

## Real render failure and correction

The first native light capture showed white, almost invisible geometry despite
the pale background and correct QML theme properties. A render-only trace verified
`scene.lightMode=true`, the uniform value `1`, and the current embedded shader
source. Disabling shader caches did not fix the picture. Forcing an opaque dark
fragment output did produce dark geometry, ruling out a missing material or a
background-only theme failure. Selecting separate light shader URLs alone also
left the white geometry unchanged.

The working correction uses premultiplied RGB/coverage with `One` /
`OneMinusSrcAlpha` in light mode. The previous `SrcAlpha` source factor also
multiplied the framebuffer alpha, making the translucent View3D composition lift
the pale geometry toward white. The light fragment programs now multiply their
ink RGB by coverage explicitly. The dark path keeps `SrcAlpha` / `One` and its
original shader code. Temporary diagnostic outputs and the trace timer were
removed.

## Evidence

All output artifacts were written to `D:\ArchieOS-Light-IRIS-Cinema`:

- `orbe-light-final.png`: inspected real native capture, dark category ink on
  `RGB(238,243,248)` background.
- `orbe-dark-final.png`: inspected real native capture, original gold/cyan
  additive geometry on the dark background.
- `orbe-light-final.log`, `orbe-dark-final.log`: Qt selected the D3D11 backend and
  NVIDIA GeForce RTX 3070 adapter for these captures.
- `native-light-final-build.log`: `archie-display` build completed successfully.
- `qml-final-after-ink.txt`: 16 QML checks passed, including the real camera
  functions/timer handlers with a passive field stub.
- `light-contrast-pixels.json`: fixed sphere region `(400,120)-(1180,770)` has
  22,261 pixels with luma below 180 after the fix, versus 1,516 in the failing
  capture. Near-white pixels (all channels above 250) fell from 32,902 to zero.
  This is a capture regression check, not a perceptual quality benchmark.

The native render command used the existing executable, `--observatory --theme
light --scene orb --preset neural --lens folders --control-port 0 --screen 0`,
the existing read-only source folder
`D:/CodexAudits/ArchieOS-Nvidia-20261026/repo`, and `--render-check` to the capture
path. `ARCHIE_RENDER_CHECK_MS=6000`, `QT_QPA_PLATFORM=windows`,
`QT_FORCE_STDERR_LOGGING=1`, and `QT_DISABLE_SHADER_DISK_CACHE=1` were set for
verification. The dark capture used the same command with `--theme dark`.

Targeted checks also passed:

```powershell
node display/native/tests/test_neural_camera.js # 10 contracts
node display/native/tests/test_exploration.js
# With the existing Qt environment; outputs directed to D:
$env:QT_QPA_PLATFORM='offscreen'
$env:QT_QUICK_BACKEND='software'
$env:QML_XHR_ALLOW_FILE_READ='1'
qmltestrunner -input display/native/tests/qml
git diff --check
```

Standalone QML checks emitted only the expected missing packaged-font warnings.
The native captures emit an initial hidden Observatory View3D camera warning and
a Qt vsync scheduling notice. These captures establish native rendering on this
adapter for these two views; they do not verify model execution, TPS accuracy,
latency, deployment, or other hardware.

## Observatory CLI theme recheck

A review raised a possible `--observatory --theme dark` regression because
`Main.qml` does not assign the singleton theme directly. Inspection found the
assignment already in `ObservatoryContent.qml`'s completion handler:

```qml
if (!root.worldHost && typeof shellColorMode !== "undefined")
    Shell.Theme.colorMode = shellColorMode
```

`Main.qml` creates that content with `worldHost=false`. No implementation change
was needed. Both original final captures above were Observatory captures, not
shell captures. After rebuilding the shared executable with the final telemetry
mapper, both CLI modes were captured again with these exact arguments:

```powershell
& 'D:\ArchieOS-Orbe-Review-20261026\native\archie-display.exe' --observatory --theme light --scene orb --preset neural --lens folders --content-root D:/CodexAudits/ArchieOS-Nvidia-20261026/repo --control-port 0 --screen 0 --render-check 'D:\ArchieOS-Light-IRIS-Cinema\orbe-cli-light-confirmation.png'
& 'D:\ArchieOS-Orbe-Review-20261026\native\archie-display.exe' --observatory --theme dark --scene orb --preset neural --lens folders --content-root D:/CodexAudits/ArchieOS-Nvidia-20261026/repo --control-port 0 --screen 0 --render-check 'D:\ArchieOS-Light-IRIS-Cinema\orbe-cli-dark-confirmation.png'
```

Both processes exited zero. Both captures were opened and inspected: the first
has pale surfaces and dark ink; the second has dark surfaces and gold/cyan
geometry. At the same background coordinate `(40,200)`, RGB is `(238,243,248)`
for light and `(0,2,6)` for dark. These are native captures from the same executable
SHA-256 `E290E7CC397553AC6939BD88B548B5BB4D9491FE48A07D90D56CE8C5355026F1`.

`D:\ArchieOS-Light-IRIS-Cinema\verify-observatory-theme-cli.ps1` preserves the
complete environment/launch procedure. `observatory-cli-themes.json` preserves
the exact argument arrays, executable/hash, process exit codes and run times;
the matching `orbe-cli-*-confirmation.log` files identify the NVIDIA adapter.
The final rebuild log is `native-cli-final-build.log`.
