//! Archie draws a painting in Photopea, by its own hand on the stage, and says what it is
//! doing while it does it.
//!
//! "Archie, draw the Mona Lisa": the model proposes `draw_picture`, Cerebra decides it
//! ([`crate::act`]) and this carries it out, in the background, on a stage connection of its
//! own so a drawing of several minutes never holds the conversation:
//!
//! 1. Photopea opens in Firefox with a white sheet the shape of the painting and two inks
//!    in hand ([`archie_paint::photopea::url`]); nothing else is opened, and only the
//!    stage policy's Photopea entry may be used.
//! 2. Archie looks at the window until Photopea's page and its paper are there, clicks the
//!    brush where Photopea keeps it, takes it with `b` (whatever the column's scale), and
//!    thins it with `[`.
//! 3. The painting becomes lines that follow its forms ([`archie_paint::flow_plan`]), in the
//!    two inks; `x` swaps them halfway.
//! 4. The lines go through the stage gate in batches ([`archie_paint::session::perform`]):
//!    a lease, a sealed command and the compositor's check, every one. A person who takes the
//!    mouse is waited for.
//! 5. After the first batch the paper is looked at again: a sheet that is still white means
//!    the brush was not in hand, and it is picked up again before anything else is drawn.
//!
//! Every step is told as it happens ([`Beat`]), in the language the person spoke, so the
//! person watching hears what Archie is doing and why.

use std::path::{Path, PathBuf};

