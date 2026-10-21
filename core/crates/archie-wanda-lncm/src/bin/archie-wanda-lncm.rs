//! Data, distillation, training and use of Wanda's LNCM. See `training/wanda_lncm/README.md`.

use archie_wanda_lncm::data::{self, Case, Example};
use archie_wanda_lncm::distill::{self, DistillConfig, Runner, Sample};
use archie_wanda_lncm::model::{ComposeOptions, Lncm, Manifest};
use archie_wanda_lncm::reader::{self, LncmInterpreter, ReaderConfig, Turn};
use archie_wanda_lncm::state::SocialState;
use archie_wanda_lncm::synth::{self, SynthConfig};
use archie_wanda_lncm::train::{self, TaskMix, TrainOptions};
use archie_wanda_lncm::{safety, Lang, LncmError, Result};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Wanda's LNCM: synthetic data, distillation, training and use")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate owned synthetic worlds: examples.jsonl (all tasks) and cases.jsonl (for teachers).
    Synth {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 200)]
        worlds: usize,
        #[arg(long, default_value_t = 20_261_020)]
        seed: u64,
        #[arg(long, default_value = "es,en")]
        langs: String,
        #[arg(long, default_value_t = 60)]
        min_turns: usize,
        #[arg(long, default_value_t = 240)]
        max_turns: usize,
    },
    /// Turn a Python `chat-v1` tranche into cases.
    ImportTranche {
        #[arg(long)]
        tranche: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Ask teachers, draft models and judges; select examples.
    Distill {
        #[command(subcommand)]
        step: DistillStep,
    },
    /// Create a checkpoint directory: train the codec and write the config.
    Init {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long, default_value = "small")]
        preset: String,
        /// 272 keeps the byte codec; more trains a BPE codec of that size.
        #[arg(long, default_value_t = 8192)]
        vocab: usize,
        #[arg(long, required = true)]
        examples: Vec<PathBuf>,
    },
    /// Train one stage.
    Train {
        #[command(subcommand)]
        stage: TrainStage,
    },
    /// Fit the decision temperature on development examples.
    Calibrate {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long, required = true)]
        examples: Vec<PathBuf>,
        #[arg(long, default_value = "cpu")]
        device: String,
    },
    /// Measure a checkpoint on development examples.
    Eval {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long, required = true)]
        examples: Vec<PathBuf>,
        #[arg(long)]
        adapter: Option<String>,
        #[arg(long, default_value_t = 200)]
        limit: usize,
        #[arg(long, default_value = "cpu")]
        device: String,
    },
    /// Ask a question: gate, state 1 and state 2.
    Ask {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long, default_value = "es")]
        lang: String,
        #[arg(long)]
        question: String,
        /// A rendered state, or a state JSON file written by `read`.
        #[arg(long)]
        state: Option<PathBuf>,
        #[arg(long)]
        draft: Option<String>,
        /// Comma-separated options for a decision.
        #[arg(long)]
        options: Option<String>,
        #[arg(long)]
        adapter: Option<String>,
        #[arg(long, default_value_t = 1)]
        candidates: usize,
        #[arg(long, default_value = "cpu")]
        device: String,
    },
    /// Read a conversation (turns JSONL) into a state with the LNCM as interpreter.
    Read {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        turns: PathBuf,
        /// State JSON; created when missing.
        #[arg(long)]
        state: PathBuf,
        #[arg(long, default_value = "owner")]
        owner: String,
        #[arg(long, default_value = "es")]
        lang: String,
        #[arg(long, default_value_t = 0.5)]
        threshold: f32,
        #[arg(long, default_value = "cpu")]
        device: String,
    },
    /// Print a checkpoint's manifest and size.
    Inspect {
        #[arg(long)]
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum DistillStep {
    Teach {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        cases: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Draft {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        cases: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Judge {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        cases: PathBuf,
        #[arg(long)]
        samples: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Select {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        cases: PathBuf,
        /// Teacher, draft and judge sample files.
        #[arg(long, required = true)]
        samples: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Args, Clone)]
struct Common {
    #[arg(long)]
    dir: PathBuf,
    #[arg(long, required = true)]
    examples: Vec<PathBuf>,
    #[arg(long, default_value = "cpu")]
    device: String,
    #[arg(long, default_value_t = 20_000)]
    steps: u64,
    #[arg(long, default_value_t = 16)]
    batch: usize,
    /// Micro-batches of --batch rows per optimiser step, gradients summed (generator and
    /// adapter stages): an effective batch of batch × accumulate in less memory.
    #[arg(long, default_value_t = 1)]
    accumulate: usize,
    #[arg(long, default_value_t = 3e-4)]
    lr: f64,
    #[arg(long, default_value_t = 500)]
    warmup: u64,
    #[arg(long, default_value_t = 1000)]
    save_every: u64,
    #[arg(long, default_value_t = 1000)]
    eval_every: u64,
    #[arg(long, default_value_t = 200)]
    eval_limit: usize,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Start from scratch even if the stage has a save.
    #[arg(long)]
    no_resume: bool,
    /// Task mix for the generator and adapters: gate=0.1,update=0.3,decide=0.3,compose=0.3
    #[arg(long)]
    mix: Option<String>,
    /// Pause when the GPU reaches this temperature (°C), read with nvidia-smi.
    #[arg(long)]
    max_gpu_temp: Option<u32>,
    /// Resume when it is back at or below this temperature.
    #[arg(long, default_value_t = 72)]
    resume_gpu_temp: u32,
}

#[derive(Subcommand)]
enum TrainStage {
    Tokenizer(Common),
    Generator(Common),
    Adapter {
        #[command(flatten)]
        common: Common,
        #[arg(long)]
        name: String,
    },
}

fn lang(text: &str) -> Result<Lang> {
    match text {
        "es" => Ok(Lang::Es),
        "en" => Ok(Lang::En),
        _ => Err(LncmError::Config("lang must be es or en".into())),
    }
}

fn load_examples(paths: &[PathBuf]) -> Result<Vec<Example>> {
    let mut out = Vec::new();
    for p in paths {
        out.extend(data::load_examples(p)?);
    }
    let mut ids = std::collections::HashSet::new();
    if let Some(dup) = out.iter().find(|e| !ids.insert(e.id.clone())) {
        return Err(LncmError::Data(format!(
            "example {} appears in more than one file",
            dup.id
        )));
    }
    Ok(out)
}

fn options(c: &Common) -> TrainOptions {
    TrainOptions {
        steps: c.steps,
        batch: c.batch,
        accumulate: c.accumulate.max(1),
        lr: c.lr,
        warmup: c.warmup,
        save_every: c.save_every.max(1),
        eval_every: c.eval_every,
        eval_limit: c.eval_limit,
        seed: c.seed,
        resume: !c.no_resume,
        thermal: c.max_gpu_temp.map(|hot| train::Thermal {
            hot_c: hot,
            cool_c: c.resume_gpu_temp.min(hot.saturating_sub(1)),
            every: 20,
        }),
        ..Default::default()
    }
}

fn mix(text: Option<&str>) -> Result<TaskMix> {
    let mut m = TaskMix::default();
    let Some(text) = text else { return Ok(m) };
    for part in text.split(',') {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| LncmError::Config("mix is task=weight,…".into()))?;
        let v: f32 = v
            .parse()
            .map_err(|_| LncmError::Config("mix weight".into()))?;
        match k.trim() {
            "gate" => m.gate = v,
            "update" => m.update = v,
            "decide" => m.decide = v,
            "compose" => m.compose = v,
            _ => return Err(LncmError::Config(format!("unknown task {k}"))),
        }
    }
    Ok(m)
}

fn print<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn read_state_text(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path)?;
    // A state file written by `read` is rendered; anything else is taken as already rendered.
    match SocialState::from_json(&text) {
        Ok(state) => Ok(state.render(&[], 2048, Lang::Es)),
        Err(_) => Ok(text),
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run(Cli::parse()).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Synth {
            out,
            worlds,
            seed,
            langs,
            min_turns,
            max_turns,
        } => {
            let langs = langs
                .split(',')
                .map(|l| lang(l.trim()))
                .collect::<Result<Vec<_>>>()?;
            let config = SynthConfig {
                seed,
                worlds,
                langs,
                min_turns,
                max_turns: max_turns.max(min_turns),
                ..Default::default()
            };
            std::fs::create_dir_all(&out)?;
            let output = synth::generate(&config)?;
            data::write_jsonl(&out.join("examples.jsonl"), &output.examples)?;
            data::write_jsonl(&out.join("cases.jsonl"), &output.cases)?;
            print(
                &serde_json::json!({"examples": output.examples.len(), "cases": output.cases.len(), "out": out}),
            )
        }
        Command::ImportTranche { tranche, out } => {
            let cases = data::import_tranche(&tranche)?;
            data::write_jsonl(&out, &cases)?;
            print(&serde_json::json!({"cases": cases.len()}))
        }
        Command::Distill { step } => match step {
            DistillStep::Teach { config, cases, out } => {
                let runner = Runner::new(DistillConfig::load(&config)?)?;
                print(&runner.teach(&data::load_cases(&cases)?, &out).await?)
            }
            DistillStep::Draft { config, cases, out } => {
                let runner = Runner::new(DistillConfig::load(&config)?)?;
                print(&runner.draft(&data::load_cases(&cases)?, &out).await?)
            }
            DistillStep::Judge {
                config,
                cases,
                samples,
                out,
            } => {
                let runner = Runner::new(DistillConfig::load(&config)?)?;
                print(
                    &runner
                        .judge(&data::load_cases(&cases)?, &samples, &out)
                        .await?,
                )
            }
            DistillStep::Select {
                config,
                cases,
                samples,
                out,
            } => {
                let config = DistillConfig::load(&config)?;
                let cases: Vec<Case> = data::load_cases(&cases)?;
                let mut all: Vec<Sample> = Vec::new();
                for p in &samples {
                    all.extend(data::read_jsonl::<Sample>(p, 50_000_000)?);
                }
                let (examples, report) = distill::select(&config, &cases, &all);
                data::write_jsonl(&out, &examples)?;
                print(&report)
            }
        },
        Command::Init {
            dir,
            preset,
            vocab,
            examples,
        } => {
            let config = train::init(&dir, &preset, vocab, &load_examples(&examples)?)?;
            print(
                &serde_json::json!({"dir": dir, "vocab_size": config.vocab_size, "parameters": config.parameters()}),
            )
        }
        Command::Train { stage } => {
            let (common, name) = match &stage {
                TrainStage::Tokenizer(c) | TrainStage::Generator(c) => (c.clone(), None),
                TrainStage::Adapter { common, name } => (common.clone(), Some(name.clone())),
            };
            let device = train::device(&common.device)?;
            let examples = load_examples(&common.examples)?;
            let opts = options(&common);
            match stage {
                TrainStage::Tokenizer(_) => print(&train::train_tokenizer(
                    &common.dir,
                    &examples,
                    &common.examples,
                    &opts,
                    &device,
                )?),
                TrainStage::Generator(_) => print(&train::train_generator(
                    &common.dir,
                    &examples,
                    &common.examples,
                    &opts,
                    &mix(common.mix.as_deref())?,
                    &device,
                )?),
                TrainStage::Adapter { .. } => print(&train::train_adapter(
                    &common.dir,
                    name.as_deref().unwrap_or_default(),
                    &examples,
                    &common.examples,
                    &opts,
                    &mix(common.mix.as_deref())?,
                    &device,
                )?),
            }
        }
        Command::Calibrate {
            dir,
            examples,
            device,
        } => {
            let t = train::calibrate(&dir, &load_examples(&examples)?, &train::device(&device)?)?;
            print(&serde_json::json!({"decision_temperature": t}))
        }
        Command::Eval {
            dir,
            examples,
            adapter,
            limit,
            device,
        } => {
            let mut model = Lncm::load(&dir, &train::device(&device)?)?;
            if let Some(a) = adapter {
                model = model.with_adapter(&a)?;
            }
            print(&train::evaluate(&model, &load_examples(&examples)?, limit)?)
        }
        Command::Ask {
            dir,
            lang: l,
            question,
            state,
            draft,
            options,
            adapter,
            candidates,
            device,
        } => {
            let lang = lang(&l)?;
            if safety::crisis(&question).is_some() {
                return print(
                    &serde_json::json!({"crisis": true, "answer": safety::crisis_message(lang)}),
                );
            }
            let mut model = Lncm::load(&dir, &train::device(&device)?)?;
            if let Some(a) = adapter {
                model = model.with_adapter(&a)?;
            }
            let state = state.as_deref().map(read_state_text).transpose()?;
            let gate = model.gate(lang, &question)?;
            let decision = match &options {
                Some(o) => {
                    let options: Vec<String> = o
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    Some(serde_json::to_value(model.decide(
                        lang,
                        &question,
                        state.as_deref(),
                        draft.as_deref(),
                        &options,
                    )?)?)
                }
                None => model
                    .quick(lang, &question, state.as_deref(), draft.as_deref())?
                    .map(serde_json::to_value)
                    .transpose()?,
            };
            let composed = model.compose(
                lang,
                &question,
                state.as_deref(),
                draft.as_deref(),
                &[],
                ComposeOptions {
                    candidates,
                    sampling: archie_wanda_lncm::generator::Sampling {
                        temperature: 0.8,
                        top_k: 32,
                    },
                    seed: 1,
                },
            )?;
            print(&serde_json::json!({"gate": gate, "state_1": decision, "state_2": composed}))
        }
        Command::Read {
            dir,
            turns,
            state,
            owner,
            lang: l,
            threshold,
            device,
        } => {
            let lang = lang(&l)?;
            let model = Lncm::load(&dir, &train::device(&device)?)?;
            let turns: Vec<Turn> = data::read_jsonl(&turns, 10_000_000)?;
            let mut social = if state.exists() {
                SocialState::from_json(&std::fs::read_to_string(&state)?)?
            } else {
                SocialState::new(&owner, &owner)?
            };
            let interpreter = LncmInterpreter {
                model: &model,
                lang,
                threshold,
            };
            let report = reader::read(
                &mut social,
                &turns,
                &interpreter,
                &ReaderConfig {
                    lang,
                    ..Default::default()
                },
            )?;
            archie_wanda_lncm::model::write_atomic(
                &state,
                serde_json::to_string_pretty(&social)?.as_bytes(),
            )?;
            print(&report)
        }
        Command::Inspect { dir } => {
            let manifest = Manifest::load(&dir)?;
            let config = archie_wanda_lncm::LncmConfig::from_toml(&std::fs::read_to_string(
                dir.join("config.toml"),
            )?)?;
            print(
                &serde_json::json!({"parameters": config.parameters(), "config": config, "manifest": manifest}),
            )
        }
    }
}
