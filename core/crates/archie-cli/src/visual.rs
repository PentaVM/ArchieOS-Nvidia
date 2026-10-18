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
        VisualCommand::Scene { name } => control::scene(name).await,
        VisualCommand::Preset { name } => control::preset(name).await,
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
