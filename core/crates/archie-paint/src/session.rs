//! Drawing on a stage window, as a library: look at the window, find the paper, and send a
//! plan's lines through the gate, saying how far along it is so a caller can narrate it.
//!
//! This is what `archie-paint` does on a stage, taken out of the command so that Archie's
//! voice can draw too. Nothing here widens what the stage allows: every look and every
//! line is an ordinary request — the stage policy, a one-operation lease, a sealed command
//! and the compositor's own check against the window as it is now.

use crate::{Canvas, Picture};
use archie_policy::Principal;
use archie_stage::{
    ActOutcome, ActRequest, KeyChord, LookOutcome, Operator, PointerButton, StageAction,
    StageClient, StageWorld, StrokePoint,
};
use archie_types::PrivacyGrade;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};

/// How long one command may take to be answered. A batch of lines at a person's tempo is
/// several seconds of pointer travel, so this is generous.
pub const RECEIPT_TIMEOUT: Duration = Duration::from_secs(90);
/// How long a line waits for a person who took the stage before giving up on it.
pub const PATIENCE: Duration = Duration::from_secs(120);

/// One application's window on a stage, and the hands that work it.
pub struct Seat<'a, R, W> {
    pub operator: &'a mut Operator,
    pub client: &'a mut StageClient<R, W>,
    pub world: &'a mut StageWorld,
    pub app_id: String,
    pub surface: u32,
    pub principal: Principal,
    /// Where the compositor beside the control socket writes its captures.
    pub snapshots: PathBuf,
}

/// What one look saw: the picture, and where in the window it starts.
pub struct Sight {
    pub picture: Picture,
    pub x: i32,
    pub y: i32,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Seat<'_, R, W> {
    fn request(&self, action: StageAction, why: &str) -> ActRequest {
        ActRequest {
            app_id: self.app_id.clone(),
            surface: Some(self.surface),
            action,
            text_grade: PrivacyGrade::Public,
            principal: self.principal.clone(),
            task_id: None,
            user_present: true,
            approved: false,
            justification: why.to_string(),
        }
    }

    /// One action, answered.
    pub async fn act(&mut self, action: StageAction, why: &str) -> Result<ActOutcome, String> {
        let request = self.request(action, why);
        self.operator
            .act(self.client, self.world, &request, RECEIPT_TIMEOUT)
            .await
            .map_err(|e| e.to_string())
    }

    /// One action that must be carried out; anything else says why not.
    pub async fn must(&mut self, action: StageAction, why: &str) -> Result<(), String> {
        match self.act(action, why).await? {
            outcome if outcome.executed() => Ok(()),
            other => Err(format!("{why}: {}", refusal_of(&other))),
        }
    }

    pub async fn click(&mut self, x: i32, y: i32, why: &str) -> Result<(), String> {
        self.must(
            StageAction::Click {
                x,
                y,
                button: PointerButton::Left,
            },
            why,
        )
        .await
    }

    pub async fn key(&mut self, chord: KeyChord, why: &str) -> Result<(), String> {
        self.must(StageAction::Key { chord }, why).await
    }

    /// Looks at the window once, through the same gate and read lease as any capture.
    pub async fn look(&mut self, why: &str) -> Result<Sight, String> {
        let request = self.request(StageAction::Snapshot { region: None }, why);
        let outcome = self
            .operator
            .look(
                self.client,
                self.world,
                &request,
                &self.snapshots,
                RECEIPT_TIMEOUT,
            )
            .await
            .map_err(|e| e.to_string())?;
        match outcome {
            LookOutcome::Captured(snapshot) => Ok(Sight {
                picture: Picture::from_png(&snapshot.png).map_err(|e| e.to_string())?,
                x: snapshot.x,
                y: snapshot.y,
            }),
            LookOutcome::NotCaptured(outcome) => Err(format!(
                "the window was not captured: {}",
                refusal_of(&outcome)
            )),
        }
    }

    /// The largest light rectangle in the window, which is where paper is.
    pub async fn find_paper(&mut self) -> Result<Canvas, String> {
        let sight = self.look("look for the paper to draw on").await?;
        let paper = crate::paper_in(&sight.picture, crate::PAPER)
            .ok_or("there is no paper in that window to draw on")?;
        Ok(Canvas::new(
            paper.x + sight.x,
            paper.y + sight.y,
            paper.width,
            paper.height,
        ))
    }
}

/// Says what an outcome that was not `executed` was, in a few words.
pub fn refusal_of(outcome: &ActOutcome) -> String {
    match outcome {
        ActOutcome::Receipt { outcome, .. } => outcome.clone(),
        ActOutcome::NeedsApproval { reason } => format!("needs approval: {reason}"),
        ActOutcome::Refused { code, message } => format!("{code}: {message}"),
        ActOutcome::NoSurface => "the window is not on the stage".into(),
    }
}

/// One thing to do on the way to a picture.
#[derive(Debug, Clone, PartialEq)]
pub enum Deed {
    /// Change colour by pressing a key the application gives for it (Photopea's `x` swaps
    /// its two colours).
    Key(KeyChord, usize),
    /// Change colour by clicking the swatch the application offers for it.
    Swatch(i32, i32, usize),
    /// Lines drawn one after another under a single authorisation.
    Lines(Vec<Vec<StrokePoint>>),
}

impl Deed {
    pub fn lines(&self) -> usize {
        match self {
            Deed::Lines(paths) => paths.len(),
            _ => 0,
        }
    }
}

