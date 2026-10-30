//! `archie-voice` — a turn of conversation, typed instead of spoken, against the real model.
//!
//! ```text
//! archie-voice ask "<what the person said>" --grade public
//!              [--model nvidia/Nemotron-3_5-Lightning] [--budget full|sentence]
//!              [--language es-CL] [--endpoint <llama-server url> [--thinking off|on]]
//!
//! archie-voice do  "<what the person said>" --grade public
//!              --socket <stage control socket> --key-file <stage key> --policy <apps.toml>
//!              [--stage-display <wayland display>] [--app-log <file>] [--steps 6]
//!              [--ground-model <vision model> [--ground-endpoint <local NIM url>]]
//!              [--model …] [--budget …] [--language …]
//!
//! archie-voice converse --grade public
//!              --asr-model <nemotron-3.5-asr .gguf>
//!              [--voice-engine piper --piper <binary> --piper-model <voice.onnx>]
//!              [--voice-engine magpie --tts-model <magpie .gguf> --codec-model <nanocodec .gguf>
//!               --tts-tokenizer <dir> [--voice <name>]]
//!              [--listen-language es-US] [--tts-language es-US] [--always] [--headphones]
//!              [--wav-in <file>] [--wav-out <file>]
//!              [the stage flags of do]
//! ```
//!
//! `draw` is the drawing on its own, with no model in the loop: Archie opens Photopea on the
//! stage and draws a painting it knows, telling what it does as it goes (printed, and shown
//! under the stage when `--subtitles` names the file the stage reads). It is how the demo is
//! rehearsed, and what `draw_picture` runs when the model proposes it:
//!
//! ```text
//! archie-voice draw "the Mona Lisa" [--language en] --socket … --key-file … --policy …
//!              [--stage-display <wayland display>] [--pictures <dir>] [--subtitles <file>]
//!              [--seconds 150] [--pen 4] [--batch 12] [--app-log <file>]
//! ```
//!
//! `live` is `converse` with a browser page for microphone, speaker and camera (loopback;
//! reach it through an SSH tunnel), IRIS for eyes, and curiosity: it looks closer at what
//! is in front, finds out what it is and reads about it, and keeps that until asked:
//!
//! ```text
//! archie-voice live --grade public --asr-model … --voice-engine magpie --tts-model …
//!              --codec-model … --tts-tokenizer … [--listen-language auto]
//!              [--tts-language es-US] [--tts-english en-US] [--port 8770]
//!              [--iris http://127.0.0.1:8765] [--iris-sweep-ms 2000] [--iris-timeout-seconds 60] [--curiosity 40] [--always] [--headphones]
//! ```
//!
//! Built with `--features live`. Finding out what an object is uses a vision model on
//! Nebius, on the conversation's `NEBIUS_TOKEN_FACTORY_KEY` (`ARCHIE_LIVE_VLM` picks the
//! models); reading about it uses `TAVILY_API_KEY`, which is optional. No key is printed.
//!
//! `converse` is the whole thing, spoken: the microphone through Nemotron 3.5 ASR, the
//! floor deciding whether to answer, Nemotron on Nebius answering and proposing, the hands
//! acting when a stage is given, and a voice — Piper on a CPU, Magpie with a GPU — saying
//! the answer. Built with `--features speech`, where NeMo-Speech.cpp and PulseAudio are
//! installed. `--wav-in` and `--wav-out` put recordings where the microphone and the
//! speaker were, which is how the loop is tested without a room.
//!
//! `ask` is the cognitive half on its own: what was heard goes in, and out come the pieces
//! a voice would say and the tools the model proposes, each stamped with how long after the
//! question it arrived. The time to the first piece is the silence a person would hear
//! after they stop talking, so it is the number this tool exists to show.
//!
//! `do` goes on to act. Every tool the model proposes is decided on — read strictly, and
//! checked against the stage policy — and then either carried out through the stage gate
//! or refused. What came of it goes back to the model, which takes the next step or says
//! what it has to say, for at most `--steps` steps. A click needs a grounding model to find
//! what was named; without `--ground-model` clicks are refused and the model is told so.
//!
//! `--grade` has no default. What a person says to their computer is theirs, and a cloud
//! model whose retention nobody has verified may only be given what they graded public; the
//! model client refuses anything else before it leaves the machine. The credential comes
//! from `NEBIUS_TOKEN_FACTORY_KEY` and is never printed.

use archie_model_runtime::routed::Routed;
use archie_model_runtime::ProviderEndpoint;
use archie_registry::ModelRegistry;
use archie_speech::{CognitiveEvent, CognitiveModelAdapter, CognitiveRequest, SpeechBudget};
use archie_types::{
    Availability, Capability, DataRetention, Locality, ModelDescriptor, ModelId, ModelRole,
    PrivacyGrade, ProviderKind, TokenCost,
};
use archie_vault::Veil;
use archie_voice::{tools, NemotronCognitive, RoutedSource};
use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    // Read before any thread exists: the local offset cannot be trusted once one does.
    let offset = time::UtcOffset::current_local_offset().ok();
    match run(std::env::args().skip(1).collect(), offset) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("archie-voice: {message}");
            ExitCode::FAILURE
        }
    }
}

fn take(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == flag)?;
    if at + 1 >= args.len() {
        return None;
    }
    let value = args.remove(at + 1);
    args.remove(at);
    Some(value)
}

/// What both commands share: who is asked, how, and about what.
struct Asking {
    heard: String,
    model: String,
    language: String,
    budget: SpeechBudget,
    grade: PrivacyGrade,
    runtime: tokio::runtime::Runtime,
    cognitive: NemotronCognitive<RoutedSource>,
    #[cfg(feature = "live")]
    wanda_access: archie_voice::wanda::WandaVoiceAccess,
    clock: String,
}

fn run(mut args: Vec<String>, offset: Option<time::UtcOffset>) -> Result<ExitCode, String> {
    const USAGE: &str =
        "usage: archie-voice ask|do \"<what was said>\" --grade public | converse --grade public …";
    let command = match args.first().map(String::as_str) {
        Some("ask") | Some("do") | Some("converse") | Some("live") | Some("draw") => args.remove(0),
        _ => return Err(USAGE.into()),
    };
    if command == "draw" {
        return draw(args);
    }
    if command == "live" || command == "converse" {
        // Its own flags first; the stage is optional, since a conversation need not act.
        let speech = speech_flags(&mut args)?;
        let live_only = if command == "live" {
            Some(live_flags(&mut args)?)
        } else {
            None
        };
        let stage = if args.iter().any(|arg| arg == "--socket") {
            Some(stage_flags(&mut args)?)
        } else {
            None
        };
        let draw = draw_flags(&mut args)?;
        let asking = asking(args, offset, false)?;
        return match live_only {
            Some(flags) => live(asking, speech, flags, stage, draw),
            None => converse(asking, speech, stage, draw),
        };
    }
    // `do` takes its stage flags out before the shared ones are read.
    let stage = if command == "do" {
        Some(stage_flags(&mut args)?)
    } else {
        None
    };
    let asking = asking(args, offset, true)?;
    match stage {
        None => ask(asking),
        Some(flags) => act(asking, flags),
    }
}

// CPU-only local models can need more time for cold prompt prefill. The caller
// chooses a bounded wait; the default transport and response-size limit stay intact.
fn request_timeout(value: Option<&str>) -> Result<Duration, String> {
    let seconds = value
        .unwrap_or("90")
        .parse::<u64>()
        .map_err(|_| "--timeout-seconds is a whole number from 1 to 900".to_string())?;
    if !(1..=900).contains(&seconds) {
        return Err("--timeout-seconds is a whole number from 1 to 900".into());
    }
    Ok(Duration::from_secs(seconds))
}

