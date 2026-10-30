//! Carrying out, on the stage, what was decided.
//!
//! [`crate::act::decide`] turns a proposal into a [`Plan`] or a refusal; this does the plan,
//! through the same seat everything else on the stage goes through: a one-operation lease
//! from policy, a sealed command, the compositor's receipt. Nothing here widens what the
//! stage allows. An application is opened only as its policy entry spells it out, text is
//! typed only where the policy lets text be typed, and a person who takes the mouse keeps
//! it — the compositor refuses the command, and the refusal is reported, not retried.
//!
//! Every result comes back as a short sentence for the model, in the person's language:
//! what happened, or that it did not happen and why. The model hears it on its next step
//! and either carries on or tells the person. A command that was refused is never reported
//! as done.

use crate::act::Plan;
use archie_iris::{LocateAnythingClient, NodeId, NodeKind};
use archie_policy::Principal;
use archie_sight::{Sight, SightError, Target};
use archie_stage::{ActOutcome, ActRequest, StageAction, StageText, StageWorld};
use archie_types::{Locality, PrivacyGrade, Timestamp, TraceId};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

/// How long an application gets to put a window on the stage after it is started.
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(25);

/// What became of one plan, as it will be told to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Performed {
    pub done: bool,
    pub report: String,
}

impl Performed {
    fn done(report: impl Into<String>) -> Self {
        Self {
            done: true,
            report: report.into(),
        }
    }

    fn not_done(why: impl AsRef<str>) -> Self {
        Self {
            done: false,
            report: format!("No se hizo: {}", why.as_ref()),
        }
    }
}

/// Where applications started by voice put their windows.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    /// The stage's Wayland display, given to the application as `WAYLAND_DISPLAY`.
    pub wayland_display: Option<String>,
    /// Where what the application prints goes. Nowhere, unless a file is named.
    pub log: Option<std::path::PathBuf>,
}

/// A grounding model that finds, in a picture of a window, what the person named.
pub struct Grounding {
    pub client: Arc<LocateAnythingClient>,
    pub locality: Locality,
}

