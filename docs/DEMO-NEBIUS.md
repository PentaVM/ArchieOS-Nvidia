# Demo para Nebius: grabar "Archie, draw the Mona Lisa"

Un solo escritorio: el shell de ArchieOS (orbe, barra, dock con Vault, Forge, Sentry,
World, Stage y Ajustes) es el fondo del Archie Stage, y lo que Archie abre —Photopea en
Firefox, World— aparece enmarcado encima, con el mismo estilo de ventana. Archie escucha;
cuando se le pide un cuadro abre Photopea, toma el pincel y lo dibuja con su propio puntero,
a través de la puerta del Stage, mientras cuenta en voz alta (y en subtítulos) qué hace y
por qué.

Diseño: Xavi (`ByXavi270` / `XaviCottier`). Implementación nativa: PentaVM.

## En la máquina del dueño (Windows + WSL), sin pensar

1. `Desktop\ArchieOS\preparar-demo-nebius.bat`: exporta el commit de este worktree y compila
   todo en la WSL (`~/archieos-nebius`, log `/var/tmp/archie-logs/nebius-build.log`).
2. Empezar a grabar (OBS, pantalla completa) y `Desktop\ArchieOS\grabar-demo-nebius.bat [segundos]`.
   Arranca Core si no está (la consola de World le pregunta) y este mismo lanzador.
3. Guion toma por toma, OBS y evidencia: `Desktop\Video-pitch\GRABACION-REAL\GUION-DE-GRABACION.md`.

Al abrir, el shell muestra la animación de arranque de 3 s (paquete del logo, diseño de Xavi;
un clic o una tecla la saltan, `ARCHIE_BOOT_SPLASH=0` o `--no-boot-splash` la quitan). La barra
lleva el logo: tinta en tema Claro, neón en Oscuro. World es una vista del mismo escritorio con
un HUD mínimo: búsqueda (lugar o «lat, lon»), riel con acercar/alejar, 2D/3D, Tierra completa,
capas, regla, detalles y la consola de Archie (`world …`, `os …`, `iris …`, siempre por Core,
Policy y el Stage).

## Lo que hay que tener

En la distro WSL (o Linux) donde corre Archie:

| Pieza | Cómo |
| --- | --- |
| Stage | `cmake -S display/stage -B build/stage -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo && cmake --build build/stage` |
| Shell (`archie-display`) | `cmake -S display/native -B build/display -G Ninja && cmake --build build/display` |
| Voz con oídos y voz | `cd core && NEMO_SPEECH_PREFIX=/opt/archie/nemo-speech cargo build --release -p archie-voice --features speech` |
| CLI del Stage | `cd core && cargo build --release -p archie-stage` (`archie-stage-cli`) |
| World con globo en vivo (opcional) | `cd core && cargo build --release -p archie-cli -p archie-worldd -p archie-displayd` |
| Firefox | el de la distro (`FIREFOX_APP_ID=firefox-esr` si es el ESR de Debian) |
| Modelo que responde | `NEBIUS_TOKEN_FACTORY_KEY`, o `~/.config/archie/nebius.key` (lo deja `archie-setup`) |

En la imagen ArchieOS-Operator el lanzador toma solo el modelo ASR, las voces Piper y la
clave de donde `archie-talk` los toma.

## Grabar

```sh
display/session/nebius-demo.sh
```

Eso es todo: abre el Stage a pantalla completa con el shell como escritorio (limpio: orbe,
barra y dock), un Firefox con perfil nuevo (Photopea no reabre documentos viejos, no hay
páginas de bienvenida) y Archie escuchando en español e inglés. La política que usa es
`display/session/nebius-demo-apps.toml`: Firefox puede recibir trazos y capturas (es
Photopea); el escritorio solo puede recibir clics y desplazamientos. Enter sigue esperando
a una persona.

Ajustes útiles (variables de entorno):

- `ARCHIE_UI_LANG=en` — toda la GUI (escritorio, Observatory, Forge, Sentry, World y los
  avisos del Stage) en inglés; por defecto en español. Con `LISTEN_LANGUAGE=en` y
  `REHEARSE_LANGUAGE=en` la voz y la narración también van en inglés.
- `DRAW_SECONDS=90` — plan más corto (por defecto 150 s). El tiempo real depende del
  equipo: en el ensayo con render por software el plan de 120 s tardó 323 s.
- `TEMPO=person` (por defecto) — el puntero se mueve a ritmo de persona, que es lo que se ve
  bien en video. `STROKE_SPEED` acelera las líneas (más de ~600 px/s y Photopea las ve como
  polígonos).
- `HEADPHONES=1` — con audífonos se puede interrumpir a Archie mientras habla.
- `SCREEN=0` — qué pantalla toma el Stage.
- `WORLD_SOURCE=blue_marble` — imagen inicial de World (por defecto `google_satellite`).

## Guion sugerido

1. **Escritorio.** Se ve el orbe. "Archie, ¿qué puedes hacer?" — el contexto le dice qué
   puede abrir, qué cuadros sabe dibujar y cómo operar las apps del dock.
2. **El dibujo.** "Archie, draw the Mona Lisa" (o "Archie, dibuja la Mona Lisa"). Responde
   con una frase y empieza; desde ahí narra solo, en el idioma en que se le habló:
   abre Photopea → explica que Photopea no expone accesibilidad y que se orienta mirando la
   ventana → encuentra el papel → toma el pincel y lo afina → cuántas líneas y en qué tintas
   (sepia y sanguina, como Leonardo) → que cada tanda pasa por la puerta del Stage → un cuarto,
   la mitad (con píxeles de línea por segundo), tres cuartos, la segunda tinta → listo.
   La cámara del Stage se acerca al papel; abajo, el subtítulo con lo que dice y el caption
   "Archie · dibujando · Photopea". La esquina inferior derecha dice quién tiene el control.
