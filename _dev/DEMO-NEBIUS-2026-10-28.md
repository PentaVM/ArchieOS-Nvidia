# Demo Nebius: escritorio único y dibujo narrado por voz (2026-10-28)

Rama `claude/laughing-lamport-5af9cs`: main 994740a + #52 (World, 36b2588) + #53 (esfera
neuronal, 3cc8d37) integrados, más:

- `archie-paint`: `session.rs` (Seat/perform: mirar, actuar, esperar a la persona, continuar
  la línea interrumpida) y `photopea.rs` (URL con documento y dos tintas, página por su gris
  71, pincel en la columna de herramientas, `[` para afinar, papel en blanco = sin pincel).
  `Picture::matted` quita el 4 % de cada borde (el viñeteado del cuadro dibujaba una franja de
  trazos rectos arriba).
- `archie-voice`: `draw_picture`, `stop_drawing`, `open_website` decididos en `act.rs`; un
  `Desk` compartido por `converse` y `live` (manos del Stage + dibujo en segundo plano en su
  propia conexión); narración en cola dicha en silencio antes de los avisos de Sentry y
  escrita como subtítulo.
- Stage: escritorio (`--desktop-app archie-display --desktop-title ArchieOS`), marcos con el
  estilo del shell, repisa, píldora de autoridad, caption con nombre de app, verbo "drawing",
  subtítulos (`--narration-file`).
- Shell: World en el dock (`DisplayControl::showScene`), ventana World sin pestañas Orb/Forge,
  accesibilidad en dock/barra/luces/filas/apps de Forge, arranque limpio dentro del Stage
  (`ARCHIE_STAGE_DESKTOP=1`).
- `display/session/nebius-demo.sh` + `nebius-demo-apps.toml`; `run-archie.sh` con `DESKTOP`,
  `WORLD_BIN_DIR`, `TEMPO`, `STROKE_SPEED`, `DRAW_SECONDS`, `PICTURES`. Runbook:
  `docs/DEMO-NEBIUS.md`.

## Ensayo (Xvfb 1920×1080, Qt 6.8.3, Mesa llvmpipe, Firefox 144 real, photopea.com real)

- `archie-voice draw "the Mona Lisa" --language en`: 470 líneas, 42 comandos, 0 rechazados,
  322,8 s (plan 120 s, tempo persona, render por software), 188 px de línea/s. Narración en
  cada hito y subtítulo visible bajo el papel.
- Repetido en español (`--language es`) con el shell como escritorio.
- Clics sellados de `archie-stage-cli` sobre el dock: World se abre enmarcado; Sentry al frente
  y World a la repisa.
- Con un bus de sesión y `at-spi2-core`: `archie-look --find Sentry|Forge|World --kind button`
  encuentra cada ícono por su nombre declarado (sin modelo) y lo presiona a través del Stage
  (`clicked_and_repainted`, 0,6–2,3 s).
- `globe_demo.py --shell` dentro del Stage (worldd, displayd y `archie world navigate` reales):
  globo continuo `ready`, 20–32 teselas NASA, USGS ~196 y EONET 50 señales, HTTP 35–48/1800.
  World pedía maximizado y la integración de QtWayland le respondía con la salida entera
  (1920×1080, recortado por el marco); el Stage ahora reenvía el tamaño del marco (1838×876).
- Corrida en español con el escritorio: primero 29 tandas rechazadas (la repisa reaccionaba a
  los trazos y movía la ventana); corregido, 463 líneas, 0 rechazadas.
- Tests: `cargo test -p archie-voice` y `-p archie-paint` verdes; clippy 1.88 `-D warnings`
  con y sin `live,speech,rvc`; ctest nativo 12/13 (falla `cpu_shader_contracts`, opcional y
  previo: `LumenScene` no encuentra `Exploration.js` en los recursos del test).

## No verificado

Micrófono/parlantes reales con NeMo-Speech, elección de herramientas por el modelo de Nebius,
clic por nombre vía AT-SPI (sin bus de accesibilidad en el ensayo), World con daemons en vivo
dentro del escritorio, rendimiento en RTX 3070/WSLg.

## GUI final de Codex (merge `65db85b` y siguientes)

- La GUI clara de Codex (Orb/Forge/World, "Claro", "Detalles", "Sin telemetría") ya está en la
  rama. El Observatory pasó de `Main.qml` a `ObservatoryContent.qml` y su tarjeta MACHINE quedó
  fuera; los tests de empaquetado leían solo `Main.qml` y CI `rust` se cayó en `cargo fmt`.
