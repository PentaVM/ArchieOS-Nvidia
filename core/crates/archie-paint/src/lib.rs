//! Turning a picture into strokes.
//!
//! A pen goes down, along, and up. That is the whole vocabulary, and everything a drawing
//! is has to be said in it. This plans what to draw from a picture and a rectangle to draw
//! it in, and decides nothing else: the stage gate still has to allow every stroke, the
//! compositor still re-checks each one against the window, and the application still
//! decides what a line does to it.
//!
//! The method is hatching, the way an engraver shades: parallel lines where the picture is
//! dark, crossed at a different angle where it is darker still. One pen, four passes, and
//! tone comes out of how much of the paper is left white.

pub mod bench;
mod flow;
mod image;
mod paper;
pub mod photopea;
pub mod session;

pub use flow::{length_of, plan as flow_plan, Flow};
pub use image::{Picture, PictureError};
pub use paper::{paper_in, PAPER};

use archie_stage::StrokePoint;
pub use archie_stage::MAX_STROKE_PATHS;

/// Where on a surface to draw, in that surface's own pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canvas {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Canvas {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The same rectangle with a margin taken off every side, which is where a window's
    /// own frame and buttons usually are.
    pub fn inset(&self, margin: u32) -> Self {
        let margin = margin.min(self.width / 2).min(self.height / 2);
        Self {
            x: self.x + margin as i32,
            y: self.y + margin as i32,
            width: self.width - margin * 2,
            height: self.height - margin * 2,
        }
    }
}

/// One pass of hatching: lines at one angle, drawn only where the picture is at least this
/// dark, that far apart. Several passes at different angles, closer together as the tone
/// gets darker, are how an engraver makes grey out of a black pen and white paper.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pass {
    pub angle_degrees: f32,
    pub darker_than: u8,
    pub spacing: u32,
}

/// How to hatch. The defaults are what the demo canvas was drawn with.
#[derive(Debug, Clone, PartialEq)]
pub struct Hatching {
    pub passes: Vec<Pass>,
    /// A run shorter than this is not worth lifting the pen for.
    pub min_run: u32,
    /// The most points one stroke may hold; a longer run is split into several strokes.
    pub max_points: usize,
}

impl Hatching {
    /// Hatching from tone levels: how dark a part of the picture has to be before a pass
    /// of the pen touches it, and how far apart that pass's lines are. The angles are
    /// spread evenly over the half turn, so no two passes lie on each other.
    pub fn from_levels(levels: &[(u8, u32)]) -> Self {
        let spread = 180.0 / levels.len().max(1) as f32;
        Self {
            passes: levels
                .iter()
                .enumerate()
                .map(|(at, (darker_than, spacing))| Pass {
                    angle_degrees: at as f32 * spread,
                    darker_than: *darker_than,
                    spacing: (*spacing).max(1),
                })
                .collect(),
            ..Self::default()
        }
    }

    /// The same hatching drawn coarser or finer: `lightest` is how far apart the lightest
    /// pass's lines are, and the darker passes keep their share of that.
    pub fn spread(mut self, lightest: u32) -> Self {
        let Some(base) = self.passes.first().map(|pass| pass.spacing.max(1)) else {
            return self;
        };
        for pass in &mut self.passes {
            pass.spacing = ((pass.spacing as f32 / base as f32) * lightest as f32)
                .round()
                .max(1.0) as u32;
        }
        self
    }
}

impl Default for Hatching {
    fn default() -> Self {
        Self {
            // Five tones, from the faintest shading on a face to the black of a dress.
            passes: vec![
                Pass {
                    angle_degrees: 0.0,
                    darker_than: 70,
                    spacing: 14,
                },
                Pass {
                    angle_degrees: 36.0,
                    darker_than: 110,
                    spacing: 11,
                },
                Pass {
                    angle_degrees: 72.0,
                    darker_than: 150,
                    spacing: 8,
                },
                Pass {
                    angle_degrees: 108.0,
                    darker_than: 190,
                    spacing: 6,
                },
                Pass {
                    angle_degrees: 144.0,
                    darker_than: 225,
                    spacing: 5,
                },
            ],
            min_run: 4,
            max_points: 2,
        }
    }
}

