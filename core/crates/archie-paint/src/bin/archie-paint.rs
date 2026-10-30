//! `archie-paint` â Archie draws a picture on the stage, one stroke at a time.
//!
//! ```text
//! archie-paint score --drawn <file.png> --against <file.png> [--lines N] [--commands N]
//!                    [--seconds S] [--refused N]
//! archie-paint --socket <path> --key-file <path> --policy <apps.toml> --app <app_id>
//!              --image <file.png> [--surface N] [--inset 40] [--spacing 5]
//!              [--canvas x,y,w,h] [--limit N] [--levels 70:14,110:11,...]
//!              [--pen 2] [--points 2] [--find-paper]
//!              [--flow [--seeding 7] [--step 4] [--steps 120] [--darker-than 96] [--least-form 0.4] [--most N] [--seconds S --speed 950 --per-command 0.11] [--batch 64] [--weight 1]
//!                      [--swatches "#141210,â¦"] [--palette x,y,step]]
//!              [--dry-run [--preview <file.png>] [--as-seen <px/s>] [--frames 60]]
//! ```
//!
//! Every stroke goes the same way as every other action: the stage policy has to allow
//! `stroke` for that application, a lease has to authorise the one operation, the command
//! is sealed, and the compositor re-checks each point against the window as it is now. A
//! picture is nothing special here; it is a few hundred ordinary commands in a row.
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A picture can be planned anywhere. Drawing one needs a stage, and a stage is a Unix
    // socket on the machine it runs on.
    if args.first().is_some_and(|first| first == "score") {
        return match scoring::main(args) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("archie-paint: {message}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    if args.iter().any(|arg| arg == "--dry-run") {
        return match planning::main(args) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("archie-paint: {message}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    #[cfg(unix)]
    {
        unix::main(args).await
    }
    #[cfg(not(unix))]
    {
        let _ = args;
        eprintln!(
            "archie-paint talks to the compositor over a Unix socket; run it on the stage's host, \
             or pass --dry-run to plan a picture here"
        );
        std::process::ExitCode::FAILURE
    }
}

/// What a drawing came to, measured against the picture it was of.
///
/// The same two files always give the same numbers: no sampling, no randomness, no clock.
/// That is the whole point of it â a score worth keeping is one that can be compared with
/// last week's, and a benchmark that wanders is not a benchmark.
///
/// Both pictures are reduced to the same grid of tiles and read as darkness from nothing to
/// one. Then, over the tiles:
///
///   said      = the darkness the drawing and the picture agree on, tile by tile
///   coverage  = said / the picture's darkness      — how much of it got said
///   restraint = said / the drawing's darkness      — how much of what was said belonged
///   balance   = the harmonic mean of the two, which neither a blank sheet nor a black one
///               can game: a blank sheet says nothing and covers nothing, a black one covers
///               everything and belongs nowhere
///
/// The score is that balance in thousandths. The counts alongside it — lines, commands,
/// seconds, pixels of line a second — are what it cost to get there, because a drawing is
/// only worth its score in the time it took.
mod scoring {
    use archie_paint::Picture;
    use std::path::Path;
    use std::process::ExitCode;

    /// How wide a tile is, in the drawing's own pixels. Small enough to tell a face from a
    /// shoulder, wide enough that a line an artist would call the same line lands in it.
    const TILE: u32 = 4;

    pub fn main(mut args: Vec<String>) -> Result<ExitCode, String> {
        args.remove(0);
        let drawn_at = super::take(&mut args, "--drawn").ok_or("--drawn is required")?;
        let against_at = super::take(&mut args, "--against").ok_or("--against is required")?;
        let lines: usize = super::number(&mut args, "--lines")?.unwrap_or(0);
        let commands: usize = super::number(&mut args, "--commands")?.unwrap_or(0);
        let refused: usize = super::number(&mut args, "--refused")?.unwrap_or(0);
        let seconds: f32 = super::number(&mut args, "--seconds")?.unwrap_or(0.0);
        if !args.is_empty() {
            return Err(format!("unexpected arguments: {args:?}"));
        }
        let drawn = Picture::read(Path::new(&drawn_at)).map_err(|e| e.to_string())?;
        let against = Picture::read(Path::new(&against_at)).map_err(|e| e.to_string())?;
        let marked = tiles(&drawn);
        let wanted = tiles(&against);

        let (mut said, mut ink, mut want) = (0.0_f32, 0.0_f32, 0.0_f32);
        let across = marked.0.min(wanted.0);
        let down = marked.1.min(wanted.1);
        for y in 0..down {
            for x in 0..across {
                let d = marked.2[y * marked.0 + x];
                let t = wanted.2[y * wanted.0 + x];
                said += d.min(t);
                ink += d;
                want += t;
            }
        }
        let coverage = if want > 0.0 { said / want } else { 0.0 };
        let restraint = if ink > 0.0 { said / ink } else { 0.0 };
        let balance = if coverage + restraint > 0.0 {
            2.0 * coverage * restraint / (coverage + restraint)
        } else {
            0.0
        };
        let score = (balance * 1000.0).round() as i64;

        println!("score {score} of 1000");
        println!("  coverage  {coverage:.3}  of the picture's darkness was said");
        println!("  restraint {restraint:.3}  of what was said belonged there");
        println!(
            "  drawn     {}x{} in tiles of {TILE}, {:.0} tiles of ink against {:.0} wanted",
            drawn.width(),
            drawn.height(),
            ink,
            want
        );
        if lines > 0 || commands > 0 || seconds > 0.0 {
            println!(
                "  cost      {lines} lines in {commands} commands, {refused} refused, {seconds:.1}s\n                 {}",
                if seconds > 0.0 && lines > 0 {
                    format!(" ({:.1} lines a second)", lines as f32 / seconds)
                } else {
                    String::new()
                }
            );
        }
        Ok(ExitCode::SUCCESS)
    }

    /// A picture as darkness per tile, from nothing to one.
    fn tiles(picture: &Picture) -> (usize, usize, Vec<f32>) {
        let across = (picture.width() / TILE).max(1) as usize;
        let down = (picture.height() / TILE).max(1) as usize;
        let mut darkness = vec![0.0_f32; across * down];
        for ty in 0..down {
            for tx in 0..across {
                let mut sum = 0.0_f32;
                let mut seen = 0.0_f32;
                for y in 0..TILE {
                    for x in 0..TILE {
                        let at_x = tx as u32 * TILE + x;
                        let at_y = ty as u32 * TILE + y;
                        if let Some(light) = picture.grey(at_x, at_y) {
                            sum += f32::from(255 - light) / 255.0;
                            seen += 1.0;
                        }
                    }
                }
                darkness[ty * across + tx] = if seen > 0.0 { sum / seen } else { 0.0 };
            }
        }
        (across, down, darkness)
    }
}

/// Everything about a picture that does not need a stage: what to draw, and where.
mod planning {
    use archie_paint::{Canvas, Hatching, Picture, Plan};
    use std::path::Path;
    use std::process::ExitCode;

    pub fn main(mut args: Vec<String>) -> Result<ExitCode, String> {
        let image = super::take(&mut args, "--image").ok_or("--image is required")?;
        let inset: u32 = super::number(&mut args, "--inset")?.unwrap_or(40);
        let spacing: u32 = super::number(&mut args, "--spacing")?.unwrap_or(10);
        let min_run: u32 = super::number(&mut args, "--min-run")?.unwrap_or(4);
        let preview = super::take(&mut args, "--preview");
        // The speed the stage will draw at, so the preview can show what will land rather
        // than what was asked for.
        let seen_at: Option<f32> = super::number(&mut args, "--as-seen")?;
        let frames: f32 = super::number(&mut args, "--frames")?.unwrap_or(60.0);
        let flowing = super::flag(&mut args, "--flow");
        let flow = super::flow_of(&mut args)?;
        let swatches = super::swatches_of(&mut args)?;
        let levels = super::levels_of(&mut args)?;
        let shares = super::shares_of(&mut args)?;
        // The canvas draws with a pen this wide, so the plan is made for one and the
        // preview is drawn with one.
        let pen: u32 = super::number(&mut args, "--pen")?.unwrap_or(2);
        let max_points: usize = super::number(&mut args, "--points")?.unwrap_or(2);
        let canvas = super::canvas_of(super::take(&mut args, "--canvas").as_deref(), None, inset)?;
        let soften: Option<u32> = super::number(&mut args, "--soften")?;
        let picture = super::prepared(Path::new(&image), soften)?;
        let asked = super::AsAsked {
            spacing,
            min_run,
            levels,
            shares: shares.clone(),
            pen,
            max_points,
        };
        if flowing {
            let mut groups =
                archie_paint::flow_plan(&picture, canvas, &flow, &swatches, max_points);
            // What the application will see of it, not what was planned: the pointer is a
            // place reported at a frame rate, and the drawing is made of those reports.
            if let Some(speed) = seen_at {
                let mut kept = 0_usize;
                for (_, plan) in &mut groups {
                    for stroke in &mut plan.strokes {
                        *stroke = archie_paint::seen_by(stroke, speed, frames);
                        kept += stroke.len();
                    }
                }
                println!(
                    "at {speed:.0} pixels a second the application sees {kept} positions of it, \n                     {frames:.0} a second"
                );
            }
            let strokes: usize = groups.iter().map(|(_, plan)| plan.strokes.len()).sum();
            let length: f32 = groups.iter().map(|(_, plan)| plan.drawn_length()).sum();
            println!(
                "{strokes} lines in {} colours, {length:.0} pixels of line, in a canvas of {}x{} at {},{}",
                groups.len(),
                canvas.width,
                canvas.height,
                canvas.x,
                canvas.y
            );
            super::say_how_covered(length, pen, canvas);
            let travelled = super::travel_of(&groups);
            super::say_what_it_costs(
                length,
                travelled,
                seen_at.unwrap_or(0.0),
                super::commands_for(&groups),
            );
            if let Some(out) = preview {
                let paper = archie_paint::preview_in_colour(&groups, canvas, &swatches, pen);
                write_colour(Path::new(&out), &paper, canvas.width, canvas.height)?;
                println!("drew the plan on paper: {out}");
            }
            return Ok(ExitCode::SUCCESS);
        }
        let plan = super::plan_with(&picture, canvas, &asked);
        report(&plan, canvas);
        println!(
            "tone levels from the picture itself: {:?}",
            levels_of(&picture, &shares, pen)
        );
        if let Some(out) = preview {
            let paper = archie_paint::preview(&plan, canvas, pen);
            write_grey(Path::new(&out), &paper, canvas.width, canvas.height)?;
            println!("drew the plan on paper: {out}");
        }
        Ok(ExitCode::SUCCESS)
    }

    /// The preview of a picture drawn in several colours.
    fn write_colour(path: &Path, rgb: &[u8], width: u32, height: u32) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(rgb))
            .map_err(|e| e.to_string())
    }

    /// The preview, as a plain grey PNG.
    fn write_grey(path: &Path, grey: &[u8], width: u32, height: u32) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(grey))
            .map_err(|e| e.to_string())
    }

    pub fn report(plan: &Plan, canvas: Canvas) {
        println!(
            "{} strokes, {:.0} pixels of line, in a canvas of {}x{} at {},{}",
            plan.strokes.len(),
            plan.drawn_length(),
            canvas.width,
            canvas.height,
            canvas.x,
            canvas.y
        );
    }

    /// How much of the picture each pass of the pen covers, and how far apart its lines
    /// are: the darkest half faintly, then less and less of it more and more closely.
    pub const SHARES: [f32; 6] = [0.95, 0.82, 0.62, 0.42, 0.24, 0.09];
    /// How far apart the lightest pass's lines are; a pass that covers less of the picture
    /// is drawn closer together, so that darker really does come out darker.
    pub const SPACING: u32 = 17;

    /// The tone levels this picture's own darkness puts them at.
    ///
    /// The pen has a width, and lines closer together than about three times it leave no
    /// paper between them: what was meant as a dark grey comes out solid black. That floor
    /// is why the planned picture and the drawn one now look like each other.
    pub fn levels_of(picture: &Picture, shares: &[f32], pen: u32) -> Vec<(u8, u32)> {
        let closest = (pen.max(1) * 3).max(3);
        picture
            .darkness_at(shares)
            .into_iter()
            .zip(shares.iter())
            .map(|(darkness, share)| {
                (
                    darkness,
                    ((SPACING as f32 * share.clamp(0.0, 1.0)).round() as u32).max(closest),
                )
            })
            .collect()
    }

    pub fn hatching(
        picture: &Picture,
        spacing: u32,
        min_run: u32,
        shares: &[f32],
        pen: u32,
        max_points: usize,
    ) -> Hatching {
        Hatching {
            min_run,
            max_points,
            ..Hatching::from_levels(&levels_of(picture, shares, pen))
        }
        .spread(spacing)
    }
}

