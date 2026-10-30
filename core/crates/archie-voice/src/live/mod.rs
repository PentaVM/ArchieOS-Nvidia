//! A live session: Archie with ears, a voice and eyes, reached from a browser page.
//!
//! ```text
//!  browser ── mic 16 kHz ──► ws ─► Hearing::Stream ─► conversation (ears, floor, Nemotron, voice)
//!     ▲                                                              │
//!     └──── voice PCM, what was heard/said, the scene ◄── ws ◄───────┘
//!  browser ── camera JPEG ─► ws ─► eyes (IRIS) ─► scene ─► context of every turn
//!                                        └──► curiosity ─► identify ─► search ─► buffer
//! ```
//!
//! The conversation is the same loop `converse` runs, with the browser in place of the
//! microphone and the speaker. The floor still decides whether Archie speaks: it answers
//! what is said to it, and what curiosity found out waits in the buffer until it is asked
//! for. The page is served on loopback only; reach it through an SSH tunnel.
//!
//! What the model is told it sees is only ever what IRIS read, said as IRIS's: what the
//! camera shows now, how old that image is, what was there before and is not any more
//! ([`memory`]). It goes beside the utterance it answers, after the past conversation, so
//! nothing Archie said earlier about the room can pass for what is true now.

pub mod curiosity;
pub mod eyes;
pub mod language;
pub mod memory;
pub mod persistent;
pub mod style;

use crate::conversation::{Moment, VoiceOut};
use crate::visual::visual_command;
use archie_model_runtime::ToolDefinition;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use curiosity::{Buffer, Identifier, Researcher};
use eyes::{IrisEyes, Scene, Sweep};
use futures_util::{SinkExt, StreamExt};
use memory::Memory;
use persistent::PersistentMemory;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Notify};

/// Who Archie is in a live session.
pub const PERSONA: &str = "Eres Archie, el asistente de ArchieOS, y ahora tienes ojos: una \
cámara que tu visión, IRIS, lee en vivo. Lo único que ves es lo que el contexto de este \
momento dice que IRIS ve ahora; IRIS todavía está aprendiendo y se equivoca, así que di \
«me parece» cuando no es seguro, y nunca afirmes que algo está si ese contexto no lo dice. \
Lo que se veía antes en la conversación puede haber cambiado: si el contexto dice que algo \
ya no se ve, o que no hay nadie, dilo así. Si la cámara no manda imagen, di que no ves. Contesta en el \
idioma en que te hablaron: español de Chile, natural y tuteando (nunca voseo), o inglés. Todo \
lo que escribes se dice en voz alta: frases cortas, sin markdown, sin listas, sin emojis; los \
números y las fechas con palabras. IRIS nombra categorías generales, como juguete, auto o \
botella, no nombres exactos: no inventes qué es exactamente algo. Si te preguntan qué es algo y \
no está en lo que ya averiguaste, usa la herramienta identify; para datos de internet, \
web_search. Lo que ya averiguaste no lo digas si nadie lo pregunta ni responde lo que te \
preguntaron. Nunca digas quién es una persona por su cara ni la busques: eso necesita su \
permiso. Puedes cambiar la vista del Observatory con observatory_scene, el aspecto del orbe \
con orb_preset, su cinemática con orb_cinematic y sus capas visuales con orb_overlay, o abrir \
un panel local con shell_panel cuando la persona lo pida. Estos cambios son solo visuales: \
no cambian inferencia ni permisos y no ejecutan operaciones del sistema. No inventes datos \
que no tienes. Si lo que oíste es un fragmento sin sentido o solo tu nombre, no lo repitas: \
pregunta en pocas palabras qué necesita, o quédate callado. Para explorar un lugar usa \
world_resolve si faltan coordenadas, y world_explore solo con coordenadas, radio en metros \
y dominio personal o work explícitos. Si falta el dominio, pregúntalo; nunca conviertas \
trabajo en personal. Una captura aceptada está pendiente: no digas que terminó hasta \
consultar world_status y observar complete. Para navegar por toda la Tierra usa world_navigate; \
las teselas siguen los movimientos y el zoom del visor dentro de los límites autorizados. \
world_navigation_status consulta esa sesión y world_navigation_stop la detiene. El dominio y \
la privacidad de navegación los fija la sesión de voz, nunca los propongas como argumentos. \
No captures lugares por actividad de aplicaciones.";

/// A string parameter limited to the names the running display reported. When the display
/// was not reachable at start-up the names are not known here, so the parameter is free and
/// the display itself refuses an unknown one.
fn display_names(names: Option<&[String]>) -> serde_json::Value {
    match names {
        Some(names) if !names.is_empty() => json!({"type": "string", "enum": names}),
        _ => json!({"type": "string"}),
    }
}

