# Pendientes — ArchieOS

Estado al 2026-10-11. `main` es la única rama; todo lo hecho hasta hoy está fusionado.
Esta lista es lo que falta para llamar terminado a cada parte. Cada punto dice quién lo
tiene y dónde está el detalle. Se marca aquí cuando se cierra.

## 1. Release y WSL

- [ ] **Imagen WSL desde `main`.** La pre-release `wsl-operator-2026.10.11` salió de
  `b22625e`: no tiene la voz (`e5cf04a`), ni Kdenlive, ni el bridge de video. El
  `build-operator.sh` de `main` ya los pide; falta construirla. Bloqueo: el portátil
  constructor tiene 2 GB libres y hacen falta unos 8. — Claude
- [ ] **Probar IRIS desde la imagen en una GPU.** Nunca se ejecutó: el constructor no
  tiene GPU y con 7 GB de RAM el modelo no carga ni en CPU. Primer dato real: la GTX 1660
  del amigo de Nelson (float32). Anotar latencia por decisión y memoria. — Nelson
- [ ] **Probar `install-archie-operator.ps1` en una máquina limpia.** No se importó la
  imagen en ningún equipo. — Sheriff / Nelson
- [ ] **Kdenlive con Sheriff.** Dos caminos, ninguno probado en la imagen:
  bridge de video (`archie-bridges call`, MLT sin ventana) y `archie-operate --app kdenlive`
  (IRIS en el Stage). El E2E del bridge existe (`ARCHIE_BRIDGE_E2E=video`). — Sheriff
- [ ] **ISO.** `distro/archiso/build.sh` incluye `archie-lan`, `archie-netwatch`,
  `archie-rem`, `archie-print` y `archie-ask`; no se ha construido ni arrancado desde
  esos cambios. CI sigue apagado por facturación. — sin dueño
- [ ] **Licencia de IRIS para distribuir.** Los pesos son de investigación no comercial;
  el release es privado. Decidir antes de cualquier publicación abierta. — Nelson

## 2. IRIS

- [ ] **Runtime sin Python.** `iris_serve` es PyTorch; la regla del proyecto es C++/Rust.
  Exportar a ONNX o escribir el runtime en Rust. Mientras tanto la imagen instala
  PyTorch en el primer arranque (3 GB) y baja SigLIP (1,5 GB). — Claude
- [ ] **Adaptadores LoRA por aplicación**, en los nodos de Sheriff y Xavi. — Nelson
- [ ] **REM no graba demostraciones de una persona**, solo corridas verificadas de Arena.
- [ ] **Medir IRIS en 6–8 GB.** El manifiesto dice 56 ms en una RTX 5090 que además
  entrenaba; no hay medida en una GPU propia.
- [x] `PentaVM/archie-iris#27` fusionado (float32 en GPU sin bf16 nativo, `einops`).

## 3. Sentry — Kimi K3 trabaja aquí

**Lo último (2026-10-12):** [`HANDOFF-2026-10-12-SENTRY-OLA3.md`](HANDOFF-2026-10-12-SENTRY-OLA3.md),
con la lista de validación en hardware, en orden, y las decisiones pendientes de Nelson.

**Empieza por leer**, en este orden: [`HANDOFF-2026-10-11-SENTRY-AUDITORIA.md`](HANDOFF-2026-10-11-SENTRY-AUDITORIA.md)
(qué se probó, qué no, doce defectos conocidos), [`SENTRY-NETWATCH.md`](SENTRY-NETWATCH.md),
[`SENTRY-LAN.md`](SENTRY-LAN.md) y `core/crates/archie-sentry/README.md`.

**Dónde está el código:**

| Pieza | Ruta | Estado |
|---|---|---|
| Inventario activo (SSDP, mDNS, Roku) | `core/crates/archie-lan/src/{ssdp,mdns,upnp,roku,inventory}.rs` | probado en una red real |
| Captura y auditoría continua | `core/crates/archie-lan/src/{capture,audit}.rs`, `bin/archie-netwatch.rs` | 3 de 6 reglas probadas en vivo |
| Importador pasivo de TShark | `core/crates/archie-core/src/passive_network.rs` | solo mDNS IPv4 desde archivo |
| Motor: intents, policy, journal, recibos | `core/crates/archie-sentry/src/{contracts,engine,journal}.rs` | probado con PostgreSQL aislado |
| Adaptadores | `core/crates/archie-sentry/src/adapters/{home_assistant,ipp}.rs`, `ipp/print.rs` | HA con fixture; IPP sin impresora real |
| Operador y Core | `core/crates/archie-core/src/{sentry,sentry_config,operator,operator_session}.rs` | sesión de operador solo en Linux |