/// Archie's hands on one stage.
pub struct Hands<R, W> {
    sight: Sight<R, W>,
    grounding: Option<Grounding>,
    launch: Launch,
    principal: Principal,
    /// The grade of what the person said, which is the grade of any text typed from it.
    grade: PrivacyGrade,
    started: Vec<Child>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Hands<R, W> {
    pub fn new(
        sight: Sight<R, W>,
        principal: Principal,
        grade: PrivacyGrade,
        launch: Launch,
    ) -> Self {
        Self {
            sight,
            grounding: None,
            launch,
            principal,
            grade,
            started: Vec::new(),
        }
    }

    /// Lets the hands click on things found by looking. Without it, a click is refused.
    pub fn seeing(mut self, grounding: Grounding) -> Self {
        self.grounding = Some(grounding);
        self
    }

    pub fn world(&self) -> &StageWorld {
        self.sight.world()
    }

    /// The application with the keyboard focus, which is what "type this" means.
    pub fn front(&self) -> Option<String> {
        let world = self.sight.world();
        let focus = world.focus()?;
        world.surface(focus).map(|surface| surface.app_id.clone())
    }

    /// Does the plan, and says what came of it.
    pub async fn perform(&mut self, plan: &Plan) -> Performed {
        self.reap();
        match plan {
            Plan::Open { app_id, argv } => self.open(app_id, argv).await,
            Plan::Click { app_id, target } => self.click(app_id, target).await,
            Plan::Type { app_id, text } => {
                let action = StageAction::Text {
                    text: StageText::new(text.clone()),
                };
                self.act(app_id, action, "Tecleado en la ventana al frente.")
                    .await
            }
            Plan::Keys { app_id, chord } => {
                let done = format!("Pulsé {}.", chord_name(chord));
                self.act(
                    app_id,
                    StageAction::Key {
                        chord: chord.clone(),
                    },
                    &done,
                )
                .await
            }
            Plan::Scroll {
                app_id,
                down,
                steps,
            } => {
                let Some((width, height)) = self.window_size(app_id) else {
                    return Performed::not_done(format!("{app_id} no tiene una ventana abierta."));
                };
                let action = StageAction::Scroll {
                    x: width / 2,
                    y: height / 2,
                    steps_x: 0,
                    steps_y: if *down { *steps } else { -*steps },
                };
                let done = if *down {
                    "Bajé la vista."
                } else {
                    "Subí la vista."
                };
                self.act(app_id, action, done).await
            }
            // A drawing runs for minutes on a connection of its own, beside the
            // conversation; one pair of hands doing one operation at a time is not where.
            Plan::Draw { .. } | Plan::StopDrawing => Performed::not_done(
                "Dibujar se hace desde una conversación con el Stage, no desde aquí.",
            ),
        }
    }

    async fn open(&mut self, app_id: &str, argv: &[String]) -> Performed {
        if self.window_size(app_id).is_some() {
            // Already open: bring it forward rather than start a second one.
            return self
                .act(
                    app_id,
                    StageAction::Focus,
                    "Ya estaba abierta; ahora está al frente.",
                )
                .await;
        }
        let Some((program, args)) = argv.split_first() else {
            return Performed::not_done("no sé cómo se abre.");
        };
        let mut command = Command::new(program);
        command.args(args).stdin(Stdio::null());
        let log = self.launch.log.as_ref().and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        });
        match log
            .as_ref()
            .and_then(|file| Some((file.try_clone().ok()?, file.try_clone().ok()?)))
        {
            Some((out, err)) => command.stdout(out).stderr(err),
            None => command.stdout(Stdio::null()).stderr(Stdio::null()),
        };
        if let Some(display) = &self.launch.wayland_display {
            command
                .env("WAYLAND_DISPLAY", display)
                .env("QT_QPA_PLATFORM", "wayland")
                .env("MOZ_ENABLE_WAYLAND", "1")
                .env("GDK_BACKEND", "wayland");
        }
        match command.spawn() {
            Ok(child) => self.started.push(child),
            Err(error) => return Performed::not_done(format!("no se pudo iniciar ({error}).")),
        }
        let wanted = app_id.to_string();
        let appeared = self
            .sight
            .wait_until(OPEN_TIMEOUT, move |world| {
                world
                    .surfaces()
                    .any(|surface| surface.app_id == wanted && !surface.popup)
            })
            .await;
        match appeared {
            Ok(true) => Performed::done("Abierta; su ventana está en la pantalla."),
            Ok(false) => Performed::not_done(format!(
                "se inició, pero en {} segundos no apareció su ventana.",
                OPEN_TIMEOUT.as_secs()
            )),
            Err(error) => Performed::not_done(explain(&error)),
        }
    }

    async fn click(&mut self, app_id: &str, target: &str) -> Performed {
        let Some(grounding) = &self.grounding else {
            return Performed::not_done(
                "todavía no puedo ver la pantalla para encontrar dónde hacer clic.",
            );
        };
        let node = match NodeId::new("voice:target".to_string()) {
            Ok(node) => node,
            Err(_) => return Performed::not_done("no pude nombrar lo que buscaba."),
        };
        let wanted = Target {
            app_id: app_id.to_string(),
            node,
            kind: kind_of(target),
            description: target.to_string(),
            description_grade: self.grade,
            screen_grade: self.grade,
            task_id: format!("voice-{}", TraceId::new()),
        };
        let operation = (Timestamp::now().as_millis() as u64).max(1);
        match self
            .sight
            .find_and_activate(&wanted, &grounding.client, grounding.locality, operation)
            .await
        {
            Ok(sighting) if sighting.outcome.outcome_code == "clicked_and_repainted" => {
                Performed::done("Hice clic, y la ventana respondió.")
            }
            Ok(_) => Performed::done("Hice clic, pero la ventana no cambió."),
            Err(error) => Performed::not_done(explain(&error)),
        }
    }

    async fn act(&mut self, app_id: &str, action: StageAction, done: &str) -> Performed {
        let request = ActRequest {
            app_id: app_id.to_string(),
            surface: None,
            action,
            text_grade: self.grade,
            principal: self.principal.clone(),
            task_id: None,
            user_present: true,
            // Voice never approves on a person's behalf: what policy holds for approval
            // waits for them.
            approved: false,
            justification: "asked for out loud".into(),
        };
        match self.sight.act(&request).await {
            Ok(outcome) => reported(&outcome, app_id, done),
            Err(error) => Performed::not_done(explain(&error)),
        }
    }

    /// The size of the application's main window, if it has one on the stage.
    fn window_size(&self, app_id: &str) -> Option<(i32, i32)> {
        self.sight
            .world()
            .surfaces()
            .find(|surface| surface.app_id == app_id && !surface.popup)
            .map(|surface| (surface.width, surface.height))
    }

    /// Collects applications that have exited, so none lingers as a zombie.
    fn reap(&mut self) {
        self.started
            .retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }
}