/// The strokes to draw, in the order to draw them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub strokes: Vec<Vec<StrokePoint>>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }

    /// How far the pen travels with its tip down, in surface pixels: the drawing itself,
    /// without the reaching between strokes.
    pub fn drawn_length(&self) -> f32 {
        self.strokes
            .iter()
            .flat_map(|stroke| stroke.windows(2))
            .map(|step| {
                let (dx, dy) = (
                    (step[1].x - step[0].x) as f32,
                    (step[1].y - step[0].y) as f32,
                );
                (dx * dx + dy * dy).sqrt()
            })
            .sum()
    }
}

/// Plans the strokes that hatch `picture` into `canvas`.
///
/// The picture is fitted into the canvas without stretching it: a tall picture leaves the
/// sides of a wide canvas alone, because a face squeezed sideways is not that face.
pub fn plan(picture: &Picture, canvas: Canvas, hatching: &Hatching) -> Plan {
    let mut plan = Plan::default();
    if canvas.width == 0 || canvas.height == 0 || picture.width() == 0 || picture.height() == 0 {
        return plan;
    }
    let fitted = fit(picture, canvas);
    let centre = (
        fitted.x as f32 + fitted.width as f32 / 2.0,
        fitted.y as f32 + fitted.height as f32 / 2.0,
    );

    for pass in &hatching.passes {
        let spacing = pass.spacing.max(1) as f32;
        let diagonal =
            ((fitted.width * fitted.width + fitted.height * fitted.height) as f32).sqrt() + spacing;
        let radians = pass.angle_degrees.to_radians();
        let (along, across) = (
            (radians.cos(), radians.sin()),
            (-radians.sin(), radians.cos()),
        );
        let lines = (diagonal / spacing) as i32;
        let mut forwards = true;
        for line in -lines / 2..=lines / 2 {
            let offset = line as f32 * spacing;
            let start = (
                centre.0 + across.0 * offset - along.0 * diagonal / 2.0,
                centre.1 + across.1 * offset - along.1 * diagonal / 2.0,
            );
            // A run of darkness along this line is one stroke of the pen.
            let mut run: Vec<StrokePoint> = Vec::new();
            let steps = diagonal as i32;
            for step in 0..=steps {
                let at = (
                    start.0 + along.0 * step as f32,
                    start.1 + along.1 * step as f32,
                );
                let point = StrokePoint {
                    x: at.0.round() as i32,
                    y: at.1.round() as i32,
                };
                let dark = fitted
                    .darkness_at(picture, point)
                    .is_some_and(|dark| dark >= pass.darker_than);
                if dark {
                    if run.last() != Some(&point) {
                        run.push(point);
                    }
                    continue;
                }
                finish(&mut plan, &mut run, hatching, forwards);
            }
            finish(&mut plan, &mut run, hatching, forwards);
            // The next line is drawn back the way this one came, so the pen travels the
            // short way between them rather than across the whole picture.
            forwards = !forwards;
        }
    }
    plan
}

/// Keeps a run that is long enough to be worth a stroke, as a line from end to end.
fn finish(plan: &mut Plan, run: &mut Vec<StrokePoint>, hatching: &Hatching, forwards: bool) {
    let taken = std::mem::take(run);
    let (Some(first), Some(last)) = (taken.first(), taken.last()) else {
        return;
    };
    let (dx, dy) = ((last.x - first.x) as f32, (last.y - first.y) as f32);
    if (dx * dx + dy * dy).sqrt() < hatching.min_run.max(1) as f32 {
        return;
    }
    let mut stroke = if hatching.max_points <= 2 {
        vec![*first, *last]
    } else {
        let step = taken.len().div_ceil(hatching.max_points).max(1);
        let mut kept: Vec<StrokePoint> = taken.iter().step_by(step).copied().collect();
        if kept.last() != Some(last) {
            kept.push(*last);
        }
        kept
    };
    if !forwards {
        stroke.reverse();
    }
    plan.strokes.push(stroke);
}

