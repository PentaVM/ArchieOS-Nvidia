//! `archie-voice` — a turn of conversation, typed instead of spoken, against the real model.
//!
//! ```text
//! archie-voice ask "<what the person said>" --grade public
//!              [--model nvidia/Nemotron-3_5-Lightning] [--budget full|sentence]
//!              [--language es-CL]
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
//! `live` is `converse` with a browser page for microphone, speaker and camera (loopback;
//! reach it through an SSH tunnel), IRIS for eyes, and curiosity: it looks closer at what
//! is in front, finds out what it is and reads about it, and keeps that until asked:
//!
//! ```text
//! archie-voice live --grade public --asr-model … --voice-engine magpie --tts-model …
//!              --codec-model … --tts-tokenizer … [--listen-language auto]
//!              [--tts-language es-US] [--tts-english en-US] [--port 8770]
//!              [--iris http://127.0.0.1:8765] [--curiosity 40] [--always] [--headphones]
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

use archie_model_runtime::{InferenceClient, ProviderEndpoint};
use archie_speech::{CognitiveEvent, CognitiveModelAdapter, CognitiveRequest, SpeechBudget};
use archie_types::{
    Availability, Capability, DataRetention, Locality, ModelDescriptor, ModelId, PrivacyGrade,
    ProviderKind, TokenCost,
};
use archie_voice::{tools, NebiusSource, NemotronCognitive};
use std::collections::BTreeMap;
use std::process::ExitCode;
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
    cognitive: NemotronCognitive<NebiusSource>,
    clock: String,
}

