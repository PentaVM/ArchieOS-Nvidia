# Relevo — Sentry ola 3 y día autónomo (2026-10-12, Claude)

PR #36 (`claude/sentry-ola3-cast-focos-servidores`). Qué hay, qué está probado y, sobre
todo, **qué hay que probar en hardware real**, en orden, con lo que debe verse en cada
paso. Nada de esto se ha probado contra un dispositivo ni en una red real. Todo pasó en
loopback, con TShark real sobre capturas construidas o con binarios reales sobre datos
sintéticos.

## Qué entró

| Área | Qué | Documento |
|---|---|---|
| Red | IPv6 en `address-moved`, `traffic-surge`, `unusual-destination` | `SENTRY-NETWATCH.md` |
| Respuesta | Tablón de avisos al dueño, `archie sentry-notices`, `GET /api/notices` | `SENTRY-RESPUESTA.md` |
| Contexto | Feeder con perfiles de foco; documentos del Vault como pilar | `SENTRY-FOCO.md` |
| Dispositivos | Chromecast CAST v2; arreglos en ADB, BLE, Matter, WoL, SSH y ONVIF | `PENDIENTES.md` §3 |
| Despliegue | `archie-netwatch` con cuenta estática; `archie-sentry-follow.service` | `SENTRY-NETWATCH.md` |
| CI | Verde: fmt, clippy `-D warnings`, `test --workspace`, nativos y e2e con TShark | `.github/workflows/ci.yml` |

## Validación en hardware, en orden

### 1. Netwatch en el Arch real

```bash
sudo systemd-sysusers                       # crea archie-netwatch
echo 'ARCHIE_NETWATCH_INTERFACE=<iface>' | sudo tee /etc/archie/netwatch.env
sudo systemctl enable --now archie-netwatch archie-sentry-follow
journalctl -u archie-netwatch -f
```

- **Debe verse:** `dumpcap` abre la interfaz sin error de permisos, y aparecen
  `/var/lib/archie/netwatch/{state.json,told.json}` en modo 0640 (`alerts.jsonl` llega
  con la primera alerta). Si `dumpcap` falla con `NoNewPrivileges=yes`, anótalo en
  `PENDIENTES.md` §3 punto 13: es justo lo que no se pudo comprobar aquí.
- `archie-sentry-follow` debe seguir vivo aunque todavía no exista `alerts.jsonl`. Eso
  se arregló en `0e05975`.

### 2. Las reglas con tráfico vivo (§3 punto 6)

Desde otro equipo de la red:

- `host-sweep`: `nmap -p 445 192.168.1.0/24`
- `port-scan`: `nmap -p 1-200 <host>`
- `dns-tunnel-suspect`: `dig $(head -c 40 /dev/urandom | base32 | tr -d = | tr A-Z a-z).example.com`
- `address-moved` (v4): `arping -U -s <ip-de-otro> <iface>`, en una red de prueba
- `address-moved` (v6): `ndsend <ipv6> <iface>` (paquete `ndisc6`)

Cada una debe aparecer en `alerts.jsonl` y, si llega al umbral, en `archie
sentry-notices`. La versión reproducible sin red está en
`cargo test -p archie-lan --test tshark_e2e -- --ignored`.

### 3. Avisos

```bash
archie sentry-notices            # pendientes, los más graves primero
archie sentry-notices --ack <id> # prefijo de 6 o más caracteres
curl -s 127.0.0.1:7666/api/notices
```

`/api/notices` debe responder `source: live`. Si dice `unavailable` con un error de
permisos, `archie-displayd` no está en el grupo `archie`.

### 4. Chromecast

```bash
archie sentry cast-probe <ip>    # huella SHA-256 del certificado
# añadir [[sentry.chromecasts]] con esa huella; luego:
archie sentry launch|stop|volume|mute|unmute ...
```

Hay que medir si el Chromecast rota su certificado y cada cuánto. Si lo rota, todo
falla cerrado hasta aprobar la huella nueva.

### 5. Documentos del Vault

```bash
archie vault-docs --dir ~/proyectos/puente --domain work/<org>/puente --once
archie focus trabajo --domain work/<org>/puente
```

Sin modelo local, el pase termina en `refused:NoLocalModel`. Es lo esperado.

## Decisiones que son de Nelson

1. **Bloquear un dispositivo** (§3 punto 11, la otra mitad). Propuesta: un
   `Command::DeviceBlock { mac, blocked }` por el router, con postcondición medida
   leyendo la regla de vuelta. Toca contratos y policy, y depende de qué exponga el
   MitraStar, que no lo sabemos. **Decidido e implementado (2026-10-15)**: ver
   `SENTRY-RESPUESTA.md` § Bloquear. Queda probarlo en el router real.
2. **Umbrales**: `traffic-surge` (50 MB, 4× lo habitual, 6σ) y `unusual-destination`
   (25 MB) son criterio. Hay que medir la tasa de falsos positivos con un día de captura
   real.
3. **Retención en el Vault**: las alertas y los documentos se sellan sin caducidad y el
   vault crece. Decidir si las alertas caducan, por ejemplo a los 90 días.
4. **La voz**: el contrato para decir los avisos está en `SENTRY-RESPUESTA.md`. Lo
   tiene que conectar quien lleva `archie-voice`; su modo `live` no compila sin el SDK
   de NeMo.
5. **`wanda-watch` y `vault-docs` como servicios**: hoy son comandos. Ejecutarlos en
   continuo pide decidir dónde vive el estado de un usuario, que no es
   `/var/lib/archie`.