fn asking(
    mut args: Vec<String>,
    offset: Option<time::UtcOffset>,
    words: bool,
) -> Result<Asking, String> {
    // A reasoning model on this machine (Bonsai on llama-server, on loopback) answers
    // instead of Nebius when its address is given. Nothing said then leaves the machine,
    // and no key is needed. The environment names it too, so the launcher that starts a
    // conversation need not know.
    let set = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let local = take(&mut args, "--endpoint").or_else(|| set("ARCHIE_VOICE_ENDPOINT"));
    let timeout = request_timeout(take(&mut args, "--timeout-seconds").as_deref())?;
    let model = take(&mut args, "--model")
        .or_else(|| set("ARCHIE_VOICE_MODEL"))
        .unwrap_or_else(|| "nvidia/Nemotron-3_5-Lightning".into());
    let language = take(&mut args, "--language").unwrap_or_else(|| "es-CL".into());
    // Whether the local model reasons before it speaks. Off by default: a voice waits for
    // the first words, and Bonsai's thought comes before them.
    let thinking = take(&mut args, "--thinking")
        .or_else(|| set("ARCHIE_VOICE_THINKING"))
        .unwrap_or_else(|| "off".into());
    if !matches!(thinking.as_str(), "on" | "off") {
        return Err(format!("--thinking is on or off, not {thinking}"));
    }
    let budget = match take(&mut args, "--budget").as_deref() {
        None | Some("full") => SpeechBudget::Full,
        Some("sentence") => SpeechBudget::Sentence,
        Some(other) => return Err(format!("--budget is full or sentence, not {other}")),
    };
    let grade = match take(&mut args, "--grade").as_deref() {
        Some("public") => PrivacyGrade::Public,
        Some("internal") => PrivacyGrade::Internal,
        Some(other) => return Err(format!("--grade is public or internal, not {other}")),
        None => {
            return Err(
                "say how private this is with --grade; it has no default, because what \
                        a person says to their computer is theirs"
                    .into(),
            )
        }
    };
    let heard = if words {
        if args.len() != 1 {
            return Err(format!("say one thing, in quotes; got {args:?}"));
        }
        args.remove(0)
    } else {
        if !args.is_empty() {
            return Err(format!("unexpected arguments: {args:?}"));
        }
        String::new()
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let models_dir = take(&mut args, "--models")
        .or_else(|| set("ARCHIE_MODELS_DIR"))
        .unwrap_or_else(|| "/etc/archie/models".into());
    let mut registry = if std::path::Path::new(&models_dir).is_dir() {
        ModelRegistry::load_from_dir(&models_dir).unwrap_or_default()
    } else {
        ModelRegistry::new()
    };
    if let Some(url) = &local {
        let _ = registry.register(local_voice(&model, url, &thinking)?);
    } else if !registry.iter().any(|m| {
        m.display_name == model || m.labels.get("provider-model").is_some_and(|p| p == &model)
    }) {
        let desc = descriptor(
            &model,
            "voice",
            ProviderKind::NebiusTokenFactory,
            PrivacyGrade::Public,
            None,
        )?;
        let _ = registry.register(desc);
    }
    // Wanda's private tool context is graded Restricted for the entire local continuation,
    // so the router can only choose models that permit it; the public/cloud path keeps its
    // caller's grade. Wanda is offered only if every model the router could choose at that
    // grade is local and on loopback (`WandaVoiceAccess::from_registry`).
    let source_grade = if local.is_some() {
        PrivacyGrade::Restricted
    } else {
        grade
    };
    #[cfg(feature = "live")]
    let wanda_access =
        archie_voice::wanda::WandaVoiceAccess::from_registry(&registry, source_grade);
    let routed = Arc::new(Routed::new(registry));
    let veil = Arc::new(Veil::new());
    let source = RoutedSource::new(
        routed,
        source_grade,
        ModelRole::CognitiveResponse,
        veil,
        timeout,
        runtime.handle().clone(),
    );
    let mut cognitive = NemotronCognitive::new(
        source,
        model.clone(),
        vec!["es".into(), "en".into()],
        tools::os_control(),
    );
    // A guide for the model that answers, added to what it is told it is. A small model
    // on the machine needs to be shown how the tools are used; a large one does not, so
    // nothing is added unless a guide is named. It is the operator's file, and bounded.
    if let Some(path) = set("ARCHIE_VOICE_GUIDE") {
        let guide = std::fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
        if guide.len() > 6 * 1024 {
            return Err(format!(
                "{path}: a guide is at most 6 KiB; a model reads it every turn"
            ));
        }
        // A SKILL.md opens with a block that describes it to whoever lists skills; the
        // model is given the guide itself.
        let body = guide
            .strip_prefix("---")
            .and_then(|rest| rest.split_once("\n---"))
            .map_or(guide.as_str(), |(_, body)| body);
        cognitive.set_persona(format!("{}\n\n{}", archie_voice::PERSONA_ES, body.trim()));
    }
    Ok(Asking {
        heard,
        model,
        language,
        budget,
        grade,
        runtime,
        cognitive,
        #[cfg(feature = "live")]
        wanda_access,
        clock: clock(offset),
    })
}

fn ask(mut asking: Asking) -> Result<ExitCode, String> {
    asking.cognitive.set_context(asking.clock.clone());
    let started = Instant::now();
    asking
        .cognitive
        .begin(CognitiveRequest {
            turn_id: "turn-1".into(),
            transcript: asking.heard.clone(),
            language: asking.language.clone(),
            budget: asking.budget,
        })
        .map_err(|error| error.to_string())?;
    println!("heard: {}", asking.heard);
    let step = listen(&mut asking.cognitive, started);
    if step.failed {
        return Ok(ExitCode::from(4));
    }
    for intent in &step.intents {
        println!(
            "        tool  {}({})   proposed, not done",
            intent.tool_name, intent.raw_arguments
        );
    }
    match step.first_piece {
        Some(ms) => println!("first words after {ms} ms, with {}", asking.model),
        None => println!(
            "no words: the model only proposed tools, with {}",
            asking.model
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// What one step of the model produced.
struct Step {
    intents: Vec<archie_speech::SpeechToolIntentCandidate>,
    first_piece: Option<u128>,
    failed: bool,
}

/// Reads one step to its end, printing what a voice would say as it arrives.
fn listen(cognitive: &mut NemotronCognitive<RoutedSource>, started: Instant) -> Step {
    let mut step = Step {
        intents: Vec::new(),
        first_piece: None,
        failed: false,
    };
    loop {
        match cognitive.poll() {
            Some(CognitiveEvent::Segment(segment)) => {
                let at = started.elapsed().as_millis();
                step.first_piece.get_or_insert(at);
                println!("{at:>6} ms  say   {}", segment.text);
            }
            Some(CognitiveEvent::ToolIntent(intent)) => {
                println!(
                    "{:>6} ms  tool  {}({})",
                    started.elapsed().as_millis(),
                    intent.tool_name,
                    intent.raw_arguments
                );
                step.intents.push(intent);
            }
            Some(CognitiveEvent::Completed { .. }) => {
                println!("{:>6} ms  done", started.elapsed().as_millis());
                return step;
            }
            Some(CognitiveEvent::Failed { reason, .. }) => {
                println!("{:>6} ms  failed: {reason}", started.elapsed().as_millis());
                step.failed = true;
                return step;
            }
            None => {
                if cognitive.running_turn().is_none() {
                    return step;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// Where paintings are drawn: the stage, and what a drawing needs besides.
struct DrawFlags {
    pictures: String,
    subtitles: Option<String>,
    seconds: f32,
    pen: u32,
    batch: usize,
}

fn draw_flags(args: &mut Vec<String>) -> Result<DrawFlags, String> {
    let number = |value: Option<String>,
                  flag: &str,
                  low: f32,
                  high: f32,
                  default: f32|
     -> Result<f32, String> {
        match value {
            None => Ok(default),
            Some(text) => text
                .parse::<f32>()
                .ok()
                .filter(|v| (low..=high).contains(v))
                .ok_or(format!("{flag} is between {low} and {high}")),
        }
    };
    Ok(DrawFlags {
        // The paintings Archie Arena ships, unless told otherwise.
        pictures: take(args, "--pictures")
            .or_else(|| std::env::var("ARCHIE_PICTURES").ok())
            .unwrap_or_else(|| {
                concat!(env!("CARGO_MANIFEST_DIR"), "/../archie-arena/tasks").into()
            }),
        subtitles: take(args, "--subtitles"),
        seconds: number(take(args, "--seconds"), "--seconds", 20.0, 900.0, 150.0)?,
        pen: number(take(args, "--pen"), "--pen", 1.0, 15.0, 4.0)? as u32,
        batch: number(take(args, "--batch"), "--batch", 1.0, 64.0, 12.0)? as usize,
    })
}

#[cfg(unix)]
fn studio(stage: &StageFlags, draw: &DrawFlags) -> archie_voice::draw::Studio {
    archie_voice::draw::Studio {
        socket: stage.socket.clone().into(),
        key_file: stage.key_file.clone().into(),
        policy: stage.policy.clone().into(),
        wayland_display: stage.display.clone(),
        pictures: draw.pictures.clone().into(),
        pen: draw.pen,
        seconds: draw.seconds,
        batch: draw.batch,
        log: stage.app_log.clone().map(Into::into),
    }
}

#[cfg(not(unix))]
fn draw(_args: Vec<String>) -> Result<ExitCode, String> {
    Err("draw works a stage, which listens on a Unix socket; run it where the stage runs".into())
}

/// `archie-voice draw "<painting>"`: the drawing with nobody in the loop but Archie.
#[cfg(unix)]
fn draw(mut args: Vec<String>) -> Result<ExitCode, String> {
    use archie_voice::draw::{self, Lang, Narrate, NarrateWith};
    use archie_voice::narration::Narration;
    let language = Lang::of(&take(&mut args, "--language").unwrap_or_else(|| "es".into()));
    let flags = draw_flags(&mut args)?;
    let stage = stage_flags(&mut args)?;
    if args.len() != 1 {
        return Err(
            "usage: archie-voice draw \"<painting>\" --socket … --key-file … --policy …".into(),
        );
    }
    let work = draw::find_work(&args[0])
        .ok_or_else(|| format!("I can draw {}; not {}", draw::repertoire(language), args[0]))?;
    let studio = studio(&stage, &flags);
    let subtitles = Narration::new(flags.subtitles.clone().map(Into::into));
    let started = Instant::now();
    let mut narrate = NarrateWith(move |said: draw::Said| {
        println!("{:>7.1}s  {}", started.elapsed().as_secs_f32(), said.text);
        subtitles.subtitle(&said.text, said.lang.code());
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    narrate.tell(draw::say(work, language, &draw::Beat::Accepted));
    let summary = runtime.block_on(draw::run(studio, work, language, &mut narrate, stop))?;
    println!(
        "{} lines, {} commands, {} refused, {:.1}s ({:.1}s waiting for a person), {:.0} pixels of line a second",
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

/// Where the stage is, and how to act on it.
struct StageFlags {
    socket: String,
    key_file: String,
    policy: String,
    display: Option<String>,
    steps: usize,
    ground_model: Option<String>,
    ground_endpoint: Option<String>,
    app_log: Option<String>,
}

fn stage_flags(args: &mut Vec<String>) -> Result<StageFlags, String> {
    let steps = match take(args, "--steps") {
        None => 6,
        Some(steps) => steps
            .parse::<usize>()
            .ok()
            .filter(|steps| (1..=8).contains(steps))
            .ok_or("--steps is between 1 and 8")?,
    };
    Ok(StageFlags {
        socket: take(args, "--socket").ok_or("do needs --socket")?,
        key_file: take(args, "--key-file").ok_or("do needs --key-file")?,
        policy: take(args, "--policy").ok_or("do needs --policy")?,
        display: take(args, "--stage-display"),
        steps,
        ground_model: take(args, "--ground-model"),
        ground_endpoint: take(args, "--ground-endpoint"),
        app_log: take(args, "--app-log"),
    })
}

#[cfg(not(unix))]
fn act(_asking: Asking, _flags: StageFlags) -> Result<ExitCode, String> {
    Err("do acts on a stage, which listens on a Unix socket; run it where the stage runs".into())
}

#[cfg(unix)]
type StageHands =
    archie_voice::Hands<tokio::net::unix::OwnedReadHalf, tokio::net::unix::OwnedWriteHalf>;

/// Hands on the stage the flags name, and the policy they act under.
#[cfg(unix)]
fn connect(
    asking: &Asking,
    flags: &StageFlags,
) -> Result<(StageHands, archie_stage::StagePolicy), String> {
    use archie_iris::LocateAnythingClient;
    use archie_policy::{PolicyEngine, Principal};
    use archie_sight::Sight;
    use archie_stage::{learn, Operator, StageClient, StagePolicy, StageWorld};
    use archie_types::AgentId;
    use archie_voice::{Grounding, Hands, Launch};
    use std::path::Path;
    /// How long a click waits to see the window it pressed repaint.
    const CLICK_REPAINT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
    use std::sync::Arc;
    use tokio::net::UnixStream;

    let policy = StagePolicy::load(&flags.policy).map_err(|error| error.to_string())?;
    let key = load_key(&flags.key_file)?;
    let snapshot_dir = Path::new(&flags.socket)
        .parent()
        .ok_or("--socket has no directory")?
        .join("snapshots");
    let (client, world) = asking.runtime.block_on(async {
        let stream = UnixStream::connect(&flags.socket)
            .await
            .map_err(|error| format!("{}: {error}", flags.socket))?;
        let (read, write) = stream.into_split();
        let mut client = StageClient::new(read, write);
        let mut world = StageWorld::default();
        learn(&mut client, &mut world, Duration::from_millis(400))
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((client, world))
    })?;
    let principal = Principal::Agent {
        agent: AgentId::new(),
    };
    let operator = Operator::new(key, policy.clone(), PolicyEngine::default());
    // A window drawn in software inside a nested compositor (the stage's own desktop under
    // WSLg) repaints well after the 300 ms a GPU-drawn one needs: a click that opened Sentry
    // was reported as not done, and retried until the dock had toggled it shut and open.
    let mut sight = Sight::new(operator, client, world, snapshot_dir, principal.clone())
        .with_timeouts(archie_sight::RECEIPT_TIMEOUT, CLICK_REPAINT_TIMEOUT);
    // What applications declare about their controls is the first way to find one: exact,
    // live, and it needs neither a capture nor a model.
    match asking
        .runtime
        .block_on(archie_a11y::Accessibility::connect())
    {
        Ok(bus) => sight = sight.asking(Box::new(bus)),
        Err(error) => eprintln!("archie-voice: nothing to ask about controls ({error})"),
    }
    let mut hands = Hands::new(
        sight,
        principal,
        asking.grade,
        Launch {
            wayland_display: flags.display.clone(),
            log: flags.app_log.clone().map(Into::into),
        },
    );
    if let Some(model) = &flags.ground_model {
        let cloud = flags.ground_endpoint.is_none();
        let endpoint = match &flags.ground_endpoint {
            None => ProviderEndpoint::nebius("https://api.tokenfactory.nebius.com/v1"),
            Some(url) => ProviderEndpoint::nvidia_nim(url, Locality::Local),
        }
        .map_err(|error| error.to_string())?;
        // The grounding model's local server is a NIM, unlike the voice's llama-server.
        let descriptor = descriptor(
            model,
            "look",
            if cloud {
                ProviderKind::NebiusTokenFactory
            } else {
                ProviderKind::NvidiaNim
            },
            asking.grade,
            flags.ground_endpoint.as_deref(),
        )?;
        let locality = descriptor.locality;
        let client =
            LocateAnythingClient::new(endpoint, descriptor).map_err(|error| error.to_string())?;
        hands = hands.seeing(Grounding {
            client: Arc::new(client),
            locality,
        });
    }
    Ok((hands, policy))
}

/// What the model is told about this moment: the clock, what is in front, what it may open.
fn context_for(clock: &str, front: Option<String>, openable: &[String]) -> String {
    let mut context = clock.to_string();
    context.push_str(&match front {
        Some(app) => format!("\nAplicación al frente: {app}."),
        None => "\nNo hay ninguna aplicación al frente.".to_string(),
    });
    if !openable.is_empty() {
        context.push_str(&format!(
            "\nAplicaciones que puedes abrir: {}.",
            openable.join(", ")
        ));
    }
    context
}

#[cfg(unix)]
fn openable(policy: &archie_stage::StagePolicy) -> Vec<String> {
    policy
        .apps()
        .filter(|app| !app.launch.is_empty())
        .map(|app| app.display_name.clone())
        .collect()
}

/// What a drawing under way is: the painting, and how to stop it.
#[cfg(all(unix, any(feature = "speech", feature = "live")))]
type Drawing = (
    &'static archie_voice::draw::Work,
    Arc<std::sync::atomic::AtomicBool>,
);

/// The stage as a conversation uses it: one pair of hands doing one operation at a time,
/// and a drawing that runs on a connection of its own, for minutes, while the conversation
/// goes on and Archie tells what it is doing.
#[cfg(all(unix, any(feature = "speech", feature = "live")))]
#[derive(Clone)]
struct Desk {
    stage: Arc<tokio::sync::Mutex<Option<(StageHands, archie_stage::StagePolicy)>>>,
    /// The application in front, as last seen; unknown while an operation holds the stage.
    front: Arc<std::sync::Mutex<Option<String>>>,
    openable: Arc<Vec<String>>,
    /// ArchieOS's shell, when it is the stage's desktop and the policy lets Archie press its
    /// controls: what the model is told to call it.
    desktop: Option<String>,
    studio: archie_voice::draw::Studio,
    narration: archie_voice::narration::Narration,
    drawing: Arc<std::sync::Mutex<Option<Drawing>>>,
    /// What the person said last, for the language a drawing is told in.
    heard: Arc<std::sync::Mutex<String>>,
}

#[cfg(all(unix, any(feature = "speech", feature = "live")))]
impl Desk {
    fn open(asking: &Asking, stage: &StageFlags, draw: &DrawFlags) -> Result<Self, String> {
        let connection = connect(asking, stage)?;
        let openable = openable(&connection.1);
        let front = connection.0.front();
        let desktop = connection
            .1
            .app("archie-display")
            .filter(|app| app.allow.contains(&archie_stage::ActionKind::Click))
            .map(|app| {
                app.aliases
                    .first()
                    .cloned()
                    .unwrap_or_else(|| app.display_name.clone())
            });
        Ok(Self {
            stage: Arc::new(tokio::sync::Mutex::new(Some(connection))),
            front: Arc::new(std::sync::Mutex::new(front)),
            openable: Arc::new(openable),
            desktop,
            studio: studio(stage, draw),
            narration: archie_voice::narration::Narration::new(
                draw.subtitles.clone().map(Into::into),
            ),
            drawing: Arc::default(),
            heard: Arc::default(),
        })
    }

    fn heard(&self, text: &str) {
        *self.heard.lock().expect("heard lock") = text.to_string();
    }

    /// What the model is told about the desk this turn, after `clock`.
    fn context(&self, clock: &str) -> String {
        let front = self.front.lock().expect("front lock").clone();
        let mut context = context_for(clock, front, &self.openable);
        if let Some(name) = &self.desktop {
            context.push_str(&format!(
                "\nEl escritorio de ArchieOS está detrás de todo: su dock tiene Orb, Vault, Forge, \
                 Sentry, World, Stage y Ajustes. Para abrir una de esas, usa click con target \
                 su nombre y app «{name}»."
            ));
        }
        context.push_str(&match self.drawing.lock().expect("drawing lock").as_ref() {
            Some((work, _)) => format!(
                "\nEstás dibujando {} en Photopea y lo vas contando tú solo; no lo empieces de \
                 nuevo. Si te piden parar, usa stop_drawing.",
                work.es
            ),
            None => format!(
                "\nPuedes dibujar en Photopea con draw_picture: {}.",
                archie_voice::draw::repertoire(archie_voice::draw::Lang::Es)
            ),
        });
        context
    }

    /// Carries out a stage tool, or says in words why not.
    async fn handle(self, intent: archie_speech::SpeechToolIntentCandidate) -> String {
        use archie_voice::Plan;
        // Own the transport while awaiting an operation. Cancellation drops it instead of
        // reusing a possibly half-read protocol connection.
        let Some(mut connection) = self.stage.lock().await.take() else {
            return "No se hizo: el escritorio no está conectado o su operación fue interrumpida. Vuelve a iniciar la sesión de voz para reconectarlo.".into();
        };
        *self.front.lock().expect("front lock") = None;
        let (hands, policy) = &mut connection;
        let outcome = match archie_voice::decide(&intent, policy, hands.front().as_deref()) {
            Ok(Plan::Draw { work, lang, .. }) => self.start_drawing(work, lang),
            Ok(Plan::StopDrawing) => self.stop_drawing(),
            Ok(plan) => hands.perform(&plan).await.report,
            Err(refusal) => format!("No se hizo: {}", refusal.spoken),
        };
        *self.front.lock().expect("front lock") = hands.front();
        *self.stage.lock().await = Some(connection);
        outcome
    }

    fn start_drawing(&self, id: &str, lang: Option<archie_voice::draw::Lang>) -> String {
        use archie_voice::draw::{self, Beat, Lang};
        let Some(work) = draw::find_work(id) else {
            return "No se hizo: no conozco ese cuadro.".into();
        };
        let lang = lang.unwrap_or_else(|| {
            Lang::of(archie_voice::language::spoken_language(
                &self.heard.lock().expect("heard lock"),
            ))
        });
        let mut slot = self.drawing.lock().expect("drawing lock");
        if let Some((under_way, _)) = slot.as_ref() {
            return format!(
                "No se hizo: ya estoy dibujando {}; termina en unos minutos o se puede detener.",
                under_way.es
            );
        }
        if !draw::exists(&self.studio, work) {
            return format!("No se hizo: no encuentro la imagen de {}.", work.es);
        }
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        *slot = Some((work, stop.clone()));
        let (studio, narration, drawing) = (
            self.studio.clone(),
            self.narration.clone(),
            self.drawing.clone(),
        );
        tokio::spawn(async move {
            let mut teller = narration.clone();
            let outcome = draw::run(studio, work, lang, &mut teller, stop).await;
            match outcome {
                Ok(summary) => println!(
                    "dibujo    {}: {} líneas, {} rechazadas, {:.0} s",
                    work.id, summary.lines, summary.refused, summary.seconds
                ),
                Err(why) => {
                    eprintln!("archie-voice: the drawing of {} failed: {why}", work.id);
                    narration.push(draw::say(work, lang, &Beat::Failed { why }));
                }
            }
            *drawing.lock().expect("drawing lock") = None;
        });
        format!(
            "Empecé a dibujar {} en Photopea. Sigue solo unos minutos y yo mismo voy contando \
             cada paso en voz alta: responde solo con una frase corta.",
            work.es
        )
    }

    fn stop_drawing(&self) -> String {
        match self.drawing.lock().expect("drawing lock").as_ref() {
            Some((work, stop)) => {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
                format!(
                    "Detengo el dibujo de {} después de la tanda de líneas en curso.",
                    work.es
                )
            }
            None => "No estoy dibujando nada.".into(),
        }
    }
}

#[cfg(unix)]
fn act(mut asking: Asking, flags: StageFlags) -> Result<ExitCode, String> {
    use archie_voice::decide;

    let (mut hands, policy) = connect(&asking, &flags)?;
    let openable = openable(&policy);
    let clock = asking.clock.clone();
    let context = |front: Option<String>| context_for(&clock, front, &openable);

    println!("heard: {}", asking.heard);
    let started = Instant::now();
    asking.cognitive.set_context(context(hands.front()));
    asking
        .cognitive
        .begin(CognitiveRequest {
            turn_id: "turn-1".into(),
            transcript: asking.heard.clone(),
            language: asking.language.clone(),
            budget: asking.budget,
        })
        .map_err(|error| error.to_string())?;
    let mut performed = 0usize;
    let mut refused = 0usize;
    for number in 1..=flags.steps {
        let step = listen(&mut asking.cognitive, started);
        if step.failed {
            return Ok(ExitCode::from(4));
        }
        if step.intents.is_empty() {
            println!(
                "finished in {} ms after {number} step(s): {performed} done, {refused} not done",
                started.elapsed().as_millis()
            );
            return Ok(ExitCode::SUCCESS);
        }
        let mut results = Vec::new();
        for intent in &step.intents {
            let front = hands.front();
            let outcome = match decide(intent, &policy, front.as_deref()) {
                Ok(plan) => {
                    let at = Instant::now();
                    let done = asking.runtime.block_on(hands.perform(&plan));
                    println!(
                        "{:>6} ms  {}  {:?} in {} ms: {}",
                        started.elapsed().as_millis(),
                        if done.done { "did " } else { "not " },
                        plan_name(&plan),
                        at.elapsed().as_millis(),
                        done.report
                    );
                    if done.done {
                        performed += 1;
                    } else {
                        refused += 1;
                    }
                    done.report
                }
                Err(refusal) => {
                    refused += 1;
                    println!(
                        "{:>6} ms  refused ({}): {}",
                        started.elapsed().as_millis(),
                        refusal.code,
                        refusal.spoken
                    );
                    format!("No se hizo: {}", refusal.spoken)
                }
            };
            results.push((intent.correlation_id.clone(), outcome));
        }
        if number == flags.steps {
            break;
        }
        asking.cognitive.set_context(context(hands.front()));
        asking
            .cognitive
            .follow_up("turn-1", &results, asking.budget)
            .map_err(|error| error.to_string())?;
    }
    println!(
        "stopped after {} steps: {performed} done, {refused} not done",
        flags.steps
    );
    Ok(ExitCode::from(5))
}

fn flag(args: &mut Vec<String>, name: &str) -> bool {
    match args.iter().position(|arg| arg == name) {
        Some(at) => {
            args.remove(at);
            true
        }
        None => false,
    }
}

/// Where the ears and the voice come from, and how the room is set up.
#[allow(dead_code)]
struct SpeechFlags {
    asr_model: String,
    engine: String,
    piper: String,
    piper_model: Option<String>,
    piper_model_en: Option<String>,
    tts_model: Option<String>,
    codec_model: Option<String>,
    tts_tokenizer: Option<String>,
    voice: Option<String>,
    tts_language: String,
    listen_language: String,
    always: bool,
    headphones: bool,
    wav_in: Option<String>,
    wav_out: Option<String>,
    threads: i32,
    end_after_ms: u64,
    lookahead: i32,
    name_boost: f32,
    /// The CUDA device the ears run on; -1 is the CPU.
    asr_gpu: i32,
    /// Where Sentry's owner notice board (`notices.json`) lives.
    notices: Option<std::path::PathBuf>,
}

fn default_notices_path() -> std::path::PathBuf {
    if let Ok(state_dir) = std::env::var("ARCHIE_STATE_DIR") {
        if !state_dir.is_empty() {
            return std::path::PathBuf::from(state_dir).join("cerebra/notices.json");
        }
    }
    if let Ok(root) = std::env::var("ARCHIE_ROOT") {
        if !root.is_empty() {
            return std::path::PathBuf::from(root).join("var/lib/archie/cerebra/notices.json");
        }
    }
    std::path::PathBuf::from("/var/lib/archie/cerebra/notices.json")
}

fn speech_flags(args: &mut Vec<String>) -> Result<SpeechFlags, String> {
    let wav_in = take(args, "--wav-in");
    let explicit_notices = take(args, "--notices")
        .or_else(|| {
            std::env::var("ARCHIE_NOTICES")
                .ok()
                .filter(|v| !v.is_empty())
        })
        .map(std::path::PathBuf::from);
    let notices = explicit_notices.or_else(|| wav_in.is_none().then(default_notices_path));
    Ok(SpeechFlags {
        asr_model: take(args, "--asr-model").ok_or("converse needs --asr-model")?,
        // Piper keeps up with speech on a laptop CPU; Magpie needs a GPU to.
        engine: take(args, "--voice-engine").unwrap_or_else(|| "piper".into()),
        piper: take(args, "--piper").unwrap_or_else(|| "piper".into()),
        piper_model: take(args, "--piper-model"),
        piper_model_en: take(args, "--piper-model-en"),
        tts_model: take(args, "--tts-model"),
        codec_model: take(args, "--codec-model"),
        tts_tokenizer: take(args, "--tts-tokenizer"),
        voice: take(args, "--voice"),
        tts_language: take(args, "--tts-language").unwrap_or_else(|| "es-US".into()),
        listen_language: take(args, "--listen-language").unwrap_or_else(|| "es-US".into()),
        always: flag(args, "--always"),
        headphones: flag(args, "--headphones"),
        wav_in,
        wav_out: take(args, "--wav-out"),
        threads: take(args, "--threads")
            .map(|n| n.parse::<i32>().map_err(|_| "--threads is a number"))
            .transpose()?
            .unwrap_or(0),
        end_after_ms: take(args, "--end-after-ms")
            .map(|n| n.parse::<u64>().map_err(|_| "--end-after-ms is a number"))
            .transpose()?
            .unwrap_or(800),
        // Thirteen 80 ms frames: the least lookahead that kept up with speech on a
        // Ryzen 5 4500U. A faster machine can answer sooner with less.
        lookahead: take(args, "--asr-lookahead")
            .map(|n| {
                n.parse::<i32>()
                    .map_err(|_| "--asr-lookahead is a number of frames")
            })
            .transpose()?
            .unwrap_or(13),
        // Measured on NVIDIA's GGUF that embeds its tokenizer (the only kind that boosts):
        // at 5 the recogniser heard the favoured words everywhere ("Photopeaw the
        // Photopea", "la mona Sentry"), at 3 still sometimes, at 1.5 it named Archie and
        // the Mona Lisa in both languages and heard "Sentry" where 5 heard "century".
        name_boost: take(args, "--name-boost")
            .map(|n| n.parse::<f32>().map_err(|_| "--name-boost is a number"))
            .transpose()?
            .unwrap_or(1.5),
        asr_gpu: take(args, "--asr-gpu")
            .map(|n| {
                n.parse::<i32>()
                    .map_err(|_| "--asr-gpu is a CUDA device index, or -1")
            })
            .transpose()?
            .unwrap_or(-1),
        notices,
    })
}

// Read only by the live path (unix + `live`); other builds parse and drop them.
#[cfg_attr(not(all(unix, feature = "live")), allow(dead_code))]
struct LiveFlags {
    /// Explicit full context domain for Core-admitted world proposals. No default.
    world_domain: Option<archie_vault::ContextDomain>,
    port: u16,
    iris: String,
    /// Milliseconds between IRIS's open looks; frames in between follow what it found.
    iris_sweep_ms: u64,
    iris_timeout_seconds: u64,
    tts_english: String,
    curiosity: u32,
    /// An offline ASR model (Parakeet TDT) that hears each finished utterance again, whole.
    rehear_model: Option<String>,
    voice_style: String,
    /// Seconds after Archie spoke that a reply needs no name.
    follow_up_s: u64,
    /// An RVC voice exported to ONNX (its `<name>.json`): every sentence is converted into it.
    rvc_voice: Option<String>,
    rvc_key: f32,
    rvc_index_rate: f32,
    /// ONNX Runtime's shared library; `ORT_DYLIB_PATH` if not given.
    ort_library: Option<String>,
}

fn live_flags(args: &mut Vec<String>) -> Result<LiveFlags, String> {
    Ok(LiveFlags {
        world_domain: take(args, "--world-domain")
            .map(archie_vault::ContextDomain::try_from)
            .transpose()
            .map_err(|error| error.to_string())?,
        port: take(args, "--port")
            .map(|n| n.parse::<u16>().map_err(|_| "--port is a port number"))
            .transpose()?
            .unwrap_or(8770),
        iris: take(args, "--iris").unwrap_or_else(|| "http://127.0.0.1:8765".into()),
        iris_timeout_seconds: take(args, "--iris-timeout-seconds")
            .map(|value| {
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|seconds| (1..=900).contains(seconds))
                    .ok_or("--iris-timeout-seconds must be from 1 to 900")
            })
            .transpose()?
            .unwrap_or(60),
        iris_sweep_ms: take(args, "--iris-sweep-ms")
            .map(|n| {
                n.parse::<u64>()
                    .map_err(|_| "--iris-sweep-ms is milliseconds (0: open every frame)")
            })
            .transpose()?
            .unwrap_or(2000),
        tts_english: take(args, "--tts-english").unwrap_or_else(|| "en-US".into()),
        // Looks closer that curiosity may take on its own in one session: each is a
        // paid call to a vision model on Nebius.
        curiosity: take(args, "--curiosity")
            .map(|n| {
                n.parse::<u32>()
                    .map_err(|_| "--curiosity is a number of looks")
            })
            .transpose()?
            .unwrap_or(40),
        rehear_model: take(args, "--rehear-model"),
        voice_style: take(args, "--voice-style").unwrap_or_else(|| "plain".into()),
        follow_up_s: take(args, "--follow-up-s")
            .map(|n| {
                n.parse::<u64>()
                    .map_err(|_| "--follow-up-s is a number of seconds")
            })
            .transpose()?
            .unwrap_or(15),
        rvc_voice: take(args, "--rvc-voice"),
        rvc_key: take(args, "--rvc-key")
            .map(|n| {
                n.parse::<f32>()
                    .map_err(|_| "--rvc-key is a number of semitones")
            })
            .transpose()?
            .unwrap_or(0.0),
        rvc_index_rate: take(args, "--rvc-index-rate")
            .map(|n| {
                n.parse::<f32>()
                    .map_err(|_| "--rvc-index-rate is between 0 and 1")
            })
            .transpose()?
            .unwrap_or(0.0),
        ort_library: take(args, "--ort-library").or_else(|| std::env::var("ORT_DYLIB_PATH").ok()),
    })
}

/// The Magpie voice converted, sentence by sentence, into an RVC voice.
#[cfg(all(unix, feature = "live", feature = "rvc"))]
fn converted_voice(
    voice: archie_voice::live::language::FollowingVoice,
    manifest: &str,
    flags: &LiveFlags,
) -> Result<Box<dyn archie_voice::voice::Speaks>, String> {
    use archie_voice::rvc::{init_runtime, ConvertedVoice, RvcVoice, Tuning};
    let library = flags
        .ort_library
        .as_deref()
        .ok_or("--rvc-voice needs ONNX Runtime: give --ort-library or set ORT_DYLIB_PATH")?;
    init_runtime(std::path::Path::new(library))?;
    let at = Instant::now();
    let rvc = RvcVoice::load(
        std::path::Path::new(manifest),
        Tuning {
            key: flags.rvc_key,
            index_rate: flags.rvc_index_rate,
        },
        4,
    )?;
    println!(
        "converted voice {manifest} ready in {} ms",
        at.elapsed().as_millis()
    );
    Ok(Box::new(ConvertedVoice::new(voice, rvc)))
}

#[cfg(all(unix, feature = "live", not(feature = "rvc")))]
fn converted_voice(
    _voice: archie_voice::live::language::FollowingVoice,
    _manifest: &str,
    _flags: &LiveFlags,
) -> Result<Box<dyn archie_voice::voice::Speaks>, String> {
    Err("this build has no converted voices: build archie-voice with --features live,rvc".into())
}

#[cfg(not(all(unix, feature = "live")))]
fn live(
    _asking: Asking,
    _speech: SpeechFlags,
    _flags: LiveFlags,
    _stage: Option<StageFlags>,
    _draw: DrawFlags,
) -> Result<ExitCode, String> {
    Err(
        "this build has no live session: build archie-voice with --features live, where \
         NeMo-Speech.cpp is installed"
            .into(),
    )
}

/// Talks with a person through a browser page: their microphone and camera in, the voice
/// out, IRIS for eyes, and curiosity keeping what it found out until it is asked.
#[cfg(all(unix, feature = "live"))]
fn live(
    asking: Asking,
    speech: SpeechFlags,
    flags: LiveFlags,
    stage: Option<StageFlags>,
    draw: DrawFlags,
) -> Result<ExitCode, String> {
    use archie_speech::{FloorPolicy, SessionPlan, SpeechCapabilities, SpeechSession};
    use archie_voice::conversation::{self, Hearing, Saying, Setup, StreamedVoice};
    use archie_voice::live::{self, curiosity, eyes, language};
    use archie_voice::native::{Ears, Voice, VoiceFiles};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc};

    if flags.rvc_voice.is_some() && speech.engine != "magpie" {
        return Err("RVC live conversion currently requires --voice-engine magpie".into());
    }
    // The stage, when one is named: the same hands, drawing and narration as `converse`.
    let desk = match &stage {
        Some(stage) => Some(Desk::open(&asking, stage, &draw)?),
        None => None,
    };
    let at = Instant::now();
    let ears = Ears::load_on(
        Path::new(&speech.asr_model),
        Some(speech.lookahead),
        speech.asr_gpu,
    )
    .map_err(|e| e.to_string())?;
    println!("ears ready in {} ms", at.elapsed().as_millis());
    let rehear = match &flags.rehear_model {
        Some(model) => {
            let at = Instant::now();
            let second =
                Ears::load_on(Path::new(model), None, speech.asr_gpu).map_err(|e| e.to_string())?;
            println!("second hearing ready in {} ms", at.elapsed().as_millis());
            Some(second)
        }
        None => None,
    };
    let at = Instant::now();
    let voice: Box<dyn archie_voice::voice::Speaks> = match speech.engine.as_str() {
        "magpie" => {
            let style = live::style::Style::parse(&flags.voice_style)?;
            let needed = |value: &Option<String>, flag: &str| {
                value
                    .clone()
                    .ok_or(format!("live Magpie voice needs {flag}"))
            };
            let (magpie, codec, tokenizer) = (
                needed(&speech.tts_model, "--tts-model")?,
                needed(&speech.codec_model, "--codec-model")?,
                needed(&speech.tts_tokenizer, "--tts-tokenizer")?,
            );
            let voice = Voice::load(
                &VoiceFiles {
                    magpie: Path::new(&magpie),
                    codec: Path::new(&codec),
                    tokenizer_dir: Path::new(&tokenizer),
                },
                &speech.tts_language,
                speech.voice.as_deref(),
                speech.threads,
            )
            .map_err(|e| e.to_string())?;
            let following =
                language::FollowingVoice::new(voice, &speech.tts_language, &flags.tts_english);
            match &flags.rvc_voice {
                Some(manifest) => converted_voice(following, manifest, &flags)?,
                None => Box::new(live::style::StyledVoice::new(following, style)),
            }
        }
        "piper" => {
            let model = speech
                .piper_model
                .as_deref()
                .ok_or("live Piper voice needs --piper-model <voice.onnx>")?;
            Box::new(archie_voice::piper::BilingualPiper::start(
                Path::new(&speech.piper),
                Path::new(model),
                speech.piper_model_en.as_deref().map(Path::new),
            )?)
        }
        other => {
            return Err(format!(
                "live --voice-engine is piper or magpie, not {other}"
            ))
        }
    };
    let rate = voice.sample_rate();
    println!("voice ready in {} ms, {rate} Hz", at.elapsed().as_millis());

    let (audio_in, audio) = mpsc::channel::<Vec<f32>>();
    let (voice_out, voice_audio) = mpsc::channel();
    let identifier = curiosity::Identifier::from_environment()?;
    let researcher = curiosity::Researcher::from_environment()?;
    println!(
        "curiosity: identify with {}; search {}",
        if identifier.means().is_empty() {
            "IRIS only".to_string()
        } else {
            identifier.means().join(" + ")
        },
        if researcher.is_some() {
            "with Tavily"
        } else {
            "off (no TAVILY_API_KEY)"
        }
    );
    let world_scope = flags
        .world_domain
        .clone()
        .map(|domain| live::WorldVoiceScope {
            domain,
            privacy: asking.grade,
        });
    let senses = live::Senses::new_with_world(
        audio_in,
        identifier,
        researcher,
        rate,
        flags.curiosity,
        asking.wanda_access,
        world_scope,
    );
    let eyes = eyes::IrisEyes::with_timeout(
        &flags.iris,
        0.35,
        std::time::Duration::from_secs(flags.iris_timeout_seconds),
    )?;
    let sweep = eyes::Sweep::new(std::time::Duration::from_millis(flags.iris_sweep_ms));
    {
        let senses = senses.clone();
        let port = flags.port;
        asking.runtime.spawn(async move {
            if let Err(error) = live::serve(senses, eyes, sweep, port).await {
                eprintln!("live page: {error}");
            }
        });
    }
    live::forward_voice(voice_audio, senses.clone());
    println!("open http://127.0.0.1:{} (through the tunnel)", flags.port);

    let languages = vec!["es".to_string(), "en".to_string()];
    let plan = SessionPlan {
        language: asking.language.clone(),
        listener: SpeechCapabilities {
            partial_transcript: true,
            languages: languages.clone(),
            input_sample_rates: vec![archie_voice::audio::CAPTURE_RATE],
            ..SpeechCapabilities::default()
        },
        voice: SpeechCapabilities {
            accepts_external_response_text: true,
            renders_external_response_verbatim: true,
            languages,
            output_sample_rates: vec![rate],
            ..SpeechCapabilities::default()
        },
    };
    let Asking {
        runtime,
        mut cognitive,
        clock,
        ..
    } = asking;
    cognitive.set_persona(live::PERSONA);
    // The Observatory lists its own scenes and presets; without it the names stay open and
    // the display refuses any it does not have.
    let catalog = runtime.block_on(archie_orb::control::catalog()).ok();
    cognitive.add_tools(live::tools(catalog.as_ref()));
    if desk.is_some() {
        cognitive.add_tools(tools::drawing());
    }
    cognitive.put_context_beside_utterance();
    let mut session =
        SpeechSession::new(plan, FloorPolicy::default(), cognitive).map_err(|e| e.to_string())?;
    let tool_senses = senses.clone();
    let action_desk = desk.clone();
    let mut act = archie_voice::actions::AsyncActions::new(
        runtime.handle().clone(),
        Duration::from_secs(90),
        move |intent: archie_speech::SpeechToolIntentCandidate| {
            let senses = tool_senses.clone();
            let desk = action_desk.clone();
            async move {
                if let Some(done) = senses
                    .handle_tool(&intent.tool_name, &intent.raw_arguments)
                    .await
                {
                    return done;
                }
                match desk {
                    Some(desk) => desk.handle(intent).await,
                    None => format!(
                        "No se hizo: aquí no tengo la herramienta {}.",
                        intent.tool_name
                    ),
                }
            }
        },
    );
    let mut context = || match &desk {
        // The clock once, then what IRIS sees and what is remembered, then the desk.
        Some(desk) => format!("{}{}", senses.context(&clock), desk.context("")),
        None => senses.context(&clock),
    };
    let started = Instant::now();
    let heard_desk = desk.clone();
    let mut report = |moment: conversation::Moment| {
        if let conversation::Moment::Heard { text, .. } = &moment {
            senses.heard(text);
            if let Some(desk) = &heard_desk {
                desk.heard(text);
            }
        }
        senses.publish(live::moment_json(&moment));
        let at = started.elapsed().as_millis();
        match &moment {
            conversation::Moment::Timing {
                turn_id,
                phase,
                elapsed_ms,
                queued_segments,
            } => println!(
                "{at:>7} ms  timing   {turn_id} {phase} +{elapsed_ms} ms queue={queued_segments}"
            ),
            conversation::Moment::Heard { text, .. } => println!("{at:>7} ms  oí       {text}"),
            conversation::Moment::Said { text, .. } => println!("{at:>7} ms  dije     {text}"),
            conversation::Moment::Proposed { tool, arguments } => {
                println!("{at:>7} ms  propone  {tool}({arguments})")
            }
            conversation::Moment::Did { report } => println!("{at:>7} ms  resultado {report}"),
            conversation::Moment::Alert(message) => println!("{at:>7} ms  alerta   {message}"),
            _ => {}
        }
    };
    let setup = Setup {
        listen_language: speech.listen_language.clone(),
        always: speech.always,
        headphones: speech.headphones,
        end_after: Duration::from_millis(speech.end_after_ms),
        follow_up: Duration::from_secs(flags.follow_up_s),
        steps: 4,
        phrases: vec!["Archie".into(), "Archi".into()],
        phrase_boost: speech.name_boost,
        notices: speech.notices.clone(),
        narration: desk.as_ref().map(|desk| desk.narration.clone()),
    };
    println!(
        "{}",
        if speech.always {
            "answering everything (--always)"
        } else {
            "answering when named, or within the follow-up window"
        }
    );
    conversation::converse_with_actions(
        &setup,
        &ears,
        rehear
            .as_ref()
            .map(|ears| ears as &dyn archie_voice::asr::SpeechRecognizer),
        voice,
        Hearing::Stream(audio),
        Saying::Stream(StreamedVoice::new(voice_out, rate)),
        &mut session,
        &mut act,
        &mut context,
        Arc::new(AtomicBool::new(false)),
        &mut report,
    )?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(not(all(unix, feature = "speech")))]
fn converse(
    _asking: Asking,
    _speech: SpeechFlags,
    _stage: Option<StageFlags>,
    _draw: DrawFlags,
) -> Result<ExitCode, String> {
    Err(
        "this build has no ears or voice: build archie-voice with --features speech, \
         where NeMo-Speech.cpp and PulseAudio are installed"
            .into(),
    )
}

/// Listens, answers out loud, and acts — until a recording runs out or the process is
/// stopped.
#[cfg(all(unix, feature = "speech"))]
fn converse(
    asking: Asking,
    speech: SpeechFlags,
    stage: Option<StageFlags>,
    draw: DrawFlags,
) -> Result<ExitCode, String> {
    use archie_speech::{
        FloorPolicy, SessionPlan, SpeechCapabilities, SpeechSession, SpeechToolIntentCandidate,
    };
    use archie_voice::audio::{Microphone, Speaker};
    use archie_voice::conversation::{self, Hearing, Moment, Saying, Setup};
    use archie_voice::native::{Ears, Voice, VoiceFiles};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let steps = stage.as_ref().map_or(6, |flags| flags.steps);
    let desk = match &stage {
        Some(flags) => Some(Desk::open(&asking, flags, &draw)?),
        None => None,
    };

    let at = Instant::now();
    let ears = Ears::load(Path::new(&speech.asr_model), Some(speech.lookahead))
        .map_err(|e| e.to_string())?;
    println!(
        "ears ready in {} ms (NeMo-Speech.cpp {})",
        at.elapsed().as_millis(),
        archie_voice::native::runtime_version()
    );
    let at = Instant::now();
    let voice: Box<dyn archie_voice::voice::Speaks> = match speech.engine.as_str() {
        "magpie" => {
            let needed = |value: &Option<String>, flag: &str| {
                value
                    .clone()
                    .ok_or(format!("the magpie voice needs {flag}"))
            };
            let (magpie, codec, tokenizer) = (
                needed(&speech.tts_model, "--tts-model")?,
                needed(&speech.codec_model, "--codec-model")?,
                needed(&speech.tts_tokenizer, "--tts-tokenizer")?,
            );
            let voice = Voice::load(
                &VoiceFiles {
                    magpie: Path::new(&magpie),
                    codec: Path::new(&codec),
                    tokenizer_dir: Path::new(&tokenizer),
                },
                &speech.tts_language,
                speech.voice.as_deref(),
                speech.threads,
            )
            .map_err(|e| e.to_string())?;
            println!("voices: {}", voice.voices().join(", "));
            Box::new(voice)
        }
        "piper" => {
            let model = speech
                .piper_model
                .clone()
                .ok_or("the piper voice needs --piper-model <voice.onnx>")?;
            Box::new(archie_voice::piper::BilingualPiper::start(
                Path::new(&speech.piper),
                Path::new(&model),
                speech.piper_model_en.as_deref().map(Path::new),
            )?)
        }
        other => return Err(format!("--voice-engine is piper or magpie, not {other}")),
    };
    let rate = voice.sample_rate();
    println!(
        "voice ({}) ready in {} ms, {rate} Hz",
        speech.engine,
        at.elapsed().as_millis()
    );

    let hearing = match &speech.wav_in {
        Some(path) => Hearing::recording(Path::new(path), Duration::from_secs(4))?,
        None => Hearing::Microphone(Microphone::open("Archie").map_err(|e| e.to_string())?),
    };
    let saying = match &speech.wav_out {
        Some(path) => Saying::Recording {
            path: path.into(),
            samples: Vec::new(),
            rate,
        },
        None => Saying::Speaker(Speaker::open("Archie", rate).map_err(|e| e.to_string())?),
    };

    let languages = vec!["es".to_string(), "en".to_string()];
    let plan = SessionPlan {
        language: asking.language.clone(),
        listener: SpeechCapabilities {
            partial_transcript: true,
            languages: languages.clone(),
            input_sample_rates: vec![archie_voice::audio::CAPTURE_RATE],
            ..SpeechCapabilities::default()
        },
        voice: SpeechCapabilities {
            accepts_external_response_text: true,
            renders_external_response_verbatim: true,
            languages,
            output_sample_rates: vec![rate],
            ..SpeechCapabilities::default()
        },
    };
    let Asking {
        runtime,
        mut cognitive,
        clock,
        ..
    } = asking;
    if desk.is_some() {
        cognitive.add_tools(tools::drawing());
    }
    let mut session =
        SpeechSession::new(plan, FloorPolicy::default(), cognitive).map_err(|e| e.to_string())?;

    // Context never waits on a tool holding the Stage across an await: the desk marks the
    // application in front unknown during an operation, and the next action still checks
    // the Stage policy.
    let action_desk = desk.clone();
    let mut act = archie_voice::actions::AsyncActions::new(
        runtime.handle().clone(),
        Duration::from_secs(90),
        move |intent: SpeechToolIntentCandidate| {
            let desk = action_desk.clone();
            async move {
                match desk {
                    Some(desk) => desk.handle(intent).await,
                    None => {
                        "No se hizo: esta conversación no tiene el escritorio conectado.".into()
                    }
                }
            }
        },
    );
    let mut context = || match &desk {
        Some(desk) => desk.context(&clock),
        None => context_for(&clock, None, &[]),
    };
    let started = Instant::now();
    let heard_desk = desk.clone();
    let mut report = |moment: Moment| {
        let at = started.elapsed().as_millis();
        match moment {
            Moment::Timing { turn_id, phase, elapsed_ms, queued_segments } => println!("{at:>7} ms  timing   {turn_id} {phase} +{elapsed_ms} ms queue={queued_segments}"),
            Moment::Heard { text, .. } => {
                if let Some(desk) = &heard_desk {
                    desk.heard(&text);
                }
                println!("{at:>7} ms  oí       {text}")
            }
            Moment::Floor {
                action,
                reason,
                evidence,
            } => println!("{at:>7} ms  piso     {action}: {reason}  [{}]", evidence.join("; ")),
            Moment::Said {
                text,
                first_audio_ms,
                audio_s,
                stopped,
            } => println!(
                "{at:>7} ms  dije     {text}  (primer audio {first_audio_ms:.0} ms, {audio_s:.1} s{})",
                if stopped { ", interrumpido" } else { "" }
            ),
            Moment::Proposed { tool, arguments } => {
                println!("{at:>7} ms  propone  {tool}({arguments})")
            }
            Moment::Did { report } => println!("{at:>7} ms  resultado {report}"),
            Moment::Interrupted => println!("{at:>7} ms  me interrumpieron: me callo"),
            Moment::Alert(message) => println!("{at:>7} ms  alerta   {message}"),
            // The loudness of the room and the voice's turns are for a display, not a log.
            Moment::Level { .. } | Moment::Speaking { .. } | Moment::Ready => {}
        }
    };
    let setup = Setup {
        listen_language: speech.listen_language.clone(),
        always: speech.always,
        headphones: speech.headphones,
        end_after: Duration::from_millis(speech.end_after_ms),
        follow_up: archie_voice::listen::FOLLOW_UP_WINDOW,
        steps,
        // The name, and with a desk the names its requests carry, which the recogniser
        // otherwise writes as other words ("moaner lea" for "Mona Lisa", "century" for
        // "Sentry"). Only words a person says: favouring "Photopea", "World" or "Forge"
        // made the recogniser hear them in everything. Boosting needs a GGUF that embeds
        // its tokenizer; without one the recogniser says so and hears the words unbiased.
        phrases: if desk.is_some() {
            ["Archie", "Mona Lisa", "Sentry", "Nebius"]
                .map(String::from)
                .to_vec()
        } else {
            vec!["Archie".into()]
        },
        phrase_boost: speech.name_boost,
        notices: speech.notices.clone(),
        narration: desk.as_ref().map(|desk| desk.narration.clone()),
    };
    println!(
        "escuchando ({}{}){}",
        speech.listen_language,
        if speech.always {
            ", respondo a todo"
        } else {
            ", di «Archie» para hablarme"
        },
        if speech.headphones {
            ", con audífonos: puedes interrumpirme"
        } else {
            ""
        }
    );
    conversation::converse_with_actions(
        &setup,
        &ears,
        None,
        voice,
        hearing,
        saying,
        &mut session,
        &mut act,
        &mut context,
        Arc::new(AtomicBool::new(false)),
        &mut report,
    )?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(unix)]
fn plan_name(plan: &archie_voice::Plan) -> String {
    use archie_voice::Plan;
    match plan {
        Plan::Open { app_id, .. } => format!("open {app_id}"),
        Plan::Click { app_id, target } => format!("click {target:?} in {app_id}"),
        Plan::Type { app_id, text } => format!("type {} chars in {app_id}", text.chars().count()),
        Plan::Keys { app_id, chord } => format!("keys {} in {app_id}", chord.key.name()),
        Plan::Draw { app_id, work, .. } => format!("draw {work} in {app_id}"),
        Plan::StopDrawing => "stop drawing".into(),
        Plan::Scroll {
            app_id,
            down,
            steps,
        } => {
            format!(
                "scroll {} {steps} in {app_id}",
                if *down { "down" } else { "up" }
            )
        }
    }
}

#[cfg(unix)]
fn load_key(path: &str) -> Result<archie_stage::CommandKey, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let bytes: [u8; 32] = hex::decode(text.trim())
        .map_err(|_| format!("{path}: not a hex key"))?
        .try_into()
        .map_err(|_| format!("{path}: a stage key is 32 bytes"))?;
    Ok(archie_stage::CommandKey::from_bytes(&bytes))
}

/// The clock, as a fact the model is given rather than a thing it guesses.
fn clock(offset: Option<time::UtcOffset>) -> String {
    let now = time::OffsetDateTime::now_utc();
    let (local, zone) = match offset {
        Some(offset) => (now.to_offset(offset), format!("UTC{offset}")),
        None => (now, "UTC (no se pudo leer la zona local)".to_string()),
    };
    format!(
        "Hora local: {:02}:{:02}, {} {} de {} de {}, zona {zone}.",
        local.hour(),
        local.minute(),
        weekday(local.weekday()),
        local.day(),
        month(local.month()),
        local.year()
    )
}

fn weekday(day: time::Weekday) -> &'static str {
    use time::Weekday::*;
    match day {
        Monday => "lunes",
        Tuesday => "martes",
        Wednesday => "miércoles",
        Thursday => "jueves",
        Friday => "viernes",
        Saturday => "sábado",
        Sunday => "domingo",
    }
}

fn month(month: time::Month) -> &'static str {
    use time::Month::*;
    match month {
        January => "enero",
        February => "febrero",
        March => "marzo",
        April => "abril",
        May => "mayo",
        June => "junio",
        July => "julio",
        August => "agosto",
        September => "septiembre",
        October => "octubre",
        November => "noviembre",
        December => "diciembre",
    }
}

/// The voice's model on this machine: a llama.cpp model, so the router asks its server's
/// /health before each turn, and each request carries the reasoning switch pinned here, in
/// both forms the local models read: `enable_thinking` (Bonsai 2 9B) and `reasoning_effort`
/// (Bonsai 2 27B, "medium" being its shorter reasoning).
fn local_voice(model: &str, url: &str, thinking: &str) -> Result<ModelDescriptor, String> {
    let mut desc = descriptor(
        model,
        "voice",
        ProviderKind::LlamaCpp,
        PrivacyGrade::Restricted,
        Some(url),
    )?;
    desc.labels.insert("thinking".into(), thinking.into());
    desc.labels.insert(
        "reasoning-effort".into(),
        if thinking == "on" { "medium" } else { "none" }.into(),
    );
    Ok(desc)
}

/// A model as the router knows it. In the cloud, with retention nobody has verified, it
/// may see what was graded public and nothing more, whatever the command line asked for.
fn descriptor(
    provider_model: &str,
    role: &str,
    provider: ProviderKind,
    grade: PrivacyGrade,
    endpoint: Option<&str>,
) -> Result<ModelDescriptor, String> {
    let cloud = provider == ProviderKind::NebiusTokenFactory;
    let mut labels = BTreeMap::new();
    labels.insert("provider-model".to_string(), provider_model.to_string());
    if let Some(url) = endpoint {
        labels.insert("base-url".to_string(), url.to_string());
    }
    let sanitized: String = provider_model
        .chars()
        .take(96)
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    Ok(ModelDescriptor {
        id: ModelId::parse(format!("{role}.model-{sanitized}"))
            .map_err(|_| "the model name cannot become an Archie model id".to_string())?,
        display_name: provider_model.to_string(),
        locality: if cloud {
            Locality::Cloud
        } else {
            Locality::Local
        },
        node: None,
        // llama-server splits its 65,536-token cache over 4 slots (archie-bonsai.service).
        context_window: if provider == ProviderKind::LlamaCpp {
            16_384
        } else {
            32_000
        },
        provider,
        // A grounding model that reasons before it answers needs room for one box.
        max_output_tokens: if role == "look" { 512 } else { 400 },
        capabilities: vec![
            Capability::known("compute.inference"),
            Capability::parse("cognition.response").expect("valid capability"),
            Capability::parse("cognition.reasoning").expect("valid capability"),
        ],
        max_privacy_grade: if cloud { PrivacyGrade::Public } else { grade },
        retention: if cloud {
            DataRetention::Unknown
        } else {
            DataRetention::ZeroVerified
        },
        availability: Availability::Ready,
        cost: TokenCost::FREE,
        latency_ema_ms: None,
        quality_tier: 50,
        required_accelerator_bytes: None,
        labels,
    })
}

#[cfg(test)]
mod iris_timeout_tests {
    use super::*;

    #[test]
    fn iris_cpu_timeout_is_bounded_and_configurable() {
        assert_eq!(live_flags(&mut vec![]).unwrap().iris_timeout_seconds, 60);
        for seconds in ["1", "60", "900"] {
            let mut args = vec!["--iris-timeout-seconds".into(), seconds.into()];
            assert_eq!(
                live_flags(&mut args)
                    .unwrap()
                    .iris_timeout_seconds
                    .to_string(),
                seconds
            );
            assert!(args.is_empty());
        }
        for invalid in ["0", "901", "-1", "infinite", "18446744073709551616"] {
            let mut args = vec!["--iris-timeout-seconds".into(), invalid.into()];
            assert!(live_flags(&mut args).is_err());
        }
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[test]
    fn voice_timeout_keeps_default_and_accepts_bounded_cpu_wait() {
        assert_eq!(request_timeout(None).unwrap(), Duration::from_secs(90));
        assert_eq!(
            request_timeout(Some("600")).unwrap(),
            Duration::from_secs(600)
        );
        assert_eq!(request_timeout(Some("1")).unwrap(), Duration::from_secs(1));
        assert_eq!(
            request_timeout(Some("900")).unwrap(),
            Duration::from_secs(900)
        );
        for bad in ["0", "901", "-1", "inf", "", "18446744073709551616"] {
            assert!(request_timeout(Some(bad)).is_err(), "accepted {bad:?}");
        }
    }
}

#[cfg(test)]
mod local_voice_tests {
    use super::*;
    use archie_model_runtime::liveness::Liveness;

    #[test]
    fn the_local_voice_is_bonsai_on_llama_cpp_with_its_thinking_pinned() {
        let desc = local_voice("bonsai", "http://127.0.0.1:8081/v1", "off").unwrap();
        assert_eq!(desc.provider, ProviderKind::LlamaCpp);
        assert_eq!(desc.locality, Locality::Local);
        assert!(desc.permits_privacy(PrivacyGrade::Restricted));
        assert_eq!(desc.labels.get("thinking").map(String::as_str), Some("off"));
        assert_eq!(
            desc.labels.get("reasoning-effort").map(String::as_str),
            Some("none")
        );
        let on = local_voice("bonsai", "http://127.0.0.1:8082/v1", "on").unwrap();
        assert_eq!(
            on.labels.get("reasoning-effort").map(String::as_str),
            Some("medium")
        );
        // The endpoint builds, and the router knows where to ask whether it is up.
        let endpoint = ProviderEndpoint::for_model(&desc).unwrap();
        assert_eq!(endpoint.locality(), Locality::Local);
        assert_eq!(
            Liveness::health_url(&desc).map(|url| url.to_string()),
            Some("http://127.0.0.1:8081/health".into())
        );
        // A switch the server would not understand is refused before any turn.
        let bad = local_voice("bonsai", "http://127.0.0.1:8081/v1", "maybe").unwrap();
        assert!(ProviderEndpoint::for_model(&bad).is_err());
        // The cloud voice stays Nebius, graded public.
        let cloud = descriptor(
            "nvidia/Nemotron-3_5-Lightning",
            "voice",
            ProviderKind::NebiusTokenFactory,
            PrivacyGrade::Internal,
            None,
        )
        .unwrap();
        assert_eq!(cloud.locality, Locality::Cloud);
        assert_eq!(cloud.max_privacy_grade, PrivacyGrade::Public);
    }
}
