# Trabajo autónomo — 2026-10-12 (Claude)

Nelson pidió trabajo autónomo durante el día: expandir el dominio (Sentry y lo que
alimenta a Cerebra) y llevarlo hacia producción. Una rutina despierta esta sesión
cada hora; cada despertar lee este archivo, toma la primera tarea no hecha y la
deja commiteada y empujada en `claude/sentry-ola3-cast-focos-servidores` (PR #36).

## Protocolo de cada despertar

1. `git fetch origin` y mirar el CI del PR #36. CI rojo es lo primero.
2. Tomar la primera tarea `[ ]` de la cola. Marcarla `[~]` mientras dura.
3. Antes de empujar: `cargo fmt --all -- --check`, `cargo clippy --workspace
   --all-targets --locked -- -D warnings` y las pruebas de los crates tocados.
4. Commit pequeño, push, anotar el resultado en `_dev/BITACORA.md` y aquí.
5. Límites que no se cruzan: GUI Quickshell/Vault y generación WSL (Claude/Xavi en
   otra línea), `archie-iris-runtime`, dominios personal/trabajo, `restricted` nunca a
   la nube. Nada se declara verificado en hardware si sólo pasó contra mocks.
6. Si el disco se llena: `cargo clean` en `core/` y repetir.

## Cola

- [x] **CI verde.** `cargo fmt` del workspace (sólo estilo), `#include <array>` en
  `sentry/src/cli/main.cpp`, clippy `-D warnings` con toolchain 1.88, `cargo test
  --workspace`, `ctest` de cortex/sentry/visual/osm.
- [x] **Sentry 11 — primera respuesta reversible.** Aviso por voz/Observatory de una
  alerta, sin bloquear nada. Bloqueo o cuarentena sólo como `Intent` aprobado.
- [x] **Sentry 14 — IPv6.** `address-moved` con Neighbor Discovery (NA/NS) además de ARP.
- [x] **Sentry 7 — baselines por dispositivo.** Mecanismo (volumen y destinos habituales,
  alerta por desviación) con prueba; la tasa de falsos positivos queda sin medir.
- [x] **Sentry 13 — unidad systemd.** `systemd-analyze verify` y endurecimiento de
  `archie-netwatch.service`; documentar lo que sólo puede comprobarse en Arch real.
- [x] **Endurecimiento general.** Revisar `unwrap`/`expect` en rutas de producción de
  `archie-sentry` y `archie-cerebra`, timeouts y límites de tamaño en E/S de red.

- [~] **CI en el último head**, con el paso nuevo de TShark e2e. Si falla, es lo primero.
- [x] **Sentry 7b — destinos inusuales.** Un dispositivo aprendido que manda mucho
  volumen a un destino que nunca usó (no cada destino nuevo: eso sería ruido).
- [x] **Sección 4 — productores de contexto.** Averiguar qué exponen Vault, IRIS y Forge
  hacia `PillarContext` y hacer el primero que no requiera la GUI ni IRIS runtime.
- [x] **Bloqueo de un dispositivo como `Intent`** (pedido por Nelson el 2026-10-15):
  `Command::DeviceBlock` por el router, `archie sentry-node block|unblock`. Falta
  probarlo en el MitraStar real.

- [ ] **Vigilancia permanente hasta 2026-10-13T10:00Z.** En cada despertar: CI del último
  head del PR #36 (rojo = arreglarlo con causa raíz), revisiones y comentarios
  (responder o corregir). No abrir funcionalidades nuevas sin Nelson: el PR ya es grande
  y lo pendiente son decisiones suyas (ver el relevo). Cuando llegue la hora, desactivar
  la rutina y dejar el resumen final.

## Hecho

- CI: fmt + clippy `-D warnings` del workspace con 1.88, `#include <array>`, la cgroup raíz
  (`0::/`) como `Unsupported` y el `Content-Length` de la prueba 429
  (`223081f`..`26b10f1`). Pendiente: confirmar en verde en GitHub.
- IPv6 en `address-moved` por Neighbor Advertisement (`e41b83d`).
- Avisos al dueño: `archie-notices`, follower, `sentry-notices`, `/api/notices`, lock
  entre escritores, `[sentry.response]`; `docs/SENTRY-RESPUESTA.md`.
- `traffic-surge` (`b1501fc`): baseline de volumen por dispositivo, persistida.
- Unidad de netwatch (`1154eca`): cuenta estática, `wireshark`, 0640; exposición 5.1→3.5.
- Follower (`0e05975`): sin `alerts.jsonl` (nodo nuevo o rotación) ya no muere.
- `archie-sentry-follow.service` (`01e61e3`): exposición 2.4; tablón 0640 para displayd.
- Auditoría de E/S de los adaptadores (`705a38f`): panic por stderr no ASCII, trabajo
  bloqueante fuera del runtime (el plazo del Intent no podía aplicarse) y tope de
  sesiones. Cada arreglo tiene una prueba que falla sin él.
- e2e con TShark 4.2.2 real (`5c9e47b`), también en el CI.
- CI verde en GitHub en `4625bb7` (rust + native-contracts). Descripción del PR al día.
- `unusual-destination` (`88670e7`).
- ONVIF (`8977076`): el nonce se repetía dentro del mismo milisegundo. La prueba de
  protocolo fallaba de forma intermitente en el CI, y una cámara real también lo habría
  rechazado.
- Productor de Vault (`vault_bridge`, `archie vault-docs`, feeder y perfiles). Humo:
  3 documentos → `focus trabajo` admite 3 (90/80/60); `personal` no ve ninguno.
- `sentry-notices --claim` (`f7f7572`) y el relevo `docs/HANDOFF-2026-10-12-SENTRY-OLA3.md`.
- Rutina verificada dos veces: disparo manual (10:38) y despertar programado (11:38).
