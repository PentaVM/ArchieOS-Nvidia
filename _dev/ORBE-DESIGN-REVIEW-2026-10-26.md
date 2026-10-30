# Orbe de Xavi + escritorio World + Forge

Remoto comprobado: `https://github.com/PentaVM/ArchieOS-Nvidia.git`.
Rama local de revisión: `codex/orbe-design-review`. Sin push ni merge remoto.

## Integración

- Base del World existente: `36b2588eb2a0afdb48af52148e9fa0642a2cee90`.
- PR de diseño #53: https://github.com/PentaVM/ArchieOS-Nvidia/pull/53
- Fuente: `design/lumenfield-0449-orbe`, `3cc8d37d47dd9c1547caaff1faeec5b97d8bb375`.
- Los seis commits del PR se incorporan completos mediante merge; se conserva su autoría. Diseño: **Xavi**. Implementación nativa Rust/C++: **PentaVM**.
- Se conserva el motor World actual y la ruta TFOG del Orbe. El preset Holograma incorpora la esfera neuronal de archivos; los perfiles clásicos siguen disponibles.

## Cambios de integración

- World comparte el escritorio y su dock completo de siete iconos; deja de crear una segunda ventana con tres pestañas superiores. Orbe, World y Forge comparten navegación.
- `ObservatoryContent.qml` comparte la escena real. `--observatory` conserva el host independiente para QA; el inicio normal usa el shell.
- Forge proyecta `forgeApps`, excluye solo manifiestos `X-Archie-Internal=true` y conserva aplicaciones, CLI e IDEs. El catálogo global mantiene Stage y sus controles de lanzamiento/autoridad.
- Forge muestra aplicaciones y sugerencias; trabajos/manifiestos internos dejan de aparecer en su superficie. Sus proyecciones de backend se conservan para diagnóstico.
- Picking neural corrige el contrato FFI de dos llamadas. Los comandos de vista devuelven el resultado efectivo; un nodo ausente o una vista no disponible ya no reciben un ACK falso.
- La esfera aplica el presupuesto de nodos, partículas, intervalos, visibilidad, posprocesado y movimiento reducido. Esto limita presentación; no modifica el envío de paquetes ni la política del daemon por uso de otras aplicaciones.
- Brillo, giro, densidad, actividad y polvo llegan al nuevo renderer. Actividad apagada prevalece sobre el techo de partículas. Links, shell y bloom se conectan; guides se rechaza explícitamente para neural. La paleta fija de categorías se identifica en Ajustes.
- La instantánea publica raíz aceptada, hora, límites, omisiones y actualización explícita. Una raíz ilegible limpia la anterior; la vista inactiva difiere escaneos y las etiquetas siguen la cámara aun con animación congelada.
- Recursos PNG grandes usan BIG_RESOURCES para evitar el pico de memoria del C++ generado. Las pruebas Windows usan fixtures ejecutables y ACL privadas reales; no se relajan credenciales de producción.

## Verificación ejecutada

Entorno: Windows, Qt 6.8.3/MinGW 13.1, Rust GNU 1.99.1. Herramientas y salidas en `D:/ArchieOS-Orbe-Review-20261026`; las cachés nuevas se movieron de forma recuperable a D por falta de espacio en C.

| Verificación | Resultado |
|---|---|
| Compilación nativa Release con CMake | Exit 0 |
| `cargo test --manifest-path core/Cargo.toml -p archie-lumen-design --locked` | 40 unitarias + 1 contrato FFI; 0 fallos |
| `cargo test --manifest-path core/Cargo.toml -p archie-orb --locked` | 19 pruebas; 0 fallos |
| Qt Quick Test, directorio completo `display/shell/prototype` | 101 pruebas, 0 fallos, 0 omitidas |
| CTest nativo + visual + OSM | 34 suites, 0 fallos |
| Host Windows real, TCP de controles | focus existente OK; inexistente ERROR; camera OK; freeze OK |
| Recorrido real Orbe → World → dock Forge → Orbe | Exit 0; clic efectivo, 0 sin efecto |
| Revisión independiente de integración y controles | Sin hallazgos concretos P1/P2 pendientes |

Los golden de Rust comprueban la geometría/cinemática portada. Las pruebas de catálogo usan manifiestos reales y temporales; las de streaming usan un servidor TCP de fixture. El recorrido real dibuja la instantánea del directorio `display` y la base NASA, con estado de autorización visible.

No se verificó el arranque del OS completo, Core/voz/modelos/Stage ni descarga remota de tiles dentro de WSL: `ArchieOS-Operator-RTX3070-Test` no responde. No se hizo un shutdown global. No se atribuye precisión de modelos, latencia ni validación NVIDIA a estos fixtures. El host Windows muestra el bridge no disponible; World declara que falta una sesión de navegación autorizada. Las capturas de fuentes propietarias continúan bloqueadas.

Advertencias restantes: el linker GNU 13.1 informa directivas LLVM no reconocidas del Rust GNU reciente; el binario y pruebas ejecutan correctamente. Qt emite avisos transitorios de cámara durante la creación de vistas y adapta el tamaño de la ventana al monitor. No se presenta la compilación como libre de advertencias.

Evidencia: [logs, estados y capturas](verification/orbe-design-review-2026-10-26/).

## Repetir en este PC

```powershell
Set-Location 'C:\Users\Penta\.codex\worktrees\archieos-nvidia-latest\ArchieOS'
. 'D:\ArchieOS-Orbe-Review-20261026\env.ps1'
cmake --build 'D:\ArchieOS-Orbe-Review-20261026\native' --parallel 2
$env:QT_QPA_PLATFORM='windows'
ctest --test-dir 'D:\ArchieOS-Orbe-Review-20261026\native' --output-on-failure --parallel 2 --timeout 60
cargo test --manifest-path core/Cargo.toml -p archie-lumen-design -p archie-orb --locked
$env:QT_QPA_PLATFORM='offscreen'
$env:QT_QPA_FONTDIR='C:\Windows\Fonts'
$env:QML_XHR_ALLOW_FILE_READ='1'
& "$env:CMAKE_PREFIX_PATH\bin\qmltestrunner.exe" -input display/shell/prototype
```

## Review y subida a cargo del usuario

La base explícita revisa toda la integración sobre el World anterior, incluidos los commits del PR y las correcciones locales.

```powershell
Set-Location 'C:\Users\Penta\.codex\worktrees\archieos-nvidia-latest\ArchieOS'
codex review --base 36b2588eb2a0afdb48af52148e9fa0642a2cee90
# Después de leer la review:
git push -u origin codex/orbe-design-review
```

El informe de conteo LOC previo permanece sin incluirse en este commit. No se sobrescribieron worktrees ni archivos sin commit de otros autores.
