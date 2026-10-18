# Sentry — la primera respuesta: avisar al dueño

Hasta ahora todo lo que Sentry veía terminaba en el Vault y en las trazas de Cerebra.
Nadie se enteraba. La primera respuesta es **avisar**, y es la única que se toma sin
preguntar: un aviso no cambia nada en la red. Lo peor que hace un aviso equivocado es
interrumpir, y acusarlo lo termina. Bloquear o poner en cuarentena un equipo es otro
tipo de acto y va siempre como `Intent` aprobado (`archie-sentry`), nunca por aquí.

## Cómo fluye

```
archie-netwatch ──► alerts.jsonl ──► archie sentry-follow ──┬─► Vault + pase de Cerebra (como antes)
                                                            └─► <state>/cerebra/notices.json
                                                                   │
                         archie sentry-notices (listar / --ack) ◄──┤
                         archie-displayd GET /api/notices ◄────────┤  (Observatory)
                         voz: lee `spoken`, marca `spoken_ms` ◄────┘  (contrato, ver abajo)
```

- El follower publica **antes** de que Cerebra razone, y aunque no pueda: un nodo sin
  modelo local también avisa.
- El tablón se guarda antes que el journal del follower. Si hay un corte, se releen las
  alertas, y como el id de cada aviso sale de su contenido (SHA-256), nada se publica
  dos veces.
- Se guardan como máximo 256 avisos. Primero salen los acusados más viejos; los
  pendientes sólo salen cuando ya no queda otra cosa.
- El archivo tiene modo 0640 (grupo `archie`: displayd lo lee; «otros», nunca) y se
  escribe de forma atómica. Toda lectura-modificación-escritura pasa por `Board::update`, con lock exclusivo sobre `notices.lock`: el follower,
  `--ack` y la voz no se pisan (probado con 8 hilos y 80 avisos).
- Un tablón dañado se aparta como `notices.bad` y se empieza uno vacío. El informe lo
  dice (`notices_reset`). Displayd no lo muestra como vacío: responde `unavailable` con
  el error.

## Qué avisa

`[sentry.response] min_severity` = `notice` | `warning` (por defecto) | `high`. Por
debajo del umbral la alerta queda registrada en Cerebra, pero no interrumpe. Con el
valor por defecto, un dispositivo nuevo o un cambio normal de DHCP no avisan; un
port-scan, un túnel DNS o alguien reclamando la dirección del router, sí.

## Dos textos por aviso

- `detail`: lo que vio la auditoría, para una pantalla que el dueño está mirando.
- `spoken`: una frase fija por regla. No nombra ningún dispositivo, dirección ni nombre
  DNS, así que puede decirse en voz alta en una sala compartida. Ejemplo: *«Sentry: otro
  equipo está reclamando la dirección del router; el tráfico podría estar pasando por
  él.»*

## Cómo los dice la voz (`archie-voice`)

En `archie-voice::conversation::converse` (`Setup::notices`, por defecto
`<state>/cerebra/notices.json` en `converse` y `live`, o `--notices <ruta>` /
`ARCHIE_NOTICES`):

1. Cuando no hay turno en curso, nadie está hablando y la sala lleva en silencio al menos
   `setup.end_after` (800 ms por defecto), lee `Board::load` y toma el primer aviso de
   `board.unspoken()` (pendientes no dichos, primero los más graves).
2. Envía `notice.spoken` literal a la voz — nunca `detail`, nunca pasando por el modelo.
3. Sólo cuando la reproducción termina sin cortarse (`FromVoice::Spoke { stopped: false }`),
   marca `spoken_ms` en disco bajo el lock exclusivo (`Board::update(path, |b| b.mark_spoken(&id, now_ms))`).
   Si la síntesis falla, el aviso sigue sin decirse.

Desde fuera de Rust sigue existiendo `archie sentry-notices --claim`, que imprime como
JSON el siguiente aviso que hay que decir (o `null`) y lo marca como dicho bajo el lock.
Un aviso acusado nunca se dice. El dueño acusa con `archie sentry-notices --ack <id>`
o `--ack-all`.

## Comandos

```
archie sentry-follow --once                 # publica avisos nuevos (además de lo de siempre)
archie sentry-notices                       # pendientes
archie sentry-notices --all                 # también los acusados
archie sentry-notices --ack 657cc778        # prefijo único de 6+ caracteres
archie sentry-notices --ack-all
archie sentry-notices --claim               # para la voz: el siguiente que decir, marcado dicho
curl 127.0.0.1:7666/api/notices             # lo que lee el Observatory
```

## Qué no está verificado

- No hay panel QML que dibuje `/api/notices`: la GUI es de Claude/Xavi.
- En la ISO, `archie-sentry-follow.service` (instalado, no habilitado) corre el
  follower como `archie` y deja el tablón 0640. Displayd (`archie-display`, grupo
  `archie`) lo lee. Si se despliega de otra forma y no puede leerlo, `/api/notices`
  responde `unavailable` con el error de permisos, nunca un vacío.
- Las pruebas son con alertas sintéticas y archivos temporales. Nada se probó contra
  tráfico real.

## Bloquear un dispositivo: `Intent` aprobado

La segunda respuesta corta el tráfico de un equipo en el router. Va siempre como
`Command::DeviceBlock { mac, blocked }`, por el mismo camino que cualquier acto de
Sentry: `prepare` → revisión humana del digest → `approve` → adaptador → postcondición
medida. No existe atajo desde el tablón, el follower ni Cerebra.

```
archie sentry-node block   <router> aa:bb:cc:dd:ee:ff   # prepara el Intent; pide aprobación
archie sentry-node unblock <router> aa:bb:cc:dd:ee:ff
```

- **Qué hace en el router** (canal SSH, argv exacto, sin shell): en cada cortafuegos
  que responde a `-S FORWARD` (`iptables`, `ip6tables`) pone
  `-I FORWARD -m mac --mac-source <mac> -m comment --comment archie-sentry-block -j DROP`
  solo si `-C` dice que falta. Desbloquear borra con `-D` hasta que `-C` dice que no
  queda, y solo reglas con ese comentario y esa MAC: nunca toca las del router.
- **Postcondición:** `-C` en todos los cortafuegos. Todos con la regla = bloqueado;
  ninguno = desbloqueado; desacuerdo o error = no verifica nada (el recibo falla).
- **Rechazos antes de tocar nada:** MAC que no sea una sola dirección de equipo en
  minúsculas (multicast, broadcast y ceros incluidos), recurso sin la capacidad, y
  cualquier MAC de `[sentry.router] protected_macs` (este nodo, el propio router).
  Desbloquear una protegida sí se permite: solo puede devolver acceso.
- **Honesto si no puede:** router sin SSH o sin cortafuegos que responda →
  `Unsupported`; ssh caído (255) o `-C` con otro código (falta el módulo `mac`) →
  `Unavailable`.
- **Sin verificar en hardware.** Probado con un cortafuegos simulado con estado. No
  sabemos si el firmware del MitraStar trae `iptables` con los módulos `mac` y
  `comment`; si no, la respuesta será `Unavailable`, no un bloqueo fingido.
- La regla no sobrevive a un reinicio del router (no se escribe en su configuración
  persistente). Es deliberado: un bloqueo equivocado se arregla reiniciando.