/// What a receipt means, told plainly. Only `executed` is done.
fn reported(outcome: &ActOutcome, app_id: &str, done: &str) -> Performed {
    match outcome {
        ActOutcome::Receipt { outcome, .. } if outcome == "executed" => Performed::done(done),
        ActOutcome::Receipt { outcome, .. } if outcome == "operator_has_control" => {
            Performed::not_done("la persona tiene el mouse y el teclado ahora.")
        }
        ActOutcome::Receipt { sequence: 0, .. } => {
            Performed::not_done("la pantalla no pudo verificar la orden.")
        }
        ActOutcome::Receipt { outcome, .. } => {
            Performed::not_done(format!("la pantalla respondió «{outcome}»."))
        }
        ActOutcome::NeedsApproval { .. } => {
            Performed::not_done("necesita que la persona lo apruebe.")
        }
        ActOutcome::Refused { code, .. } => {
            Performed::not_done(format!("la política no lo permite ({code})."))
        }
        ActOutcome::NoSurface => {
            Performed::not_done(format!("{app_id} no tiene una ventana abierta."))
        }
    }
}

fn chord_name(chord: &archie_stage::KeyChord) -> String {
    use archie_stage::Modifier;
    let mut parts: Vec<String> = chord
        .modifiers
        .iter()
        .map(|modifier| {
            match modifier {
                Modifier::Ctrl => "ctrl",
                Modifier::Shift => "shift",
                Modifier::Alt => "alt",
                Modifier::Super => "super",
            }
            .to_string()
        })
        .collect();
    parts.push(chord.key.name());
    parts.join("+")
}

/// A guess at what kind of control the words describe; grounding looks for that kind.
fn kind_of(target: &str) -> NodeKind {
    let lowered = target.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| lowered.contains(word));
    if has(&["enlace", "link", "vínculo"]) {
        NodeKind::Link
    } else if has(&[
        "campo",
        "cuadro",
        "barra",
        "casilla de texto",
        "buscador",
        "input",
    ]) {
        NodeKind::Input
    } else if has(&["pestaña", "tab"]) {
        NodeKind::Tab
    } else if has(&["opción del menú", "elemento del menú"]) {
        NodeKind::MenuItem
    } else if has(&["casilla", "checkbox"]) {
        NodeKind::Checkbox
    } else {
        NodeKind::Button
    }
}

fn explain(error: &SightError) -> String {
    let mut message = error.to_string();
    let mut cause = std::error::Error::source(error);
    while let Some(inner) = cause {
        let text = inner.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        cause = inner.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_executed_receipt_is_reported_as_done() {
        let executed = ActOutcome::Receipt {
            sequence: 4,
            outcome: "executed".into(),
            finished: 0,
        };
        assert!(reported(&executed, "firefox", "Escrito.").done);
        for outcome in [
            ActOutcome::Receipt {
                sequence: 5,
                outcome: "operator_has_control".into(),
                finished: 0,
            },
            ActOutcome::Receipt {
                sequence: 0,
                outcome: "command_unverified".into(),
                finished: 0,
            },
            ActOutcome::Receipt {
                sequence: 6,
                outcome: "cancelled".into(),
                finished: 0,
            },
            ActOutcome::NeedsApproval {
                reason: "enter".into(),
            },
            ActOutcome::Refused {
                code: "no_lease",
                message: "denied".into(),
            },
            ActOutcome::NoSurface,
        ] {
            let told = reported(&outcome, "firefox", "Escrito.");
            assert!(!told.done, "{outcome:?}");
            assert!(told.report.starts_with("No se hizo"), "{}", told.report);
        }
        assert!(reported(
            &ActOutcome::Receipt {
                sequence: 5,
                outcome: "operator_has_control".into(),
                finished: 0,
            },
            "firefox",
            ""
        )
        .report
        .contains("la persona tiene el mouse"));
    }

    #[test]
    fn the_words_for_a_control_suggest_its_kind() {
        assert_eq!(kind_of("el enlace de noticias"), NodeKind::Link);
        assert_eq!(kind_of("la barra de direcciones"), NodeKind::Input);
        assert_eq!(kind_of("la pestaña de configuración"), NodeKind::Tab);
        assert_eq!(kind_of("Guardar"), NodeKind::Button);
    }
}