fn run(mut args: Vec<String>, offset: Option<time::UtcOffset>) -> Result<ExitCode, String> {
    const USAGE: &str =
        "usage: archie-voice ask|do \"<what was said>\" --grade public | converse --grade public …";
    let command = match args.first().map(String::as_str) {
        Some("ask") | Some("do") | Some("converse") | Some("live") => args.remove(0),
        _ => return Err(USAGE.into()),
    };
    if command == "live" {
        let speech = speech_flags(&mut args)?;
        let flags = live_flags(&mut args)?;
        let asking = asking(args, offset, false)?;
        return live(asking, speech, flags);
    }
    if command == "converse" {
        // Its own flags first; the stage is optional, since a conversation need not act.
        let speech = speech_flags(&mut args)?;
        let stage = if args.iter().any(|arg| arg == "--socket") {
            Some(stage_flags(&mut args)?)
        } else {
            None
        };
        let asking = asking(args, offset, false)?;
        return converse(asking, speech, stage);
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

fn asking(
    mut args: Vec<String>,
    offset: Option<time::UtcOffset>,
    words: bool,
) -> Result<Asking, String> {
    // A reasoning model on this machine (any OpenAI-compatible server on loopback, such as
    // llama-server) answers instead of Nebius when its address is given. Nothing said then
    // leaves the machine, and no key is needed. The environment names it too, so the
    // launcher that starts a conversation need not know.
    let set = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let local = take(&mut args, "--endpoint").or_else(|| set("ARCHIE_VOICE_ENDPOINT"));
    let model = take(&mut args, "--model")
        .or_else(|| set("ARCHIE_VOICE_MODEL"))
        .unwrap_or_else(|| "nvidia/Nemotron-3_5-Lightning".into());
    let language = take(&mut args, "--language").unwrap_or_else(|| "es-CL".into());
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
    let endpoint = match &local {
        None => ProviderEndpoint::nebius("https://api.tokenfactory.nebius.com/v1"),
        Some(url) => ProviderEndpoint::nvidia_nim(url, Locality::Local),
    }
    .map_err(|error| error.to_string())?;
    let client = InferenceClient::new(endpoint).map_err(|error| error.to_string())?;
    let source = NebiusSource::new(
        client,
        descriptor(&model, "voice", local.is_none(), grade)?,
        grade,
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
fn listen(cognitive: &mut NemotronCognitive<NebiusSource>, started: Instant) -> Step {
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
    let mut sight = Sight::new(operator, client, world, snapshot_dir, principal.clone());
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
        let descriptor = descriptor(model, "look", cloud, asking.grade)?;
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
        name_boost: take(args, "--name-boost")
            .map(|n| n.parse::<f32>().map_err(|_| "--name-boost is a number"))
            .transpose()?
            .unwrap_or(5.0),
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
    port: u16,
    iris: String,
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
        port: take(args, "--port")
            .map(|n| n.parse::<u16>().map_err(|_| "--port is a port number"))
            .transpose()?
            .unwrap_or(8770),
        iris: take(args, "--iris").unwrap_or_else(|| "http://127.0.0.1:8765".into()),
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
fn live(_asking: Asking, _speech: SpeechFlags, _flags: LiveFlags) -> Result<ExitCode, String> {
    Err(
        "this build has no live session: build archie-voice with --features live, where \
         NeMo-Speech.cpp is installed"
            .into(),
    )
}

/// Talks with a person through a browser page: their microphone and camera in, the voice
/// out, IRIS for eyes, and curiosity keeping what it found out until it is asked.
#[cfg(all(unix, feature = "live"))]
fn live(asking: Asking, speech: SpeechFlags, flags: LiveFlags) -> Result<ExitCode, String> {
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
    let senses = live::Senses::new(audio_in, identifier, researcher, rate, flags.curiosity);
    let eyes = eyes::IrisEyes::new(&flags.iris, 0.35)?;
    {
        let senses = senses.clone();
        let port = flags.port;
        asking.runtime.spawn(async move {
            if let Err(error) = live::serve(senses, eyes, port).await {
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
    cognitive.put_context_beside_utterance();
    let mut session =
        SpeechSession::new(plan, FloorPolicy::default(), cognitive).map_err(|e| e.to_string())?;
    let mut act = |intent: &archie_speech::SpeechToolIntentCandidate| -> String {
        runtime
            .block_on(senses.handle_tool(&intent.tool_name, &intent.raw_arguments))
            .unwrap_or_else(|| {
                format!(
                    "No se hizo: aquí no tengo la herramienta {}.",
                    intent.tool_name
                )
            })
    };
    let mut context = || senses.context(&clock);
    let started = Instant::now();
    let mut report = |moment: conversation::Moment| {
        if let conversation::Moment::Heard { text, .. } = &moment {
            senses.heard(text);
        }
        senses.publish(live::moment_json(&moment));
        let at = started.elapsed().as_millis();
        match &moment {
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
    };
    println!(
        "{}",
        if speech.always {
            "answering everything (--always)"
        } else {
            "answering when named, or within the follow-up window"
        }
    );
    conversation::converse(
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
) -> Result<ExitCode, String> {
    use archie_speech::{
        FloorPolicy, SessionPlan, SpeechCapabilities, SpeechSession, SpeechToolIntentCandidate,
    };
    use archie_voice::audio::{Microphone, Speaker};
    use archie_voice::conversation::{self, Hearing, Moment, Saying, Setup};
    use archie_voice::native::{Ears, Voice, VoiceFiles};
    use std::cell::RefCell;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let steps = stage.as_ref().map_or(6, |flags| flags.steps);
    let stage = match &stage {
        Some(flags) => Some(connect(&asking, flags)?),
        None => None,
    };
    let openable = stage
        .as_ref()
        .map(|(_, policy)| openable(policy))
        .unwrap_or_default();

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
        cognitive,
        clock,
        ..
    } = asking;
    let mut session =
        SpeechSession::new(plan, FloorPolicy::default(), cognitive).map_err(|e| e.to_string())?;

    let stage = RefCell::new(stage);
    let mut act = |intent: &SpeechToolIntentCandidate| -> String {
        let mut stage = stage.borrow_mut();
        let Some((hands, policy)) = stage.as_mut() else {
            return "No se hizo: en esta conversación no tengo el escritorio conectado.".into();
        };
        match archie_voice::decide(intent, policy, hands.front().as_deref()) {
            Ok(plan) => runtime.block_on(hands.perform(&plan)).report,
            Err(refusal) => format!("No se hizo: {}", refusal.spoken),
        }
    };
    let mut context = || {
        let front = stage.borrow().as_ref().and_then(|(hands, _)| hands.front());
        context_for(&clock, front, &openable)
    };
    let started = Instant::now();
    let mut report = |moment: Moment| {
        let at = started.elapsed().as_millis();
        match moment {
            Moment::Heard { text, .. } => println!("{at:>7} ms  oí       {text}"),
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
        phrases: vec!["Archie".into()],
        phrase_boost: speech.name_boost,
        notices: speech.notices.clone(),
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
    conversation::converse(
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

/// A model as the router knows it. In the cloud, with retention nobody has verified, it
/// may see what was graded public and nothing more, whatever the command line asked for.
fn descriptor(
    provider_model: &str,
    role: &str,
    cloud: bool,
    grade: PrivacyGrade,
) -> Result<ModelDescriptor, String> {
    let mut labels = BTreeMap::new();
    labels.insert("provider-model".to_string(), provider_model.to_string());
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
        provider: if cloud {
            ProviderKind::NebiusTokenFactory
        } else {
            ProviderKind::NvidiaNim
        },
        locality: if cloud {
            Locality::Cloud
        } else {
            Locality::Local
        },
        node: None,
        context_window: 32_000,
        // A grounding model that reasons before it answers needs room for one box.
        max_output_tokens: if role == "look" { 512 } else { 400 },
        capabilities: vec![Capability::known("compute.inference")],
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
