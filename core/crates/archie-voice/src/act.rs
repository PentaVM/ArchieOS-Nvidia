//! From a proposal to an action — or to a reason, said aloud, why not.
//!
//! Models advise; Cerebra decides; Policy authorises. This is the deciding. A tool the
//! model proposed arrives as a name and some text it says is JSON, and before anything
//! touches the stage that text is read strictly: a known tool, only the arguments that
//! tool takes, of the right kinds and within bounds, aimed at an application the stage
//! policy names and allows that kind of touch in. Whatever fails comes back as a refusal
//! with a sentence the voice can say, because a person who asked out loud deserves to
//! hear why nothing happened rather than wonder.
//!
//! Passing here is not permission. The stage's gate checks the same things again with a
//! lease in hand, at the moment of acting, against the window as it is then. This only
//! decides whether it is worth asking, and says no in words when it is not.

use archie_speech::SpeechToolIntentCandidate;
use archie_stage::{ActionKind, Key, KeyChord, StagePolicy};
use serde_json::{Map, Value};

/// What will be done, once decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Start an application, exactly as its policy entry says.
    Open {
        app_id: String,
        argv: Vec<String>,
    },
    /// Find something by what it looks like, and press it.
    Click {
        app_id: String,
        target: String,
    },
    Type {
        app_id: String,
        text: String,
    },
    Keys {
        app_id: String,
        chord: KeyChord,
    },
    Scroll {
        app_id: String,
        down: bool,
        steps: i32,
    },
    /// Draw a painting in Photopea, in the background, telling it as it goes
    /// ([`crate::draw`]). `lang` is the language to tell it in, when the model said.
    Draw {
        app_id: String,
        work: &'static str,
        lang: Option<crate::draw::Lang>,
    },
    /// Stop the drawing under way, between two batches of lines.
    StopDrawing,
}

impl Plan {
    pub fn app_id(&self) -> &str {
        match self {
            Self::Open { app_id, .. }
            | Self::Click { app_id, .. }
            | Self::Type { app_id, .. }
            | Self::Keys { app_id, .. }
            | Self::Scroll { app_id, .. }
            | Self::Draw { app_id, .. } => app_id,
            Self::StopDrawing => "",
        }
    }
}

/// Why a proposal will not be carried out, for the log and for the voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    /// Said to the person, in their language.
    pub spoken: String,
}

fn refuse(code: &'static str, spoken: impl Into<String>) -> Refusal {
    Refusal {
        code,
        spoken: spoken.into(),
    }
}

/// The longest description of something to click.
pub const MAX_TARGET_CHARS: usize = 200;
/// The most scroll steps one proposal may ask for.
pub const MAX_SCROLL_STEPS: i32 = 20;
/// The longest address or search the browser is asked to open.
pub const MAX_URL_CHARS: usize = 2048;
/// Where a search is opened when the person asked for one rather than for an address.
pub const SEARCH: &str = "https://duckduckgo.com/?q=";

