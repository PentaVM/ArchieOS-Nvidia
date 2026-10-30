//! `archie visual` — the Observatory's look is controlled from here, never from an overlay.
//!
//! Every visual parameter lives in a named preset; a scene is one of the Observatory views.
//! The names are not kept here: the running display owns them (its scenes and the preset
//! files embedded in it), lists them on request and validates every command. This is a thin
//! client of [`archie_orb::control`], the same one the voice uses.

use archie_orb::control::{self, ControlError};
use clap::Subcommand;
use serde_json::json;

#[derive(Debug, Subcommand)]
pub enum VisualCommand {
    /// Switch the Observatory view (`archie visual list` shows which exist).
    Scene { name: String },
    /// Apply a look preset to the orb; opens Orb (`archie visual list` shows which exist).
    Preset { name: String },
    /// List the scenes and presets the running display accepts.
    List,
    /// Focus a node of the orb by id (no id lets go of it).
    Focus {
        #[arg(default_value = "")]
        id: String,
    },
    /// Move the camera: home, closer, farther, left, right, up or down.
    Camera { movement: String },
    /// Change the window: windowed, fullscreen or maximized.
    Window { state: String },
    /// Show a pointer at a place of the view (0..1 on each axis); `--click` also clicks there.
    Pointer {
        x: f64,
        y: f64,
        #[arg(long)]
        click: bool,
    },
    /// Light a node of the orb, or put it out with `--off`.
    Highlight {
        id: String,
        #[arg(long)]
        off: bool,
    },
    /// Put a short message (200 characters at most) on the screen for 1 to 30 seconds.
    Popup {
        text: String,
        #[arg(long, default_value_t = 4.0)]
        seconds: f64,
    },
    /// Stop the sphere's clock (`--off` starts it again): the same frame twice, for tests.
    Freeze {
        #[arg(long)]
        off: bool,
    },
    /// Save the window as drawn now to a PNG and print where it is.
    Capture,
}

fn fail(error: &ControlError, json: bool) -> std::process::ExitCode {
    if json {
        println!("{}", json!({"ok": false, "error": error.to_string()}));
    } else {
        eprintln!("archie: {error}");
    }
    std::process::ExitCode::FAILURE
}

pub async fn run(command: &VisualCommand, json: bool) -> std::process::ExitCode {
    let applied = match command {
        VisualCommand::List => {
            return match control::catalog().await {
                Ok(catalog) if json => {
                    println!(
                        "{}",
                        json!({"scenes": catalog.scenes, "presets": catalog.presets})
                    );
                    std::process::ExitCode::SUCCESS
                }
                Ok(catalog) => {
                    println!("scenes:  {}", catalog.scenes.join(" "));
                    println!("presets: {}", catalog.presets.join(" "));
                    std::process::ExitCode::SUCCESS
                }
                Err(error) => fail(&error, json),
            };
        }
        VisualCommand::Capture => {
            return match control::capture().await {
                Ok(path) if json => {
                    println!("{}", json!({"ok": true, "path": path}));
                    std::process::ExitCode::SUCCESS
                }
                Ok(path) => {
                    println!("{path}");
                    std::process::ExitCode::SUCCESS
                }
                Err(error) => fail(&error, json),
            };
        }
        VisualCommand::Scene { name } => control::scene(name).await,
        VisualCommand::Preset { name } => control::preset(name).await,
        VisualCommand::Focus { id } => control::focus(id).await,
        VisualCommand::Camera { movement } => control::camera(movement).await,
        VisualCommand::Window { state } => control::window(state).await,
        VisualCommand::Pointer { x, y, click } => control::pointer(*x, *y, *click).await,
        VisualCommand::Highlight { id, off } => control::highlight(id, !*off).await,
        VisualCommand::Popup { text, seconds } => control::popup(text, *seconds).await,
        VisualCommand::Freeze { off } => control::freeze(!*off).await,
    };
    match applied {
        Ok(()) if json => {
            println!("{}", json!({"ok": true}));
            std::process::ExitCode::SUCCESS
        }
        Ok(()) => {
            println!("ok");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => fail(&error, json),
    }
}