use archie_paint::{Canvas, Picture, Plan};

/// The picture as the pen sees it: its tones pulled apart, and its grain taken out so that
/// what the pen follows is tone rather than noise.
fn prepared(path: &std::path::Path, soften: Option<u32>) -> Result<Picture, String> {
    let picture = Picture::read(path).map_err(|e| e.to_string())?;
    let radius = soften.unwrap_or_else(|| (picture.width() / 200).max(1));
    Ok(picture.softened(radius).stretched())
}

/// The swatches a drawing application is asked to draw with, and where to click for each.
/// Finding them by looking is the accessibility path's job; here they are given.
#[cfg_attr(not(unix), allow(dead_code))]
struct Palette {
    colours: Vec<[u8; 3]>,
    at: Option<(i32, i32, i32)>,
}

#[cfg_attr(not(unix), allow(dead_code))]
impl Palette {
    /// Where to click for swatch `at`, when the application's palette was described.
    fn swatch(&self, at: usize) -> Option<(i32, i32)> {
        let (x, y, step) = self.at?;
        Some((x + step * at as i32, y))
    }
}

/// `--swatches "#1b1a17,#6b4a2f,â¦"`, and the default the demo canvas has.
fn swatches_of(args: &mut Vec<String>) -> Result<Vec<[u8; 3]>, String> {
    let text = take(args, "--swatches")
        .unwrap_or_else(|| "#141210,#4a352a,#8a6a46,#c39c6d,#e6d3b3,#5d6b63".to_string());
    let mut colours = Vec::new();
    for part in text.split(',') {
        let hex = part.trim().trim_start_matches('#');
        if hex.len() != 6 {
            return Err("--swatches is colours like #1b1a17,#6b4a2f".into());
        }
        let mut colour = [0_u8; 3];
        for (at, pair) in hex.as_bytes().chunks(2).enumerate() {
            let pair = std::str::from_utf8(pair).map_err(|_| "a colour is hex")?;
            colour[at] = u8::from_str_radix(pair, 16).map_err(|_| "a colour is hex")?;
        }
        colours.push(colour);
    }
    if colours.is_empty() {
        return Err("--swatches needs at least one colour".into());
    }
    Ok(colours)
}