/// The line as the application will actually receive it.
///
/// A stroke is sent as a path, but the pointer is not a path: it is a place, reported again
/// and again. An application learns where the pointer went by being told where it is, at its
/// own frame rate, and between two frames it learns nothing. So a line drawn quickly reaches
/// it as a handful of positions joined by straight runs, however many points were sent.
///
/// This is the drawing the application will make of it: the stroke walked at `speed` pixels
/// a second and sampled `frames` times a second. Planning against anything else is planning
/// against a picture nobody will see.
pub fn seen_by(stroke: &[StrokePoint], speed: f32, frames: f32) -> Vec<StrokePoint> {
    if stroke.len() < 2 || speed <= 0.0 || frames <= 0.0 {
        return stroke.to_vec();
    }
    let every = speed / frames;
    let mut seen = vec![stroke[0]];
    let mut carried = 0.0_f32;
    for pair in stroke.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let (dx, dy) = ((to.x - from.x) as f32, (to.y - from.y) as f32);
        let run = (dx * dx + dy * dy).sqrt();
        if run <= 0.0 {
            continue;
        }
        let mut along = every - carried;
        while along <= run {
            let share = along / run;
            seen.push(StrokePoint {
                x: from.x + (dx * share).round() as i32,
                y: from.y + (dy * share).round() as i32,
            });
            along += every;
        }
        carried = (carried + run) % every;
    }
    let last = stroke[stroke.len() - 1];
    if seen.last() != Some(&last) {
        seen.push(last);
    }
    seen
}

/// Where the picture sits inside the canvas once it is fitted into it, and how to read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fitted {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The largest the picture can be drawn inside the canvas without stretching it, centred.
pub fn fit_into(picture: &Picture, canvas: Canvas) -> Fitted {
    fit(picture, canvas)
}

fn fit(picture: &Picture, canvas: Canvas) -> Fitted {
    let by_width = canvas.width as f32 / picture.width() as f32;
    let by_height = canvas.height as f32 / picture.height() as f32;
    let scale = by_width.min(by_height);
    let width = ((picture.width() as f32 * scale).floor() as u32).max(1);
    let height = ((picture.height() as f32 * scale).floor() as u32).max(1);
    Fitted {
        x: canvas.x + (canvas.width.saturating_sub(width) / 2) as i32,
        y: canvas.y + (canvas.height.saturating_sub(height) / 2) as i32,
        width,
        height,
    }
}

impl Fitted {
    /// Where a point of the canvas falls in the picture, in the picture's own pixels. The
    /// canvas is not the picture: a small picture drawn large, or a large one drawn small,
    /// and the distance from the corner has to be read at the picture's own scale.
    pub fn in_picture(&self, picture: &Picture, x: f32, y: f32) -> Option<(f32, f32)> {
        let (dx, dy) = (x - self.x as f32, y - self.y as f32);
        if dx < 0.0 || dy < 0.0 || dx >= self.width as f32 || dy >= self.height as f32 {
            return None;
        }
        Some((
            dx * picture.width() as f32 / self.width.max(1) as f32,
            dy * picture.height() as f32 / self.height.max(1) as f32,
        ))
    }

    /// How dark the picture is under a point of the canvas.
    pub fn darkness_at_canvas(&self, picture: &Picture, x: f32, y: f32) -> Option<u8> {
        self.darkness_at(
            picture,
            StrokePoint {
                x: x.round() as i32,
                y: y.round() as i32,
            },
        )
    }

    /// What colour the picture is under a point of the canvas.
    pub fn colour_at(&self, picture: &Picture, x: f32, y: f32) -> Option<[u8; 3]> {
        let (px, py) = self.in_picture(picture, x, y)?;
        picture.colour(px as u32, py as u32)
    }

    /// How dark the picture is under that point of the canvas, or nothing when the point is
    /// off the picture.
    fn darkness_at(&self, picture: &Picture, point: StrokePoint) -> Option<u8> {
        let (dx, dy) = (point.x - self.x, point.y - self.y);
        if dx < 0 || dy < 0 || dx as u32 >= self.width || dy as u32 >= self.height {
            return None;
        }
        let x = (dx as u32 as u64 * picture.width() as u64 / self.width as u64) as u32;
        let y = (dy as u32 as u64 * picture.height() as u64 / self.height as u64) as u32;
        picture.grey(x, y).map(|light| 255 - light)
    }
}