3. **Tomar el control.** Mover el mouse sobre el Stage: "CONTROL HUMANO", Archie dice que
   espera; al soltar, "Sigo" y continúa la línea interrumpida, sin repetir.
4. **Parar** (opcional): "Archie, para de dibujar" — se detiene entre dos tandas.
5. **Internet.** "Archie, abre la página de Nebius" / "busca en internet el clima en
   Santiago": abre la dirección o la búsqueda (DuckDuckGo) en el mismo Firefox. Solo http(s);
   `file:`, `javascript:` y argumentos extra se rechazan en voz alta.
6. **Las apps del escritorio.** "Archie, abre Sentry" — Archie hace clic en el ícono del dock
   por su nombre (accesibilidad), las ventanas del Stage se apartan a la repisa (abajo a la
   izquierda) y Sentry queda al frente. Igual con Forge, Vault, World. Tocar la píldora de la
   repisa trae de vuelta Photopea.
7. **World.** Ícono World del dock: el globo nativo como una ventana más. Con los daemons de
   World compilados (`WORLD_BIN_DIR`), el lanzador abre una sesión de navegación acotada (1 h,
   renovada mientras el shell esté abierto) y el globo baja hasta ciudad y calle con
   fotogrametría.

Los cuadros que sabe dibujar: la Mona Lisa, La noche estrellada, La dama del armiño y El
aquelarre (imágenes de dominio público en `core/crates/archie-arena/tasks`).

## Si algo sale mal

- **Photopea abre con un dibujo viejo:** el lanzador borra el perfil en cada toma; si se
  lanzó Firefox a mano con el mismo perfil, cerrarlo. Logs en `/tmp/archie-demo/`.
- **"Firefox is already running":** quedó un Firefox de una toma anterior; el lanzador lo
  cierra al empezar (`pkill -f -- --profile /tmp/archie-demo/firefox`).
- **El dibujo no empieza:** `/tmp/archie-demo/apps.log` (Firefox) y la salida de la voz
  (línea `dibujo …`). Sin la entrada de Photopea en la política, Archie lo dice en voz alta.
- **No abre Sentry por nombre:** la accesibilidad necesita un bus de sesión; `run-archie.sh`
  usa `dbus-run-session` y lanza `at-spi-bus-launcher` si existe. Sin él, Archie recurre a
  mirar la ventana (grounding) si hay modelo de grounding configurado.
- **Subtítulos:** los escribe la voz en `/tmp/archie-demo/narration.json`; el Stage los lee
  con `--narration-file`.

## Qué está verificado y qué no

Verificado en un ensayo sin pantalla (Xvfb, render por software, Firefox real, photopea.com
real): el shell como escritorio dentro del Stage; Sentry, Forge y World abiertos desde el dock
por su nombre de accesibilidad (AT-SPI, sin modelo de visión) con clics sellados a través del
Stage (`archie-look --find Sentry --kind button`); la repisa (Sentry al frente, World a un
lado, World de vuelta al pedirlo otra vez); World con sus daemons en vivo dentro del
escritorio (`globe_demo.py --shell`: globo continuo listo, teselas NASA, señales USGS y
EONET); y el dibujo narrado completo en inglés (470 líneas) y en español (463 líneas), 0
rechazadas, con subtítulos y caption, lanzado con `archie-voice draw`. Las herramientas de
voz, la política de la demo y la narración dentro de la conversación tienen tests
(`cargo test -p archie-voice`).

### En la RTX 3070 del dueño, bajo WSLg (28-oct)

- **Ensayo sin voz** (`REHEARSE="the Mona Lisa"`): escritorio detrás, Photopea enmarcado, 221
  líneas en dos tintas, 19 comandos, 0 rechazados, 141 s, sin procesos huérfanos al cerrar.
  Encontró que el clic en el pincel caía en la herramienta Desenfocar (la columna de Photopea
  se dibuja a otra escala): ahora el pincel se toma con `b`. Y que una toma cortada dejaba los
  daemons de World en 17666/17667: ahora la limpieza ignora la segunda señal y el lanzador
  borra los restos.
- **Voz desde una grabación** (`WAV_IN`, Piper diciendo la frase): Nemotron en Nebius propone
  `draw_picture` y Archie responde con su voz. Encontró que Piper moría en la primera respuesta
  (el puente instalado arrancaba con el Python del sistema) y que el reconocedor deforma «Mona
  Lisa» («moaner lea», «owner leaser»): ambos arreglados. En español el reconocedor se comió
  «Archie»; para una persona sola en la sala, `ALWAYS=1`.
- **Apps del dock por turnos escritos** (`STAGE_ONLY=1` + `archie-voice do`): «open Sentry» y
  «open World» presionan el ícono del dock (encontrado por accesibilidad en 0,4–1,3 s, sin
  modelo) y «open the Nebius website» abre el sitio. Hizo falta `GROUND_MODEL` (sin modelo de
  grounding todo clic se rechazaba), que `open_app` presione el dock en vez de buscar un
  programa, y esperar 1,5 s el repintado del escritorio dibujado por software.
- La barra del shell dice «archied en vivo» con Core arriba (World's displayd lee su raíz).
- Un mouse que se mueve sobre el Stage toma el control (prioridad humana): durante una toma no
  se toca nada; con `--seats shared` Archie tiene puntero y teclado propios.

No verificado: la conversación con el micrófono y la voz del dueño (con `--wav-in` la voz lee
el archivo más rápido que en tiempo real y se cierra al poco de empezar el dibujo), y la
fotogrametría de Google a nivel de calle dentro del escritorio.