/// `--palette 40,1130,44`: the first swatch's middle, and how far apart they are.
#[cfg_attr(not(unix), allow(dead_code))]
fn palette_at(args: &mut Vec<String>) -> Result<Option<(i32, i32, i32)>, String> {
    let Some(text) = take(args, "--palette") else {
        return Ok(None);
    };
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 3 {
        return Err("--palette is x,y,step".into());
    }
    let mut numbers = [0_i32; 3];
    for (at, part) in parts.iter().enumerate() {
        numbers[at] = part
            .trim()
            .parse()
            .map_err(|_| "--palette is three numbers".to_string())?;
    }
    Ok(Some((numbers[0], numbers[1], numbers[2])))
}

/// How many commands a plan actually takes: the lines batched as far as one command may
/// carry them, and one more for each visit to the palette.
fn commands_for(groups: &[(usize, archie_paint::Plan)]) -> usize {
    groups
        .iter()
        .map(|(_, plan)| plan.strokes.len().div_ceil(archie_paint::MAX_STROKE_PATHS) + 1)
        .sum()
}

/// How far the pointer travels between the lines, which is time in which nothing is drawn.
fn travel_of(groups: &[(usize, archie_paint::Plan)]) -> f32 {
    let mut travelled = 0.0;
    let mut at: Option<archie_stage::StrokePoint> = None;
    for (_, plan) in groups {
        for stroke in &plan.strokes {
            if let (Some(from), Some(to)) = (at, stroke.first()) {
                let (dx, dy) = ((to.x - from.x) as f32, (to.y - from.y) as f32);
                travelled += (dx * dx + dy * dy).sqrt();
            }
            at = stroke.last().copied();
        }
    }
    travelled
}