/// Draws a plan the way the canvas will: black line on white paper, in the canvas's own
/// size. What comes out is what the stage should end up with, without the stage.
///
/// This is how the hatching was tuned, and it is also the honest comparison to publish: the
/// picture as planned, beside the picture as drawn.
pub fn preview(plan: &Plan, canvas: Canvas, width_px: u32) -> Vec<u8> {
    let (w, h) = (canvas.width.max(1) as i64, canvas.height.max(1) as i64);
    let mut paper = vec![255_u8; (w * h) as usize];
    let mut ink = |x: i64, y: i64| {
        // As many pixels across as the pen is wide, centred on the line: a pen of two
        // covers two, not three. The preview is only worth looking at if it inks what the
        // canvas inks.
        let width = width_px.max(1) as i64;
        let from = -(width / 2);
        for dy in from..from + width {
            for dx in from..from + width {
                let (px, py) = (x + dx, y + dy);
                if px >= 0 && py >= 0 && px < w && py < h {
                    paper[(py * w + px) as usize] = 22;
                }
            }
        }
    };
    for stroke in &plan.strokes {
        for step in stroke.windows(2) {
            let (mut x, mut y) = ((step[0].x - canvas.x) as i64, (step[0].y - canvas.y) as i64);
            let (to_x, to_y) = ((step[1].x - canvas.x) as i64, (step[1].y - canvas.y) as i64);
            // A line of whole pixels, the way a pen leaves one.
            let (dx, dy) = ((to_x - x).abs(), -(to_y - y).abs());
            let (sx, sy) = (if x < to_x { 1 } else { -1 }, if y < to_y { 1 } else { -1 });
            let mut error = dx + dy;
            loop {
                ink(x, y);
                if x == to_x && y == to_y {
                    break;
                }
                let doubled = error * 2;
                if doubled >= dy {
                    error += dy;
                    x += sx;
                }
                if doubled <= dx {
                    error += dx;
                    y += sy;
                }
            }
        }
    }
    paper
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture with a black disc in the middle of white paper.
    fn disc(size: u32, radius: f32) -> Picture {
        let centre = size as f32 / 2.0;
        let pixels = (0..size * size)
            .map(|at| {
                let (x, y) = ((at % size) as f32, (at / size) as f32);
                let inside = (x - centre).powi(2) + (y - centre).powi(2) <= radius * radius;
                if inside {
                    0
                } else {
                    255
                }
            })
            .collect();
        Picture::from_grey(pixels, size, size).unwrap()
    }

    #[test]
    fn it_draws_where_the_picture_is_dark_and_nowhere_else() {
        let picture = disc(100, 30.0);
        let canvas = Canvas::new(10, 20, 200, 200);
        let plan = plan(&picture, canvas, &Hatching::default());
        assert!(!plan.is_empty(), "a black disc is something to draw");

        // Every point of every stroke is inside the disc as it lands on the canvas: the
        // disc is 30 of 100 wide, the canvas 200 square, so 60 of 200 from its centre.
        let centre = (10.0 + 100.0, 20.0 + 100.0);
        for stroke in &plan.strokes {
            for point in stroke {
                let away = ((point.x as f32 - centre.0).powi(2)
                    + (point.y as f32 - centre.1).powi(2))
                .sqrt();
                assert!(
                    away <= 62.0,
                    "a stroke at {:?} is {away} from the middle, outside the disc",
                    (point.x, point.y)
                );
            }
        }
    }

    #[test]
    fn white_paper_is_left_alone() {
        let white = Picture::from_grey(vec![255; 64 * 64], 64, 64).unwrap();
        let plan = plan(&white, Canvas::new(0, 0, 120, 120), &Hatching::default());
        assert!(plan.is_empty(), "nothing to draw on an empty picture");
    }

    #[test]
    fn a_darker_picture_is_drawn_with_more_of_the_pen() {
        let canvas = Canvas::new(0, 0, 160, 160);
        let grey = Picture::from_grey(vec![140; 64 * 64], 64, 64).unwrap();
        let darker = Picture::from_grey(vec![40; 64 * 64], 64, 64).unwrap();
        let light = plan(&grey, canvas, &Hatching::default());
        let heavy = plan(&darker, canvas, &Hatching::default());
        assert!(
            heavy.drawn_length() > light.drawn_length() * 1.5,
            "darker paper takes more line: {} against {}",
            heavy.drawn_length(),
            light.drawn_length()
        );
    }

    #[test]
    fn a_picture_keeps_its_shape_in_a_canvas_of_another_shape() {
        // A tall picture in a wide canvas is drawn tall, in the middle, not stretched.
        let tall = disc(40, 18.0);
        let plan = plan(&tall, Canvas::new(0, 0, 400, 100), &Hatching::default());
        assert!(!plan.is_empty());
        let (mut left, mut right) = (i32::MAX, i32::MIN);
        for point in plan.strokes.iter().flatten() {
            left = left.min(point.x);
            right = right.max(point.x);
        }
        assert!(
            left > 100 && right < 300,
            "the drawing sits in the middle of the canvas, {left}..{right}"
        );
    }

    #[test]
    fn a_point_of_the_canvas_is_read_at_the_pictures_own_scale() {
        // A small picture drawn large: the far corner of the drawing is the far corner of
        // the picture, not the point that many pixels along it.
        let small = disc(40, 18.0);
        let fitted = fit_into(&small, Canvas::new(0, 0, 400, 400));
        let (x, y) = fitted
            .in_picture(&small, 399.0, 399.0)
            .expect("the last point of the canvas is on the picture");
        assert!(
            x > 38.0 && x < 40.0 && y > 38.0 && y < 40.0,
            "the corner of the canvas is the corner of the picture, not ({x},{y})"
        );
        let (mx, my) = fitted
            .in_picture(&small, 200.0, 200.0)
            .expect("the middle of the canvas is on the picture");
        assert!(
            (mx - 20.0).abs() < 1.5 && (my - 20.0).abs() < 1.5,
            "the middle is the middle, not ({mx},{my})"
        );
    }

    #[test]
    fn a_line_reaches_the_application_as_the_positions_it_had_time_to_see() {
        // Six hundred pixels at six hundred a second is one second, and an application that
        // looks sixty times a second sees sixty of them, however many were sent.
        let sent: Vec<StrokePoint> = (0..=600).map(|at| StrokePoint { x: at, y: 0 }).collect();
        let seen = seen_by(&sent, 600.0, 60.0);
        assert!(
            (59..=62).contains(&seen.len()),
            "about sixty positions, not {}",
            seen.len()
        );
        assert_eq!(seen[0], sent[0], "it starts where the line starts");
        assert_eq!(
            seen[seen.len() - 1],
            sent[sent.len() - 1],
            "and ends where the line ends"
        );
        // Drawn slower, it sees more of the same line.
        assert!(seen_by(&sent, 200.0, 60.0).len() > seen.len() * 2);
    }
}

