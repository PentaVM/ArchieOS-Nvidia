<div align="center">

[English](README.md) · **Español** · [Português](README.pt-BR.md) · [Français](README.fr.md) · [中文](README.zh-CN.md)

<br>

<img src="docs/readme/boot.gif" width="760" alt="La animación de arranque de ArchieOS: la marca de Archie se dibuja y se enciende">

<br>
<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-logo-dark.png">
  <img src="display/shell/brand/archieos-logo-light.png" width="360" alt="ArchieOS">
</picture>

### Una IA personal que vive dentro del sistema operativo — y pide permiso antes de tocar nada.

[![Licencia: Apache-2.0](https://img.shields.io/badge/licencia-Apache--2.0-20d79d?style=flat-square)](LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-45e3f5?style=flat-square)
![C++ · Qt 6](https://img.shields.io/badge/display-C%2B%2B%20%C2%B7%20Qt%206-9a75ff?style=flat-square)
![NVIDIA Nemotron](https://img.shields.io/badge/NVIDIA-Nemotron-76b900?style=flat-square)
![Nebius Token Factory](https://img.shields.io/badge/Nebius-Token%20Factory-ffca70?style=flat-square)

</div>

---

## ¿Qué es ArchieOS?

ArchieOS es un sistema operativo con un asistente, **Archie**, integrado. Le hablas con tus
propias palabras, en español o en inglés. Te responde en voz alta, abre tus aplicaciones y las
usa como lo haría una persona: mueve su propio puntero, hace clic, escribe y dibuja.

Lo que lo hace distinto es **quién manda**. El modelo de IA nunca toca tu computador
directamente. Solo puede *proponer* una acción. El sistema operativo decide si esa acción está
permitida, la deja ocurrir una sola vez, la vuelve a comprobar en el último momento y guarda un
recibo. En cuanto mueves el mouse, el control vuelve a ser tuyo.

<div align="center">
<img src="docs/readme/voice.svg" width="900" alt="Dices: Archie, dibuja la Mona Lisa. Nemotron propone draw_picture. La política lo permite. Archie dibuja en Photopea.">
</div>

## Míralo

**«Archie, dibuja la Mona Lisa.»** Archie abre Photopea en el navegador, toma el pincel y dibuja
el cuadro línea a línea con su propio puntero — primero sepia, luego sanguina, como trabajaba
Leonardo — y va contando lo que hace.

<div align="center">
<img src="docs/readme/mona-lisa-timelapse.gif" width="760" alt="Time-lapse: Archie dibuja la Mona Lisa en Photopea, trazo a trazo">
</div>

**El escritorio.** Una sola ventana para todo: el orbe que muestra lo que está haciendo la
máquina, el dock (Vault, Forge, Sentry, World, Stage, Ajustes) y lo que Archie abra encima. Sigue
tu tema claro u oscuro.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/desktop-dark.jpg">
  <img src="docs/readme/desktop-light.jpg" width="760" alt="El escritorio de ArchieOS: el orbe, la barra superior y el dock">
</picture>
</div>

**World.** La Tierra iluminada por el sol real, bajo el cielo nocturno real, con una búsqueda
discreta (un lugar o «lat, lon»), capas, una regla y la consola de Archie.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/readme/world-dark.jpg">
  <img src="docs/readme/world-light.jpg" width="760" alt="World: el globo, iluminado por el sol, con las constelaciones detrás">
</picture>
</div>

## Cómo te mantiene al mando

<div align="center">
<img src="docs/readme/authority.svg" width="900" alt="El modelo propone, Cerebra decide, la política concede un permiso para una sola operación, el Stage lo vuelve a comprobar y actúa, y vuelve un recibo.">
</div>

1. **El modelo propone.** Nemotron (en Nebius) o un modelo local convierte lo que dijiste en una
   acción sugerida, como `draw_picture("Mona Lisa")` u `open_app("Sentry")`.
2. **Cerebra decide.** La capa de razonamiento propia de ArchieOS comprueba que la petición
   tenga sentido — o te pregunta antes.
3. **La política autoriza.** Un archivo de texto simple (`apps.toml`) dice qué aplicaciones
   puede abrir Archie y qué puede hacer en cada una. Lo permitido recibe un permiso sellado que
   sirve para una sola operación; todo lo demás se rechaza.
4. **El Stage actúa.** El Stage es el sistema de ventanas donde trabaja Archie. Revisa una vez
   más el permiso y la ventana de destino, hace exactamente esa única cosa y devuelve un recibo.
5. **Tú ganas, siempre.** Toca el mouse o el teclado y la pantalla dice **CONTROL HUMANO**;
   Archie se detiene y espera. Tras unos segundos de calma puede continuar.

## Lo que puedes hacer hoy

| | |
| --- | --- |
| 🎙️ **Hablar** | Voz que entra, voz que sale, en tu máquina. Reconocimiento de voz en streaming NVIDIA Nemotron 3.5, Nemotron en Nebius para la respuesta y una voz local (Piper en CPU, Magpie en GPU). Archie responde en el idioma en que le hablaste. |
| 🖱️ **Actuar** | Abrir apps, hacer clic, escribir, desplazarse, arrastrar y dibujar — siempre a través del Stage y su política. Archie encuentra los botones primero por accesibilidad y solo mira la pantalla cuando no hay otra forma. |
| 🎨 **Dibujar** | «Dibuja la Mona Lisa» (y otros cuadros que conoce) en Photopea, a un ritmo humano que se puede seguir en video. |
| 🌍 **Explorar** | World: el globo, la luz del sol real, las estrellas reales, lugares por nombre o coordenadas. |
| 🛡️ **Vigilar** | Sentry: qué hay en tu red y quién habla con quién. |
| 👁️ **Ver** | IRIS, los ojos de Archie: le dices qué buscar y te dice dónde está. Las caras siempre se tapan — nunca identifica personas. |
| 🎬 **Editar video** | Líneas de tiempo con clips, títulos, música y fundidos renderizadas con MLT (el motor de Kdenlive), sin ventana. |
| 🧩 **Conectar** | Las herramientas de Archie para agentes de programación vía MCP. |

## Pruébalo

ArchieOS funciona como un sistema Linux: en un PC, en una máquina virtual o en **WSL2 sobre
Windows**.

**En Windows (WSL2).** Construye la imagen e instálala:

```bash
sudo ./scripts/wsl/build-rootfs.sh ./dist
```

```powershell
wsl --install --from-file .\dist\ArchieOS.wsl
```

Luego sigue el [primer arranque](docs/FIRST-RUN.md). La [guía de construcción WSL](docs/WSL-BUILD.md)
y el [límite de seguridad](docs/SECURITY-BOUNDARY.md) explican qué contiene la imagen y qué
mantiene separado.

**La demo de la Mona Lisa.** Dentro de ArchieOS (o de cualquier Linux con las piezas
compiladas), un solo comando abre el escritorio, el navegador y a Archie escuchando en español e
inglés:

```bash
display/session/nebius-demo.sh
```

Qué compilar antes, las opciones y un guion sugerido están en
[`docs/DEMO-NEBIUS.md`](docs/DEMO-NEBIUS.md).

<details>
<summary><b>Compilar desde el código</b></summary>

<br>

El plano de control es Rust (1.88+); el escritorio y el Stage son C++ con Qt 6.

```bash
# La voz, las manos y las herramientas de Archie
cd core
cargo build --release -p archie-voice --features speech
cargo test --workspace --locked

# El Stage (compositor Wayland) y el escritorio
cmake -S display/stage  -B build/stage   -G Ninja && cmake --build build/stage
cmake -S display/native -B build/display -G Ninja && cmake --build build/display
```

`--features speech` necesita NeMo-Speech.cpp (`NEMO_SPEECH_PREFIX`) y PulseAudio;
`scripts/models/install-nemotron-speech.sh` descarga el modelo de reconocimiento fijado, y
`scripts/models/install-piper-runtime.sh` / `install-piper-voices.sh` las voces locales.

Habla con Archie y el Stage usando una clave de Nebius Token Factory:

```bash
NEBIUS_TOKEN_FACTORY_KEY=... display/stage/run-archie.sh public
```

El modelo que responde se puede cambiar: NVIDIA NIM, Nebius Token Factory o cualquier servidor
local compatible con OpenAI. Las claves se leen al momento de cada petición y nunca se escriben
en el repositorio ni en la imagen. `scripts/check.sh` corre las comprobaciones que vale la pena
hacer antes de un push.

</details>

## Por dentro

```text
 micrófono ─► Nemotron 3.5 ASR ─► turno ─► Nemotron (Nebius) ─► voz (Piper / Magpie)
                                              │ propone
 cámara ─► IRIS ─► escena + memoria ──────────┤
                                              ▼
                                Cerebra ─► Política ─► Stage ─► aplicaciones
                                              │
                        señales TFOG ─► archie-displayd ─► escritorio (Orbe · Forge · World)
```

- **Rust** para la autoridad, la política, el planificador, el enrutador de modelos, la memoria y
  el estado (PostgreSQL/TimescaleDB), Cerebra y el Vault.
- **C++ / Qt Quick 3D** para el escritorio, el Orbe, World, Cortex, Sentry y el Stage.
- La identidad, la memoria, la política y la autoridad viven en ArchieOS; los modelos de lenguaje
  y de visión son recursos reemplazables a los que llama.

## Estado

Es una alfa, hecha para el hackathon NVIDIA × Nebius. Con honestidad:

- **Funciona y está probado:** voz a voz en español e inglés, el Stage y su política, abrir y
  usar apps por voz, el dibujo de la Mona Lisa, World, el escritorio claro y oscuro, la detección
  de IRIS, el render de video, la imagen WSL2.
- **En progreso:** el entrenamiento de IRIS Beta 2; Cortex moviendo la vista del escritorio por
  comando; IRIS proponiendo acciones más allá de los clics; la compilación CUDA del runtime de
  IRIS sin Python.
- **Límites conocidos:** el dibujo va a ritmo de persona (minutos, no segundos); en la imagen
  ArchISO las apps se abren a través del Stage y no desde Forge; la CI corre en local mientras
  GitHub Actions está en pausa.

## Para saber más

[Arquitectura](ARCHITECTURE.md) · [Archie](ARCHIE.md) · [Procedencia del hackathon](HACKATHON.md) ·
[Stage](docs/STAGE.md) · [Voz](docs/SPEECH-NEMOTRON.md) · [IRIS](docs/IRIS.md) ·
[Runtime de IRIS](docs/IRIS-RUNTIME.md) · [Seguridad](docs/SECURITY.md) ·
[World](docs/WORLD-EXPLORATION.md) · [Render del orbe](docs/TFOG-ORB-RENDERING.md)

## Créditos

- **PentaVM** — la arquitectura, la implementación en Rust y C++, el plano de control, IRIS, la
  voz y el escritorio nativo.
- **Xavi** ([`ByXavi270`](https://github.com/ByXavi270) / XaviCottier) — el diseño: el
  escritorio, la esfera neuronal, el logo y su animación de arranque.
- **Santiago Rearte** (Sheriff) — la integración con Nebius Token Factory y Nemotron, y las
  pruebas en su propia WSL.
- Construido sobre NVIDIA Nemotron, NeMo y NIM, Nebius Token Factory, Piper, MLT, Qt y Wayland.

## Licencia

Apache License 2.0 — ver [LICENSE](LICENSE) y [NOTICE](NOTICE). Apache y no MIT por su
concesión de patentes: este repositorio contiene control de dispositivos, actuación física y
enrutamiento de inferencia. Los proveedores de datos mantienen sus propios términos; `providers/`
registra la atribución que exige cada uno, y ArchieOS se niega a mostrar datos sin su atribución.

<div align="center">

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="display/shell/brand/archieos-icon-256.png">
  <img src="display/shell/brand/archieos-icon-light-256.png" width="56" alt="">
</picture>

**Made with love ♥**

*por PentaVM, Xavi y Sheriff*

</div>