**Reglas que no se rompen:** C++ y Rust, nada de Python. Sentry observa y propone; actuar
pasa por `Intent` → policy → aprobación del operador → postcondición medida. Nada de
identificar personas por su teléfono ni mandar datos de la red a la nube. Una prueba con
fixture se reporta como fixture, no como dispositivo real. Un PR por avance, contra `main`.

**En este orden** (cada punto dice cuándo está hecho):

- [x] **1. Una sola vista de dispositivos.** Unir `archie-lan` (inventario),
  `archie-netwatch` (`state.json`) y `passive_network` en un registro por dispositivo,
  con clave por dirección de hardware y la IP como atributo. *Hecho cuando:* un comando
  (`archie sentry-devices --json`) devuelve cada dispositivo una vez, con qué fuente dijo
  qué, y hay prueba con las tres fuentes describiendo el mismo equipo.
  **Hecho (Kimi, 2026-10-11):** `archie_core::unify` + `archie sentry-devices --json`;
  prueba con las tres fuentes describiendo el mismo equipo y atribución por fuente.
- [~] **2. Llevar esa vista al world graph** por `archie-worldd`. *Hecho cuando:* el
  Observatory dibuja los dispositivos de la red real desde el grafo y no desde un archivo.
  **Casi (Kimi, 2026-10-11):** la vista entra al grafo como entidades de medición con
  procedencia (RFC-008), el snapshot las sirve y la ruta de datos del display las consume
  desde el grafo (probado sobre socket loopback real). Falta el panel QML que las dibuje.
- [x] **3. Alertas → `PillarContext` → Cerebra.** Hoy nada produce un `PillarContext`.
  Cada alerta de `archie-netwatch` entra como referencia del pilar Sentry. *Hecho
  cuando:* un escaneo de puertos generado a propósito termina en un pase de Cerebra con
  su traza, y la prueba lo recorre de punta a punta. Ver la sección 4: el pase se
  gradúa `restricted` y sin modelo local se rechaza.
  **Hecho (Kimi, 2026-10-11):** `archie_cerebra::sentry_bridge` — la alerta se valida,
  deduplica, guarda en Vault (Restricted, con su propio timestamp) y entra como referencia
  Sentry; el pase gradúa `restricted` y sin modelo local queda el rechazo `NoLocalModel`
  registrado en la traza. La prueba recorre el port-scan de punta a punta (fixture).
  **Llamador en vivo (2026-10-12):** `archie_cerebra::alert_follower` sigue
  `alerts.jsonl` (offset en journal atómico, rotación `alerts.1.jsonl` drenada, línea
  parcial retenida), recupera el conteo de `address-moved` desde `told.json` y dispara
  un pase por lote; el Vault persiste sellado en disco (`vault_io`). CLI:
  `archie sentry-follow [--once]`, con `[sentry.netwatch] state_dir` en la config.
- [x] **4. Persistir lo que ya se avisó.** ~~La deduplicación de alertas vive en memoria~~
  **Hecho (Kimi, 2026-10-11):** `told.json` (0600, atómico, acotado, expulsa las más
  viejas). Reiniciar no repite alertas; `address-moved` ahora sigue la dirección: un
  vaivén de ARP produce una alerta que cuenta los cambios y reavisa solo tras una hora
  de enfriamiento y al menos un cambio más.
- [x] **5. Límites al cargar el estado.** **Hecho (Kimi, 2026-10-11):** `State::parse`
  con tope de bytes (256 MiB) antes de deserializar y errores tipados que nombran el
  límite (dispositivos, nombres, destinos, direcciones). Lo que el programa guarda,
  siempre recarga; hay prueba de cada tope.