/// Decides what a proposal would do, or why it will not be done.
///
/// `front` is the application in front — the one with keyboard focus — which is what
/// "type this" and "press enter" mean when nobody names an application.
pub fn decide(
    intent: &SpeechToolIntentCandidate,
    policy: &StagePolicy,
    front: Option<&str>,
) -> Result<Plan, Refusal> {
    match intent.tool_name.as_str() {
        "open_app" => {
            let args = arguments(&intent.raw_arguments, &["app"])?;
            let name = text(&args, "app", 128)?;
            // Orb, World, Vault, Forge, Sentry, Stage and Ajustes are not programs but buttons
            // of the desktop's dock: opening one is pressing it, by the name it gives the
            // accessibility bus, when the policy lets Archie press the desktop at all.
            if policy.named(name).is_none() {
                if let Some(button) = dock_button(name) {
                    permitted(policy, DESKTOP_APP, ActionKind::Click, "hacer clic")?;
                    return Ok(Plan::Click {
                        app_id: DESKTOP_APP.into(),
                        target: button.into(),
                    });
                }
            }
            let app = policy.named(name).ok_or_else(|| {
                refuse(
                    "unknown_app",
                    format!("No conozco una aplicación que se llame {name}."),
                )
            })?;
            if app.launch.is_empty() {
                return Err(refuse(
                    "not_openable",
                    format!("No tengo permiso para abrir {}.", app.display_name),
                ));
            }
            Ok(Plan::Open {
                app_id: app.app_id.clone(),
                argv: app.launch.clone(),
            })
        }
        "click" => {
            let args = arguments(&intent.raw_arguments, &["target", "app"])?;
            let target = text(&args, "target", MAX_TARGET_CHARS)?.to_string();
            let app_id = match args.get("app") {
                Some(_) => {
                    let name = text(&args, "app", 128)?;
                    policy
                        .named(name)
                        .map(|app| app.app_id.clone())
                        .ok_or_else(|| {
                            refuse(
                                "unknown_app",
                                format!("No conozco una aplicación que se llame {name}."),
                            )
                        })?
                }
                None => front_app(front)?,
            };
            permitted(policy, &app_id, ActionKind::Click, "hacer clic")?;
            Ok(Plan::Click { app_id, target })
        }
        "type_text" => {
            let args = arguments(&intent.raw_arguments, &["text"])?;
            let words = text(&args, "text", usize::MAX)?.to_string();
            let app_id = front_app(front)?;
            let app = permitted(policy, &app_id, ActionKind::Text, "escribir")?;
            if words.chars().count() > app.max_text_chars {
                return Err(refuse(
                    "text_too_long",
                    format!(
                        "Eso es más largo de lo que puedo escribir en {}.",
                        app.display_name
                    ),
                ));
            }
            Ok(Plan::Type {
                app_id,
                text: words,
            })
        }
        "press_keys" => {
            let args = arguments(&intent.raw_arguments, &["keys"])?;
            let spec = text(&args, "keys", 64)?;
            let chord = chord_of(spec)
                .ok_or_else(|| refuse("unknown_keys", format!("No sé qué teclas son «{spec}».")))?;
            let app_id = front_app(front)?;
            let app = permitted(policy, &app_id, ActionKind::Key, "usar el teclado")?;
            if !chord.modifiers.is_empty() && !app.allow_modified_keys {
                return Err(refuse(
                    "modified_keys_not_allowed",
                    format!(
                        "No tengo permiso para usar combinaciones de teclas en {}.",
                        app.display_name
                    ),
                ));
            }
            if let Key::Named(named) = &chord.key {
                if app.approval_keys.contains(named) {
                    return Err(refuse(
                        "needs_approval",
                        "Esa tecla necesita que me lo apruebes tú.",
                    ));
                }
            }
            Ok(Plan::Keys { app_id, chord })
        }
        "scroll" => {
            let args = arguments(&intent.raw_arguments, &["direction", "amount"])?;
            let down = match text(&args, "direction", 8)? {
                "down" | "abajo" => true,
                "up" | "arriba" => false,
                other => {
                    return Err(refuse(
                        "bad_arguments",
                        format!("No sé desplazarme hacia «{other}»."),
                    ))
                }
            };
            let steps = match args.get("amount") {
                None => 3,
                Some(value) => value
                    .as_i64()
                    .filter(|steps| (1..=i64::from(MAX_SCROLL_STEPS)).contains(steps))
                    .map(|steps| steps as i32)
                    .ok_or_else(|| refuse("bad_arguments", "No entendí cuánto desplazarme."))?,
            };
            let app_id = front_app(front)?;
            permitted(policy, &app_id, ActionKind::Scroll, "desplazar la vista")?;
            Ok(Plan::Scroll {
                app_id,
                down,
                steps,
            })
        }
        "open_website" => {
            let args = arguments(&intent.raw_arguments, &["url", "search"])?;
            let address = match (args.get("url"), args.get("search")) {
                (Some(_), None) => web_address(text(&args, "url", MAX_URL_CHARS)?)?,
                (None, Some(_)) => {
                    format!("{SEARCH}{}", query_encode(text(&args, "search", 256)?))
                }
                _ => {
                    return Err(refuse(
                        "bad_arguments",
                        "Dime una dirección o qué buscar, una de las dos.",
                    ))
                }
            };
            let app = policy
                .named("browser")
                .or_else(|| policy.named("navegador"))
                .ok_or_else(|| refuse("unknown_app", "No tengo un navegador que pueda abrir."))?;
            if app.launch.is_empty() {
                return Err(refuse(
                    "not_openable",
                    format!("No tengo permiso para abrir {}.", app.display_name),
                ));
            }
            let mut argv = app.launch.clone();
            argv.push(address);
            Ok(Plan::Open {
                app_id: app.app_id.clone(),
                argv,
            })
        }
        "draw_picture" => {
            let args = arguments(&intent.raw_arguments, &["picture", "language"])?;
            let named = text(&args, "picture", 128)?;
            let lang = match args.get("language").and_then(Value::as_str) {
                Some(code) if code.starts_with("en") => Some(crate::draw::Lang::En),
                Some(code) if code.starts_with("es") => Some(crate::draw::Lang::Es),
                _ => None,
            };
            let work = crate::draw::find_work(named).ok_or_else(|| {
                refuse(
                    "unknown_picture",
                    format!(
                        "Todavía no sé dibujar {named}. Sé dibujar {}.",
                        crate::draw::repertoire(crate::draw::Lang::Es)
                    ),
                )
            })?;
            let app = policy.named("photopea").ok_or_else(|| {
                refuse(
                    "unknown_app",
                    "No tengo Photopea entre las aplicaciones que puedo usar.",
                )
            })?;
            if app.launch.is_empty() {
                return Err(refuse(
                    "not_openable",
                    "No tengo permiso para abrir Photopea.",
                ));
            }
            for (kind, doing) in [
                (ActionKind::Stroke, "dibujar"),
                (ActionKind::Click, "tomar el pincel"),
                (ActionKind::Key, "cambiar de tinta"),
                (ActionKind::Snapshot, "mirar el papel"),
            ] {
                permitted(policy, &app.app_id, kind, doing)?;
            }
            Ok(Plan::Draw {
                app_id: app.app_id.clone(),
                work: work.id,
                lang,
            })
        }
        "stop_drawing" => {
            if !intent.raw_arguments.trim().is_empty() {
                arguments(&intent.raw_arguments, &[])?;
            }
            Ok(Plan::StopDrawing)
        }
        _ => Err(refuse("unknown_tool", "Eso todavía no sé hacerlo.")),
    }
}

