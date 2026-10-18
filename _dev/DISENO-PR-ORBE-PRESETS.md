# PR de diseño: presets del orbe controlados por CLI (Xavi)

Un solo PR de diseño abierto a la vez; las siguientes tandas de diseño se suman a este PR.

## Hecho
- `archie visual scene|preset|list` (archie-cli) y herramienta de voz `orb_preset`: un solo cliente,
  `archie_orb::control`, sobre el canal loopback `127.0.0.1:7668`.
- El protocolo del display acepta `list`, `scene` y `preset`. Se retiran las acciones `cinematic`
  (crown/atlas/dense) y `overlay` (hud/shell/ambient/guides/links/pulses/bloom): los parámetros
  visuales ya no se conmutan por capas, viven en presets.
- `field` = «Regiones vivas», sin sobrescrituras: se dibuja exactamente igual que antes.
- `sphere`, `orb`, `neural` solo cambian parámetros que este renderizador ya tiene.
- Los presets son archivos: `display/native/presets/<nombre>.json` (CMake los embebe todos en
  `:/archie/presets`). Nadie más guarda la lista: la CLI y la voz la piden con `list`.
- Revisión (segundo filtro): el display valida cada preset entero con la misma regla que el panel
  (clave conocida, tipo, color `#rrggbb`, layout conocido, número dentro de rango sin recortar);
  si algo falla no cambia nada y responde `error: <motivo>`. El preset se aplica en C++ antes de
  responder, así que `ok` significa aplicado. Las etiquetas del HUD son el ajuste `labels`.
  Probado sin GPU: `archie visual list|preset|scene` contra `LumenField` + `DisplayControl`
  reales (Qt 6.4, offscreen), incluido un preset roto, uno desconocido y peticiones malformadas.
- Se retiró el botón «Antes» (`toggleCompare`, aspecto V2) del orbe nativo.

## No verificado (honesto)
- Compilado y arrancado en Windows (MSVC 2022, Qt 6.8.3, CMake/Ninja): `archie-display` compila sin
  errores, abre la ventana y responde `ok` a `preset sphere` por el canal 7668; el orbe se dibuja.
  Sin el observador del sistema (puerto 7666) no hay nodos, así que **no se han visto** los nodos
  de cada preset en pantalla; los valores de `sphere`, `orb` y `neural` siguen siendo una
  aproximación de diseño a revisar con datos reales. Solo se probó en Windows (Qt/D3D), no en Linux/WSLg.
- No se portaron aún los nodos con halo/formas, los estilos de enlace ni el cerebro vivo
  (emisiones dentro de los enlaces) de Archie2: requieren shaders nuevos en `display/native/shaders`.

## Fuera de alcance
Áreas de Santiago y Penta (Sentry, Wanda, IRIS, Core, Policy).