- Arreglado sin cambiar la vista por defecto: el estado de la máquina (presión o "sin medir",
  tier de render con su razón, zonas térmicas con su propia escala) vive dentro de "Detalles":
  en la esfera neural, en el popup que ya existía; en las otras esferas, con el mismo botón en
  el mismo lugar. Los tests leen `Main.qml` + `ObservatoryContent.qml`; ninguna aserción cambió.
- Visto en el Stage (Xvfb, render por software): `archie-display --observatory`, clic sellado
  en "Detalles", MÁQUINA bajo "Instantánea de archivos" (neural) y en su propio popup (tfog).
- `cargo test --workspace` 1.88: todo verde salvo `archie-lan autonomy_cli`, que falla solo en
  este contenedor (ruta 192.0.2.2) y pasa en CI. ctest nativo 18/20: `cpu_shader_contracts`
  (previo, `Exploration.js` fuera de los recursos del test) y `world_texture_contracts` (sin
  RHI en el ensayo); CI no corre el ctest nativo.

## El lanzador en todos lados, y el icono (30 oct)

Fallos encontrados probando `display/session/nebius-demo.sh` fuera del ensayo de siempre:

- **Escritorio Wayland normal (GNOME, KDE, Sway): el Stage no abría.** `WAYLAND_DISPLAY=wayland-0`
  es relativo al `XDG_RUNTIME_DIR` de la persona, y `run-archie.sh` le cambia ese directorio al
  Stage ("Could not load the Qt platform plugin wayland"). Solo funcionaba en WSLg porque el
  lanzador pasaba la ruta absoluta. Ahora se hace absoluta antes.
- **Escritorio X11 o sin pantalla: rechazado** ("set HOST_WAYLAND"). Ahora el Stage abre como
  ventana X11 (`xcb`), y sin pantalla en un Xvfb propio (`DISPLAY=:90…`, grabable con x11grab).
- **WSL con systemd:** `/run/user/UID` no tiene el socket de WSLg; se busca en `/mnt/wslg`.
- **Firefox:** fuera del PATH o como `firefox-esr` el lanzador fallaba; el snap de Ubuntu no
  puede ver el Stage (sandbox). Ahora busca uno que sirva, salta el snap y lo dice. El app id
  sale del `RemotingName` de la build (el ESR de Mozilla en tarball también es `firefox-esr`;
  con el app id equivocado, "Photopea's window never came up").
- **Firefox < 121 bajo X11** abría en la pantalla del host y no en el Stage: `draw.rs` ahora
  le pone `MOZ_ENABLE_WAYLAND`/`GDK_BACKEND` como ya hacía `hands.rs`.
- `at-spi-bus-launcher` se buscaba solo en `/usr/lib` (Arch); Debian/Ubuntu/Fedora usan `/usr/libexec`.
- El Stage tenía 10 s para arrancar; con render por software tarda ~15 s. Ahora 60 s, y si muere
  se dice al momento con las últimas líneas de su log. Ctrl+C cierra todo lo que se abrió.
- **Binario viejo sin aviso:** la captura del dueño mostraba la barra anterior a #54 (círculo con
  "A" + "Archie"). Si el binario es más viejo que el checkout y CMake lo hizo de este checkout,
  el lanzador lo recompila (`NO_REBUILD=1` lo evita); si no, lo avisa.

Icono: `archieos-app-512.png` (icono de ventana del shell y del Stage) y `archieos.ico` tenían
fondo negro (10,10,10); ahora son transparentes (el trazo menta opaco, el brillo translúcido).
La imagen WSL pone el `.ico` como icono de la distro (`[shortcut] icon=`), instala los iconos
hicolor y entradas de escritorio para los app id `archie-display` y `org.archieos.Stage`
(ocultas del menú), y `archie-offline.desktop` declara `StartupWMClass=archie-display`. El
lanzador instala lo mismo en `~/.local/share` para quien corre desde un checkout.

Probado en este contenedor (Ubuntu 24.04, Xvfb, Mesa por software), con Ctrl+C real y sin
procesos sobrantes en cada caso: Stage + escritorio sobre un Wayland anfitrión con
`WAYLAND_DISPLAY` relativo; sobre X11; sin pantalla; WSLg simulado (socket en
`/mnt/wslg/runtime-dir`, con y sin `XDG_RUNTIME_DIR`). Ensayo completo sin pantalla: la Mona Lisa
en Photopea, 463 líneas, narrada. Detección de Firefox con un wrapper de snap y un `firefox-esr`
envoltorio. Recompilación automática: la primera vez recompiló shell y Stage, la segunda no.
No probado aquí: WSLg real (si su barra de tareas toma el icono de entradas `NoDisplay`), GNOME
o KDE reales, Firefox snap real, Firefox < 121.