/// A painting Archie can draw: its picture and how to say its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Work {
    pub id: &'static str,
    pub file: &'static str,
    pub en: &'static str,
    pub es: &'static str,
    /// How a person may call it, without accents, lower case.
    pub aliases: &'static [&'static str],
    /// The two inks: the darker one first.
    pub inks: [[u8; 3]; 2],
    pub ink_names_en: [&'static str; 2],
    pub ink_names_es: [&'static str; 2],
    /// What the lines follow, said in passing.
    pub forms_en: &'static str,
    pub forms_es: &'static str,
}

/// The paintings shipped with Archie Arena (core/crates/archie-arena/tasks), all public
/// domain.
pub const WORKS: &[Work] = &[
    Work {
        id: "mona-lisa",
        file: "mona-lisa.png",
        en: "the Mona Lisa",
        es: "la Mona Lisa",
        aliases: &[
            "mona lisa",
            "monalisa",
            "gioconda",
            "joconde",
            "la joconda",
            "leonardo",
            // What the Nemotron 3.5 recogniser heard for "the Mona Lisa" in rehearsals.
            "moaner lisa",
            "moaner lea",
            "owner leaser",
            "mona lease",
            "mona leaser",
            "mona liza",
        ],
        inks: [[0x4a, 0x2e, 0x1a], [0xa5, 0x52, 0x2e]],
        ink_names_en: ["sepia", "red chalk"],
        ink_names_es: ["sepia", "sanguina"],
        forms_en: "her face, her hands and the landscape behind her",
        forms_es: "su rostro, sus manos y el paisaje detrás de ella",
    },
    Work {
        id: "noche-estrellada",
        file: "noche.png",
        en: "The Starry Night",
        es: "La noche estrellada",
        aliases: &[
            "starry night",
            "noche estrellada",
            "van gogh",
            "la noche estrellada",
        ],
        inks: [[0x1c, 0x2f, 0x5e], [0xc0, 0x8a, 0x22]],
        ink_names_en: ["night blue", "ochre"],
        ink_names_es: ["azul noche", "ocre"],
        forms_en: "the swirls of the sky, the village and the cypress",
        forms_es: "los remolinos del cielo, el pueblo y el ciprés",
    },
    Work {
        id: "dama-armino",
        file: "dama.png",
        en: "the Lady with an Ermine",
        es: "la Dama del armiño",
        aliases: &[
            "lady with an ermine",
            "dama del armino",
            "dama con armino",
            "dama con el armino",
        ],
        inks: [[0x3b, 0x28, 0x1c], [0x9e, 0x4b, 0x2c]],
        ink_names_en: ["sepia", "red chalk"],
        ink_names_es: ["sepia", "sanguina"],
        forms_en: "her face, the ermine and her hand",
        forms_es: "su rostro, el armiño y su mano",
    },
    Work {
        id: "aquelarre",
        file: "aquelarre.png",
        en: "Goya's Witches' Sabbath",
        es: "El aquelarre de Goya",
        aliases: &["aquelarre", "witches sabbath", "witches' sabbath", "goya"],
        inks: [[0x26, 0x20, 0x1a], [0x8a, 0x5a, 0x2b]],
        ink_names_en: ["lamp black", "umber"],
        ink_names_es: ["negro de humo", "sombra"],
        forms_en: "the goat, the crowd and the night around them",
        forms_es: "el macho cabrío, la multitud y la noche alrededor",
    },
];

fn plain(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'Á' => 'a',
            'é' | 'è' | 'ë' | 'É' => 'e',
            'í' | 'ì' | 'ï' | 'Í' => 'i',
            'ó' | 'ò' | 'ö' | 'Ó' => 'o',
            'ú' | 'ù' | 'ü' | 'Ú' => 'u',
            'ñ' | 'Ñ' => 'n',
            c if c.is_alphanumeric() || c == ' ' || c == '\'' => c.to_ascii_lowercase(),
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The painting a person named, however they said it.
pub fn find_work(named: &str) -> Option<&'static Work> {
    let said = plain(named);
    if said.is_empty() {
        return None;
    }
    let found = WORKS.iter().find(|work| {
        work.id.replace('-', " ") == said
            || work.aliases.iter().any(|alias| said.contains(alias))
            || plain(work.en).contains(&said)
            || plain(work.es).contains(&said)
    });
    // However the first word comes out ("minner", "moon or", "owner", "mother"), the
    // recogniser keeps something like "Lisa", and no other painting here has it.
    found.or_else(|| {
        said.split(' ')
            .any(|word| LISA.contains(&word))
            .then(|| WORKS.iter().find(|work| work.id == "mona-lisa"))
            .flatten()
    })
}

/// "Lisa" as the recogniser writes it at the end of "Mona Lisa".
const LISA: &[&str] = &["lisa", "liza", "leaser", "leasa", "leesa", "lisas"];

/// The names of what can be drawn, for the model and for a refusal.
pub fn repertoire(lang: Lang) -> String {
    WORKS
        .iter()
        .map(|w| match lang {
            Lang::En => w.en,
            Lang::Es => w.es,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
}

impl Lang {
    pub fn of(code: &str) -> Lang {
        if code.trim().to_ascii_lowercase().starts_with("en") {
            Lang::En
        } else {
            Lang::Es
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }
}

/// A moment of a drawing worth telling.
#[derive(Debug, Clone, PartialEq)]
pub enum Beat {
    Accepted,
    Opening,
    Loading,
    Paper {
        width: u32,
        height: u32,
    },
    Brush {
        size: u32,
    },
    Planned {
        lines: usize,
        seconds: f32,
    },
    Gate,
    Progress {
        percent: u32,
        lines: usize,
        pixels_per_second: f32,
    },
    SecondInk,
    Waiting,
    Resumed,
    Retrying,
    Done {
        lines: usize,
        seconds: f32,
        /// Batches the stage did not carry out.
        refused: usize,
    },
    Stopped {
        lines: usize,
    },
    Failed {
        why: String,
    },
}

/// One thing said: progress lines replace one another in a queue that has fallen behind;
/// the rest are all said.
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
    pub text: String,
    pub progress: bool,
    pub lang: Lang,
}

fn minutes(seconds: f32, lang: Lang) -> String {
    let whole = (seconds / 60.0).round().max(1.0) as u32;
    match (lang, whole) {
        (Lang::En, 1) => "about a minute".into(),
        (Lang::En, n) => format!("about {n} minutes"),
        (Lang::Es, 1) => "como un minuto".into(),
        (Lang::Es, n) => format!("unos {n} minutos"),
    }
}

/// What Archie says at `beat` while drawing `work`.
pub fn say(work: &Work, lang: Lang, beat: &Beat) -> Said {
    let (title, inks, forms) = match lang {
        Lang::En => (work.en, work.ink_names_en, work.forms_en),
        Lang::Es => (work.es, work.ink_names_es, work.forms_es),
    };
    let progress = matches!(beat, Beat::Progress { .. });
    let text = match (lang, beat) {
        (Lang::En, Beat::Accepted) => format!("Sure. I'll draw {title} for you, with my own hand on the screen."),
        (Lang::Es, Beat::Accepted) => format!("Claro. Voy a dibujar {title}, con mi propia mano en la pantalla."),
        (Lang::En, Beat::Opening) => "First I'm opening Photopea, an image editor that runs in the browser, with a blank sheet the shape of the painting.".into(),
        (Lang::Es, Beat::Opening) => "Primero abro Photopea, un editor de imágenes que corre en el navegador, con una hoja en blanco de la forma del cuadro.".into(),
        (Lang::En, Beat::Loading) => "Photopea draws its whole interface on one canvas and tells the accessibility system nothing, so I find what I need by looking at the window.".into(),
        (Lang::Es, Beat::Loading) => "Photopea dibuja toda su interfaz en un solo lienzo y no le dice nada a la accesibilidad, así que encuentro lo que necesito mirando la ventana.".into(),
        (Lang::En, Beat::Paper { width, height }) => format!("There's the paper: {width} by {height} pixels. Now I pick up the brush."),
        (Lang::Es, Beat::Paper { width, height }) => format!("Ahí está el papel: {width} por {height} píxeles. Ahora tomo el pincel."),
        (Lang::En, Beat::Brush { size }) => format!("Brush in hand, thinned to {size} pixels, with {} ink loaded.", inks[0]),
        (Lang::Es, Beat::Brush { size }) => format!("Pincel en mano, afinado a {size} píxeles, con tinta {} cargada.", inks[0]),
        (Lang::En, Beat::Planned { lines, seconds }) => format!(
            "I've turned the painting into {lines} lines that follow {forms}, in two inks, {} and {}, the way Leonardo drew. It should take {}.",
            inks[0], inks[1], minutes(*seconds, lang)
        ),
        (Lang::Es, Beat::Planned { lines, seconds }) => format!(
            "Convertí el cuadro en {lines} líneas que siguen {forms}, en dos tintas, {} y {}, como dibujaba Leonardo. Debería tomar {}.",
            inks[0], inks[1], minutes(*seconds, lang)
        ),
        (Lang::En, Beat::Gate) => "Every batch of strokes goes through the Stage's gate: a permission for one operation, sealed, and checked against the window as it is right now. If you touch the mouse, control is yours and I wait.".into(),
        (Lang::Es, Beat::Gate) => "Cada tanda de trazos pasa por la puerta del Stage: un permiso para una operación, sellado y verificado contra la ventana tal como está ahora. Si tocas el mouse, el control es tuyo y espero.".into(),
        (Lang::En, Beat::Progress { percent, lines, pixels_per_second }) => match percent {
            p if *p < 40 => "A quarter done. The darkest shapes go first, so the picture shows up early.".into(),
            p if *p < 65 => format!("Halfway: {lines} lines so far, at about {pixels_per_second:.0} pixels of line a second."),
            _ => "Three quarters. Now the softer tones.".into(),
        },
        (Lang::Es, Beat::Progress { percent, lines, pixels_per_second }) => match percent {
            p if *p < 40 => "Un cuarto listo. Las formas más oscuras van primero, para que el cuadro aparezca pronto.".into(),
            p if *p < 65 => format!("La mitad: {lines} líneas hasta ahora, a unos {pixels_per_second:.0} píxeles de línea por segundo."),
            _ => "Tres cuartos. Ahora los tonos más suaves.".into(),
        },
        (Lang::En, Beat::SecondInk) => format!("Now the second ink, {}, for the lighter tones.", inks[1]),
        (Lang::Es, Beat::SecondInk) => format!("Ahora la segunda tinta, {}, para los tonos más claros.", inks[1]),
        (Lang::En, Beat::Waiting) => "You have the mouse, so I'll wait until you're done.".into(),
        (Lang::Es, Beat::Waiting) => "Tienes el mouse, así que espero a que termines.".into(),
        (Lang::En, Beat::Resumed) => "Back to it.".into(),
        (Lang::Es, Beat::Resumed) => "Sigo.".into(),
        (Lang::En, Beat::Retrying) => "The first lines left no ink: the brush wasn't in my hand. Picking it up again.".into(),
        (Lang::Es, Beat::Retrying) => "Las primeras líneas no dejaron tinta: el pincel no estaba en mi mano. Lo tomo de nuevo.".into(),
        (Lang::En, Beat::Done { lines, seconds, refused: 0 }) => format!(
            "Done. {lines} lines in {seconds:.0} seconds, every one drawn by my own pointer, through the Stage's gate."
        ),
        (Lang::Es, Beat::Done { lines, seconds, refused: 0 }) => format!(
            "Listo. {lines} líneas en {seconds:.0} segundos, cada una dibujada por mi propio puntero, a través de la puerta del Stage."
        ),
        (Lang::En, Beat::Done { lines, refused, .. }) => format!(
            "I've finished, but the Stage refused {refused} of my batches, so only {lines} lines reached the paper."
        ),
        (Lang::Es, Beat::Done { lines, refused, .. }) => format!(
            "Terminé, pero el Stage rechazó {refused} de mis tandas, así que solo {lines} líneas llegaron al papel."
        ),
        (Lang::En, Beat::Stopped { lines }) => format!("Stopped, as you asked. {lines} lines are on the paper."),
        (Lang::Es, Beat::Stopped { lines }) => format!("Me detuve, como pediste. Hay {lines} líneas en el papel."),
        (Lang::En, Beat::Failed { why }) => format!("I couldn't finish the drawing: {why}."),
        // The reason is the program's, in English; the log has it.
        (Lang::Es, Beat::Failed { .. }) => "No pude terminar el dibujo; el motivo quedó en el registro.".into(),
    };
    Said {
        text,
        progress,
        lang,
    }
}

/// Where what is said goes: a voice's queue, a subtitle on the stage, a log.
pub trait Narrate: Send {
    fn tell(&mut self, said: Said);
}

/// Narration into a function, for the CLI and tests.
pub struct NarrateWith<F: FnMut(Said) + Send>(pub F);

impl<F: FnMut(Said) + Send> Narrate for NarrateWith<F> {
    fn tell(&mut self, said: Said) {
        (self.0)(said)
    }
}

/// What a drawing needs from the place it runs.
#[derive(Debug, Clone)]
pub struct Studio {
    pub socket: PathBuf,
    pub key_file: PathBuf,
    pub policy: PathBuf,
    /// The stage's Wayland display, given to Firefox.
    pub wayland_display: Option<String>,
    /// Where the paintings' pictures are.
    pub pictures: PathBuf,
    /// Pen width on the sheet, in pixels.
    pub pen: u32,
    /// How long the drawing is planned to take, in seconds.
    pub seconds: f32,
    /// Lines per command.
    pub batch: usize,
    /// Where Firefox's output goes.
    pub log: Option<PathBuf>,
}

impl Studio {
    pub fn picture(&self, work: &Work) -> PathBuf {
        self.pictures.join(work.file)
    }
}

/// The document Photopea is opened with: the painting's shape, small enough to fit a
/// window at its own size; [`archie_paint::fit_into`] keeps the shape on whatever of the
/// sheet is in view.
pub fn sheet_for(width: u32, height: u32) -> (u32, u32) {
    let tall = 640_u32;
    let wide = ((u64::from(width) * u64::from(tall)) / u64::from(height.max(1))) as u32;
    if wide > 900 {
        (
            900,
            ((u64::from(height) * 900) / u64::from(width.max(1))) as u32,
        )
    } else {
        (wide.max(64), tall)
    }
}

pub fn exists(studio: &Studio, work: &Work) -> bool {
    Path::new(&studio.picture(work)).is_file()
}

#[cfg(unix)]
pub use job::run;

#[cfg(unix)]
mod job {
    use super::{say, sheet_for, Beat, Lang, Narrate, Studio, Work};
    use archie_paint::photopea;
    use archie_paint::session::{self, Deed, Progress, Seat, Summary};
    use archie_policy::{PolicyEngine, Principal};
    use archie_stage::{
        learn, CommandKey, KeyChord, Operator, StageClient, StagePolicy, StageWorld,
    };
    use archie_types::AgentId;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::net::UnixStream;

    fn load_key(path: &std::path::Path) -> Result<CommandKey, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let hex = text.trim();
        let bytes = hex::decode(hex).map_err(|_| "the stage key is not hex".to_string())?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "a stage key is 32 bytes".to_string())?;
        Ok(CommandKey::from_bytes(&bytes))
    }

    /// The stage policy's Photopea entry: its application, and the browser that opens it.
    /// The entry may open Photopea itself, with a document of its own in the address; the
    /// address is then replaced by the one this drawing needs, which is photopea.com too.
    fn photopea_entry(policy: &StagePolicy) -> Result<(String, Vec<String>), String> {
        let app = policy
            .named("photopea")
            .ok_or("the stage policy has no Photopea entry")?;
        let mut argv = app.launch.clone();
        if let Some(at) = argv
            .iter()
            .position(|arg| arg.starts_with("https://www.photopea.com/"))
        {
            argv.truncate(at);
        }
        if argv.is_empty() {
            return Err("the Photopea entry opens nothing".into());
        }
        Ok((app.app_id.clone(), argv))
    }

    /// Draws `work` and tells it as it goes. Returns what the drawing came to.
    pub async fn run(
        studio: Studio,
        work: &'static Work,
        lang: Lang,
        narrate: &mut dyn Narrate,
        stop: Arc<AtomicBool>,
    ) -> Result<Summary, String> {
        // Saying yes is the caller's: a voice answers in its own words, a command prints it.
        let mut tell = |beat: Beat| narrate.tell(say(work, lang, &beat));
        let picture_path = studio.picture(work);
        let picture = archie_paint::Picture::read(&picture_path)
            .map_err(|e| format!("{}: {e}", picture_path.display()))?;
        let radius = (picture.width() / 200).max(1);
        // A mat of a twenty-fifth: the darkened edges of an old painting are not its form.
        let picture = picture.matted(0.04).softened(radius).stretched();

        let policy = StagePolicy::load(&studio.policy).map_err(|e| e.to_string())?;
        let (app_id, argv) = photopea_entry(&policy)?;
        let key = load_key(&studio.key_file)?;
        let stream = UnixStream::connect(&studio.socket)
            .await
            .map_err(|e| format!("{}: {e}", studio.socket.display()))?;
        let (read, write) = stream.into_split();
        let mut client = StageClient::new(read, write);
        let mut world = StageWorld::default();
        learn(&mut client, &mut world, Duration::from_millis(400))
            .await
            .map_err(|e| e.to_string())?;
        let before: Vec<u32> = world.surfaces_of(&app_id).map(|s| s.surface).collect();

        // 1. Photopea, with the sheet and both inks.
        tell(Beat::Opening);
        let (w, h) = sheet_for(picture.width(), picture.height());
        // The document is named for the painting, without the article it is said with.
        let name = work.en.trim_start_matches("the ");
        let address = photopea::url(w, h, name, work.inks[0], work.inks[1]);
        let (program, args) = argv
            .split_first()
            .ok_or("the Photopea entry starts nothing")?;
        let mut command = Command::new(program);
        command.args(args).arg(&address).stdin(Stdio::null());
        if let Some(display) = &studio.wayland_display {
            command.env("WAYLAND_DISPLAY", display);
        }
        match &studio.log {
            Some(path) => {
                let log = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                command
                    .stdout(log.try_clone().map_err(|e| e.to_string())?)
                    .stderr(log);
            }
            None => {
                command.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        command
            .spawn()
            .map_err(|e| format!("could not start {program}: {e}"))?;

        // 2. A new window of it on the stage.
        let opened = Instant::now();
        let surface = loop {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped before Photopea opened".into());
            }
            let _ = learn(&mut client, &mut world, Duration::from_millis(500)).await;
            if let Some(s) = world
                .surfaces_of(&app_id)
                .filter(|s| !before.contains(&s.surface))
                .max_by_key(|s| s.surface)
            {
                break s.surface;
            }
            if opened.elapsed() > Duration::from_secs(45) {
                return Err("Photopea's window never came up on the stage".into());
            }
        };
        let mut operator = Operator::new(key, policy, PolicyEngine::default());
        let snapshots = studio
            .socket
            .parent()
            .ok_or("the stage socket has no directory")?
            .join("snapshots");
        let mut seat = Seat {
            operator: &mut operator,
            client: &mut client,
            world: &mut world,
            app_id: app_id.clone(),
            surface,
            principal: Principal::Agent {
                agent: AgentId::new(),
            },
            snapshots,
        };

        // 3. Photopea's page and its paper, by looking.
        tell(Beat::Loading);
        let (page_top, paper) = loop {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped while Photopea was loading".into());
            }
            if let Ok(sight) = seat
                .look("see whether Photopea and its paper are there")
                .await
            {
                if let (Some(top), Some(paper)) = (
                    photopea::page_top(&sight.picture),
                    archie_paint::paper_in(&sight.picture, archie_paint::PAPER),
                ) {
                    // The paper is the sheet in the page, not the browser's own white.
                    if paper.y > top + 60 {
                        break (
                            top + sight.y,
                            archie_paint::Canvas::new(
                                paper.x + sight.x,
                                paper.y + sight.y,
                                paper.width,
                                paper.height,
                            ),
                        );
                    }
                }
            }
            if opened.elapsed() > Duration::from_secs(120) {
                return Err("Photopea did not finish loading".into());
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        };
        tell(Beat::Paper {
            width: paper.width,
            height: paper.height,
        });

        // 4. The brush, thinned.
        take_brush(&mut seat, page_top, studio.pen).await?;
        tell(Beat::Brush { size: studio.pen });

        // 5. The plan: lines that follow the form, in the two inks.
        let canvas = paper.inset(12);
        let flow = archie_paint::Flow {
            seconds: studio.seconds,
            speed: 950.0,
            ..archie_paint::Flow::default()
        };
        let groups = archie_paint::flow_plan(&picture, canvas, &flow, &work.inks, 24);
        let swap = KeyChord::parse("x").ok_or("no x key")?;
        let mut deeds: Vec<Deed> = Vec::new();
        let mut in_hand = 0_usize;
        let mut ordered = groups;
        ordered.sort_by_key(|(colour, _)| *colour);
        for (colour, plan) in ordered {
            if colour != in_hand {
                deeds.push(Deed::Key(swap.clone(), colour));
                in_hand = colour;
            }
            deeds.extend(session::in_batches(plan.strokes, studio.batch));
        }
        let lines: usize = deeds.iter().map(Deed::lines).sum();
        if lines == 0 {
            return Err("the painting gave no lines to draw".into());
        }
        tell(Beat::Planned {
            lines,
            seconds: studio.seconds.max(30.0),
        });
        tell(Beat::Gate);

        // 6. The first batch, and a look at the paper: ink, or the brush again.
        let started = Instant::now();
        let first = deeds.iter().position(|d| matches!(d, Deed::Lines(_)));
        let mut done = 0_usize;
        if let Some(first) = first {
            let head = &deeds[..=first];
            let summary = session::perform(&mut seat, head, &stop, &mut |_| {}).await?;
            done += summary.lines;
            let sight = seat.look("see whether the brush left ink").await?;
            let local = archie_paint::Canvas::new(
                canvas.x - sight.x,
                canvas.y - sight.y,
                canvas.width,
                canvas.height,
            );
            if photopea::inked(&sight.picture, local) < 0.0005 {
                tell(Beat::Retrying);
                take_brush(&mut seat, page_top, studio.pen).await?;
                let again = session::perform(&mut seat, head, &stop, &mut |_| {}).await?;
                done = again.lines;
            }
            deeds.drain(..=first);
        }

        // 7. The rest, told as it goes: a command at a time, so what is said is said between
        // commands and the narrator is never held across an await.
        let mut told = 0_u32;
        let mut second_told = false;
        let mut summary = Summary::default();
        for deed in deeds.chunks(1) {
            let mut heard: Vec<Progress> = Vec::new();
            let part = session::perform(&mut seat, deed, &stop, &mut |p| heard.push(p)).await?;
            summary.lines += part.lines;
            summary.commands += part.commands;
            summary.refused += part.refused;
            summary.pixels_of_line += part.pixels_of_line;
            summary.waited_seconds += part.waited_seconds;
            for progress in heard {
                match progress {
                    Progress::Waiting => tell(Beat::Waiting),
                    Progress::Resumed => tell(Beat::Resumed),
                    Progress::Colour(1) if !second_told => {
                        second_told = true;
                        tell(Beat::SecondInk);
                    }
                    Progress::Refused(why) => {
                        eprintln!("archie-voice: the stage refused a batch: {why}")
                    }
                    Progress::Colour(_) | Progress::Drew { .. } => {}
                }
            }
            let drawn = done + summary.lines;
            let percent = (drawn * 100 / lines.max(1)) as u32;
            if let Some(mark) = [25, 50, 75]
                .into_iter()
                .filter(|m| percent >= *m && told < *m)
                .next_back()
            {
                told = mark;
                let drawing = (started.elapsed().as_secs_f32() - summary.waited_seconds).max(0.1);
                tell(Beat::Progress {
                    percent: mark,
                    lines: drawn,
                    pixels_per_second: summary.pixels_of_line / drawing,
                });
            }
            if part.stopped || stop.load(Ordering::Relaxed) {
                summary.stopped = true;
                break;
            }
        }
        summary.lines += done;
        summary.seconds = started.elapsed().as_secs_f32();
        if summary.stopped {
            tell(Beat::Stopped {
                lines: summary.lines,
            });
        } else {
            // The inks back as they were, so the next drawing starts the same way.
            if in_hand != 0 {
                let _ = seat.key(swap, "put the first ink back in hand").await;
            }
            tell(Beat::Done {
                lines: summary.lines,
                seconds: summary.seconds,
                refused: summary.refused,
            });
        }
        Ok(summary)
    }

    /// Picks up Photopea's brush where its tool column keeps it, and thins it to `pen`.
    async fn take_brush(
        seat: &mut Seat<'_, tokio::net::unix::OwnedReadHalf, tokio::net::unix::OwnedWriteHalf>,
        page_top: i32,
        pen: u32,
    ) -> Result<(), String> {
        let (x, y) = photopea::brush_at(page_top);
        seat.click(x, y, "pick up Photopea's brush").await?;
        // The click gives the page the keyboard; B is the brush whatever the column's scale,
        // which is not the same under every display (under WSLg the click lands two tools
        // lower, on the blur tool, and the lines leave no ink).
        let brush = KeyChord::parse("b").ok_or("no b key")?;
        seat.key(brush, "take the brush").await?;
        let thinner = KeyChord::parse("[").ok_or("no [ key")?;
        for _ in 0..photopea::thinner_presses(pen) {
            seat.key(thinner.clone(), "thin the brush").await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_painting_is_found_however_it_is_named() {
        for said in [
            "the Mona Lisa",
            "la Gioconda",
            "MONA LISA!",
            "la mona lisa de Leonardo",
            "monalisa",
            "the moaner lea",
            "the owner leaser",
            "the minner leaser",
            "the moon or Lisa",
            "la mona. Lisa",
        ] {
            assert_eq!(find_work(said).map(|w| w.id), Some("mona-lisa"), "{said}");
        }
        assert_eq!(
            find_work("La noche estrellada").map(|w| w.id),
            Some("noche-estrellada")
        );
        assert_eq!(
            find_work("starry night by van gogh").map(|w| w.id),
            Some("noche-estrellada")
        );
        assert_eq!(
            find_work("la dama del armiño").map(|w| w.id),
            Some("dama-armino")
        );
        assert_eq!(find_work("el aquelarre").map(|w| w.id), Some("aquelarre"));
        assert_eq!(find_work("a cat"), None);
        assert_eq!(find_work(""), None);
    }

    #[test]
    fn every_beat_is_said_in_both_languages() {
        let beats = [
            Beat::Accepted,
            Beat::Opening,
            Beat::Loading,
            Beat::Paper {
                width: 480,
                height: 716,
            },
            Beat::Brush { size: 4 },
            Beat::Planned {
                lines: 900,
                seconds: 150.0,
            },
            Beat::Gate,
            Beat::Progress {
                percent: 25,
                lines: 200,
                pixels_per_second: 600.0,
            },
            Beat::Progress {
                percent: 50,
                lines: 450,
                pixels_per_second: 600.0,
            },
            Beat::Progress {
                percent: 75,
                lines: 700,
                pixels_per_second: 600.0,
            },
            Beat::SecondInk,
            Beat::Waiting,
            Beat::Resumed,
            Beat::Retrying,
            Beat::Done {
                lines: 900,
                seconds: 149.6,
                refused: 0,
            },
            Beat::Done {
                lines: 300,
                seconds: 149.6,
                refused: 4,
            },
            Beat::Stopped { lines: 120 },
            Beat::Failed { why: "x".into() },
        ];
        for work in WORKS {
            for lang in [Lang::En, Lang::Es] {
                for beat in &beats {
                    let said = say(work, lang, beat);
                    assert!(said.text.len() > 4, "{beat:?}");
                    assert_eq!(said.progress, matches!(beat, Beat::Progress { .. }));
                    assert!(!said.text.contains("{"), "{}", said.text);
                }
            }
        }
        assert!(say(&WORKS[0], Lang::En, &Beat::Accepted)
            .text
            .contains("the Mona Lisa"));
        assert!(say(&WORKS[0], Lang::Es, &Beat::SecondInk)
            .text
            .contains("sanguina"));
        assert!(say(
            &WORKS[0],
            Lang::En,
            &Beat::Planned {
                lines: 10,
                seconds: 150.0
            }
        )
        .text
        .contains("about 3 minutes"));
    }

    #[test]
    fn the_sheet_keeps_the_paintings_shape_and_fits_a_window() {
        assert_eq!(sheet_for(384, 572), (429, 640));
        let (w, h) = sheet_for(2000, 800);
        assert_eq!(w, 900);
        assert_eq!(h, 360);
    }

    #[test]
    fn every_painting_ships_its_picture() {
        let pictures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../archie-arena/tasks");
        for work in WORKS {
            assert!(pictures.join(work.file).is_file(), "{}", work.file);
        }
    }
}