/// What the plan will cost in time, which is the only budget a drawing really has.
///
/// An application looks at the pointer at its own rate and joins what it saw with straight
/// runs, so a line drawn faster than about fifteen pixels between looks arrives as a polygon.
/// That fixes how much line a second can carry â near a thousand pixels â and everything
/// else the stage does only decides how much of each second goes into line rather than into
/// travelling, waiting for a receipt, or drawing the stage's own cinematics.
fn say_what_it_costs(drawn: f32, travelled: f32, speed: f32, commands: usize) {
    let speed = if speed > 0.0 { speed } else { 950.0 };
    let drawing = drawn / speed;
    let travelling = travelled / speed;
    // Measured on this stage: about a tenth of a second for each command's round trip.
    let waiting = commands as f32 * 0.11;
    let total = drawing + travelling + waiting;
    println!(
        "at {speed:.0} pixels a second: {drawing:.1}s drawing, {travelling:.1}s travelling, \n         {waiting:.1}s waiting on {commands} commands -- {total:.1}s, {:.0} pixels of line \n         a second",
        if total > 0.0 { drawn / total } else { 0.0 }
    );
}

/// How much of the paper the pen will actually cover, and a word about it.
///
/// Lines closer together than the pen is wide leave no paper between them, and what was
/// planned as shading comes out as a filled shape. The plan knows this before anything is
/// drawn â it is the ink the pen lays down against the paper there is â and twenty minutes
/// of drawing is a long way to go to find it out.
fn say_how_covered(length: f32, pen: u32, canvas: archie_paint::Canvas) {
    let paper = (canvas.width as f32) * (canvas.height as f32);
    if paper <= 0.0 {
        return;
    }
    let covered = length * pen.max(1) as f32 / paper;
    let note = if covered < 0.35 {
        "paper between the lines"
    } else if covered < 0.8 {
        "close, but still a drawing"
    } else {
        "more ink than paper: the dark will fill in"
    };
    println!("the pen covers {covered:.2} of the paper â {note}");
}