- [ ] **6. Probar en vivo las tres reglas que faltan**: `dns-tunnel-suspect`,
  `address-moved`, `host-sweep`. En WSL el DNS sale por loopback; hace falta Linux real o
  capturar en `any`. *Hecho cuando:* cada una salta con tráfico generado y queda el
  comando que lo reproduce.
  **Parcial (Claude, 2026-10-12):** con TShark real, no con red real.
  `archie-lan/tests/tshark_e2e.rs` construye byte a byte una captura con ARP spoofing
  del router, su gemelo IPv6 (Neighbor Advertisement), un túnel DNS y un barrido de 50
  hosts, la pasa por `archie-netwatch read` y comprueba cada alerta. El CI instala
  tshark y la ejecuta (`--ignored`). Reproducir:
  `cargo test -p archie-lan --test tshark_e2e -- --ignored`. Falta el tráfico vivo.
- [~] **7. Baselines por dispositivo**: volumen y destinos habituales, alerta por
  desviación. **Parcial (Claude, 2026-10-12):** volumen, sí: `traffic-surge`, con media
  y varianza móviles por tramos de 10 minutos, persistidas en `state.json`, aprendidas
  durante un día y avisadas como mucho una vez por hora. Destinos: `unusual-destination`,
  que salta con más de 25 MB a un destino que el dispositivo no usaba al aprender. La
  tasa de falsos positivos sigue sin medir. Los umbrales actuales (100 puertos, 50 hosts, entropía 4,0) son criterio,
  no medida. *Hecho cuando:* hay una tasa de falsos positivos medida sobre una captura
  real de al menos un día.
- [x] **8. Adaptador del router** (DHCP, vecinos, Wi-Fi), según el firmware real: el de
  Nelson es un MitraStar GPT-2741GNAC. Sin eso o un puerto espejo, un nodo conmutado
  solo ve su propio tráfico. *Hecho cuando:* la lista de clientes del router entra a la
  vista del punto 1, solo lectura.
  **Hecho (Kimi, 2026-10-12), como administración, no solo lectura:**
  `adapters/router/` — sonda de capacidades (`probe.rs`) que declara honestamente qué
  expone el firmware; clientes reales (`clients.rs`); UPnP IGD con
  AddPortMapping/DeletePortMapping/GetSpecific/GetGeneric (acotado a 256) y
  GetStatusInfo (`igd.rs`); canal SSH con tablas de comandos permitidos por familia de
  firmware (`ssh.rs`). Todo write pasa por Intent → policy → aprobación → postcondición
  medida (`PortMapping` presente en GetSpecific). Lo que el MitraStar no exponga queda
  `Unsupported` declarado, nunca omitido en silencio. 25 pruebas. Falta probarlo contra
  el router real.
- [x] **9. Imprimir dentro del motor.** ~~`archie-print` habla IPP directo~~
  **Hecho (Kimi, 2026-10-11):** `Command::PrintSubmit` con policy, journal y aprobación
  por digest; la postcondición es la propuesta aquí: «la impresora tiene el trabajo» —
  una entrada de cola no-completada con nombre `archie-<sha256[0..16]>`, cuyo contenido se
  verifica por SHA-256 antes de enviar un byte. El formato se comprueba contra
  `document-format-supported` antes de enviar; el completado queda como observación
  posterior fuera del plazo de 30 s. CLI: `archie sentry print`. Recibo durable vía el
  flujo normal de intents. `archie-print` queda como herramienta de operador.
- [ ] **10. Imprimir en una impresora real.** La Brother DCP-T720DW no respondía. *Hecho
  cuando:* sale una página y el recibo dice `completed`, con el formato que aceptó.
- [~] **11. Respuesta.** Hoy todo es observación. Primera respuesta reversible: aviso
  por voz y por el Observatory. Bloqueo o cuarentena solo como `Intent` aprobado.
  **Parcial (Claude, 2026-10-12; voz cableada 2026-10-18):** tablón de avisos del dueño
  (`archie-notices`), que `sentry-follow` alimenta aunque no haya modelo local. Lo leen
  `archie sentry-notices` (listar y acusar), `GET /api/notices` de displayd y
  `archie-voice` (`converse`/`live`, que dice `notice.spoken` literal durante el silencio
  por orden de gravedad y marca `spoken_ms` al terminar de reproducirlo; ver
  [`SENTRY-RESPUESTA.md`](SENTRY-RESPUESTA.md)). Cada aviso trae una frase hablable que
  no nombra nada de la red. Umbral en `[sentry.response] min_severity`. Falta el panel
  QML (GUI de Claude/Xavi). **Bloqueo (Claude, 2026-10-15):**
  `Command::DeviceBlock` por el router como `Intent` aprobado (`archie sentry-node
  block|unblock`), regla `FORWARD` por MAC en iptables/ip6tables con postcondición
  `-C`, `protected_macs` rechazadas. *Hecho cuando:* bloquear y desbloquear un equipo
  real en el MitraStar con recibo `verified`. Cuarentena (VLAN/aislamiento) no existe.