/// An address the browser may be asked to open: the web, and nothing else — no files, no
/// scripts, no other programs' schemes. A bare host is taken as https.
fn web_address(said: &str) -> Result<String, Refusal> {
    let bad = || {
        refuse(
            "bad_url",
            format!("«{said}» no es una dirección web que pueda abrir."),
        )
    };
    if said.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad());
    }
    let address = match said.split_once("://") {
        Some((scheme, _))
            if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") =>
        {
            said.to_string()
        }
        Some(_) => return Err(bad()),
        None if said.contains(':') && !said.contains('.') => return Err(bad()),
        None => format!("https://{said}"),
    };
    let host = address
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    if host.is_empty()
        || host.starts_with('-')
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
    {
        return Err(bad());
    }
    Ok(address)
}

/// A search as a query string: letters and digits as they are, everything else escaped.
fn query_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The proposal's arguments as an object holding only the keys this tool takes.
fn arguments(raw: &str, known: &[&str]) -> Result<Map<String, Value>, Refusal> {
    let value: Value = serde_json::from_str(raw.trim())
        .map_err(|_| refuse("bad_arguments", "No entendí bien qué hacer."))?;
    let Value::Object(object) = value else {
        return Err(refuse("bad_arguments", "No entendí bien qué hacer."));
    };
    if let Some(stray) = object.keys().find(|key| !known.contains(&key.as_str())) {
        return Err(refuse(
            "bad_arguments",
            format!("No entendí bien qué hacer: sobra «{stray}»."),
        ));
    }
    Ok(object)
}