/// How the lines that follow the form are to be drawn, as the command line says it.
fn flow_of(args: &mut Vec<String>) -> Result<archie_paint::Flow, String> {
    let mut flow = archie_paint::Flow::default();
    if let Some(seeding) = number(args, "--seeding")? {
        flow.seeding = seeding;
    }
    if let Some(step) = number::<f32>(args, "--step")? {
        flow.step = step;
    }
    if let Some(steps) = number(args, "--steps")? {
        flow.steps = steps;
    }
    if let Some(darker) = number(args, "--darker-than")? {
        flow.darker_than = darker;
    }
    if let Some(coherence) = number(args, "--coherence")? {
        flow.coherence = coherence;
    }
    if let Some(least) = number::<f32>(args, "--least-form")? {
        flow.least_form = least;
    }
    if let Some(most) = number(args, "--most")? {
        flow.most = most;
    }
    // The budget a drawing really has. `--speed` is what the stage will be told to draw at
    // and `--per-command` what one command costs before any of it is drawing, so that the
    // plan is made against the clock it will actually run on.
    if let Some(seconds) = number::<f32>(args, "--seconds")? {
        flow.seconds = seconds;
    }
    if let Some(speed) = number::<f32>(args, "--speed")? {
        flow.speed = speed;
    }
    if let Some(each) = number::<f32>(args, "--per-command")? {
        flow.per_command = each;
    }
    if let Some(weight) = number(args, "--weight")? {
        flow.weight = weight;
    }
    if let Some(each) = number::<f32>(args, "--per-line")? {
        flow.per_line = each;
    }
    if let Some(each) = number::<f32>(args, "--per-colour")? {
        flow.per_colour = each;
    }
    Ok(flow)
}