- [~] **12. Control más allá de Home Assistant.** **Parcial (Kimi, 2026-10-11):** Roku
  ECP actúa (teclas con postcondición medible y lanzamiento de apps, verificado contra el
  propio dispositivo); MQTT actúa switches/luces/clima con estado medido; ONVIF observa
  (sin PTZ: sin postcondición medible); Chromecast declara su ausencia (CAST v2 necesita
  TLS+protobuf que el crate no tiene). Falta Matter. Cada adaptador con su prueba (mocks
  loopback; falta hardware real).
  **Ampliado (Kimi, 2026-10-12):** WoL (paquete mágico de 102 bytes, verificación por
  tabla ARP / `ip neigh`, sin ping) y ADB para Android (`adapters/adb.rs`): lanzar apps
  (monkey), bloquear sesión (keyevent solo si el estado difiere), volumen con índice
  leído de `dumpsys audio`, `adb connect` + `adb devices` antes de actuar; pausa/play
  multimedia declara `Unsupported` por no tener postcondición medible. Nuevos contratos:
  `WakeHost`, `WifiSet`, `SessionLock`, `VolumeSet` y `PortMapping`; el estado del Host
  ahora sigue `reachable/awake/session_locked/volume_percent/foreground_app` y
  `AppLaunch` verifica contra el proceso en primer plano. Runner de procesos acotado
  (`adapters/process.rs`): binario absoluto, sin shell, entorno limpio, salida acotada,
  timeout con kill.
  **Cableado en vivo (2026-10-12):** `[sentry.router]` (host, IGD control URL, SSH
  user/key), `[sentry.wol]` y `[sentry.android]` en `sentry_config.rs`; el registro y
  `adapter_for` del flujo revisado los construyen. CLI: `archie sentry-node
  wake|lock|unlock|volume|launch` — cada uno pasa por Intent → aprobación →
  postcondición medida, sin atajo.
  **Ola 3 (Claude, 2026-10-12):** Chromecast ya no declara su ausencia: CAST v2 real
  sobre TLS 8009 (`rustls`/`tokio-rustls` + `prost`), LAUNCH/STOP/volumen/mute con
  postcondición medida contra el RECEIVER_STATUS del propio receptor, certificado fijado
  por SHA-256 (sin cadena ni autenticación de dispositivo de Google: documentado en el
  módulo). Config `[[sentry.chromecasts]]`, CLI `archie sentry launch|stop|volume|mute|
  unmute` y `archie sentry cast-probe <ip>`. Sigue abierto: nada de esto ni de lo
  anterior se ha probado contra hardware real.
- [x] **15. Feeder unificado con perfiles de foco.** *Hecho cuando:* un solo camino lleva
  alertas, dispositivos y Wanda a `PillarContext`, y un perfil decide qué entra, con qué
  prioridad y qué se descarta, sin cruzar dominios ni bajar `restricted`.
  **Hecho (Claude, 2026-10-12):** `archie_cerebra::{feeder, focus}`, perfiles
  `pantalla-1`, `trabajo`, `personal`, `red`; productor nuevo de cambios de dispositivo;
  `archie focus <perfil>`. Detalle: [`SENTRY-FOCO.md`](SENTRY-FOCO.md). Pesos y umbrales
  son criterio, no medida.
- [x] **16. Fixtures enlatados → servidores de protocolo con estado.** *Hecho cuando:*
  cada adaptador pasa por el motor completo contra un servidor loopback que guarda el
  estado de verdad. **Hecho (Claude, 2026-10-12):** `archie-sentry/tests/servers/`
  (Roku, HA, broker MQTT con dispositivos, IPP con cola, ONVIF con digest, IGD con
  tabla, Matter tras chip-tool, CAST v2) y `tests/protocol_servers.rs`. Encontraron dos
  defectos reales, ya corregidos: Roku en inicio (`<app>Roku</app>`) y la carrera del
  informe MQTT. Matter no habla Matter en el cable: el servidor emula chip-tool. Son
  pruebas de protocolo, no dispositivos verificados.
