//! The native Observatory's loopback control channel.
//!
//! One JSON line in, one line out, on `127.0.0.1:7668`. The display is the only authority on
//! what it can show: its scenes and its look presets (the JSON files embedded in its
//! resources). Callers ask it with [`catalog`] instead of keeping their own copy of the
//! names, and every request is validated by the display, which answers `ok` or
//! `error: <reason>`. Nothing here can run commands, reach Stage or write to displayd.

use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// Where the native display listens (loopback only).
pub const ADDRESS: &str = "127.0.0.1:7668";

/// How long a caller waits for the display before giving up.
pub const TIMEOUT: Duration = Duration::from_millis(1500);

/// The longest reply line accepted; the catalog is the largest and is a few hundred bytes.
const MAX_REPLY: u64 = 64 * 1024;

/// What the running display can show.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Catalog {
    pub scenes: Vec<String>,
    pub presets: Vec<String>,
}

/// Legacy camera framings; these do not replace the display's preset catalog.
pub const CINEMATICS: [&str; 4] = ["field", "crown", "atlas", "dense"];
/// Presentation layers that can be switched on and off.
pub const OVERLAYS: [&str; 7] = [
    "hud", "shell", "ambient", "guides", "links", "pulses", "bloom",
];
/// Local panels supported by the native shell.
pub const PANELS: [&str; 7] = [
    "forge",
    "inspector",
    "settings",
    "sentry",
    "activity",
    "notifications",
    "search",
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ControlError {
    #[error("`{action} {value}` is not a visual command the display accepts")]
    Invalid { action: String, value: String },
    #[error("the Observatory is not listening on {ADDRESS}: {0}")]
    Unreachable(String),
    #[error("the Observatory did not answer within {}ms", TIMEOUT.as_millis())]
    Timeout,
    #[error("the Observatory refused: {0}")]
    Refused(String),
    #[error("the Observatory answered something unexpected: {0}")]
    Malformed(String),
}

/// Check presentation command shape; scene and preset membership belongs to the display.
pub fn valid(action: &str, value: &str, enabled: Option<bool>) -> bool {
    match action {
        "scene" | "preset" => {
            !value.trim().is_empty()
                && value.len() <= 128
                && !value
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\')
                && value != "."
                && value != ".."
                && enabled.is_none()
        }
        "cinematic" => CINEMATICS.contains(&value) && enabled.is_none(),
        "panel" => PANELS.contains(&value) && enabled.is_none(),
        "overlay" => OVERLAYS.contains(&value) && enabled.is_some(),
        _ => false,
    }
}

async fn exchange(request: &Value) -> Result<String, ControlError> {
    let line = format!("{request}\n");
    tokio::time::timeout(TIMEOUT, async {
        let stream = TcpStream::connect(ADDRESS)
            .await
            .map_err(|error| ControlError::Unreachable(error.to_string()))?;
        let (read, mut write) = stream.into_split();
        write
            .write_all(line.as_bytes())
            .await
            .map_err(|error| ControlError::Unreachable(error.to_string()))?;
        let mut reply = String::new();
        BufReader::new(tokio::io::AsyncReadExt::take(read, MAX_REPLY))
            .read_line(&mut reply)
            .await
            .map_err(|error| ControlError::Malformed(error.to_string()))?;
        Ok(reply.trim_end().to_owned())
    })
    .await
    .map_err(|_| ControlError::Timeout)?
}

/// `ok` is success, `error: <reason>` a refusal, anything else a protocol fault.
fn outcome(reply: &str) -> Result<(), ControlError> {
    if reply == "ok" {
        Ok(())
    } else if let Some(reason) = reply.strip_prefix("error:") {
        Err(ControlError::Refused(reason.trim().to_owned()))
    } else {
        Err(ControlError::Malformed(reply.chars().take(200).collect()))
    }
}

fn parse_catalog(reply: &str) -> Result<Catalog, ControlError> {
    if let Some(reason) = reply.strip_prefix("error:") {
        return Err(ControlError::Refused(reason.trim().to_owned()));
    }
    serde_json::from_str(reply).map_err(|error| ControlError::Malformed(error.to_string()))
}

/// Send a bounded presentation command; the display validates its current capabilities.
pub async fn send(action: &str, value: &str, enabled: Option<bool>) -> Result<(), ControlError> {
    if !valid(action, value, enabled) {
        return Err(ControlError::Invalid {
            action: action.chars().take(64).collect(),
            value: value.chars().take(64).collect(),
        });
    }
    let mut request = json!({"action": action, "value": value});
    if let Some(enabled) = enabled {
        request["enabled"] = json!(enabled);
    }
    outcome(&exchange(&request).await?)
}

/// Camera moves the display accepts.
pub const CAMERA_MOVES: [&str; 7] = ["home", "closer", "farther", "left", "right", "up", "down"];
/// Window states the display accepts.
pub const WINDOW_STATES: [&str; 3] = ["windowed", "fullscreen", "maximized"];
/// The longest popup text the display shows.
pub const POPUP_LENGTH: usize = 200;

fn invalid(action: &str, value: &str) -> ControlError {
    ControlError::Invalid {
        action: action.chars().take(64).collect(),
        value: value.chars().take(64).collect(),
    }
}

/// A node id as the display reads one: up to `longest` characters, none of them control characters.
fn plain(text: &str, longest: usize) -> bool {
    text.chars().count() <= longest && !text.chars().any(char::is_control)
}

/// Whether a pointer coordinate is in the range the display accepts.
pub fn valid_unit(value: f64) -> bool {
    (0.0..=1.0).contains(&value)
}

/// Each of the verbs below is checked here first and again by the display, which is the one
/// that knows what it can show.
async fn command(request: Value) -> Result<(), ControlError> {
    outcome(&exchange(&request).await?)
}

/// Focus a node by id; an empty id lets go.
pub async fn focus(id: &str) -> Result<(), ControlError> {
    if !plain(id, 256) {
        return Err(invalid("focus", id));
    }
    command(json!({"action": "focus", "value": id})).await
}

/// Move the camera.
pub async fn camera(movement: &str) -> Result<(), ControlError> {
    if !CAMERA_MOVES.contains(&movement) {
        return Err(invalid("camera", movement));
    }
    command(json!({"action": "camera", "value": movement})).await
}

/// Change the window's state.
pub async fn window(state: &str) -> Result<(), ControlError> {
    if !WINDOW_STATES.contains(&state) {
        return Err(invalid("window", state));
    }
    command(json!({"action": "window", "value": state})).await
}

/// Show a pointer at a place of the view (0..1 on each axis), and click there when asked.
pub async fn pointer(x: f64, y: f64, click: bool) -> Result<(), ControlError> {
    if !valid_unit(x) || !valid_unit(y) {
        return Err(invalid("pointer", &format!("{x},{y}")));
    }
    command(json!({"action": "pointer", "x": x, "y": y, "click": click})).await
}

/// Light a node, or put it out again.
pub async fn highlight(id: &str, enabled: bool) -> Result<(), ControlError> {
    if id.is_empty() || !plain(id, 256) {
        return Err(invalid("highlight", id));
    }
    command(json!({"action": "highlight", "value": id, "enabled": enabled})).await
}

/// Put a short message on the screen for a few seconds (1..30).
pub async fn popup(text: &str, seconds: f64) -> Result<(), ControlError> {
    if text.trim().is_empty() || !plain(text, POPUP_LENGTH) || !(1.0..=30.0).contains(&seconds) {
        return Err(invalid("popup", text));
    }
    command(json!({"action": "popup", "value": text, "seconds": seconds})).await
}

/// Stop the sphere's clock, or start it again (for tests that need the same frame twice).
pub async fn freeze(enabled: bool) -> Result<(), ControlError> {
    command(json!({"action": "freeze", "enabled": enabled})).await
}

/// Write the window as drawn now to a PNG in the display's own folder; returns its path.
pub async fn capture() -> Result<String, ControlError> {
    capture_path(&exchange(&json!({"action": "capture"})).await?)
}

/// `ok <path>` is a capture; `error: <reason>` a refusal; anything else a protocol fault.
fn capture_path(reply: &str) -> Result<String, ControlError> {
    match reply.strip_prefix("ok ") {
        Some(path) if !path.trim().is_empty() => Ok(path.trim().to_owned()),
        _ => outcome(reply).and(Err(ControlError::Malformed(
            reply.chars().take(200).collect(),
        ))),
    }
}

/// The scenes and presets the running display accepts.
pub async fn catalog() -> Result<Catalog, ControlError> {
    parse_catalog(&exchange(&json!({"action": "list"})).await?)
}

/// Switch the Observatory to a scene.
pub async fn scene(name: &str) -> Result<(), ControlError> {
    outcome(&exchange(&json!({"action": "scene", "value": name})).await?)
}

/// Apply a look preset to the orb (the display opens Orb first).
pub async fn preset(name: &str) -> Result<(), ControlError> {
    outcome(&exchange(&json!({"action": "preset", "value": name})).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_controls_preserve_dynamic_scene_and_preset_names() {
        assert!(valid("scene", "future-scene", None));
        assert!(valid("preset", "future-preset", None));
        for name in CINEMATICS {
            assert!(valid("cinematic", name, None));
        }
        for name in OVERLAYS {
            assert!(valid("overlay", name, Some(false)));
            assert!(!valid("overlay", name, None));
        }
        assert!(valid("panel", "inspector", None));
        assert!(!valid("panel", "execute", None));
        assert!(!valid("preset", "../crown", None));
        assert!(!valid("preset", "", None));
        assert!(!valid("scene", "orb\n", None));
    }

    #[tokio::test]
    async fn invalid_controls_are_rejected_before_connecting() {
        assert!(matches!(
            send("panel", "execute", None).await,
            Err(ControlError::Invalid { .. })
        ));
        assert!(matches!(
            send("overlay", "hud", None).await,
            Err(ControlError::Invalid { .. })
        ));
    }

    #[tokio::test]
    async fn the_new_verbs_are_checked_before_connecting() {
        for result in [
            camera("sideways").await,
            window("minimized").await,
            pointer(1.5, 0.5, false).await,
            pointer(0.5, f64::NAN, false).await,
            highlight("", true).await,
            highlight("a\nb", true).await,
            popup("   ", 4.0).await,
            popup("hola", 0.5).await,
            popup("hola", 31.0).await,
            popup(&"x".repeat(POPUP_LENGTH + 1), 4.0).await,
            focus("a\u{7}").await,
        ] {
            assert!(matches!(result, Err(ControlError::Invalid { .. })));
        }
    }

    #[test]
    fn a_capture_reply_carries_its_path() {
        assert_eq!(
            capture_path("ok /run/user/1000/archie-display/capture-1.png"),
            Ok("/run/user/1000/archie-display/capture-1.png".to_owned())
        );
        assert_eq!(
            capture_path("error: the capture failed"),
            Err(ControlError::Refused("the capture failed".into()))
        );
        // A bare `ok` carries no path: that is not a capture.
        assert!(matches!(
            capture_path("ok"),
            Err(ControlError::Malformed(_))
        ));
        assert!(matches!(
            capture_path("fine"),
            Err(ControlError::Malformed(_))
        ));
    }

    #[test]
    fn replies_are_read_as_the_display_writes_them() {
        assert_eq!(outcome("ok"), Ok(()));
        assert_eq!(
            outcome("error: unknown preset sparkle"),
            Err(ControlError::Refused("unknown preset sparkle".into()))
        );
        assert!(matches!(outcome("okay"), Err(ControlError::Malformed(_))));
        assert_eq!(
            parse_catalog(r#"{"scenes":["orb"],"presets":["field","neural"]}"#).unwrap(),
            Catalog {
                scenes: vec!["orb".into()],
                presets: vec!["field".into(), "neural".into()]
            }
        );
        assert!(matches!(
            parse_catalog("[]"),
            Err(ControlError::Malformed(_))
        ));
    }
}