/// How a picture is to be hatched, as the command line says it.
struct AsAsked {
    spacing: u32,
    min_run: u32,
    levels: Option<Vec<(u8, u32)>>,
    shares: Vec<f32>,
    pen: u32,
    max_points: usize,
}

fn plan_with(picture: &Picture, canvas: Canvas, asked: &AsAsked) -> Plan {
    let hatching = match &asked.levels {
        Some(levels) => archie_paint::Hatching {
            min_run: asked.min_run,
            max_points: asked.max_points,
            ..archie_paint::Hatching::from_levels(levels)
        },
        None => planning::hatching(
            picture,
            asked.spacing,
            asked.min_run,
            &asked.shares,
            asked.pen,
            asked.max_points,
        ),
    };
    archie_paint::plan(picture, canvas, &hatching)
}

/// `--shares 0.85,0.65,0.45`: how much of the picture each pass of the pen covers, darkest
/// first. Without it, what `planning::SHARES` says.
fn shares_of(args: &mut Vec<String>) -> Result<Vec<f32>, String> {
    let Some(text) = take(args, "--shares") else {
        return Ok(planning::SHARES.to_vec());
    };
    let mut shares = Vec::new();
    for part in text.split(',') {
        shares.push(
            part.trim()
                .parse::<f32>()
                .map_err(|_| "--shares is numbers from 0 to 1".to_string())?,
        );
    }
    if shares.is_empty() {
        return Err("--shares needs at least one pass".into());
    }
    Ok(shares)
}

/// `--levels 70:14,110:11,150:8`: each pass of the pen, as how dark the picture has to be
/// before it touches, and how far apart its lines are.
fn levels_of(args: &mut Vec<String>) -> Result<Option<Vec<(u8, u32)>>, String> {
    let Some(text) = take(args, "--levels") else {
        return Ok(None);
    };
    let mut levels = Vec::new();
    for part in text.split(',') {
        let (darkness, spacing) = part
            .trim()
            .split_once(':')
            .ok_or("--levels is pairs like 70:14,110:11")?;
        levels.push((
            darkness
                .trim()
                .parse()
                .map_err(|_| "a level's darkness is 0 to 255".to_string())?,
            spacing
                .trim()
                .parse()
                .map_err(|_| "a level's spacing is a number of pixels".to_string())?,
        ));
    }
    if levels.is_empty() {
        return Err("--levels needs at least one pass".into());
    }
    Ok(Some(levels))
}

fn take(args: &mut Vec<String>, name: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == name)?;
    if at + 1 >= args.len() {
        return None;
    }
    args.remove(at);
    Some(args.remove(at))
}

#[cfg_attr(not(unix), allow(dead_code))]
fn flag(args: &mut Vec<String>, name: &str) -> bool {
    match args.iter().position(|arg| arg == name) {
        Some(at) => {
            args.remove(at);
            true
        }
        None => false,
    }
}

fn number<T: std::str::FromStr>(args: &mut Vec<String>, name: &str) -> Result<Option<T>, String> {
    match take(args, name) {
        Some(value) => Ok(Some(
            value.parse().map_err(|_| format!("{name} is a number"))?,
        )),
        None => Ok(None),
    }
}

/// Where to draw: what was asked for, or the window with a margin taken off it.
fn canvas_of(
    asked: Option<&str>,
    surface: Option<(i32, i32)>,
    inset: u32,
) -> Result<Canvas, String> {
    if let Some(text) = asked {
        let parts: Vec<&str> = text.split(',').collect();
        if parts.len() != 4 {
            return Err("--canvas is x,y,width,height".into());
        }
        let number = |at: usize| -> Result<i64, String> {
            parts[at]
                .trim()
                .parse()
                .map_err(|_| "--canvas is x,y,width,height".to_string())
        };
        let (width, height) = (number(2)?, number(3)?);
        if width <= 0 || height <= 0 {
            return Err("a canvas with no room in it is not a canvas".into());
        }
        return Ok(Canvas::new(
            number(0)? as i32,
            number(1)? as i32,
            width as u32,
            height as u32,
        ));
    }
    let (width, height) = surface.ok_or("without a stage, --canvas says where to draw")?;
    if width <= 0 || height <= 0 {
        return Err("that window has no room to draw in".into());
    }
    Ok(Canvas::new(0, 0, width as u32, height as u32).inset(inset))
}