- [~] **13. `archie-netwatch.service` bajo systemd real.** No se sabe si `DynamicUser`
  con capacidades ambientales basta para `dumpcap` en Arch.
  **Parcial (Claude, 2026-10-12):** no bastaba, por dos motivos que se ven sin hardware.
  En Arch, `dumpcap` es `root:wireshark` 0754 y un usuario dinámico no puede ni
  ejecutarlo. Además, el estado de un `DynamicUser` vive en `/var/lib/private` (0700) y
  el follower no podía leer las alertas. Ahora corre con una cuenta estática
  `archie-netwatch` (sysusers), `Group=archie`, `SupplementaryGroups=wireshark` y
  archivos 0640. `systemd-analyze verify` pasa y la exposición baja de 5.1 a 3.5. Hay
  una prueba en `packaging.rs`. Falta arrancarla en un Arch real con una interfaz.
- [x] 14. IPv6: `address-moved` solo mira ARP; falta Neighbor Discovery. **Hecho (Claude,
  2026-10-12):** TShark lee `icmpv6.nd.na.target_address`; una Neighbor Advertisement
  por otra MAC es un movimiento, grave si es la dirección del router (`--gateway` acepta
  v4 o v6; en vivo se toma de `/proc/net/ipv6_route`). `owners` pasa a `IpAddr` y los
  estados viejos cargan igual. Probado con líneas de TShark sintéticas, no en una red
  IPv6 real.

## 4. Contexto personal y trabajo

- [ ] **`PillarContext` gradúa todo `restricted`**, y `restricted` no sale a la nube: en
  un nodo sin modelo local esos pases terminan en rechazo. Hace falta un modelo local o
  que Cerebra gradúe por contenido y use `ask_private` (el velo).
- [ ] **Transporte Enterprise.** `POST /sync` responde 501: no hay envío a ningún tablero.
- [ ] Productores reales de Sentry, Wanda, Vault, IRIS y Forge hacia `PillarContext`.
  **Parcial (Kimi, 2026-10-12):** Sentry (punto 3 de la sección 3) y Wanda ya producen —
  `archie_cerebra::wanda_bridge` parsea exports de WhatsApp (.txt), Instagram (JSON) y
  notificaciones on-device (JSONL), clasifica cada conversación en un vector de 10
  categorías con saliencia 0–100 (ES/EN) y lo registra con `record_chat_context`:
  Vault en grado Restricted + referencia `PillarContext` del pilar Wanda. El seudónimo
  usa doble FNV (sha2 no es dependencia de cerebra): es seudónimo, no protección.
  Faltan Vault, IRIS y Forge. **Llamador en vivo (2026-10-12):**
  `archie_cerebra::wanda_watcher` sigue el directorio configurado
  (`[sentry.chat_exports] watch_dir` + `salt`), con journal de hashes de contenido
  para que reiniciar no duplique. CLI: `archie wanda-watch [--once]` y
  `archie wanda-ingest <archivo>`.
  **Feeder (Claude, 2026-10-12):** todo lo que producen Sentry y Wanda, más los cambios
  de dispositivo, llega a Cerebra por un solo `feeder::plan` con perfil de foco
  (sección 3, punto 15).
  **Vault (Claude, 2026-10-12):** `archie_cerebra::vault_bridge` sella los documentos
  del dueño (`.md`, `.txt`, hasta 256 KiB) como `Document`/`Restricted` en un dominio
  explícito y los incluye como `Pillar::Vault`. El tipo (encargo, restricción, versión,
  nota) sale del front-matter `kind:` o del nombre del archivo, nunca de adivinar por la
  prosa. CLI `archie vault-docs --dir … --domain work/<org>/<proyecto>`. El feeder los
  pesa (encargo 90, restricción 80, versión 60, nota 40): entran en `trabajo` y
  `personal`, nunca en `pantalla-1`. Faltan IRIS (nada escribe sus episodios ni sus
  recibos a disco) y Forge (`core.forge` no tiene worker).

## 5. Voz

Detalle: [`HANDOFF-2026-10-11-SPEECH-FLOOR.md`](HANDOFF-2026-10-11-SPEECH-FLOOR.md). — Sheriff

- [ ] Voz dentro de la imagen WSL (hoy no va).
- [ ] La voz y Arena clásico no pasan por el router ni por `ask_private`.
- [ ] Magpie y su envoltorio RVC en vivo; la detección de idioma es heurística.
- [ ] Reconocimiento del hablante. Es distinto de presencia y de autorización.