fn text<'a>(args: &'a Map<String, Value>, key: &str, most: usize) -> Result<&'a str, Refusal> {
    let value = args
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| refuse("bad_arguments", "Me faltó un dato para hacerlo."))?;
    if value.chars().count() > most {
        return Err(refuse("bad_arguments", "Eso es demasiado largo."));
    }
    Ok(value)
}

fn front_app(front: Option<&str>) -> Result<String, Refusal> {
    front.map(str::to_string).ok_or_else(|| {
        refuse(
            "nothing_in_front",
            "No hay ninguna aplicación al frente para eso.",
        )
    })
}

/// The ArchieOS desktop's app id, as the stage lays it under every window.
pub const DESKTOP_APP: &str = "archie-display";

/// The dock button a spoken app name means, by the name the dock gives the accessibility
/// bus (display/shell/prototype/Synthetic.qml), in either language.
pub fn dock_button(name: &str) -> Option<&'static str> {
    let said = name.trim().to_lowercase();
    let said = said
        .strip_prefix("el ")
        .or_else(|| said.strip_prefix("la "))
        .or_else(|| said.strip_prefix("the "))
        .unwrap_or(&said);
    Some(match said {
        "orb" | "orbe" | "la esfera" | "esfera" => "Orb",
        "world" | "mundo" | "el mundo" | "globo" | "el globo" | "globe" => "World",
        "vault" | "bóveda" | "boveda" | "archivos" | "files" => "Vault",
        "forge" | "forja" => "Forge",
        "sentry" | "sentinel" | "centinela" => "Sentry",
        "stage" | "escenario" => "Stage",
        "ajustes" | "settings" | "configuración" | "configuracion" | "preferencias" => {
            settings_button()
        }
        _ => return None,
    })
}

/// The settings button's name on the dock, which follows the desktop's language
/// (ARCHIE_UI_LANG, as display/shell/prototype/Lang.js reads it).
fn settings_button() -> &'static str {
    match std::env::var("ARCHIE_UI_LANG") {
        Ok(lang) if lang.starts_with("en") => "Settings",
        _ => "Ajustes",
    }
}

fn permitted<'a>(
    policy: &'a StagePolicy,
    app_id: &str,
    kind: ActionKind,
    doing: &str,
) -> Result<&'a archie_stage::AppPolicy, Refusal> {
    let app = policy.app(app_id).ok_or_else(|| {
        refuse(
            "app_not_allowed",
            "No tengo permiso para usar la aplicación que está al frente.",
        )
    })?;
    if !app.allow.contains(&kind) {
        return Err(refuse(
            "action_not_allowed",
            format!("No tengo permiso para {doing} en {}.", app.display_name),
        ));
    }
    if app.requires_approval.contains(&kind) {
        return Err(refuse(
            "needs_approval",
            format!(
                "Para {doing} en {} necesito que me lo apruebes tú.",
                app.display_name
            ),
        ));
    }
    Ok(app)
}