#[cfg(unix)]
mod unix {

    use archie_paint::session::{self, Deed, Progress, Seat};
    use archie_paint::Canvas;
    use archie_policy::{PolicyEngine, Principal};
    use archie_stage::{learn, CommandKey, Operator, StageClient, StagePolicy, StageWorld};
    use std::path::Path;
    use std::process::ExitCode;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
    use tokio::net::UnixStream;

    pub async fn main(args: Vec<String>) -> ExitCode {
        match run(args).await {
            Ok(code) => code,
            Err(message) => {
                eprintln!("archie-paint: {message}");
                ExitCode::FAILURE
            }
        }
    }

    use super::{
        canvas_of, flag, flow_of, levels_of, number, palette_at, plan_with, prepared, shares_of,
        swatches_of, take, AsAsked, Palette,
    };

    async fn run(mut args: Vec<String>) -> Result<ExitCode, String> {
        let socket = take(&mut args, "--socket");
        let key_file = take(&mut args, "--key-file");
        let policy_path = take(&mut args, "--policy");
        let app = take(&mut args, "--app");
        let image = take(&mut args, "--image").ok_or("--image is required")?;
        let surface: Option<u32> = number(&mut args, "--surface")?;
        let inset: u32 = number(&mut args, "--inset")?.unwrap_or(40);
        let spacing: u32 = number(&mut args, "--spacing")?.unwrap_or(10);
        let min_run: u32 = number(&mut args, "--min-run")?.unwrap_or(4);
        let limit: Option<usize> = number(&mut args, "--limit")?;
        let canvas_arg = take(&mut args, "--canvas");
        // Where to draw can be looked for rather than given: an application that can be
        // drawn in has paper in it, and paper is the large light rectangle in its window.
        let find_paper = flag(&mut args, "--find-paper");
        let flowing = flag(&mut args, "--flow");
        let flow = flow_of(&mut args)?;
        let palette = Palette {
            colours: swatches_of(&mut args)?,
            at: palette_at(&mut args)?,
        };
        let levels = levels_of(&mut args)?;
        let shares = shares_of(&mut args)?;
        let pen: u32 = number(&mut args, "--pen")?.unwrap_or(2);
        // How many points one command may carry. A line of many points is one command, and
        // a command is what costs; this is the whole difference between a slow picture and
        // a fast one.
        let max_points: usize = number(&mut args, "--points")?.unwrap_or(2);
        // How many lines one command may carry. One at a time is how it worked before there
        // was a drawing command, and is kept so the difference can be measured rather than
        // asserted.
        let batch: usize = number(&mut args, "--batch")?.unwrap_or(archie_stage::MAX_STROKE_PATHS);
        let soften: Option<u32> = number(&mut args, "--soften")?;
        if !args.is_empty() {
            return Err(format!("unexpected arguments: {args:?}"));
        }

        let picture = prepared(Path::new(&image), soften)?;
        let socket = socket.ok_or("--socket is required")?;
        let app = app.ok_or("--app is required")?;
        let key = load_key(&key_file.ok_or("--key-file is required")?)?;
        let policy = StagePolicy::load(&policy_path.ok_or("--policy is required")?)
            .map_err(|e| e.to_string())?;
        let stream = UnixStream::connect(&socket)
            .await
            .map_err(|e| format!("{socket}: {e}"))?;
        let (read, write) = stream.into_split();
        let mut client = StageClient::new(read, write);
        let mut world = StageWorld::default();
        learn(&mut client, &mut world, Duration::from_millis(400))
            .await
            .map_err(|e| e.to_string())?;
        let found = match surface {
            Some(id) => world.surface(id).filter(|s| s.app_id == app).cloned(),
            None => world.surfaces_of(&app).next().cloned(),
        };
        let found = found.ok_or_else(|| format!("{app} has no surface on this stage"))?;
        let mut operator = Operator::new(key, policy, PolicyEngine::default());
        let mut seat = Seat {
            operator: &mut operator,
            client: &mut client,
            world: &mut world,
            app_id: app.clone(),
            surface: found.surface,
            principal: Principal::Human {
                id: "archie-paint".into(),
                authenticated: true,
            },
            snapshots: Path::new(&socket)
                .parent()
                .ok_or("--socket has no directory")?
                .join("snapshots"),
        };
        let canvas: Canvas = if find_paper {
            let paper = seat.find_paper().await?;
            println!(
                "the paper is {}x{} at {},{} of that window",
                paper.width, paper.height, paper.x, paper.y
            );
            paper.inset(inset)
        } else {
            canvas_of(
                canvas_arg.as_deref(),
                Some((found.width, found.height)),
                inset,
            )?
        };
        // What to do, in order: a colour to pick up, or a line to draw with it.
        let mut deeds: Vec<Deed> = Vec::new();
        if flowing {
            let groups =
                archie_paint::flow_plan(&picture, canvas, &flow, &palette.colours, max_points);
            let lines: usize = groups.iter().map(|(_, plan)| plan.strokes.len()).sum();
            let length: f32 = groups.iter().map(|(_, plan)| plan.drawn_length()).sum();
            println!(
                "{lines} lines in {} colours, {length:.0} pixels of line, in a canvas of {}x{} at {},{}",
                groups.len(),
                canvas.width,
                canvas.height,
                canvas.x,
                canvas.y
            );
            super::say_how_covered(length, pen, canvas);
            let travelled = super::travel_of(&groups);
            super::say_what_it_costs(length, travelled, 0.0, super::commands_for(&groups));
            for (colour, plan) in groups {
                if let Some((x, y)) = palette.swatch(colour) {
                    deeds.push(Deed::Swatch(x, y, colour));
                }
                deeds.extend(session::in_batches(plan.strokes, batch));
            }
        } else {
            let plan = plan_with(
                &picture,
                canvas,
                &AsAsked {
                    spacing,
                    min_run,
                    levels,
                    shares,
                    pen,
                    max_points,
                },
            );
            super::planning::report(&plan, canvas);
            deeds.extend(session::in_batches(plan.strokes, batch));
        }
        deeds.truncate(limit.unwrap_or(usize::MAX));
        let stop = AtomicBool::new(false);
        let mut commands = 0_usize;
        let summary = session::perform(&mut seat, &deeds, &stop, &mut |progress| match progress {
            Progress::Waiting => println!("a person has the stage; waiting for them"),
            Progress::Resumed => println!("back to it"),
            Progress::Drew { lines, of } => {
                commands += 1;
                if commands % 25 == 0 {
                    println!("{lines} of {of} lines");
                }
            }
            Progress::Refused(why) => println!("refused: {why}"),
            Progress::Colour(_) => {}
        })
        .await?;
        // Pixels of line a second is the number that decides whether a picture finishes.
        // Commands a second only says how fast it was asking.
        println!(
            "drew {} lines in {} commands and had {} refused in {:.1}s, of which {:.1}s was waiting for a person -- {:.0} pixels of line a second",
            summary.lines,
            summary.commands,
            summary.refused,
            summary.seconds,
            summary.waited_seconds,
            summary.pixels_per_second()
        );
        Ok(if summary.refused == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(4)
        })
    }

    fn load_key(path: &str) -> Result<CommandKey, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let hex = text.trim();
        if hex.len() != 64 {
            return Err(format!("{path}: a stage key is 64 hex characters"));
        }
        let mut bytes = [0_u8; 32];
        for (at, pair) in hex.as_bytes().chunks(2).enumerate() {
            let pair = std::str::from_utf8(pair).map_err(|_| "a key is hex")?;
            bytes[at] = u8::from_str_radix(pair, 16).map_err(|_| "a key is hex")?;
        }
        Ok(CommandKey::from_bytes(&bytes))
    }
}