## 6. Diseño — Xavi termina aquí

**Lo que hay** (commit `4b282cd`, ya en `main`): `display/shell/prototype/`, doce
componentes QML con `Theme.qml` (tokens de las tres direcciones y la síntesis) y
`Synthetic.qml` (todos los datos, inventados y rotulados). Documentos en
[`gui/`](gui/README.md): `DESIGN_SYSTEM.md`, `SCREEN_SPECS.md`, `MOTION_CINEMATICS.md`,
`ARCHITECTURE_GUI.md`, `GUI_FUNCTION_BACKLOG.md`. Capturas en `gui/screenshots/`.

**Lo que no hay:** nada del diseño nuevo de **Lumenfield para el Orbe**. El Orbe de
`display/native` (`LumenScene.qml`, `LumenField.cpp`, shaders `lumen_orb.*`) es el de
septiembre, y desde `4b282cd` no entró ningún commit de diseño. Si existe fuera del
repositorio, hay que subirlo.

**Decisiones que son de Xavi** (bloquean lo demás):

- [ ] **D1. La dirección visual.** Frost, Workspace, Signature o la síntesis propuesta
  (superficies de Frost, acento `#6fc8f5`, radio 12, densidad 0,94). Dejarla como la única
  en `Theme.qml`.
- [ ] **D2. Tipografía.** El prototipo usa la del sistema; las capturas salieron con
  Segoe UI porque se renderizaron en Windows.
- [ ] **D3. Los ADR** de `gui/ARCHITECTURE_GUI.md`: aceptar o rechazar cada uno.
- [ ] **D4. Lumenfield nuevo para el Orbe.** Subir el diseño y decir qué reemplaza de
  `display/native`. El acento de la síntesis se eligió para convivir con el cian del
  Orbe actual (`#6fe3f5`); si el Orbe cambia, revisar ese par.

**Diseño por terminar:**

- [ ] **10 de 15 superficies sin dibujar** (lista en `gui/SCREEN_SPECS.md`). Dibujadas:
  barra superior, dock, Vault (lista y cuadrícula), centro de control, centro de
  actividad, paleta de comandos.
- [ ] **Movimiento**: `MOTION_CINEMATICS.md` es especificación; nada está animado.
- [ ] **Estados** de cada superficie: vacío, cargando, error, sin permiso, sin conexión.
- [ ] **Vista de Sentry**: dispositivos de la red y alertas (depende del punto 1 de
  Sentry). Hoy no tiene superficie.
- [ ] **Vista del Stage**: quién tiene la autoridad y cómo se toma el control.

**Integración** (Claude, cuando D1–D3 estén decididas):

- [ ] **Conectar a datos reales** por el bridge de solo lectura (`display/native/src/ArchieBridge.h`),
  reemplazando `Synthetic.qml` superficie por superficie. Primero: dominio
  (personal o trabajo), autoridad del Stage, presión de CPU.
- [ ] **Chips de contexto de Vault** desde `ContextDomain` (`core/crates/archie-vault/src/domain.rs`).
- [ ] **Pausa de emergencia de autoridad**: `archied` no expone ese control; el
  prototipo ya lo muestra como no disponible.
- [ ] **Vault**: las 24 acciones `GUI-VAULT-*` de `gui/GUI_FUNCTION_BACKLOG.md`.
- [ ] **Sin verificar**: Quickshell, blur del compositor, NVIDIA, Wayland, lectores de
  pantalla, presupuesto de frame. El prototipo solo se renderizó por software.

## 7. Plataforma

- [ ] **Módulos**: 31 manifiestos siguen `unimplemented`.
- [ ] **Modelo local para datos `restricted`** (6–8 GB): manifiesto y servidor.
- [ ] **Unidad de servicio del Stage** en la ISO.
- [ ] **Un solo prompt de planner**: `ARCHIE.md` es el contrato; Arena aún lleva el suyo.
- [ ] **Prueba de punta a punta** con voz → planner → IRIS → resultado verificado → REM.
- [ ] Compatibilidad multiplataforma: [`TODO-COMPATIBILIDAD-MULTIPLATAFORMA.md`](TODO-COMPATIBILIDAD-MULTIPLATAFORMA.md).
- [ ] Biometría real, conectores de nodos y móviles, app iOS.
- [ ] Trello al día con esta lista. — Nelson