/// The tools a live session adds: looking closer, reading the web, remembering, and the
/// Observatory's views and presets as the running display lists them (`catalog`), plus
/// bounded native presentation controls.
pub fn tools(catalog: Option<&archie_orb::control::Catalog>) -> Vec<ToolDefinition> {
    let mut definitions = vec![
        crate::wanda::tool(),
        ToolDefinition {
            name: "identify".into(),
            description: "Mira de cerca un objeto frente a la cámara o un recorte visual \
                          reciente guardado por IRIS; averigua qué parece ser (marca, modelo, \
                          personaje). El modelo puede equivocarse. Nunca personas."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {"what": {"type": "string",
                    "description": "Cuál objeto, como lo dijo la persona: «esto que tengo en la mano», «el auto»."}},
                "required": ["what"]
            }),
        },
        ToolDefinition {
            name: "web_search".into(),
            description: "Busca en internet y devuelve lo que dicen las primeras fuentes. \
                          Solo con palabras públicas: nada privado de la persona."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {"query": {"type": "string", "description": "Qué buscar."}},
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "remember".into(),
            description: "Clasifica una frase personal que la persona acaba de decir y la guarda localmente con su fuente y hora. Úsala para algo importante o que deberá recordarse después; no para cada frase.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "La afirmación exacta que se debe recordar, sin añadir inferencias."},
                    "tier": {"type": "string", "enum": ["t1", "t2", "t3", "t4"], "description": "t1 persistente; t2 hasta un mes; t3 hasta una semana; t4 hoy y unos días."}
                },
                "required": ["text", "tier"]
            }),
        },
        ToolDefinition {
            name: "observatory_scene".into(),
            description: "Cambia la vista principal del Observatory cuando la persona lo pide.".into(),
            parameters: json!({
                "type": "object",
                "properties": {"scene": display_names(catalog.map(|c| c.scenes.as_slice()))},
                "required": ["scene"]
            }),
        },
        ToolDefinition {
            name: "world_explore".into(),
            description: "Envía una captura geográfica acotada a worldd solo cuando la persona la pide. Requiere latitud, longitud, radio en metros y dominio explícito personal o work; pregunta lo que falte. La respuesta acepta un trabajo pendiente, no certifica finalización. No usa foco ni actividad de aplicaciones.".into(),
            parameters: json!({
                "type":"object", "additionalProperties":false,
                "properties": {
                    "latitude":{"type":"number","minimum":-90,"maximum":90},
                    "longitude":{"type":"number","minimum":-180,"maximum":180},
                    "radius_m":{"type":"number","exclusiveMinimum":0,"maximum":100000},
                    "domain":{"type":"string","enum":["personal","work"]},
                    "layers":{"type":"array","items":{"type":"string","enum":["terrain","buildings","imagery","signals"]},"minItems":1,"maxItems":4},
                    "imagery_sources":{"type":"array","items":{"type":"string","enum":["google_satellite","blue_marble","sentinel2","landsat","natural_color","infrared","night_lights","thermal"]},"maxItems":8},
                    "imagery_date":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
                    "signal_updates":{"type":"integer","minimum":1,"maximum":60},
                    "signal_interval_seconds":{"type":"integer","minimum":60,"maximum":3600},
                    "zoom":{"type":"integer","minimum":0,"maximum":22}
                }, "required":["latitude","longitude","radius_m","domain"]
            }),
        },
        ToolDefinition {
            name: "world_status".into(),
            description: "Consulta el estado de capturas propias del dominio explícito personal o work. Solo un estado complete observado permite afirmar que terminó una captura.".into(),
            parameters: json!({"type":"object","additionalProperties":false,"properties":{"domain":{"type":"string","enum":["personal","work"]}},"required":["domain"]}),
        },
        ToolDefinition {
            name: "world_resolve".into(),
            description: "Busca coordenadas candidatas de un nombre de lugar público mediante el proveedor geográfico. Requiere dominio explícito personal o work. No inicia capturas; pide elegir entre resultados ambiguos y definir el radio.".into(),
            parameters: json!({"type":"object","additionalProperties":false,"properties":{"query":{"type":"string","minLength":1,"maxLength":256},"domain":{"type":"string","enum":["personal","work"]}},"required":["query","domain"]}),
        },
        ToolDefinition {
            name: "orb_preset".into(),
            description: "Cambia el aspecto del orbe con uno de los presets del Observatory y abre Orb. field es la vista original (Regiones vivas).".into(),
            parameters: json!({
                "type": "object",
                "properties": {"preset": display_names(catalog.map(|c| c.presets.as_slice()))},
                "required": ["preset"]
            }),
        },
        ToolDefinition {
            name: "orb_cinematic".into(),
            description: "Cambia la cinemática del orbe y abre Orb. field es la vista original; crown, atlas y dense son variaciones.".into(),
            parameters: json!({
                "type": "object",
                "properties": {"profile": {"type": "string", "enum": archie_orb::control::CINEMATICS}},
                "required": ["profile"]
            }),
        },
        ToolDefinition {
            name: "orb_overlay".into(),
            description: "Enciende o apaga una capa visual del orbe. hud controla los textos; shell la membrana; ambient las partículas; guides las guías; links las conexiones; pulses las señales; bloom el brillo.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "overlay": {"type": "string", "enum": archie_orb::control::OVERLAYS},
                    "state": {"type": "string", "enum": ["on", "off"]}
                },
                "required": ["overlay", "state"]
            }),
        },
        ToolDefinition {
            name: "shell_panel".into(),
            description: "Abre un panel local del escritorio nativo: inspector, settings, sentry, activity, notifications, search o forge. No ejecuta operaciones del sistema.".into(),
            parameters: json!({"type":"object", "properties":{"panel":{"type":"string","enum": archie_orb::control::PANELS}}, "required":["panel"]}),
        },
    ];
    if let Some(region) = definitions.iter().find(|tool| tool.name == "world_explore") {
        let parameters = region.parameters.clone();
        definitions.push(ToolDefinition {
            name: "world_plan".into(),
            description: "Calcula un plan geográfico acotado con coordenadas, radio en metros y dominio explícito personal o work. No inicia una captura ni descarga datos de proveedores.".into(),
            parameters,
        });
    }
    for (name, description) in [
        ("world_pause", "Pausa una captura propia por su identificador exacto y dominio explícito personal o work."),
        ("world_resume", "Solicita reanudar una captura propia por su identificador exacto y dominio explícito personal o work. Core valida los proveedores y permisos de la solicitud guardada; aceptación no significa finalización."),
        ("world_cancel", "Cancela una captura propia por su identificador exacto y dominio explícito personal o work."),
    ] {
        definitions.push(ToolDefinition {
            name: name.into(), description: description.into(),
            parameters: json!({"type":"object","additionalProperties":false,
                "properties":{"id":{"type":"string","pattern":"^[0-9a-fA-F]{32}$"},"domain":{"type":"string","enum":["personal","work"]}},
                "required":["id","domain"]}),
        });
    }
    definitions.push(ToolDefinition {
        name: "world_navigate".into(),
        description: "Autoriza una sesión acotada de navegación de toda la Tierra solicitada por la persona. El visor obtiene teselas automáticamente al mover o ampliar el mapa. El dominio y privacidad vienen de esta sesión; aceptación no acredita calidad ni disponibilidad de proveedores.".into(),
        parameters: json!({"type":"object","additionalProperties":false,"properties":{
            "sources":{"type":"array","items":{"type":"string","enum":["google_satellite","blue_marble","sentinel2","landsat","natural_color","infrared","night_lights","thermal"]},"minItems":1,"maxItems":8},
            "imagery_date":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
            "max_zoom":{"type":"integer","minimum":0,"maximum":19},
            "max_visible_tiles":{"type":"integer","minimum":1,"maximum":96},
            "max_requests":{"type":"integer","minimum":1,"maximum":10000},
            "max_bytes":{"type":"integer","minimum":1024,"maximum":1073741824},
            "duration_seconds":{"type":"integer","minimum":1,"maximum":3600},
            "signals":{"type":"boolean"},
            "photogrammetry":{"type":"boolean","description":"Habilita mallas 3D de Earth Web solo para vista previa en RAM cuando se selecciona Google Satellite; sin análisis ni persistencia."}
        },"required":[]}),
    });
    for (name, description) in [
        ("world_navigation_status", "Consulta metadatos de una sesión de navegación propia por su identificador. El alcance viene de esta sesión de voz."),
        ("world_navigation_stop", "Detiene una sesión de navegación propia por su identificador. El alcance viene de esta sesión de voz."),
    ] {
        definitions.push(ToolDefinition {
            name: name.into(), description: description.into(),
            parameters: json!({"type":"object","additionalProperties":false,
                "properties":{"id":{"type":"string","pattern":"^[0-9a-fA-F]{32}$"}},"required":["id"]}),
        });
    }
    definitions
}

