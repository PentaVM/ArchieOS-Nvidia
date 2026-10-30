# Orbe neuronal de Lumenfield en el host Qt (tanda 2026-10-24)

Continúa `_dev/LUMENFIELD-PORT-2026-10-22.md`. El diseño es de Xavi; la implementación, de PentaVM.

- Rama: `design/lumenfield-0449-orbe`, creada desde `claude/lumenfield-port`. Solo commits locales; sin push ni PR.
- Alcance: reemplazar el orbe provisional por la esfera neuronal de Lumenfield (módulo `neural-sphere`) y añadir los
  verbos nuevos del display. No se tocó Core, Cortex, IRIS, Wanda, Sentry, Policy, voz, veil, Forge, World, shell ni
  instalador.

## Qué quedó

- Núcleo en Rust con C ABI `neural_*` (puntos, vértices y chispas con layout fijo de 10, 6 y 7 floats).
- Capa Qt/Quick3D: `NeuralField` y `NeuralGeometry` (`QQuick3DGeometry`), nueve shaders `neural_*`,
  `NeuralScene/NeuralViewport/NeuralHud/NeuralHolo.qml`, bloom aproximado con `ExtendedSceneEnvironment` y pase de
  holograma por `ShaderEffect`. Integración por `fieldSource: "neural"`; HUD con Inter.
- Preset `holograma` ahora apunta a la esfera neuronal; `holograma-calido` eliminado (ya no hay variante cálida).
- Verbos nuevos en `DisplayControl.cpp` y en `archie visual`: `focus`, `camera`, `window`, `pointer`, `highlight`,
  `popup`, `freeze` y `capture`. Se validan en el display y en el cliente Rust (`archie-orb/src/control.rs`). En modo
  shell solo `capture` funciona; los demás responden `error: <verbo> is available in Observatory, not in the shell`.
- `capture` escribe en `$XDG_RUNTIME_DIR/archie-display` (o el directorio temporal) con nombre fijo
  `capture-yyyyMMdd-HHmmss-zzz.png`; el cliente no elige la ruta.
- README de `display/native` actualizado.

## Qué se midió (WSL `archlinux`, Qt offscreen, llvmpipe, `--parallel 2`)

- Compilación nativa: código de salida 0.
- CTest: 5/5 en 17,59 s (`native`, `shell_runtime`, `display_control`, `neural_sphere`, `cpu_shader`), incluidas las
  pruebas nuevas de verbos y de contratos neuronales.
- Render offscreen de la esfera: 13–17 fps en llvmpipe.
- `cargo fmt --all -- --check`: limpio.
- `cargo test -p archie-orb -p archie-cli -p archie-lumen-design --locked`: 28 + 1 + 4 + 40 + 19 pruebas pasan, 0 fallos.
- `cargo clippy` de mis crates con `-D warnings`: ver «No se pudo verificar» (hay avisos previos ajenos).

## Qué no se pudo verificar

- SSIM ≥ 0,98 por preset: no medido. No hay captura de referencia de Lumenfield (`?still`) en este árbol.
- Todo lo medido es llvmpipe; nada está verificado en GPU real.
- Bloom y tonemap son aproximaciones de Quick3D, no los de Three.js; no hay amortiguación de OrbitControls.
- Clippy con toolchain 1.99 (CI usa 1.88) falla en código que esta tanda no toca: `archie-orb/src/model.rs:1385`
  (`needless_range_loop`), `archie-sentry/src/contracts.rs:239`, `archie-sentry/src/adapters/onvif.rs:226` y
  `archie-vault/src/veil.rs:142` (`manual_is_multiple_of`). No se corrigieron por quedar fuera del alcance; conviene
  comprobarlo con 1.88.
- `focus` sin id suelta la selección con `select(".")`; no se contrastó con el comportamiento de Lumenfield.
- Aviso de QML en `Main.qml:495` sin tratar.

## Qué decide Xavi o Penta

- Xavi: el orbe `sphere-layer.ts` (0449) no se portó en esta tanda; falta decidir si va ahora y aportar la captura de
  referencia (`?still`) y el arte 4K. Los mapas 4K no aplican al orbe neuronal, por eso no hay generador con sha256.
- Penta: `Demo.qml` todavía lista `holograma-calido`. El panel de ajustes del conmutador de conexión no se empezó.
- Push y PR borrador hacia `claude/unified-0.1`: pendientes de que el usuario lo autorice.