/// The lines grouped into commands of at most `most` lines (and the stage's limit on points).
pub fn in_batches(lines: Vec<Vec<StrokePoint>>, most: usize) -> Vec<Deed> {
    let most = most.clamp(1, archie_stage::MAX_STROKE_PATHS);
    let mut deeds = Vec::new();
    let mut batch: Vec<Vec<StrokePoint>> = Vec::new();
    let mut points = 0;
    for line in lines {
        let full =
            batch.len() >= most || points + line.len() > archie_stage::MAX_STROKE_POINTS_IN_ALL;
        if full && !batch.is_empty() {
            deeds.push(Deed::Lines(std::mem::take(&mut batch)));
            points = 0;
        }
        points += line.len();
        batch.push(line);
    }
    if !batch.is_empty() {
        deeds.push(Deed::Lines(batch));
    }
    deeds
}

/// How a drawing is going, for whoever is telling it.
#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    /// A colour was picked up: the index of the colour in the plan's palette.
    Colour(usize),
    /// Lines drawn so far, of how many.
    Drew { lines: usize, of: usize },
    /// A person has the stage; the drawing waits for them.
    Waiting,
    /// The person let go, and the drawing carries on.
    Resumed,
    /// A command the stage did not carry out, and why.
    Refused(String),
}

/// What a drawing came to.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Summary {
    pub lines: usize,
    pub commands: usize,
    pub refused: usize,
    pub seconds: f32,
    pub waited_seconds: f32,
    pub pixels_of_line: f32,
    pub stopped: bool,
}

impl Summary {
    /// Pixels of line a second while drawing, which is what decides whether a picture
    /// finishes.
    pub fn pixels_per_second(&self) -> f32 {
        self.pixels_of_line / (self.seconds - self.waited_seconds).max(0.001)
    }
}

/// Carries out `deeds` in order. A person who takes the stage is waited for, and the lines
/// they interrupted are carried on rather than started again. `stop` ends the drawing
/// between commands.
pub async fn perform<R, W>(
    seat: &mut Seat<'_, R, W>,
    deeds: &[Deed],
    stop: &AtomicBool,
    on: &mut (dyn FnMut(Progress) + Send),
) -> Result<Summary, String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let total: usize = deeds.iter().map(Deed::lines).sum();
    let started = Instant::now();
    let mut summary = Summary::default();
    let mut waited = Duration::ZERO;
    for deed in deeds {
        if stop.load(Ordering::Relaxed) {
            summary.stopped = true;
            break;
        }
        let (mut action, why) = match deed {
            Deed::Key(chord, _) => (
                StageAction::Key {
                    chord: chord.clone(),
                },
                "change colour".to_string(),
            ),
            Deed::Swatch(x, y, _) => (
                StageAction::Click {
                    x: *x,
                    y: *y,
                    button: PointerButton::Left,
                },
                "pick up a colour the application offers".to_string(),
            ),
            Deed::Lines(paths) => (
                StageAction::Strokes {
                    paths: paths.clone(),
                    button: PointerButton::Left,
                },
                format!("draw {} strokes of a picture", paths.len()),
            ),
        };
        let mut carried = 0_usize;
        let mut waiting_since: Option<Instant> = None;
        let outcome = loop {
            let outcome = seat.act(action.clone(), &why).await?;
            summary.commands += 1;
            let held = matches!(
                &outcome,
                ActOutcome::Receipt { outcome, .. }
                    if outcome == "operator_has_control" || outcome == "preempted_by_operator"
                        || outcome == "stage_paused"
            );
            if let (ActOutcome::Receipt { finished, .. }, StageAction::Strokes { paths, button }) =
                (&outcome, &action)
            {
                if held && *finished > 0 && *finished < paths.len() {
                    carried += *finished;
                    action = StageAction::Strokes {
                        paths: paths[*finished..].to_vec(),
                        button: *button,
                    };
                }
            }
            if !held {
                break outcome;
            }
            let since = *waiting_since.get_or_insert_with(|| {
                on(Progress::Waiting);
                Instant::now()
            });
            if since.elapsed() >= PATIENCE || stop.load(Ordering::Relaxed) {
                break outcome;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        if let Some(since) = waiting_since {
            waited += since.elapsed();
            if outcome.executed() {
                on(Progress::Resumed);
            }
        }
        if outcome.executed() {
            match deed {
                Deed::Lines(paths) => {
                    summary.lines += paths.len();
                    summary.pixels_of_line +=
                        paths.iter().map(|p| crate::length_of(p)).sum::<f32>();
                    on(Progress::Drew {
                        lines: summary.lines,
                        of: total,
                    });
                }
                Deed::Key(_, colour) | Deed::Swatch(_, _, colour) => on(Progress::Colour(*colour)),
            }
        } else {
            summary.refused += 1;
            summary.lines += carried;
            on(Progress::Refused(refusal_of(&outcome)));
        }
    }
    summary.seconds = started.elapsed().as_secs_f32();
    summary.waited_seconds = waited.as_secs_f32();
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize) -> Vec<StrokePoint> {
        (0..n).map(|i| StrokePoint { x: i as i32, y: 0 }).collect()
    }

    #[test]
    fn batches_keep_to_the_stage_limits() {
        let lines: Vec<_> = (0..40).map(|_| line(10)).collect();
        let deeds = in_batches(lines, 16);
        assert_eq!(
            deeds.iter().map(Deed::lines).collect::<Vec<_>>(),
            vec![16, 16, 8]
        );
        let long: Vec<_> = (0..3)
            .map(|_| line(archie_stage::MAX_STROKE_POINTS_IN_ALL / 2 + 1))
            .collect();
        assert_eq!(
            in_batches(long, 64).len(),
            3,
            "no command carries too many points"
        );
    }
}