/// Parse strict tool arguments before dispatch. Capture scopes are explicit;
/// navigation scope is supplied only by the host-selected WorldVoiceScope.
fn world_command(
    name: &str,
    raw: &str,
) -> Result<archie_worldd::exploration::ExploreCommand, String> {
    use archie_geofabric::exploration::{
        self, ExplorationDomain, ExplorationLayer, ExplorationRequest,
    };
    use archie_worldd::exploration::ExploreCommand;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Region {
        latitude: f64,
        longitude: f64,
        radius_m: f64,
        domain: ExplorationDomain,
        zoom: Option<u8>,
        layers: Option<Vec<ExplorationLayer>>,
        imagery_sources: Option<Vec<exploration::ImagerySource>>,
        imagery_date: Option<String>,
        signal_updates: Option<u32>,
        signal_interval_seconds: Option<u32>,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Scope {
        domain: ExplorationDomain,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Place {
        query: String,
        domain: ExplorationDomain,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Job {
        id: String,
        domain: ExplorationDomain,
    }
    #[derive(serde::Deserialize, Default)]
    #[serde(deny_unknown_fields)]
    struct Navigation {
        sources: Option<Vec<exploration::ImagerySource>>,
        imagery_date: Option<String>,
        max_zoom: Option<u8>,
        max_visible_tiles: Option<u32>,
        max_requests: Option<u32>,
        max_bytes: Option<u64>,
        duration_seconds: Option<u32>,
        signals: Option<bool>,
        photogrammetry: Option<bool>,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NavigationId {
        id: String,
    }
    let invalid =
        |_| "Faltan argumentos explícitos o hay un dominio o valor no válido.".to_string();
    match name {
        "world_navigate" => {
            let args: Navigation = serde_json::from_str(raw).map_err(invalid)?;
            let defaults = archie_worldd::navigation::NavigationRequest::default();
            let request = archie_worldd::navigation::NavigationRequest {
                sources: args.sources.unwrap_or(defaults.sources),
                imagery_date: args.imagery_date,
                max_zoom: args.max_zoom.unwrap_or(defaults.max_zoom),
                max_visible_tiles: args.max_visible_tiles.unwrap_or(defaults.max_visible_tiles),
                max_requests: args.max_requests.unwrap_or(defaults.max_requests),
                max_bytes: args.max_bytes.unwrap_or(defaults.max_bytes),
                duration_seconds: args.duration_seconds.unwrap_or(defaults.duration_seconds),
                signals: args.signals.unwrap_or(defaults.signals),
                photogrammetry: args.photogrammetry.unwrap_or(defaults.photogrammetry),
                // This placeholder is replaced only from WorldVoiceScope below.
                domain: ExplorationDomain::Personal,
            };
            request.validate()?;
            Ok(ExploreCommand::Navigate { request })
        }
        "world_navigation_status" | "world_navigation_stop" => {
            let args: NavigationId = serde_json::from_str(raw).map_err(invalid)?;
            if args.id.len() != 32 || !args.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("Identificador de navegación no válido.".into());
            }
            Ok(if name == "world_navigation_status" {
                ExploreCommand::NavigationStatus {
                    id: args.id,
                    domain: ExplorationDomain::Personal,
                }
            } else {
                ExploreCommand::NavigationStop {
                    id: args.id,
                    domain: ExplorationDomain::Personal,
                }
            })
        }
        "world_explore" | "world_plan" => {
            let region: Region = serde_json::from_str(raw).map_err(invalid)?;
            let request = ExplorationRequest {
                latitude: region.latitude,
                longitude: region.longitude,
                radius_m: region.radius_m,
                domain: region.domain,
                zoom: region.zoom.unwrap_or(14),
                layers: region
                    .layers
                    .unwrap_or_else(|| vec![ExplorationLayer::Terrain]),
                imagery_sources: region.imagery_sources.unwrap_or_default(),
                imagery_date: region.imagery_date,
                signal_updates: region.signal_updates.unwrap_or(1),
                signal_interval_seconds: region.signal_interval_seconds.unwrap_or(60),
                ..ExplorationRequest::default()
            };
            exploration::plan(&request).map_err(|error| error.to_string())?;
            Ok(if name == "world_plan" {
                ExploreCommand::Plan { request }
            } else {
                ExploreCommand::Explore { request }
            })
        }
        "world_pause" | "world_resume" | "world_cancel" => {
            let job: Job = serde_json::from_str(raw).map_err(invalid)?;
            if job.id.len() != 32 || !job.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("El identificador de captura debe tener treinta y dos caracteres hexadecimales.".into());
            }
            Ok(match name {
                "world_pause" => ExploreCommand::Pause {
                    id: job.id,
                    domain: job.domain,
                },
                "world_resume" => ExploreCommand::Resume {
                    id: job.id,
                    domain: job.domain,
                },
                _ => ExploreCommand::Cancel {
                    id: job.id,
                    domain: job.domain,
                },
            })
        }
        "world_status" => {
            let scope: Scope = serde_json::from_str(raw).map_err(invalid)?;
            Ok(ExploreCommand::Status {
                domain: scope.domain,
            })
        }
        "world_resolve" => {
            let place: Place = serde_json::from_str(raw).map_err(invalid)?;
            if place.query.trim().is_empty() || place.query.chars().count() > 256 {
                return Err(
                    "El lugar debe tener entre uno y doscientos cincuenta y seis caracteres."
                        .into(),
                );
            }
            Ok(ExploreCommand::Resolve {
                query: place.query,
                domain: place.domain,
            })
        }
        _ => Err("Herramienta geográfica desconocida.".into()),
    }
}

/// Host-selected session scope; neither field is read from model tool arguments.
#[derive(Clone)]
pub struct WorldVoiceScope {
    pub domain: archie_vault::ContextDomain,
    pub privacy: archie_types::PrivacyGrade,
}

fn scoped_world_request(
    scope: &WorldVoiceScope,
    mut command: archie_worldd::exploration::ExploreCommand,
) -> Result<archie_core::control::ControlRequest, String> {
    use archie_core::control::{ControlRequest, WorldJobAction};
    use archie_geofabric::exploration::ExplorationDomain;
    use archie_worldd::exploration::ExploreCommand;
    let expected = if scope.domain.is_work() {
        ExplorationDomain::Work
    } else {
        ExplorationDomain::Personal
    };
    // New navigation tools have no model-supplied scope fields. Bind their
    // geographic DTO solely to the complete host-selected context.
    match &mut command {
        ExploreCommand::Navigate { request } => request.domain = expected,
        ExploreCommand::NavigationStatus { domain, .. }
        | ExploreCommand::NavigationStop { domain, .. } => *domain = expected,
        _ => {}
    }
    let requested = match &command {
        ExploreCommand::Navigate { request } => request.domain,
        ExploreCommand::NavigationStatus { domain, .. }
        | ExploreCommand::NavigationStop { domain, .. } => *domain,
        ExploreCommand::Explore { request } | ExploreCommand::Plan { request } => request.domain,
        ExploreCommand::Resolve { domain, .. } | ExploreCommand::Status { domain } => *domain,
        ExploreCommand::Pause { domain, .. }
        | ExploreCommand::Resume { domain, .. }
        | ExploreCommand::Cancel { domain, .. } => *domain,
    };
    if requested != expected {
        return Err(
            "El dominio pedido no coincide con el dominio explícito de esta sesión.".into(),
        );
    }
    let domain = scope.domain.clone();
    let privacy = scope.privacy;
    Ok(match command {
        ExploreCommand::Navigate { request } => ControlRequest::WorldNavigate {
            request,
            domain,
            privacy,
        },
        ExploreCommand::NavigationStatus { id, .. } => ControlRequest::WorldNavigationStatus {
            id,
            domain,
            privacy,
        },
        ExploreCommand::NavigationStop { id, .. } => ControlRequest::WorldNavigationStop {
            id,
            domain,
            privacy,
        },
        ExploreCommand::Explore { request } => ControlRequest::WorldExplore {
            request,
            domain,
            privacy,
        },
        ExploreCommand::Plan { request } => ControlRequest::WorldPlan {
            request,
            domain,
            privacy,
        },
        ExploreCommand::Resolve { query, .. } => ControlRequest::WorldResolve {
            query,
            domain,
            privacy,
        },
        ExploreCommand::Status { .. } => ControlRequest::WorldStatus { domain, privacy },
        ExploreCommand::Pause { id, .. } => ControlRequest::WorldJob {
            action: WorldJobAction::Pause,
            id,
            domain,
            privacy,
        },
        ExploreCommand::Resume { id, .. } => ControlRequest::WorldJob {
            action: WorldJobAction::Resume,
            id,
            domain,
            privacy,
        },
        ExploreCommand::Cancel { id, .. } => ControlRequest::WorldJob {
            action: WorldJobAction::Cancel,
            id,
            domain,
            privacy,
        },
    })
}

async fn world_tool(scope: Option<&WorldVoiceScope>, name: &str, raw: &str) -> String {
    let Some(scope) = scope else {
        return "No envié la operación: inicia esta sesión con --world-domain personal/owner o work/organización/proyecto y un grado explícito. Core debe autorizarla.".into();
    };
    let command = match world_command(name, raw) {
        Ok(command) => command,
        Err(error) => return format!("No envié la operación geográfica: {error}"),
    };
    let request = match scoped_world_request(scope, command) {
        Ok(request) => request,
        Err(error) => return format!("No envié la operación geográfica: {error}"),
    };
    match archie_core::world_control::send(request).await {
        Err(error) => format!("Core no confirmó la operación geográfica: {error}. No se reintentó."),
        Ok(value) if matches!(name, "world_navigate" | "world_navigation_status" | "world_navigation_stop") => format!(
            "Navegación {}. Estado {}. Solicitudes observadas {}. Bytes observados {}.",
            value["navigation"]["id"].as_str().unwrap_or("sin identificador"),
            value["navigation"]["state"].as_str().unwrap_or("desconocido"),
            value["navigation"]["requests_sent"], value["navigation"]["bytes_received"]),
        Ok(value) if matches!(name, "world_explore" | "world_pause" | "world_resume" | "world_cancel") => format!(
            "Captura aceptada con identificador {}. Estado {}. Esto confirma aceptación, no finalización.",
            value["job"]["id"].as_str().unwrap_or("no informado"),
            value["job"]["state"].as_str().unwrap_or("pendiente")),
        Ok(value) if name == "world_plan" => format!("Plan calculado: {} teselas, radio {} metros. No se inició una captura.",
            crate::world_response::plan_tile_count(&value), value["plan"]["request"]["radius_m"]),
        Ok(value) if name == "world_resolve" => {
            let places = value["places"].as_array().map(|places| places.iter().take(5).cloned().collect::<Vec<_>>()).unwrap_or_default();
            format!("Lugares candidatos, sin captura iniciada: {}. Confirma lugar, radio y dominio antes de explorar.", json!(places))
        },
        Ok(value) => {
            let jobs = value["jobs"].as_array().map(|jobs| jobs.iter().map(|job| json!({
                "id":job["id"],"state":job["state"],"tiles_done":job["tiles_done"],
                "tiles_total":job["tiles_total"],"bytes_received":job["bytes_received"],"error":job["error"]
            })).collect::<Vec<_>>()).unwrap_or_default();
            format!("Estado observado de worldd, dominio {}: {}.", value.get("context_domain").unwrap_or(&value["domain"]), json!(jobs))
        },
    }
}

#[cfg(test)]
mod tool_tests {
    #[test]
    fn photogrammetry_tool_toggle_is_optional_typed_and_cannot_add_authority() {
        for (raw, expected) in [
            ("{}", true),
            (r#"{"photogrammetry":true}"#, true),
            (r#"{"photogrammetry":false}"#, false),
        ] {
            let command = super::world_command("world_navigate", raw).unwrap();
            let value = serde_json::to_value(command).unwrap();
            assert_eq!(value["request"]["photogrammetry"], expected);
        }
        for raw in [
            r#"{"photogrammetry":"true"}"#,
            r#"{"photogrammetry":true,"host":"kh.google.com"}"#,
            r#"{"photogrammetry":true,"domain":"work"}"#,
            r#"{"photogrammetry":true,"provider":"google_photorealistic"}"#,
        ] {
            assert!(super::world_command("world_navigate", raw).is_err());
        }
        let definition = super::tools(None)
            .into_iter()
            .find(|definition| definition.name == "world_navigate")
            .unwrap();
        assert_eq!(
            definition.parameters["properties"]["photogrammetry"]["type"],
            "boolean"
        );
    }
    #[test]
    fn navigation_tools_use_only_the_host_scope_and_reject_injected_authority() {
        use archie_core::control::ControlRequest;
        let scope = super::WorldVoiceScope {
            domain: archie_vault::ContextDomain::work("org", "project").unwrap(),
            privacy: archie_types::PrivacyGrade::Public,
        };
        for (name, raw) in [
            ("world_navigate", "{}".to_string()),
            (
                "world_navigation_status",
                format!(r#"{{"id":"{}"}}"#, "a".repeat(32)),
            ),
            (
                "world_navigation_stop",
                format!(r#"{{"id":"{}"}}"#, "a".repeat(32)),
            ),
        ] {
            let command = super::world_command(name, &raw).unwrap();
            let request = super::scoped_world_request(&scope, command).unwrap();
            let value = serde_json::to_value(&request).unwrap();
            assert_eq!(value["domain"], "work/org/project");
            assert_eq!(value["privacy"], "public");
            assert!(request.is_world());
            for field in ["domain", "privacy", "endpoint", "token"] {
                let mut injected: serde_json::Value = serde_json::from_str(&raw).unwrap();
                injected[field] = serde_json::json!("personal");
                assert!(super::world_command(name, &injected.to_string()).is_err());
            }
            assert!(matches!(
                request,
                ControlRequest::WorldNavigate { .. }
                    | ControlRequest::WorldNavigationStatus { .. }
                    | ControlRequest::WorldNavigationStop { .. }
            ));
        }
        assert!(super::world_command("world_navigation_stop", r#"{"id":"../escape"}"#).is_err());
    }

    use super::*;

    #[test]
    fn navigation_tool_schemas_expose_limits_but_no_model_scope_or_endpoint() {
        let definitions = super::tools(None);
        for name in [
            "world_navigate",
            "world_navigation_status",
            "world_navigation_stop",
        ] {
            let definition = definitions
                .iter()
                .find(|definition| definition.name == name)
                .unwrap();
            assert_eq!(definition.parameters["additionalProperties"], false);
            for field in ["domain", "privacy", "token", "endpoint"] {
                assert!(definition.parameters["properties"].get(field).is_none());
            }
        }
        for raw in [
            r#"{"max_requests":0}"#,
            r#"{"duration_seconds":3601}"#,
            r#"{"sources":["unknown"]}"#,
            r#"{"signals":"true"}"#,
        ] {
            assert!(super::world_command("world_navigate", raw).is_err());
        }
    }

    #[test]
    fn world_plan_and_job_controls_are_strict_and_keep_host_scope() {
        let scope = WorldVoiceScope {
            domain: archie_vault::ContextDomain::work("org", "project").unwrap(),
            privacy: archie_types::PrivacyGrade::Public,
        };
        let plan = world_command(
            "world_plan",
            r#"{"domain":"work","latitude":1,"longitude":2,"radius_m":100}"#,
        )
        .unwrap();
        assert!(matches!(
            scoped_world_request(&scope, plan).unwrap(),
            archie_core::control::ControlRequest::WorldPlan { .. }
        ));
        for (name, expected) in [
            ("world_pause", archie_core::control::WorldJobAction::Pause),
            ("world_resume", archie_core::control::WorldJobAction::Resume),
            ("world_cancel", archie_core::control::WorldJobAction::Cancel),
        ] {
            let command = world_command(
                name,
                &json!({"id":"a".repeat(32),"domain":"work"}).to_string(),
            )
            .unwrap();
            let request = scoped_world_request(&scope, command).unwrap();
            assert!(request.is_mutating());
            let archie_core::control::ControlRequest::WorldJob {
                action,
                id,
                domain,
                privacy,
            } = request
            else {
                panic!("typed job control");
            };
            assert_eq!(action, expected);
            assert_eq!(id, "a".repeat(32));
            assert_eq!(domain.path(), "work/org/project");
            assert_eq!(privacy, archie_types::PrivacyGrade::Public);
            assert!(world_command(name, r#"{"domain":"work"}"#).is_err());
            assert!(world_command(name, r#"{"domain":"work","id":"../bad"}"#).is_err());
            assert!(world_command(
                name,
                &json!({"id":"a".repeat(32),"domain":"work","privacy":"public"}).to_string()
            )
            .is_err());
            let foreign = world_command(
                name,
                &json!({"id":"a".repeat(32),"domain":"personal"}).to_string(),
            )
            .unwrap();
            assert!(scoped_world_request(&scope, foreign).is_err());
        }
    }

    #[test]
    fn model_domain_cannot_widen_host_domain_or_privacy() {
        let scope = WorldVoiceScope {
            domain: archie_vault::ContextDomain::work("org", "project").unwrap(),
            privacy: archie_types::PrivacyGrade::Restricted,
        };
        let personal = world_command("world_status", r#"{"domain":"personal"}"#).unwrap();
        assert!(scoped_world_request(&scope, personal).is_err());
        let work = world_command("world_status", r#"{"domain":"work"}"#).unwrap();
        let archie_core::control::ControlRequest::WorldStatus { domain, privacy } =
            scoped_world_request(&scope, work).unwrap()
        else {
            panic!("status");
        };
        assert_eq!(domain.path(), "work/org/project");
        assert_eq!(privacy, archie_types::PrivacyGrade::Restricted);
        assert!(world_command(
            "world_resolve",
            r#"{"domain":"work","query":"place","privacy":"public"}"#
        )
        .is_err());
    }

    #[test]
    fn world_tools_require_explicit_domain_and_explore_coordinates() {
        let definitions = tools(None);
        for name in [
            "world_plan",
            "world_explore",
            "world_status",
            "world_resolve",
            "world_pause",
            "world_resume",
            "world_cancel",
        ] {
            let tool = definitions.iter().find(|tool| tool.name == name).unwrap();
            assert!(tool.parameters["required"]
                .as_array()
                .unwrap()
                .contains(&json!("domain")));
            assert_eq!(
                tool.parameters["properties"]["domain"]["enum"],
                json!(["personal", "work"])
            );
        }
        assert!(world_command("world_status", r#"{}"#).is_err());
        assert!(world_command("world_status", r#"{"domain":"corporate"}"#).is_err());
        assert!(world_command(
            "world_explore",
            r#"{"domain":"work","latitude":91,"longitude":0,"radius_m":500}"#
        )
        .is_err());
        assert!(world_command(
            "world_explore",
            r#"{"domain":"work","latitude":0,"longitude":0}"#
        )
        .is_err());
        let command = world_command(
            "world_explore",
            r#"{"domain":"work","latitude":-33,"longitude":-70,"radius_m":500}"#,
        )
        .unwrap();
        let archie_worldd::exploration::ExploreCommand::Explore { request } = command else {
            panic!("explore command");
        };
        assert_eq!(
            request.domain,
            archie_geofabric::exploration::ExplorationDomain::Work
        );
    }

    #[test]
    fn voice_imagery_and_refresh_options_are_typed_and_strict() {
        let args = json!({"domain":"work","latitude":0,"longitude":0,"radius_m":500,
            "layers":["imagery","signals"],"imagery_sources":["google_satellite","blue_marble"],
            "imagery_date":"2024-02-29","signal_updates":2,"signal_interval_seconds":120});
        let archie_worldd::exploration::ExploreCommand::Explore { request } =
            world_command("world_explore", &args.to_string()).unwrap()
        else {
            panic!("explore")
        };
        assert_eq!(request.signal_updates, 2);
        assert_eq!(request.signal_interval_seconds, 120);
        assert_eq!(request.imagery_sources.len(), 2);
        for (field, value) in [
            ("endpoint", json!("https://evil.invalid")),
            ("signal_updates", json!(61)),
            ("imagery_date", json!("2023-02-29")),
        ] {
            let mut invalid = args.clone();
            invalid[field] = value;
            assert!(world_command("world_explore", &invalid.to_string()).is_err());
        }
        let definitions = tools(None);
        let tool = definitions
            .iter()
            .find(|tool| tool.name == "world_explore")
            .unwrap();
        assert_eq!(
            tool.parameters["properties"]["signal_updates"]["maximum"],
            60
        );
    }

    #[test]
    fn display_catalog_controls_scene_and_preset_enums_without_duplicate_tools() {
        let catalog = archie_orb::control::Catalog {
            scenes: vec!["new-scene".into()],
            presets: vec!["new-preset".into()],
        };
        let definitions = tools(Some(&catalog));
        let mut names = std::collections::HashSet::new();
        for definition in &definitions {
            assert!(
                names.insert(definition.name.as_str()),
                "duplicate {}",
                definition.name
            );
        }
        for (name, parameter, expected) in [
            ("observatory_scene", "scene", json!(["new-scene"])),
            ("orb_preset", "preset", json!(["new-preset"])),
        ] {
            let tool = definitions.iter().find(|tool| tool.name == name).unwrap();
            assert_eq!(tool.parameters["properties"][parameter]["enum"], expected);
        }
        for name in ["orb_cinematic", "orb_overlay", "shell_panel"] {
            assert!(names.contains(name));
        }
    }

    #[test]
    fn an_unavailable_display_keeps_scene_and_preset_names_open() {
        let definitions = tools(None);
        for (name, parameter) in [("observatory_scene", "scene"), ("orb_preset", "preset")] {
            let tool = definitions.iter().find(|tool| tool.name == name).unwrap();
            assert_eq!(
                tool.parameters["properties"][parameter],
                json!({"type": "string"})
            );
        }
    }
}

/// What the page is sent.
#[derive(Clone)]
pub enum Outgoing {
    /// Tag byte 1, then PCM16 little-endian at the voice's rate.
    Audio(Arc<Vec<u8>>),
    Text(Arc<str>),
}

/// Everything a live session shares between the page, the eyes and the conversation.
pub struct Senses {
    world_scope: Option<WorldVoiceScope>,
    wanda: crate::wanda::WandaVoice,
    pub scene: Mutex<Scene>,
    /// What IRIS has seen over time, thing by thing.
    pub memory: Mutex<Memory>,
    persistent: Mutex<Option<PersistentMemory>>,
    pub buffer: Mutex<Buffer>,
    pub identifier: Identifier,
    pub researcher: Option<Researcher>,
    audio: mpsc::Sender<Vec<f32>>,
    frame: Mutex<Option<Arc<Vec<u8>>>>,
    recent_scenes: Mutex<VecDeque<Scene>>,
    frame_ready: Notify,
    out: broadcast::Sender<Outgoing>,
    /// Looks closer that curiosity may still take this session: each is a paid call.
    curiosity_left: AtomicU32,
    last_look: Mutex<Option<Instant>>,
    voice_rate: u32,
    /// What the person said last, so the model is told which language to answer in.
    last_heard: Mutex<String>,
    /// The page whose microphone the ears hear: the newest. Two pages streaming into the
    /// one ear interleave their frames, and the recogniser hears noise at twice the speed.
    microphone_owner: AtomicU64,
    pages: AtomicU64,
}

impl Senses {
    pub fn new(
        audio: mpsc::Sender<Vec<f32>>,
        identifier: Identifier,
        researcher: Option<Researcher>,
        voice_rate: u32,
        curiosity_budget: u32,
    ) -> Arc<Self> {
        Self::new_with_wanda(
            audio,
            identifier,
            researcher,
            voice_rate,
            curiosity_budget,
            crate::wanda::WandaVoiceAccess::default(),
        )
    }

    pub fn new_with_wanda(
        audio: mpsc::Sender<Vec<f32>>,
        identifier: Identifier,
        researcher: Option<Researcher>,
        voice_rate: u32,
        curiosity_budget: u32,
        access: crate::wanda::WandaVoiceAccess,
    ) -> Arc<Self> {
        Self::new_with_world(
            audio,
            identifier,
            researcher,
            voice_rate,
            curiosity_budget,
            access,
            None,
        )
    }

    pub fn new_with_world(
        audio: mpsc::Sender<Vec<f32>>,
        identifier: Identifier,
        researcher: Option<Researcher>,
        voice_rate: u32,
        curiosity_budget: u32,
        access: crate::wanda::WandaVoiceAccess,
        world_scope: Option<WorldVoiceScope>,
    ) -> Arc<Self> {
        let (out, _) = broadcast::channel(512);
        let persistent = match PersistentMemory::open_default() {
            Ok(memory) => Some(memory),
            Err(error) => {
                eprintln!("personal memory unavailable: {error}");
                None
            }
        };
        Arc::new(Self {
            world_scope,
            wanda: crate::wanda::WandaVoice::from_env(access),
            scene: Mutex::new(Scene::default()),
            memory: Mutex::new(Memory::default()),
            persistent: Mutex::new(persistent),
            buffer: Mutex::new(Buffer::default()),
            identifier,
            researcher,
            audio,
            frame: Mutex::new(None),
            recent_scenes: Mutex::new(VecDeque::new()),
            frame_ready: Notify::new(),
            out,
            curiosity_left: AtomicU32::new(curiosity_budget),
            last_look: Mutex::new(None),
            voice_rate,
            last_heard: Mutex::new(String::new()),
            microphone_owner: AtomicU64::new(0),
            pages: AtomicU64::new(0),
        })
    }

    /// Remembers what was just heard (before the turn that answers it asks for context).
    pub fn heard(&self, text: &str) {
        *self.last_heard.lock().expect("heard lock") = text.to_string();
        if let Some(memory) = self
            .persistent
            .lock()
            .expect("persistent memory lock")
            .as_mut()
        {
            if let Err(error) = memory.record_utterance(text, unix_ms()) {
                eprintln!("personal memory write failed: {error}");
            }
        }
    }

    /// Local OCR and notification adapters can send source-attributed text here.
    pub fn captured_text(&self, source: &str, text: &str, at_ms: i64, durable: bool) {
        if let Some(memory) = self
            .persistent
            .lock()
            .expect("persistent memory lock")
            .as_mut()
        {
            if let Err(error) = memory.record_external_text(source, text, at_ms, durable) {
                eprintln!("external text memory write failed: {error}");
            }
        }
    }

    pub fn publish(&self, value: serde_json::Value) {
        let _ = self.out.send(Outgoing::Text(value.to_string().into()));
    }

    /// What the model should know this turn: the clock, what the camera shows now and
    /// what it showed before, and what was found out and not yet said.
    pub fn context(&self, clock: &str) -> String {
        let scene = {
            let scene = self.scene.lock().expect("scene lock");
            self.memory
                .lock()
                .expect("memory lock")
                .describe(&scene, Instant::now())
        };
        let known = self.buffer.lock().expect("buffer lock").describe();
        let heard = self.last_heard.lock().expect("heard lock").clone();
        let remembered = self
            .persistent
            .lock()
            .expect("persistent memory lock")
            .as_ref()
            .map(|memory| memory.context(unix_ms(), &heard))
            .unwrap_or_default();
        // Last, where a small model weighs it most, and in the language it asks for.
        let answer_in = match language::spoken_language(&heard) {
            "en" => "IMPORTANT: the person just spoke English. Reply only in English, even though these instructions are in Spanish.",
            _ => "La persona te habló en español: responde en español.",
        };
        let memory_policy = "Memoria personal: usa remember en T1 para fallecimientos, cambios familiares, salud y hechos estables; T3 para planes de la semana; T4 para contexto del día y deseos puntuales, que caducan a los pocos días. No guardes cada frase ni conviertas algo dicho por alguien en un hecho verificado.";
        [
            clock.to_string(),
            scene,
            remembered,
            known,
            memory_policy.to_string(),
            answer_in.to_string(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
    }

    /// Carries out a live tool; `None` when the tool is not one of them.
    pub async fn handle_tool(&self, name: &str, raw_arguments: &str) -> Option<String> {
        if matches!(
            name,
            "world_plan"
                | "world_explore"
                | "world_status"
                | "world_resolve"
                | "world_pause"
                | "world_resume"
                | "world_cancel"
                | "world_navigate"
                | "world_navigation_status"
                | "world_navigation_stop"
        ) {
            return Some(world_tool(self.world_scope.as_ref(), name, raw_arguments).await);
        }
        if name == "wanda_context" {
            return Some(self.wanda.handle_tool(raw_arguments).await);
        }
        let arguments: serde_json::Value = serde_json::from_str(raw_arguments).unwrap_or_default();
        match name {
            "identify" => Some(
                self.identify(arguments["what"].as_str().unwrap_or(""))
                    .await,
            ),
            "web_search" => Some(
                self.web_search(arguments["query"].as_str().unwrap_or(""))
                    .await,
            ),
            "remember" => Some(self.remember(
                arguments["text"].as_str().unwrap_or(""),
                arguments["tier"].as_str().unwrap_or("t4"),
            )),
            "observatory_scene" => {
                Some(visual_command("scene", arguments["scene"].as_str().unwrap_or(""), None).await)
            }
            "orb_preset" => Some(
                visual_command("preset", arguments["preset"].as_str().unwrap_or(""), None).await,
            ),
            "orb_cinematic" => Some(
                visual_command(
                    "cinematic",
                    arguments["profile"].as_str().unwrap_or(""),
                    None,
                )
                .await,
            ),
            "shell_panel" => {
                Some(visual_command("panel", arguments["panel"].as_str().unwrap_or(""), None).await)
            }
            "orb_overlay" => Some(
                visual_command(
                    "overlay",
                    arguments["overlay"].as_str().unwrap_or(""),
                    match arguments["state"].as_str() {
                        Some("on") => Some(true),
                        Some("off") => Some(false),
                        _ => None,
                    },
                )
                .await,
            ),
            _ => None,
        }
    }

    fn remember(&self, text: &str, tier: &str) -> String {
        let tier = match tier {
            "t1" => persistent::Tier::T1,
            "t2" => persistent::Tier::T2,
            "t3" => persistent::Tier::T3,
            "t4" => persistent::Tier::T4,
            _ => return "No guardé el recuerdo: el nivel no es válido.".into(),
        };
        let mut memory = self.persistent.lock().expect("persistent memory lock");
        let Some(memory) = memory.as_mut() else {
            return "No pude guardar el recuerdo porque la memoria local no está disponible."
                .into();
        };
        match memory.classify_utterance(text, tier, unix_ms()) {
            Ok(()) => "Recuerdo guardado con su hora y como frase reportada por la persona.".into(),
            Err(error) => format!("No pude guardar el recuerdo: {error}"),
        }
    }

    async fn identify(&self, what: &str) -> String {
        let scene = self.scene.lock().expect("scene lock").clone();
        if scene
            .at
            .is_none_or(|at| at.elapsed() > Duration::from_secs(10))
        {
            let evidence = self
                .persistent
                .lock()
                .expect("persistent memory lock")
                .as_ref()
                .and_then(|memory| memory.visual_evidence(what));
            let Some((event_id, category, clip)) = evidence else {
                return "No me está llegando imagen nueva, y no tengo un recorte visual reciente que coincida.".into();
            };
            let mut result = None;
            let mut failure = None;
            for frame in &clip {
                match self.identifier.identify(frame, &category).await {
                    Ok(Some(identified)) => {
                        result = Some(identified);
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => failure = Some(error),
                }
            }
            return match (result, failure) {
                (Some(identified), _) => {
                    if let Some(memory) = self.persistent.lock().expect("persistent memory lock").as_mut() {
                        if let Err(error) = memory.refine_visual(event_id, &identified.name, &identified.model) {
                            eprintln!("visual memory refinement failed: {error}");
                        }
                    }
                    format!("En los recortes que IRIS guardó cuando detectó una mano sosteniendo {category}, el modelo de visión {} interpreta que era {}. Es una identificación visual y puede equivocarse.", identified.model, identified.name)
                }
                (None, Some(error)) => format!("No pude volver a analizar los recortes guardados: {error}"),
                (None, None) => format!("IRIS guardó la categoría {category}, pero no hay una identificación más específica fiable en esos recortes."),
            };
        }
        let Some(seen) = eyes::pick(&scene, what).cloned() else {
            return "No veo ningún objeto claro frente a la cámara (a las personas no las identifico).".into();
        };
        if let Some(known) = self
            .buffer
            .lock()
            .expect("buffer lock")
            .entries()
            .find(|entry| entry.seen_as == seen.phrase && curiosity_overlap(entry, &seen))
            .cloned()
        {
            return spoken(&known);
        }
        match curiosity::look_closer(&scene, &seen, &self.identifier, self.researcher.as_ref())
            .await
        {
            Ok(known) => {
                let told = spoken(&known);
                self.publish(json!({"type": "known", "known": &known}));
                self.buffer.lock().expect("buffer lock").remember(known);
                told
            }
            Err(error) => format!("No pude mirarlo de cerca: {error}"),
        }
    }

    async fn web_search(&self, query: &str) -> String {
        let Some(researcher) = &self.researcher else {
            return "No tengo búsqueda web configurada (falta TAVILY_API_KEY).".into();
        };
        if query.trim().is_empty() {
            return "No se dijo qué buscar.".into();
        }
        match researcher.search(query).await {
            Ok(results) if !results.is_empty() => {
                let titles: Vec<String> =
                    results.iter().map(|(title, _, _)| title.clone()).collect();
                format!(
                    "Fuentes: {}. Dicen: {}",
                    titles.join("; "),
                    curiosity::facts_from(&results)
                )
            }
            Ok(_) => "La búsqueda no devolvió nada.".into(),
            Err(error) => format!("La búsqueda falló: {error}"),
        }
    }
}

fn curiosity_overlap(known: &curiosity::Known, seen: &eyes::Seen) -> bool {
    let a = known.bbox;
    let b = seen.bbox;
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    w * h > 0.0
}

fn spoken(known: &curiosity::Known) -> String {
    let by = match known.by.as_str() {
        "vision-model" => "según el modelo de visión de Nebius, que puede equivocarse",
        _ => "IRIS solo sabe la categoría",
    };
    let mut told = format!("Es {} ({by}).", known.name);
    if !known.facts.is_empty() {
        told.push_str(&format!(" Datos: {}", known.facts));
    }
    told
}

/// A moment of the conversation, as the page is told it.
pub fn moment_json(moment: &Moment) -> serde_json::Value {
    match moment {
        Moment::Timing {
            turn_id,
            phase,
            elapsed_ms,
            queued_segments,
        } => {
            json!({"type":"timing", "turn_id":turn_id, "phase":phase, "elapsed_ms":elapsed_ms, "queued_segments":queued_segments})
        }
        Moment::Heard { text, .. } => json!({"type": "heard", "text": text}),
        Moment::Floor { action, reason, .. } => {
            json!({"type": "floor", "action": action, "reason": reason})
        }
        Moment::Said {
            text,
            first_audio_ms,
            stopped,
            ..
        } => {
            json!({"type": "said", "text": text, "first_audio_ms": first_audio_ms, "stopped": stopped})
        }
        Moment::Speaking { text } => json!({"type": "speaking", "text": text}),
        Moment::Proposed { tool, arguments } => {
            json!({"type": "tool", "tool": tool, "arguments": arguments})
        }
        Moment::Did { report } => json!({"type": "did", "report": report}),
        Moment::Interrupted => json!({"type": "interrupted"}),
        Moment::Alert(message) => json!({"type": "alert", "text": message}),
        Moment::Level { level, voiced } => {
            json!({"type": "level", "level": level, "voiced": voiced})
        }
        Moment::Ready => json!({"type": "ready"}),
    }
}

/// Carries the voice to the page: a thread, because the conversation's voice is a
/// `std` channel and the page's socket lives on the runtime.
pub fn forward_voice(
    voice: mpsc::Receiver<VoiceOut>,
    senses: Arc<Senses>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for out in voice {
            match out {
                VoiceOut::Audio(pcm) => {
                    let mut bytes = Vec::with_capacity(1 + pcm.len() * 2);
                    bytes.push(1u8);
                    for sample in pcm {
                        bytes.extend_from_slice(&sample.to_le_bytes());
                    }
                    let _ = senses.out.send(Outgoing::Audio(Arc::new(bytes)));
                }
                VoiceOut::Hush => senses.publish(json!({"type": "hush"})),
            }
        }
    })
}

/// Serves the page and its socket on `127.0.0.1:port`, and runs the eyes and curiosity.
pub async fn serve(
    senses: Arc<Senses>,
    eyes: IrisEyes,
    sweep: Sweep,
    port: u16,
) -> Result<(), String> {
    tokio::spawn(see(senses.clone(), eyes, sweep));
    tokio::spawn(wonder(senses.clone()));
    tokio::spawn(compact_memory(senses.clone()));
    let app = Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route("/ws", get(upgrade))
        .with_state(senses);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("binding 127.0.0.1:{port}: {e}"))?;
    axum::serve(listener, app).await.map_err(|e| e.to_string())
}

const PAGE: &str = include_str!("page.html");

async fn compact_memory(senses: Arc<Senses>) {
    let start = tokio::time::Instant::now() + Duration::from_secs(60 * 60);
    let mut tick = tokio::time::interval_at(start, Duration::from_secs(60 * 60));
    loop {
        tick.tick().await;
        if let Some(memory) = senses
            .persistent
            .lock()
            .expect("persistent memory lock")
            .as_mut()
        {
            if let Err(error) = memory.compact_now() {
                eprintln!("personal memory compaction failed: {error}");
            }
        }
    }
}

async fn upgrade(ws: WebSocketUpgrade, State(senses): State<Arc<Senses>>) -> impl IntoResponse {
    ws.max_message_size(4 << 20)
        .on_upgrade(move |socket| session(socket, senses))
}

async fn session(socket: WebSocket, senses: Arc<Senses>) {
    let page = senses.pages.fetch_add(1, Ordering::SeqCst) + 1;
    senses.microphone_owner.store(page, Ordering::SeqCst);
    senses.publish(json!({"type": "microphone_owner", "page": page}));
    let (mut sink, mut stream) = socket.split();
    let mut outgoing = senses.out.subscribe();
    let hello = json!({
        "type": "hello",
        "page": page,
        "voice_rate": senses.voice_rate,
        "identify": senses.identifier.means(),
        "search": senses.researcher.is_some(),
    });
    if sink.send(Message::Text(hello.to_string())).await.is_err() {
        return;
    }
    let writer = tokio::spawn(async move {
        loop {
            let message = match outgoing.recv().await {
                Ok(Outgoing::Audio(bytes)) => Message::Binary(bytes.as_ref().clone()),
                Ok(Outgoing::Text(text)) => Message::Text(text.to_string()),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });
    // What the microphone actually delivers: a page resampling wrong sends sound the ears
    // cannot read, and the level meter alone does not show it.
    let mut counted = (Instant::now(), 0usize, 0f32);
    while let Some(Ok(message)) = stream.next().await {
        if counted.0.elapsed() >= Duration::from_secs(5) && counted.1 > 0 {
            let rate = counted.1 as f32 / counted.0.elapsed().as_secs_f32();
            eprintln!(
                "microphone: {rate:.0} samples/s (the ears want 16000), peak {:.3}",
                counted.2
            );
            senses
                .publish(json!({"type": "mic", "samples_per_s": rate.round(), "peak": counted.2}));
            counted = (Instant::now(), 0, 0.0);
        }
        match message {
            Message::Binary(data) if data.len() > 1 => match data[0] {
                // Microphone: f32 little-endian at 16 kHz, in 20 ms frames for the loop.
                1 if senses.microphone_owner.load(Ordering::SeqCst) != page => {}
                1 => {
                    let samples: Vec<f32> = data[1..]
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    counted.1 += samples.len();
                    counted.2 = samples.iter().fold(counted.2, |peak, s| peak.max(s.abs()));
                    for frame in samples.chunks(crate::audio::FRAME) {
                        let _ = senses.audio.send(frame.to_vec());
                    }
                }
                // Camera: one JPEG, replacing whatever the eyes have not read yet.
                2 => {
                    *senses.frame.lock().expect("frame lock") = Some(Arc::new(data[1..].to_vec()));
                    senses.frame_ready.notify_one();
                }
                _ => {}
            },
            Message::Close(_) => break,
            _ => {}
        }
    }
    writer.abort();
}

/// Reads each new frame with IRIS, one at a time (a frame that arrives while IRIS is busy
/// replaces the one waiting), and publishes the scene. Open looks and focused ones
/// alternate as [`Sweep`] decides.
async fn see(senses: Arc<Senses>, eyes: IrisEyes, mut sweep: Sweep) {
    loop {
        senses.frame_ready.notified().await;
        let Some(frame) = senses.frame.lock().expect("frame lock").take() else {
            continue;
        };
        let asked = Instant::now();
        let look = sweep.next(asked);
        match eyes.look(frame, &look).await {
            Ok(scene) => {
                sweep.observe(&look, &scene, Instant::now());
                let clip_scenes = senses.cache_scene(&scene);
                let (seen, recalled, held_events) = {
                    let mut memory = senses.memory.lock().expect("memory lock");
                    memory.update(&scene);
                    let seen: Vec<serde_json::Value> = scene
                        .seen
                        .iter()
                        .enumerate()
                        .map(|(index, item)| {
                            json!({
                                "phrase": item.phrase, "score": item.score, "box": item.bbox,
                                "id": memory.id_of(index),
                            })
                        })
                        .collect();
                    let recalled = memory.recall(Instant::now());
                    let held_events = memory.take_held_events();
                    (seen, recalled, held_events)
                };
                if let Some(memory) = senses
                    .persistent
                    .lock()
                    .expect("persistent memory lock")
                    .as_mut()
                {
                    for held in &held_events {
                        if let Err(error) = memory.record_held_frames(held, &clip_scenes) {
                            eprintln!("visual memory write failed: {error}");
                        }
                    }
                }
                senses.publish(json!({
                    "type": "scene", "seen": seen, "width": scene.width,
                    "height": scene.height, "step": scene.step,
                    "iris_ms": asked.elapsed().as_millis() as u64, "iris_look": look.name(),
                    "memory": recalled,
                }));
                *senses.scene.lock().expect("scene lock") = scene;
            }
            Err(error) => senses.publish(json!({"type": "alert", "text": error})),
        }
    }
}

impl Senses {
    fn cache_scene(&self, scene: &Scene) -> Vec<Scene> {
        const MAX_SCENES: usize = 4;
        const MAX_BYTES: usize = 8 * 1024 * 1024;
        const SAMPLE_MS: i64 = 250;
        const WINDOW_MS: i64 = 2_000;
        let Some(at_ms) = scene.observed_at_ms else {
            return vec![scene.clone()];
        };
        let mut recent = self.recent_scenes.lock().expect("recent scenes lock");
        let should_sample = recent.back().is_none_or(|last| {
            last.observed_at_ms
                .is_none_or(|previous| at_ms.saturating_sub(previous) >= SAMPLE_MS)
        });
        if should_sample {
            recent.push_back(scene.clone());
        }
        while recent.len() > MAX_SCENES
            || recent
                .iter()
                .map(|sample| sample.frame.len())
                .sum::<usize>()
                > MAX_BYTES
        {
            recent.pop_front();
        }
        let mut clip: Vec<Scene> = recent
            .iter()
            .filter(|sample| {
                sample
                    .observed_at_ms
                    .is_some_and(|sample_at| at_ms.saturating_sub(sample_at) <= WINDOW_MS)
            })
            .cloned()
            .collect();
        if clip
            .last()
            .is_none_or(|last| last.observed_at_ms != scene.observed_at_ms)
        {
            clip.push(scene.clone());
        }
        clip
    }
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// Every few seconds, looks closer at the most prominent object it does not know yet.
async fn wonder(senses: Arc<Senses>) {
    if senses.identifier.means().is_empty() {
        return; // nothing to look closer with: IRIS's categories are all there is
    }
    let mut tick = tokio::time::interval(Duration::from_secs(4));
    loop {
        tick.tick().await;
        if senses.curiosity_left.load(Ordering::Relaxed) == 0 {
            continue;
        }
        let recent = senses
            .last_look
            .lock()
            .expect("look lock")
            .is_some_and(|at| at.elapsed() < Duration::from_secs(12));
        if recent {
            continue;
        }
        let scene = senses.scene.lock().expect("scene lock").clone();
        if scene
            .at
            .is_none_or(|at| at.elapsed() > Duration::from_secs(3))
        {
            continue;
        }
        let Some(seen) =
            curiosity::next_to_look_at(&scene, &senses.buffer.lock().expect("buffer lock"))
                .cloned()
        else {
            continue;
        };
        *senses.last_look.lock().expect("look lock") = Some(Instant::now());
        senses.curiosity_left.fetch_sub(1, Ordering::Relaxed);
        match curiosity::look_closer(
            &scene,
            &seen,
            &senses.identifier,
            senses.researcher.as_ref(),
        )
        .await
        {
            Ok(known) => {
                senses.publish(json!({"type": "known", "known": &known}));
                senses.buffer.lock().expect("buffer lock").remember(known);
            }
            Err(error) => {
                senses.publish(json!({"type": "alert", "text": format!("curiosidad: {error}")}))
            }
        }
    }
}