/// Keys as a model tends to write them — "Ctrl + S", "Enter", "control+shift+z" — read
/// as the stage writes them.
fn chord_of(spec: &str) -> Option<KeyChord> {
    let normalised: String = spec
        .to_lowercase()
        .split('+')
        .map(|part| match part.trim() {
            "control" => "ctrl",
            "return" | "intro" => "enter",
            "esc" => "escape",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+");
    KeyChord::parse(&normalised)
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str = r#"
[[app]]
app_id = "org.mozilla.firefox"
display_name = "Firefox"
allow = ["click", "text", "key", "scroll", "focus"]
max_text_chars = 64
allow_modified_keys = true
approval_keys = ["enter"]
launch = ["firefox", "--new-window"]

[[app]]
app_id = "org.qt-project.qml"
display_name = "Lienzo"
allow = ["stroke", "click"]
"#;

    fn policy() -> StagePolicy {
        StagePolicy::parse(POLICY).unwrap()
    }

    fn intent(tool: &str, arguments: &str) -> SpeechToolIntentCandidate {
        SpeechToolIntentCandidate {
            correlation_id: "t1#0".into(),
            tool_name: tool.into(),
            raw_arguments: arguments.into(),
            source_adapter: "nemotron:test".into(),
            confidence: None,
        }
    }

    const FOX: Option<&str> = Some("org.mozilla.firefox");

    #[test]
    fn open_runs_exactly_what_the_policy_wrote_and_nothing_the_model_said() {
        let plan = decide(&intent("open_app", r#"{"app":"Firefox"}"#), &policy(), None).unwrap();
        assert_eq!(
            plan,
            Plan::Open {
                app_id: "org.mozilla.firefox".into(),
                argv: vec!["firefox".into(), "--new-window".into()],
            }
        );
        // The model cannot smuggle a command in as the name, or as an extra argument.
        let smuggled = decide(
            &intent("open_app", r#"{"app":"firefox; rm -rf ~"}"#),
            &policy(),
            None,
        );
        assert_eq!(smuggled.unwrap_err().code, "unknown_app");
        let extra = decide(
            &intent("open_app", r#"{"app":"firefox","args":"--kiosk evil.com"}"#),
            &policy(),
            None,
        );
        assert_eq!(extra.unwrap_err().code, "bad_arguments");
    }

    #[test]
    fn an_application_without_a_launch_entry_is_not_opened_and_the_person_hears_why() {
        let refused =
            decide(&intent("open_app", r#"{"app":"lienzo"}"#), &policy(), None).unwrap_err();
        assert_eq!(refused.code, "not_openable");
        assert!(refused.spoken.contains("Lienzo"), "{}", refused.spoken);
        let unknown =
            decide(&intent("open_app", r#"{"app":"chrome"}"#), &policy(), None).unwrap_err();
        assert!(unknown.spoken.contains("chrome"), "{}", unknown.spoken);
    }

    #[test]
    fn typing_goes_to_the_application_in_front_within_its_bounds() {
        let plan = decide(
            &intent("type_text", r#"{"text":"clima santiago"}"#),
            &policy(),
            FOX,
        )
        .unwrap();
        assert_eq!(plan.app_id(), "org.mozilla.firefox");
        let long = format!(r#"{{"text":"{}"}}"#, "a".repeat(65));
        assert_eq!(
            decide(&intent("type_text", &long), &policy(), FOX)
                .unwrap_err()
                .code,
            "text_too_long"
        );
        assert_eq!(
            decide(&intent("type_text", r#"{"text":"hola"}"#), &policy(), None)
                .unwrap_err()
                .code,
            "nothing_in_front"
        );
        // An application the policy lets Archie draw in, but not type in.
        assert_eq!(
            decide(
                &intent("type_text", r#"{"text":"hola"}"#),
                &policy(),
                Some("org.qt-project.qml")
            )
            .unwrap_err()
            .code,
            "action_not_allowed"
        );
        assert_eq!(
            decide(
                &intent("type_text", r#"{"text":"hola"}"#),
                &policy(),
                Some("org.unknown.app")
            )
            .unwrap_err()
            .code,
            "app_not_allowed"
        );
    }

    #[test]
    fn keys_are_read_the_way_a_model_writes_them_and_approval_keys_wait_for_a_person() {
        let plan = decide(
            &intent("press_keys", r#"{"keys":"Ctrl + S"}"#),
            &policy(),
            FOX,
        )
        .unwrap();
        let Plan::Keys { chord, .. } = plan else {
            panic!("a key plan");
        };
        assert_eq!(Some(chord), KeyChord::parse("ctrl+s"));
        assert_eq!(
            decide(&intent("press_keys", r#"{"keys":"Enter"}"#), &policy(), FOX)
                .unwrap_err()
                .code,
            "needs_approval"
        );
        assert_eq!(
            decide(
                &intent("press_keys", r#"{"keys":"hyper+q"}"#),
                &policy(),
                FOX
            )
            .unwrap_err()
            .code,
            "unknown_keys"
        );
    }

    #[test]
    fn scroll_is_bounded_and_speaks_either_language() {
        assert_eq!(
            decide(
                &intent("scroll", r#"{"direction":"abajo"}"#),
                &policy(),
                FOX
            )
            .unwrap(),
            Plan::Scroll {
                app_id: "org.mozilla.firefox".into(),
                down: true,
                steps: 3
            }
        );
        assert_eq!(
            decide(
                &intent("scroll", r#"{"direction":"down","amount":500}"#),
                &policy(),
                FOX
            )
            .unwrap_err()
            .code,
            "bad_arguments"
        );
    }

    #[test]
    fn a_click_can_name_its_application_or_mean_the_one_in_front() {
        let named = decide(
            &intent("click", r#"{"target":"el botón Buscar","app":"firefox"}"#),
            &policy(),
            None,
        )
        .unwrap();
        assert_eq!(named.app_id(), "org.mozilla.firefox");
        let in_front = decide(
            &intent("click", r#"{"target":"el color rojo"}"#),
            &policy(),
            Some("org.qt-project.qml"),
        )
        .unwrap();
        assert_eq!(in_front.app_id(), "org.qt-project.qml");
        let long = format!(r#"{{"target":"{}"}}"#, "x".repeat(MAX_TARGET_CHARS + 1));
        assert!(decide(&intent("click", &long), &policy(), FOX).is_err());
    }

    const DEMO: &str = r#"
[[app]]
app_id = "firefox"
display_name = "Firefox"
allow = ["click", "text", "key", "scroll", "focus", "stroke", "snapshot"]
launch = ["firefox", "--new-window"]
aliases = ["browser", "navegador", "photopea"]
"#;

    #[test]
    fn a_website_opens_in_the_browser_the_policy_names_and_nothing_else_does() {
        let policy = StagePolicy::parse(DEMO).unwrap();
        let open = |raw: &str| decide(&intent("open_website", raw), &policy, None);
        assert_eq!(
            open(r#"{"url":"photopea.com"}"#).unwrap(),
            Plan::Open {
                app_id: "firefox".into(),
                argv: vec![
                    "firefox".into(),
                    "--new-window".into(),
                    "https://photopea.com".into()
                ],
            }
        );
        let Plan::Open { argv, .. } = open(r#"{"search":"clima en Santiago & más"}"#).unwrap()
        else {
            panic!("an open plan");
        };
        assert_eq!(
            argv.last().unwrap(),
            "https://duckduckgo.com/?q=clima+en+Santiago+%26+m%C3%A1s"
        );
        for raw in [
            r#"{"url":"file:///etc/passwd"}"#,
            r#"{"url":"javascript:alert(1)"}"#,
            r#"{"url":"https://a.com --kiosk"}"#,
            r#"{"url":"-profile"}"#,
            r#"{"url":"a.com","search":"b"}"#,
            r#"{}"#,
        ] {
            assert!(open(raw).is_err(), "{raw}");
        }
        // Without a browser entry there is nothing to open it in.
        assert_eq!(
            decide(
                &intent("open_website", r#"{"url":"a.com"}"#),
                &self::policy(),
                None
            )
            .unwrap_err()
            .code,
            "unknown_app"
        );
    }

    #[test]
    fn a_painting_is_drawn_only_where_the_policy_lets_archie_draw() {
        let policy = StagePolicy::parse(DEMO).unwrap();
        let plan = decide(
            &intent(
                "draw_picture",
                r#"{"picture":"the Mona Lisa","language":"en"}"#,
            ),
            &policy,
            None,
        )
        .unwrap();
        assert_eq!(
            plan,
            Plan::Draw {
                app_id: "firefox".into(),
                work: "mona-lisa",
                lang: Some(crate::draw::Lang::En),
            }
        );
        let unknown = decide(
            &intent("draw_picture", r#"{"picture":"el grito"}"#),
            &policy,
            None,
        )
        .unwrap_err();
        assert_eq!(unknown.code, "unknown_picture");
        assert!(
            unknown.spoken.contains("la Mona Lisa"),
            "{}",
            unknown.spoken
        );
        // An everyday Firefox, granted no strokes, is not a canvas.
        let everyday = StagePolicy::parse(&DEMO.replace(", \"stroke\"", "")).unwrap();
        assert_eq!(
            decide(
                &intent("draw_picture", r#"{"picture":"mona lisa"}"#),
                &everyday,
                None
            )
            .unwrap_err()
            .code,
            "action_not_allowed"
        );
        assert_eq!(
            decide(&intent("stop_drawing", "{}"), &policy, None).unwrap(),
            Plan::StopDrawing
        );
    }

    #[test]
    fn the_nebius_demo_policy_draws_browses_and_presses_the_desktop_but_never_opens_it() {
        let text = include_str!("../../../../display/session/nebius-demo-apps.toml")
            .replace("@FIREFOX_APP_ID@", "firefox")
            .replace("@FIREFOX@", "/usr/bin/firefox")
            .replace("@PROFILE@", "/tmp/archie-demo/firefox");
        let policy = StagePolicy::parse(&text).unwrap();
        assert!(matches!(
            decide(
                &intent("draw_picture", r#"{"picture":"Mona Lisa","language":"en"}"#),
                &policy,
                None
            ),
            Ok(Plan::Draw {
                work: "mona-lisa",
                ..
            })
        ));
        let Ok(Plan::Open { argv, .. }) = decide(
            &intent("open_website", r#"{"url":"nebius.com"}"#),
            &policy,
            None,
        ) else {
            panic!("the browser opens a website");
        };
        assert_eq!(
            argv,
            [
                "/usr/bin/firefox",
                "--profile",
                "/tmp/archie-demo/firefox",
                "--new-window",
                "https://nebius.com"
            ]
        );
        assert_eq!(
            decide(
                &intent("click", r#"{"target":"Sentry","app":"escritorio"}"#),
                &policy,
                None
            )
            .unwrap()
            .app_id(),
            "archie-display"
        );
        assert_eq!(
            decide(
                &intent("open_app", r#"{"app":"escritorio"}"#),
                &policy,
                None
            )
            .unwrap_err()
            .code,
            "not_openable"
        );
        // The dock's apps open by pressing their buttons, in either language.
        for (said, button) in [
            ("Sentry", "Sentry"),
            ("World", "World"),
            ("el mundo", "World"),
            ("Forge", "Forge"),
            ("ajustes", "Ajustes"),
        ] {
            assert_eq!(
                decide(
                    &intent("open_app", &format!(r#"{{"app":"{said}"}}"#)),
                    &policy,
                    None
                )
                .unwrap(),
                Plan::Click {
                    app_id: "archie-display".into(),
                    target: button.into()
                },
                "{said}"
            );
        }
        assert_eq!(
            decide(
                &intent("type_text", r#"{"text":"hola"}"#),
                &policy,
                Some("archie-display")
            )
            .unwrap_err()
            .code,
            "action_not_allowed"
        );
    }

    #[test]
    fn nonsense_is_refused_in_words_and_never_panics() {
        for raw in ["", "null", "[]", "{", r#"{"app": 7}"#, r#"{"app": ""}"#] {
            let refused = decide(&intent("open_app", raw), &policy(), FOX).unwrap_err();
            assert!(!refused.spoken.is_empty(), "{raw:?} gets a sentence");
        }
        assert_eq!(
            decide(&intent("format_disk", "{}"), &policy(), FOX)
                .unwrap_err()
                .code,
            "unknown_tool"
        );
    }
}
