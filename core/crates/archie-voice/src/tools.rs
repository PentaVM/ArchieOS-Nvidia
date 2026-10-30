//! What Archie can be asked to do on the computer, described for the model.
//!
//! Each of these is offered to the cognitive model so it can *propose* it. None of them
//! does anything here: a proposal becomes a [`archie_speech::SpeechToolIntentCandidate`],
//! and whether it happens is decided by Cerebra and authorised by Policy, exactly as with
//! any other intent. The descriptions are written for the model in the language the
//! person speaks, because a model picks tools by reading their descriptions.
//!
//! The set is the stage's own vocabulary and nothing more — open, click, type, press,
//! scroll — so that every proposal maps to an action the stage already checks, leases and
//! can refuse.

use archie_model_runtime::ToolDefinition;
use serde_json::json;

/// The tools that operate the computer.
pub fn os_control() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "open_app".into(),
            description: "Abre una aplicación del sistema por su nombre, por ejemplo \
                          firefox, terminal o el editor de texto. Las apps del escritorio de \
                          ArchieOS también: Orb, World (el globo terráqueo de ArchieOS, no un \
                          sitio web), Vault, Forge, Sentry, Stage y Ajustes."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {"app": {"type": "string", "description": "Nombre de la aplicación."}},
                "required": ["app"]
            }),
        },
        ToolDefinition {
            name: "click".into(),
            description: "Hace clic en algo que se ve en la pantalla, nombrado como aparece: \
                          un botón por su texto (por ejemplo Guardar), un campo por su \
                          etiqueta, un enlace, un menú. Úsalo para seleccionar un campo antes \
                          de escribir en él y para pulsar botones."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {"type": "string", "description": "Lo que hay que tocar, como se ve."},
                    "app": {"type": "string", "description": "En qué aplicación está, si se sabe."}
                },
                "required": ["target"]
            }),
        },
        ToolDefinition {
            name: "type_text".into(),
            description: "Escribe texto con el teclado en el campo que tenga el foco. Si el \
                          campo no está seleccionado, primero haz clic en él con click."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"]
            }),
        },
        ToolDefinition {
            name: "press_keys".into(),
            description: "Pulsa una tecla o una combinación, por ejemplo enter, escape o ctrl+s."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {"keys": {"type": "string"}},
                "required": ["keys"]
            }),
        },
        ToolDefinition {
            name: "open_website".into(),
            description: "Abre una página web en el navegador (open a website, browse the web): \
                          una dirección como wikipedia.org, o una búsqueda en internet. Da url o \
                          search, uno de los dos."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "La dirección, por ejemplo es.wikipedia.org/wiki/Gioconda."},
                    "search": {"type": "string", "description": "Qué buscar en internet, si no hay una dirección."}
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "scroll".into(),
            description: "Desplaza la vista hacia arriba o hacia abajo.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "direction": {"type": "string", "enum": ["up", "down"]},
                    "amount": {"type": "integer", "minimum": 1, "maximum": 20}
                },
                "required": ["direction"]
            }),
        },
    ]
}

/// The tools for drawing a painting in Photopea, by Archie's own hand on the stage.
pub fn drawing() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "draw_picture".into(),
            description: format!(
                "Dibuja un cuadro famoso en Photopea con tu propia mano en la pantalla (draw \
                 a painting): abre Photopea, toma el pincel y lo dibuja línea a línea mientras \
                 cuentas en voz alta lo que haces. Tarda unos minutos y sigue solo; la \
                 conversación continúa. Sabes dibujar {}. El reconocimiento de voz deforma \
                 estos nombres (oye «moaner lisa», «owner leaser», «minner leaser» o «the moon \
                 or Lisa» por «Mona Lisa»): si lo que se oyó suena como uno de ellos, dibújalo \
                 sin preguntar y pásalo con su nombre correcto. Pedir dibujar algo con «Lisa» \
                 es pedir la Mona Lisa.",
                crate::draw::repertoire(crate::draw::Lang::Es)
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "picture": {"type": "string", "description": "El cuadro, como lo dijo la persona: «la Mona Lisa»."},
                    "language": {"type": "string", "enum": ["es", "en"], "description": "El idioma en que habló la persona."}
                },
                "required": ["picture", "language"]
            }),
        },
        ToolDefinition {
            name: "stop_drawing".into(),
            description: "Detiene el dibujo que estás haciendo (stop drawing), entre una tanda \
                          de líneas y la siguiente."
                .into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_a_named_object_with_its_requirements_stated() {
        let tools: Vec<ToolDefinition> = os_control().into_iter().chain(drawing()).collect();
        let mut names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), tools.len(), "no two tools share a name");
        for tool in &tools {
            assert_eq!(tool.parameters["type"], "object", "{}", tool.name);
            // What a tool needs is stated, and only of what it takes.
            let required = tool.parameters["required"].as_array().expect("required");
            for name in required {
                assert!(
                    tool.parameters["properties"][name.as_str().unwrap()].is_object(),
                    "{} needs {name}, which it does not take",
                    tool.name
                );
            }
            assert!(
                tool.description.len() > 20,
                "{} says what it does",
                tool.name
            );
        }
    }
}
