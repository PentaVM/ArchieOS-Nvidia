# Orbe de Xavi, navegación de escritorio y Forge — plan de integración

> Implementar en este worktree con `superpowers:executing-plans`; revisión independiente antes del commit final. El usuario autorizó integrar PR #53 y confirmó conservar IDEs en Forge.

**Objetivo:** integrar el PR completo de la esfera neuronal de Xavi conservando el World actual, hacer que World use la navegación inferior del escritorio y limitar Forge a aplicaciones de usuario/CLI.

**Especificación:** petición del usuario de 2026-10-26 y PR #53, `design/lumenfield-0449-orbe` en `3cc8d37`. Diseño de Xavi / implementación nativa Rust-C++ de PentaVM; conservar ambos créditos.

**Base:** `codex/world-geofabric-rust` en `36b2588`. Rama local de revisión: `codex/orbe-design-review`. No publicar ni fusionar en GitHub: el usuario ejecutará el comando de revisión y publicación en su shell.

## Límites

- Integración por diferencia respecto de main; preservar todos los cambios de World y los archivos ajenos sin commit.
- Core, Cortex/Cerebra, IRIS, Policy, leases y separación de dominios mantienen su autoridad.
- La geometría del filesystem se presenta como una instantánea y sus conexiones decorativas no se describen como actividad observada de Core.
- Reutilizar el dock, tema, ventanas y rutas del shell. No dejar un World flotante con tres pestañas superiores como sustituto del escritorio.
- Forge conserva IDEs, apps normales, editor y CLI; oculta únicamente herramientas internas/pruebas/entrenamiento. Stage sigue disponible desde el dock; su ruta global no depende de la lista visible de Forge.
- Capturas de Google siguen bloqueadas. No capturar ni enviar sus píxeles al modelo.
- Presupuesto de render del OS, movimiento reducido, vista oculta y exclusividad del controller mandan sobre el diseño.

## Tareas y prueba

- [x] Verificar remoto y worktree; localizar PR #53 y comparar sus cambios con la rama actual.
- [x] Revisión de lectura independiente del nuevo renderer y del catálogo de Forge.
- [x] Iniciar merge completo sin commit; resolver conflictos preservando World, sus capturas protegidas y ambas suites de pruebas.
- [x] Corregir selección ABI de dos llamadas, reloj y estados de fuente de la esfera. Agregar regresiones antes del cambio.
- [x] Hacer que el renderer neuronal respete intervalos, límites, posprocesado, visibilidad, navegación y movimiento reducido.
- [x] Propagar resultados verdaderos de focus/camera/pointer/highlight/freeze al cliente CLI; probar rechazos y acciones que sí se aplican.
- [x] Compartir el contenido World con el shell; mantener dock inferior completo, ventanas y navegación de vuelta al Orbe/Forge sin un segundo controller.
- [x] Introducir proyección Forge separada del catálogo global; probar IDE, navegador, CLI, internos y permisos con manifiestos reales/temporales.
- [x] Ejecutar Rust focalizado, CTests nativos/visuales, QML del shell y regresiones World. Guardar comandos, salidas y límites reales en `_dev`.
- [x] Recorrido visual real del Orbe/Forge y World con fuente abierta; sin afirmar NVIDIA, voz o sistemas remotos a partir de fixtures.
- [x] Revisar diff final y contra main; crear commit local y proporcionar comando de revisión y push al usuario. No ejecutar push.

## Focos de revisión

- Estado inválido, nodo inexistente y raíz ilegible deben mostrarse como error, no ACK satisfactorio ni instantánea nueva.
- Congelar la animación no debe inmovilizar etiquetas durante una navegación permitida.
- Cambiar World ↔ Orbe no deja dos vistas/controladores trabajando ni borra las capacidades privadas de navegación.
- Filtrar Forge no rompe Stage ni altera permisos de ejecución del catálogo global.
- Captura por el nuevo verbo CLI sigue usando la misma protección de fuentes propietarias.

## Entorno

WSL de pruebas no respondía al inicio. Las cachés nuevas de compilación Windows se trasladaron de forma recuperable a `D:/ArchieOS-Orbe-Review-20261026/windows-debug-cache`; no se borraron fuentes. Compilar en D para no agotar C. Registrar cualquier prueba que el entorno impida, sin marcarla como pasada.

Resultados y límites: `_dev/ORBE-DESIGN-REVIEW-2026-10-26.md`. La integración y el commit se mantienen locales para review/subida del usuario.