#[cfg(test)]
mod ink_on_paper {
    use super::*;

    #[test]
    fn the_preview_inks_the_line_and_leaves_the_paper() {
        let canvas = Canvas::new(0, 0, 40, 20);
        let plan = Plan {
            strokes: vec![vec![
                StrokePoint { x: 5, y: 10 },
                StrokePoint { x: 35, y: 10 },
            ]],
        };
        let paper = preview(&plan, canvas, 1);
        assert_eq!(paper.len(), 40 * 20);
        assert_eq!(paper[10 * 40 + 20], 22, "the line is on the paper");
        assert_eq!(paper[2 * 40 + 20], 255, "and nothing else is");
    }
}

/// The same as `preview`, for a plan that is drawn in several colours: each group in the
/// colour it will be drawn with, on white paper. What comes out is what the canvas should
/// end up with, without the canvas.
pub fn preview_in_colour(
    groups: &[(usize, Plan)],
    canvas: Canvas,
    palette: &[[u8; 3]],
    width_px: u32,
) -> Vec<u8> {
    let (w, h) = (canvas.width.max(1) as usize, canvas.height.max(1) as usize);
    let mut paper = vec![255_u8; w * h * 3];
    for (at, plan) in groups {
        let ink = palette.get(*at).copied().unwrap_or([20, 20, 20]);
        let marks = preview(plan, canvas, width_px);
        for (pixel, mark) in marks.iter().enumerate() {
            if *mark < 128 {
                paper[pixel * 3] = ink[0];
                paper[pixel * 3 + 1] = ink[1];
                paper[pixel * 3 + 2] = ink[2];
            }
        }
    }
    paper
}
