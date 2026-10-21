//! Training, in stages, into one checkpoint directory.
//!
//! | stage       | trains                          | frozen                 | writes                     |
//! |-------------|---------------------------------|------------------------|----------------------------|
//! | `init`      | the BPE codec (counting)        | —                      | `codec.json`, `config.toml`|
//! | `tokenizer` | the multiscale VQ-VAE           | codec                  | `vq.safetensors`           |
//! | `generator` | generator and the state-1 heads | tokenizer              | `generator.safetensors`    |
//! | `adapter`   | one domain's low-rank adapters  | tokenizer, generator   | `lora/<name>.safetensors`  |
//! | `calibrate` | the decision temperature        | everything             | `config.toml`              |
//!
//! Every stage records the SHA-256 of what it wrote and of the data it read in the manifest,
//! appends its metrics to `train-log.jsonl`, and can resume from its last save (weights and
//! step; the optimiser's moments restart, which a short warm-up absorbs).
//!
//! Nothing here claims a model is good: [`evaluate`] measures it on development examples, on
//! the device it runs on, and its numbers go into the manifest as measured.

use crate::codec::{Codec, Prompt, PromptParts};
use crate::config::LncmConfig;
use crate::data::{Choice, Example, Split};
use crate::generator::{Generator, PromptBatch};
use crate::model::{
    file_sha256, load_tensors, write_atomic, AdapterRecord, Lncm, Manifest, StageRecord,
    CODEC_FILE, CONFIG_FILE, GENERATOR_FILE, VQ_FILE,
};
use crate::nn::{self, Ctx};
use crate::vq::{codebook_name, VqVae};
use crate::{LncmError, Result, Task};
use candle_core::backprop::GradStore;
use candle_core::{DType, Device, Tensor, Var, D};
use candle_nn::{AdamW, Optimizer, ParamsAdamW, VarBuilder, VarMap};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const LOG_FILE: &str = "train-log.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainOptions {
    pub steps: u64,
    pub batch: usize,
    /// Micro-batches of `batch` rows per optimiser step in the generator and adapter stages,
    /// gradients summed: an effective batch of `batch × accumulate` on a card too small for it.
    #[serde(default = "one")]
    pub accumulate: usize,
    pub lr: f64,
    pub warmup: u64,
    pub weight_decay: f64,
    /// Global gradient-norm clip.
    pub clip: f64,
    pub eval_every: u64,
    pub save_every: u64,
    pub seed: u64,
    /// Continue from the stage's last save.
    pub resume: bool,
    /// Development examples per task used by periodic evaluation.
    pub eval_limit: usize,
    /// Pause on a hot GPU (read with `nvidia-smi`) until it cools.
    #[serde(default)]
    pub thermal: Option<Thermal>,
}

/// A consumer card that overheats is paused between steps rather than throttled or lost.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Thermal {
    /// Pause at this temperature (°C) or above.
    pub hot_c: u32,
    /// Resume at this temperature or below.
    pub cool_c: u32,
    /// Check every this many steps.
    pub every: u64,
}

/// The hottest GPU's temperature, if `nvidia-smi` answers.
pub fn gpu_temperature() -> Option<u32> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .max()
}

/// The file whose presence stops a stage cleanly at its next step (after a save).
pub const STOP_FILE: &str = "STOP";

/// Waits while the GPU is hot. Returns `true` when a stop was requested.
fn pause_or_stop(dir: &Path, opts: &TrainOptions, step_no: u64) -> Result<bool> {
    if dir.join(STOP_FILE).exists() {
        return Ok(true);
    }
    let Some(t) = opts.thermal else {
        return Ok(false);
    };
    if t.every == 0 || step_no % t.every != 0 {
        return Ok(false);
    }
    let Some(mut temperature) = gpu_temperature() else {
        return Ok(false);
    };
    if temperature < t.hot_c {
        return Ok(false);
    }
    log(
        dir,
        serde_json::json!({"thermal": "pause", "step": step_no, "celsius": temperature}),
    )?;
    let paused = Instant::now();
    while temperature > t.cool_c {
        std::thread::sleep(std::time::Duration::from_secs(15));
        if dir.join(STOP_FILE).exists() {
            return Ok(true);
        }
        temperature = gpu_temperature().unwrap_or(0);
    }
    log(
        dir,
        serde_json::json!({"thermal": "resume", "step": step_no, "celsius": temperature, "paused_s": paused.elapsed().as_secs()}),
    )?;
    Ok(false)
}

fn one() -> usize {
    1
}

fn stopped(stage: &str, step_no: u64) -> LncmError {
    LncmError::Data(format!("{stage} stopped at step {step_no} by the {STOP_FILE} file; it was saved, remove the file and run the same command to resume"))
}

impl Default for TrainOptions {
    fn default() -> Self {
        Self {
            steps: 20_000,
            batch: 16,
            accumulate: 1,
            lr: 3e-4,
            warmup: 500,
            weight_decay: 0.01,
            clip: 1.0,
            eval_every: 1000,
            save_every: 1000,
            seed: 1,
            resume: true,
            eval_limit: 200,
            thermal: None,
        }
    }
}

pub fn device(spec: &str) -> Result<Device> {
    match spec {
        "cpu" => Ok(Device::Cpu),
        s if s.starts_with("cuda") => {
            let ordinal = s.strip_prefix("cuda:").unwrap_or("0").parse().unwrap_or(0);
            Ok(Device::new_cuda(ordinal)?)
        }
        _ => Err(LncmError::Config(format!(
            "unknown device {spec}; use cpu or cuda:N"
        ))),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Every text an example holds, for training the codec.
fn example_texts(e: &Example) -> Vec<&str> {
    let mut out: Vec<&str> = [
        &e.question,
        &e.state,
        &e.chunk,
        &e.draft,
        &e.target.plan,
        &e.target.text,
    ]
    .into_iter()
    .filter_map(|t| t.as_deref())
    .collect();
    out.extend(e.options.iter().map(String::as_str));
    out
}

/// Creates a checkpoint directory: trains the codec over the examples' text and writes the
/// config of the chosen preset.
pub fn init(
    dir: &Path,
    preset: &str,
    vocab_size: usize,
    examples: &[Example],
) -> Result<LncmConfig> {
    std::fs::create_dir_all(dir)?;
    if dir.join(CONFIG_FILE).exists() {
        return Err(LncmError::Checkpoint(format!(
            "{} already holds a checkpoint",
            dir.display()
        )));
    }
    let codec = if vocab_size <= (crate::codec::BYTE_TOKENS + crate::codec::SPECIAL_COUNT) as usize
    {
        Codec::bytes()
    } else {
        Codec::train_bpe(examples.iter().flat_map(example_texts), vocab_size, 2)?
    };
    let config = LncmConfig::preset(preset, codec.vocab_size())?;
    config.validate()?;
    codec.save(&dir.join(CODEC_FILE))?;
    write_atomic(&dir.join(CONFIG_FILE), config.to_toml()?.as_bytes())?;
    let mut manifest = Manifest::new();
    manifest.record(dir, CODEC_FILE)?;
    manifest.record(dir, CONFIG_FILE)?;
    manifest.save(dir)?;
    Ok(config)
}

fn open_config(dir: &Path) -> Result<(LncmConfig, Codec, Manifest)> {
    let manifest = Manifest::load(dir)?;
    manifest.verify(dir, CONFIG_FILE)?;
    manifest.verify(dir, CODEC_FILE)?;
    let config = LncmConfig::from_toml(&crate::codec::read_bounded(
        &dir.join(CONFIG_FILE),
        1 << 20,
    )?)?;
    let codec = Codec::load(&dir.join(CODEC_FILE))?;
    if codec.vocab_size() != config.vocab_size {
        return Err(LncmError::Checkpoint("codec and config disagree".into()));
    }
    Ok((config, codec, manifest))
}

/// An example turned into ids.
struct Prepared {
    task: Task,
    prompt: Prompt,
    gate: Option<f32>,
    yes_no: Option<u32>,
    change: Option<f32>,
    choice: Option<u32>,
    target: Option<Vec<u32>>,
    weight: f32,
}

fn prepare(codec: &Codec, config: &LncmConfig, e: &Example) -> Result<Prepared> {
    if e.options.len() > config.max_options {
        return Err(LncmError::Data(format!(
            "{}: more options than the model has",
            e.id
        )));
    }
    let parts = PromptParts {
        task: e.task,
        lang: e.lang,
        question: e.question.as_deref(),
        options: &e.options,
        state: e.state.as_deref(),
        chunk: e.chunk.as_deref(),
        draft: e.draft.as_deref(),
    };
    let prompt = codec.build_prompt(&parts, config.prompt_len, config.option_len)?;
    let t = &e.target;
    let target = match e.task {
        Task::Compose | Task::Update => match &t.text {
            Some(text) => Some(codec.encode_target(t.plan.as_deref(), text, config.target_len)?),
            None => None,
        },
        _ => None,
    };
    Ok(Prepared {
        task: e.task,
        prompt,
        gate: t.gate.map(|g| if g { 1.0 } else { 0.0 }),
        yes_no: t.yes_no.map(u32::from),
        change: t.change.map(|c| if c { 1.0 } else { 0.0 }),
        choice: t.choice.map(|c| match c {
            Choice::Option(i) => i as u32,
            Choice::Abstain => config.max_options as u32,
        }),
        target,
        weight: e.weight,
    })
}

fn prepare_all(
    codec: &Codec,
    config: &LncmConfig,
    examples: &[Example],
    split: Split,
) -> (Vec<Prepared>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0;
    for e in examples.iter().filter(|e| e.split == split) {
        match prepare(codec, config, e) {
            Ok(p) => out.push(p),
            Err(_) => skipped += 1,
        }
    }
    (out, skipped)
}

fn learning_rate(opts: &TrainOptions, step: u64) -> f64 {
    if step < opts.warmup {
        return opts.lr * (step + 1) as f64 / opts.warmup.max(1) as f64;
    }
    let progress =
        (step - opts.warmup) as f64 / (opts.steps.saturating_sub(opts.warmup)).max(1) as f64;
    let cosine = 0.5 * (1.0 + (std::f64::consts::PI * progress.min(1.0)).cos());
    opts.lr * (0.1 + 0.9 * cosine)
}

fn optimiser(vars: Vec<Var>, opts: &TrainOptions) -> Result<AdamW> {
    Ok(AdamW::new(
        vars,
        ParamsAdamW {
            lr: opts.lr,
            beta1: 0.9,
            beta2: 0.95,
            eps: 1e-8,
            weight_decay: opts.weight_decay,
        },
    )?)
}

/// Backward, clip to the global norm, step. Returns the norm before clipping.
fn step(opt: &mut AdamW, vars: &[Var], loss: &Tensor, clip: f64) -> Result<f32> {
    apply_gradients(opt, vars, loss.backward()?, clip)
}

/// Adds `part` into `sum`, variable by variable.
fn add_gradients(mut sum: GradStore, part: &GradStore, vars: &[Var]) -> Result<GradStore> {
    for v in vars {
        let Some(g) = part.get(v.as_tensor()) else {
            continue;
        };
        let added = match sum.get(v.as_tensor()) {
            Some(s) => (s + g)?,
            None => g.clone(),
        };
        sum.insert(v.as_tensor(), added);
    }
    Ok(sum)
}

/// Clips `grads` to a global norm of `clip` and takes one optimiser step. Returns the norm
/// before clipping.
fn apply_gradients(opt: &mut AdamW, vars: &[Var], mut grads: GradStore, clip: f64) -> Result<f32> {
    let mut total = 0f32;
    for v in vars {
        if let Some(g) = grads.get(v.as_tensor()) {
            total += nn::scalar(&g.sqr()?.sum_all()?)?;
        }
    }
    let norm = total.sqrt();
    if norm.is_finite() && f64::from(norm) > clip && clip > 0.0 {
        let scale = clip / f64::from(norm);
        for v in vars {
            if let Some(g) = grads.get(v.as_tensor()) {
                let scaled = (g * scale)?;
                grads.insert(v.as_tensor(), scaled);
            }
        }
    }
    if !norm.is_finite() {
        return Err(LncmError::Data(
            "the gradient is not finite; lower the learning rate".into(),
        ));
    }
    opt.step(&grads)?;
    Ok(norm)
}

fn log(dir: &Path, record: serde_json::Value) -> Result<()> {
    crate::data::append_jsonl(&dir.join(LOG_FILE), &record)
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Progress {
    step: u64,
}

fn progress_path(dir: &Path, stage: &str) -> PathBuf {
    dir.join(format!("{stage}.progress.json"))
}

fn load_progress(dir: &Path, stage: &str) -> u64 {
    std::fs::read_to_string(progress_path(dir, stage))
        .ok()
        .and_then(|t| serde_json::from_str::<Progress>(&t).ok())
        .map_or(0, |p| p.step)
}

fn save_varmap(dir: &Path, map: &VarMap, file: &str, stage: &str, step: u64) -> Result<()> {
    let tmp = dir.join(format!("{file}.tmp"));
    map.save(&tmp)?;
    std::fs::rename(&tmp, dir.join(file))?;
    write_atomic(
        &progress_path(dir, stage),
        serde_json::to_string(&Progress { step })?.as_bytes(),
    )
}

fn data_digests(paths: &[PathBuf]) -> Result<Vec<String>> {
    paths.iter().map(|p| file_sha256(p)).collect()
}

fn finish_stage(
    dir: &Path,
    file: &str,
    stage: &str,
    steps: u64,
    data: &[PathBuf],
    metrics: BTreeMap<String, f64>,
) -> Result<()> {
    let mut manifest = Manifest::load(dir)?;
    manifest.record(dir, file)?;
    manifest.stages.retain(|s| s.stage != stage);
    manifest.stages.push(StageRecord {
        stage: stage.to_string(),
        steps,
        data_sha256: data_digests(data)?,
        metrics,
        finished_at_ms: now_ms(),
    });
    manifest.save(dir)
}

// ---------------------------------------------------------------------------------------------
// Tokenizer

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TokenizerMetrics {
    pub token_accuracy: f64,
    pub exact_sequences: f64,
    pub char_f1: f64,
    pub utilisation: Vec<f64>,
}

fn generation_targets(prepared: &[Prepared]) -> Vec<Vec<u32>> {
    prepared.iter().filter_map(|p| p.target.clone()).collect()
}

pub fn train_tokenizer(
    dir: &Path,
    examples: &[Example],
    data: &[PathBuf],
    opts: &TrainOptions,
    device: &Device,
) -> Result<TokenizerMetrics> {
    let (config, codec, _) = open_config(dir)?;
    let (train, _) = prepare_all(&codec, &config, examples, Split::Train);
    let (dev, _) = prepare_all(&codec, &config, examples, Split::Dev);
    let train = generation_targets(&train);
    let dev = generation_targets(&dev);
    if train.is_empty() {
        return Err(LncmError::Data(
            "no generation targets to train the tokenizer on".into(),
        ));
    }
    let mut map = VarMap::new();
    let vq = VqVae::new(&VarBuilder::from_varmap(&map, DType::F32, device), &config)?;
    let mut start = 0;
    if opts.resume && dir.join(VQ_FILE).exists() {
        map.load(dir.join(VQ_FILE))?;
        start = load_progress(dir, "tokenizer");
    }
    let vars = map.all_vars();
    let mut opt = optimiser(vars.clone(), opts)?;
    let mut rng = StdRng::seed_from_u64(opts.seed ^ start);
    let levels = config.levels.len();
    let mut usage: Vec<Vec<u32>> = vec![vec![0; config.codebook_size]; levels];
    let started = Instant::now();
    for step_no in start..opts.steps {
        if pause_or_stop(dir, opts, step_no)? {
            save_varmap(dir, &map, VQ_FILE, "tokenizer", step_no)?;
            return Err(stopped("tokenizer", step_no));
        }
        opt.set_learning_rate(learning_rate(opts, step_no));
        let rows: Vec<Vec<u32>> = (0..opts.batch)
            .map(|_| train.choose(&mut rng).expect("nonempty").clone())
            .collect();
        let ids = nn::ids_tensor(&rows, device)?;
        let keep = if rng.gen::<f32>() < config.vq.nested_dropout {
            rng.gen_range(1..=levels)
        } else {
            levels
        };
        let loss = vq.loss(&ids, keep, Ctx::train(config.dropout))?;
        let norm = step(&mut opt, &vars, &loss.total, opts.clip)?;
        for (k, codes) in loss.codes.iter().enumerate() {
            for c in codes.flatten_all()?.to_vec1::<u32>()? {
                usage[k][c as usize] = usage[k][c as usize].saturating_add(1);
            }
        }
        if (step_no + 1) % 100 == 0 {
            restart_dead_codes(&map, &loss.residuals, &mut usage, &mut rng)?;
        }
        if (step_no + 1) % 50 == 0 || step_no + 1 == opts.steps {
            log(
                dir,
                serde_json::json!({"stage": "tokenizer", "step": step_no + 1, "reconstruction": loss.reconstruction,
                    "commitment": loss.commitment, "codebook": loss.codebook, "accuracy": loss.accuracy,
                    "grad_norm": norm, "seconds": started.elapsed().as_secs_f32()}),
            )?;
        }
        if (step_no + 1) % opts.save_every == 0 {
            save_varmap(dir, &map, VQ_FILE, "tokenizer", step_no + 1)?;
        }
    }
    save_varmap(dir, &map, VQ_FILE, "tokenizer", opts.steps)?;
    let metrics = measure_tokenizer(
        &vq,
        &codec,
        &config,
        if dev.is_empty() { &train } else { &dev },
        opts.eval_limit,
        device,
    )?;
    let mut record = BTreeMap::new();
    record.insert("token_accuracy".into(), metrics.token_accuracy);
    record.insert("exact_sequences".into(), metrics.exact_sequences);
    record.insert("char_f1".into(), metrics.char_f1);
    for (k, u) in metrics.utilisation.iter().enumerate() {
        record.insert(format!("utilisation_{k}"), *u);
    }
    finish_stage(dir, VQ_FILE, "tokenizer", opts.steps, data, record)?;
    Ok(metrics)
}

/// Codes nothing chose since the last restart are moved onto residuals that were quantised
/// recently, so the whole codebook stays in use.
fn restart_dead_codes(
    map: &VarMap,
    residuals: &[Tensor],
    usage: &mut [Vec<u32>],
    rng: &mut StdRng,
) -> Result<()> {
    let data = map
        .data()
        .lock()
        .map_err(|_| LncmError::Checkpoint("variable map poisoned".into()))?;
    for (k, counts) in usage.iter_mut().enumerate() {
        let Some(var) = data.get(&codebook_name(k)) else {
            continue;
        };
        let dead: Vec<usize> = counts
            .iter()
            .enumerate()
            .filter(|(_, &n)| n == 0)
            .map(|(i, _)| i)
            .collect();
        if !dead.is_empty() {
            let pool = residuals[k].to_dtype(DType::F32)?.to_vec2::<f32>()?;
            let mut rows = var.as_tensor().to_dtype(DType::F32)?.to_vec2::<f32>()?;
            for i in dead {
                let source = pool.choose(rng).expect("residuals of a nonempty batch");
                rows[i] = source
                    .iter()
                    .map(|v| v + rng.gen_range(-1e-3..1e-3))
                    .collect();
            }
            let (n, c) = (rows.len(), rows[0].len());
            let flat: Vec<f32> = rows.into_iter().flatten().collect();
            var.set(&Tensor::from_vec(flat, (n, c), var.device())?)?;
        }
        counts.iter_mut().for_each(|n| *n = 0);
    }
    Ok(())
}

fn measure_tokenizer(
    vq: &VqVae,
    codec: &Codec,
    config: &LncmConfig,
    targets: &[Vec<u32>],
    limit: usize,
    device: &Device,
) -> Result<TokenizerMetrics> {
    let mut correct = 0usize;
    let mut total = 0usize;
    let mut exact = 0usize;
    let mut f1 = 0f64;
    let mut used: Vec<std::collections::HashSet<u32>> =
        vec![Default::default(); config.levels.len()];
    let sample: Vec<&Vec<u32>> = targets.iter().take(limit.max(1)).collect();
    for row in &sample {
        let ids = nn::ids_tensor(&[(*row).clone()], device)?;
        let z = vq.encode(&ids, Ctx::EVAL)?;
        let q = vq.quantize(&z, config.levels.len())?;
        for (k, codes) in q.codes.iter().enumerate() {
            used[k].extend(codes.flatten_all()?.to_vec1::<u32>()?);
        }
        let out = vq.tokens(&q.full)?;
        correct += out.iter().zip(row.iter()).filter(|(a, b)| a == b).count();
        total += row.len();
        exact += usize::from(out == **row);
        f1 += f64::from(crate::model::char_ngram_f1(
            &codec.decode_target(&out).1,
            &codec.decode_target(row).1,
        ));
    }
    let n = sample.len().max(1) as f64;
    Ok(TokenizerMetrics {
        token_accuracy: correct as f64 / total.max(1) as f64,
        exact_sequences: exact as f64 / n,
        char_f1: f1 / n,
        utilisation: used
            .iter()
            .map(|u| u.len() as f64 / config.codebook_size as f64)
            .collect(),
    })
}

// ---------------------------------------------------------------------------------------------
// Generator and heads

/// How often each task is drawn while training the generator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMix {
    pub gate: f32,
    pub update: f32,
    pub decide: f32,
    pub compose: f32,
}

impl Default for TaskMix {
    fn default() -> Self {
        Self {
            gate: 0.1,
            update: 0.3,
            decide: 0.3,
            compose: 0.3,
        }
    }
}

impl TaskMix {
    fn weight(&self, task: Task) -> f32 {
        match task {
            Task::Gate => self.gate,
            Task::Update => self.update,
            Task::Decide => self.decide,
            Task::Compose => self.compose,
        }
    }
}

fn bce_with_logits(x: &Tensor, y: &Tensor) -> Result<Tensor> {
    // max(x, 0) - x·y + log(1 + e^-|x|), stable for any x.
    let softplus = ((x.abs()?.neg()?.exp()? + 1.0)?).log()?;
    Ok(((x.relu()? - (x * y)?)? + softplus)?)
}

fn nll_rows(logits: &Tensor, targets: &Tensor) -> Result<Tensor> {
    let logp = candle_nn::ops::log_softmax(logits, D::Minus1)?;
    Ok(logp
        .gather(&targets.unsqueeze(D::Minus1)?, D::Minus1)?
        .squeeze(D::Minus1)?
        .neg()?)
}

fn weighted_mean(per_row: &Tensor, weights: &[f32], device: &Device) -> Result<Tensor> {
    let total: f32 = weights.iter().sum::<f32>().max(1e-6);
    let w = Tensor::from_vec(weights.to_vec(), weights.len(), device)?;
    Ok(((per_row * w)?.sum_all()? / f64::from(total))?)
}

struct Models<'a> {
    vq: &'a VqVae,
    generator: &'a Generator,
    config: &'a LncmConfig,
    device: &'a Device,
}

impl Models<'_> {
    fn batch(&self, rows: &[&Prepared], pad: u32) -> Result<PromptBatch> {
        let ids: Vec<Vec<u32>> = rows.iter().map(|p| p.prompt.ids.clone()).collect();
        let spans: Vec<Vec<(usize, usize)>> =
            rows.iter().map(|p| p.prompt.option_spans.clone()).collect();
        Ok(PromptBatch::new(&ids, spans, pad, self.device)?)
    }

    /// The loss of one homogeneous batch.
    fn loss(
        &self,
        task: Task,
        rows: &[&Prepared],
        pad: u32,
        ctx: Ctx,
        rng: &mut StdRng,
    ) -> Result<Tensor> {
        let batch = self.batch(rows, pad)?;
        let weights: Vec<f32> = rows.iter().map(|p| p.weight).collect();
        let levels = if task.generates() {
            let targets: Vec<Vec<u32>> = rows
                .iter()
                .map(|p| p.target.clone().unwrap_or_default())
                .collect();
            if targets.iter().any(Vec::is_empty) {
                return Err(LncmError::Data("a generating example has no target".into()));
            }
            let codes = self.vq.codes_for(&nn::ids_tensor(&targets, self.device)?)?;
            let corrupted = corrupt(
                &codes,
                &self.config.corruption,
                self.config.codebook_size,
                rng,
            )?;
            Some((codes, self.generator.level_inputs(self.vq, &corrupted)?))
        } else {
            None
        };
        let hidden = self.generator.forward(
            self.vq,
            &batch,
            levels.as_ref().map(|(_, i)| i.as_slice()),
            ctx,
        )?;
        let heads = self
            .generator
            .heads(&hidden.prompt, &batch.lengths, &batch.option_spans)?;
        let mut loss = Tensor::zeros((), DType::F32, self.device)?;
        match task {
            Task::Gate => {
                let y: Vec<f32> = rows.iter().map(|p| p.gate.unwrap_or(0.0)).collect();
                let y = Tensor::from_vec(y, rows.len(), self.device)?;
                loss = (loss
                    + weighted_mean(&bce_with_logits(&heads.gate, &y)?, &weights, self.device)?)?;
                let labelled: Vec<(usize, u32)> = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| p.yes_no.map(|y| (i, y)))
                    .collect();
                if !labelled.is_empty() {
                    let idx = Tensor::from_vec(
                        labelled.iter().map(|(i, _)| *i as u32).collect::<Vec<_>>(),
                        labelled.len(),
                        self.device,
                    )?;
                    let y = Tensor::from_vec(
                        labelled.iter().map(|(_, y)| *y).collect::<Vec<_>>(),
                        labelled.len(),
                        self.device,
                    )?;
                    let w: Vec<f32> = labelled.iter().map(|(i, _)| weights[*i]).collect();
                    let kind = heads.kind.index_select(&idx, 0)?;
                    loss = (loss + weighted_mean(&nll_rows(&kind, &y)?, &w, self.device)?)?;
                }
            }
            Task::Decide => {
                let y: Vec<u32> = rows
                    .iter()
                    .map(|p| p.choice.unwrap_or(self.config.max_options as u32))
                    .collect();
                let y = Tensor::from_vec(y, rows.len(), self.device)?;
                loss = (loss
                    + weighted_mean(&nll_rows(&heads.decision, &y)?, &weights, self.device)?)?;
            }
            Task::Update | Task::Compose => {
                if task == Task::Update {
                    let y: Vec<f32> = rows.iter().map(|p| p.change.unwrap_or(0.0)).collect();
                    let y = Tensor::from_vec(y, rows.len(), self.device)?;
                    loss = (loss
                        + weighted_mean(
                            &bce_with_logits(&heads.change, &y)?,
                            &weights,
                            self.device,
                        )?)?;
                }
                let (codes, _) = levels.expect("generating tasks have levels");
                let logits = self.generator.code_logits(&hidden.levels)?;
                let level_weights = self.config.level_weights();
                for ((logit, code), (w, &l)) in logits
                    .iter()
                    .zip(&codes)
                    .zip(level_weights.iter().zip(&self.config.levels))
                {
                    let per_row = nll_rows(logit, code)?.mean(D::Minus1)?;
                    let term = (weighted_mean(&per_row, &weights, self.device)? * (w * l as f64))?;
                    loss = (loss + term)?;
                }
            }
        }
        Ok(loss)
    }
}

/// Replaces each conditioning code with a random one at its level's rate. Labels stay true.
fn corrupt(
    codes: &[Tensor],
    rates: &[f32],
    codebook: usize,
    rng: &mut StdRng,
) -> Result<Vec<Tensor>> {
    codes
        .iter()
        .zip(rates)
        .map(|(c, &p)| {
            if p <= 0.0 {
                return Ok(c.clone());
            }
            let (b, l) = c.dims2()?;
            let mut v = c.flatten_all()?.to_vec1::<u32>()?;
            for code in &mut v {
                if rng.gen::<f32>() < p {
                    *code = rng.gen_range(0..codebook as u32);
                }
            }
            Ok(Tensor::from_vec(v, (b, l), c.device())?)
        })
        .collect()
}

fn by_task(prepared: Vec<Prepared>) -> BTreeMap<Task, Vec<Prepared>> {
    let mut out: BTreeMap<Task, Vec<Prepared>> = BTreeMap::new();
    for p in prepared {
        out.entry(p.task).or_default().push(p);
    }
    out
}

fn draw_task(
    pools: &BTreeMap<Task, Vec<Prepared>>,
    mix: &TaskMix,
    rng: &mut StdRng,
) -> Option<Task> {
    let tasks: Vec<(Task, f32)> = pools
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(t, _)| (*t, mix.weight(*t)))
        .filter(|(_, w)| *w > 0.0)
        .collect();
    let total: f32 = tasks.iter().map(|(_, w)| w).sum();
    let mut r = rng.gen::<f32>() * total;
    for (t, w) in &tasks {
        r -= w;
        if r <= 0.0 {
            return Some(*t);
        }
    }
    tasks.last().map(|(t, _)| *t)
}

fn draw<'a>(pool: &'a [Prepared], n: usize, rng: &mut StdRng) -> Vec<&'a Prepared> {
    (0..n)
        .map(|_| pool.choose(rng).expect("nonempty pool"))
        .collect()
}

/// One optimiser step over `opts.accumulate` micro-batches of `opts.batch` rows of one task.
/// Each micro-batch's loss is divided by their count and the gradients are summed before
/// clipping, so the step follows the mean loss of the whole effective batch while only one
/// micro-batch's activations are in memory. With one micro-batch this is the plain step,
/// random draws included. Returns the mean loss and the gradient norm before clipping.
#[allow(clippy::too_many_arguments)]
fn task_step(
    models: &Models<'_>,
    task: Task,
    pool: &[Prepared],
    pad: u32,
    ctx: Ctx,
    opts: &TrainOptions,
    opt: &mut AdamW,
    vars: &[Var],
    rng: &mut StdRng,
) -> Result<(f32, f32)> {
    let parts = opts.accumulate.max(1);
    if parts == 1 {
        let rows = draw(pool, opts.batch, rng);
        let loss = models.loss(task, &rows, pad, ctx, rng)?;
        let value = nn::scalar(&loss)?;
        return Ok((value, step(opt, vars, &loss, opts.clip)?));
    }
    let mut total: Option<GradStore> = None;
    let mut value = 0f32;
    for _ in 0..parts {
        let rows = draw(pool, opts.batch, rng);
        let loss = models.loss(task, &rows, pad, ctx, rng)?;
        value += nn::scalar(&loss)? / parts as f32;
        let part = (loss / parts as f64)?.backward()?;
        total = Some(match total {
            None => part,
            Some(sum) => add_gradients(sum, &part, vars)?,
        });
    }
    let grads = total.expect("at least one micro-batch");
    apply_gradients(opt, vars, grads, opts.clip).map(|norm| (value, norm))
}

/// Trains the generator and heads on every task, with the tokenizer frozen.
pub fn train_generator(
    dir: &Path,
    examples: &[Example],
    data: &[PathBuf],
    opts: &TrainOptions,
    mix: &TaskMix,
    device: &Device,
) -> Result<EvalReport> {
    let (config, codec, manifest) = open_config(dir)?;
    manifest.verify(dir, VQ_FILE)?;
    let vq_vb = VarBuilder::from_tensors(
        load_tensors(&dir.join(VQ_FILE), device)?,
        DType::F32,
        device,
    );
    let vq = VqVae::new(&vq_vb, &config)?;
    let mut map = VarMap::new();
    let generator = Generator::new(&VarBuilder::from_varmap(&map, DType::F32, device), &config)?;
    let mut start = 0;
    if opts.resume && dir.join(GENERATOR_FILE).exists() {
        map.load(dir.join(GENERATOR_FILE))?;
        start = load_progress(dir, "generator");
    }
    let (train, skipped) = prepare_all(&codec, &config, examples, Split::Train);
    let pools = by_task(train);
    if pools.values().all(Vec::is_empty) {
        return Err(LncmError::Data("no training examples".into()));
    }
    let vars = map.all_vars();
    let mut opt = optimiser(vars.clone(), opts)?;
    let mut rng = StdRng::seed_from_u64(opts.seed ^ start.wrapping_mul(31));
    let models = Models {
        vq: &vq,
        generator: &generator,
        config: &config,
        device,
    };
    let pad = codec.special(crate::Special::Pad);
    let started = Instant::now();
    log(
        dir,
        serde_json::json!({"stage": "generator", "start": start, "skipped_examples": skipped, "parameters": config.parameters()}),
    )?;
    for step_no in start..opts.steps {
        if pause_or_stop(dir, opts, step_no)? {
            save_varmap(dir, &map, GENERATOR_FILE, "generator", step_no)?;
            return Err(stopped("generator", step_no));
        }
        opt.set_learning_rate(learning_rate(opts, step_no));
        let task = draw_task(&pools, mix, &mut rng)
            .ok_or_else(|| LncmError::Data("no task to draw".into()))?;
        let (value, norm) = task_step(
            &models,
            task,
            &pools[&task],
            pad,
            Ctx::train(config.dropout),
            opts,
            &mut opt,
            &vars,
            &mut rng,
        )?;
        if (step_no + 1) % 50 == 0 || step_no + 1 == opts.steps {
            log(
                dir,
                serde_json::json!({"stage": "generator", "step": step_no + 1, "task": task, "loss": value,
                    "grad_norm": norm, "lr": learning_rate(opts, step_no), "seconds": started.elapsed().as_secs_f32()}),
            )?;
        }
        if (step_no + 1) % opts.save_every == 0 {
            save_varmap(dir, &map, GENERATOR_FILE, "generator", step_no + 1)?;
        }
        if opts.eval_every > 0 && (step_no + 1) % opts.eval_every == 0 && step_no + 1 != opts.steps
        {
            let lncm = Lncm::from_parts(
                config.clone(),
                codec.clone(),
                vq.clone(),
                generator.clone(),
                device.clone(),
            );
            let report = evaluate(&lncm, examples, opts.eval_limit)?;
            log(
                dir,
                serde_json::json!({"stage": "generator", "step": step_no + 1, "eval": report}),
            )?;
        }
    }
    save_varmap(dir, &map, GENERATOR_FILE, "generator", opts.steps)?;
    let lncm = Lncm::from_parts(config.clone(), codec, vq, generator, device.clone());
    let report = evaluate(&lncm, examples, opts.eval_limit)?;
    finish_stage(
        dir,
        GENERATOR_FILE,
        "generator",
        opts.steps,
        data,
        report.metrics(),
    )?;
    log(
        dir,
        serde_json::json!({"stage": "generator", "final_eval": report}),
    )?;
    Ok(report)
}

/// Trains one domain's adapter; the tokenizer and generator stay exactly as they were.
pub fn train_adapter(
    dir: &Path,
    name: &str,
    examples: &[Example],
    data: &[PathBuf],
    opts: &TrainOptions,
    mix: &TaskMix,
    device: &Device,
) -> Result<EvalReport> {
    if !crate::model::adapter_name_valid(name) {
        return Err(LncmError::Input("invalid_adapter_name"));
    }
    let (config, codec, manifest) = open_config(dir)?;
    manifest.verify(dir, VQ_FILE)?;
    let base_sha256 = manifest.verify(dir, GENERATOR_FILE)?;
    let mut tensors = load_tensors(&dir.join(VQ_FILE), device)?;
    tensors.extend(load_tensors(&dir.join(GENERATOR_FILE), device)?);
    let vb = VarBuilder::from_tensors(tensors, DType::F32, device);
    let vq = VqVae::new(&vb, &config)?;
    let mut generator = Generator::new(&vb, &config)?;
    let mine: Vec<Example> = examples
        .iter()
        .filter(|e| e.adapter.as_deref() == Some(name))
        .cloned()
        .collect();
    if mine.is_empty() {
        return Err(LncmError::Data(format!("no examples carry adapter {name}")));
    }
    let lora_dir = dir.join("lora");
    std::fs::create_dir_all(&lora_dir)?;
    let file = format!("lora/{name}.safetensors");
    let stage = format!("adapter-{name}");
    let mut map = VarMap::new();
    generator.attach_lora(
        &VarBuilder::from_varmap(&map, DType::F32, device),
        &config.lora,
    )?;
    let mut start = 0;
    if opts.resume && dir.join(&file).exists() {
        map.load(dir.join(&file))?;
        start = load_progress(dir, &stage);
    }
    let (train, _) = prepare_all(&codec, &config, &mine, Split::Train);
    let pools = by_task(train);
    let vars = map.all_vars();
    let mut opt = optimiser(vars.clone(), opts)?;
    let mut rng = StdRng::seed_from_u64(opts.seed ^ 0xADA7 ^ start);
    let models = Models {
        vq: &vq,
        generator: &generator,
        config: &config,
        device,
    };
    let pad = codec.special(crate::Special::Pad);
    for step_no in start..opts.steps {
        if pause_or_stop(dir, opts, step_no)? {
            save_varmap(dir, &map, &file, &stage, step_no)?;
            return Err(stopped(&stage, step_no));
        }
        opt.set_learning_rate(learning_rate(opts, step_no));
        let Some(task) = draw_task(&pools, mix, &mut rng) else {
            return Err(LncmError::Data(format!(
                "adapter {name} has no training examples"
            )));
        };
        let (value, _) = task_step(
            &models,
            task,
            &pools[&task],
            pad,
            Ctx::train(config.dropout),
            opts,
            &mut opt,
            &vars,
            &mut rng,
        )?;
        if (step_no + 1) % 50 == 0 || step_no + 1 == opts.steps {
            log(
                dir,
                serde_json::json!({"stage": stage, "step": step_no + 1, "task": task, "loss": value}),
            )?;
        }
        if (step_no + 1) % opts.save_every == 0 {
            save_varmap(dir, &map, &file, &stage, step_no + 1)?;
        }
    }
    save_varmap(dir, &map, &file, &stage, opts.steps)?;
    let lncm = Lncm::from_parts(config, codec, vq, generator, device.clone());
    let report = evaluate(&lncm, &mine, opts.eval_limit)?;
    let mut manifest = Manifest::load(dir)?;
    manifest.adapters.insert(
        name.to_string(),
        AdapterRecord {
            file: file.clone(),
            sha256: file_sha256(&dir.join(&file))?,
            base_sha256,
            steps: opts.steps,
            metrics: report.metrics(),
        },
    );
    manifest.stages.retain(|s| s.stage != stage);
    manifest.stages.push(StageRecord {
        stage,
        steps: opts.steps,
        data_sha256: data_digests(data)?,
        metrics: report.metrics(),
        finished_at_ms: now_ms(),
    });
    manifest.save(dir)?;
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// Evaluation and calibration

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    pub device: String,
    pub gate_accuracy: Option<f64>,
    pub yes_no_accuracy: Option<f64>,
    pub change_accuracy: Option<f64>,
    pub decision_accuracy: Option<f64>,
    /// Expected calibration error of the decision head, 10 bins.
    pub decision_ece: Option<f64>,
    pub abstain_recall: Option<f64>,
    pub compose_char_f1: Option<f64>,
    pub update_line_f1: Option<f64>,
    pub ms_gate: Option<f64>,
    pub ms_decide: Option<f64>,
    pub ms_compose: Option<f64>,
    pub counts: BTreeMap<String, usize>,
}

impl EvalReport {
    pub fn metrics(&self) -> BTreeMap<String, f64> {
        let mut m = BTreeMap::new();
        for (k, v) in [
            ("gate_accuracy", self.gate_accuracy),
            ("yes_no_accuracy", self.yes_no_accuracy),
            ("change_accuracy", self.change_accuracy),
            ("decision_accuracy", self.decision_accuracy),
            ("decision_ece", self.decision_ece),
            ("abstain_recall", self.abstain_recall),
            ("compose_char_f1", self.compose_char_f1),
            ("update_line_f1", self.update_line_f1),
            ("ms_gate", self.ms_gate),
            ("ms_decide", self.ms_decide),
            ("ms_compose", self.ms_compose),
        ] {
            if let Some(v) = v {
                m.insert(k.to_string(), v);
            }
        }
        m
    }
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn ece(confidence_correct: &[(f32, bool)]) -> Option<f64> {
    if confidence_correct.is_empty() {
        return None;
    }
    let mut bins = [(0f64, 0f64, 0usize); 10];
    for &(c, ok) in confidence_correct {
        let b = ((c * 10.0) as usize).min(9);
        bins[b].0 += f64::from(c);
        bins[b].1 += if ok { 1.0 } else { 0.0 };
        bins[b].2 += 1;
    }
    let n = confidence_correct.len() as f64;
    Some(
        bins.iter()
            .filter(|b| b.2 > 0)
            .map(|(c, a, k)| (*k as f64 / n) * ((c / *k as f64) - (a / *k as f64)).abs())
            .sum(),
    )
}

fn line_f1(predicted: &str, gold: &str) -> f64 {
    let p: std::collections::BTreeSet<&str> = predicted
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let g: std::collections::BTreeSet<&str> = gold
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if p.is_empty() && g.is_empty() {
        return 1.0;
    }
    let hit = p.intersection(&g).count() as f64;
    if hit == 0.0 {
        return 0.0;
    }
    let (precision, recall) = (hit / p.len() as f64, hit / g.len() as f64);
    2.0 * precision * recall / (precision + recall)
}

/// Measures a model on development examples (all examples when none are marked dev), at most
/// `limit` per task, with wall-clock latency on the model's own device.
pub fn evaluate(model: &Lncm, examples: &[Example], limit: usize) -> Result<EvalReport> {
    let dev: Vec<&Example> = if examples.iter().any(|e| e.split == Split::Dev) {
        examples.iter().filter(|e| e.split == Split::Dev).collect()
    } else {
        examples.iter().collect()
    };
    let mut report = EvalReport {
        device: format!("{:?}", model.device()),
        ..Default::default()
    };
    let mut seen: BTreeMap<Task, usize> = BTreeMap::new();
    let (mut gate, mut kind, mut change, mut decide, mut abstain, mut f1, mut lines) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let (mut t_gate, mut t_decide, mut t_compose) = (vec![], vec![], vec![]);
    let mut calibration = Vec::new();
    for e in dev {
        let count = seen.entry(e.task).or_default();
        if *count >= limit {
            continue;
        }
        *count += 1;
        let question = e.question.as_deref().unwrap_or("");
        match e.task {
            Task::Gate => {
                let t = Instant::now();
                let g = model.gate(e.lang, question)?;
                t_gate.push(t.elapsed().as_secs_f64() * 1000.0);
                if let Some(y) = e.target.gate {
                    gate.push(if (g.social >= 0.5) == y { 1.0 } else { 0.0 });
                }
                if let Some(y) = e.target.yes_no {
                    kind.push(if (g.yes_no >= 0.5) == y { 1.0 } else { 0.0 });
                }
            }
            Task::Decide => {
                if e.options.len() > model.config().max_options {
                    continue;
                }
                let t = Instant::now();
                let d = model.decide(
                    e.lang,
                    question,
                    e.state.as_deref(),
                    e.draft.as_deref(),
                    &e.options,
                )?;
                t_decide.push(t.elapsed().as_secs_f64() * 1000.0);
                let gold = e.target.choice.map(|c| match c {
                    Choice::Option(i) => Some(i),
                    Choice::Abstain => None,
                });
                if let Some(gold) = gold {
                    let ok = d.choice == gold;
                    decide.push(if ok { 1.0 } else { 0.0 });
                    calibration.push((d.confidence, ok));
                    if gold.is_none() {
                        abstain.push(if d.choice.is_none() { 1.0 } else { 0.0 });
                    }
                }
            }
            Task::Update => {
                let out = model.update(
                    e.lang,
                    e.state.as_deref().unwrap_or(""),
                    e.chunk.as_deref().unwrap_or(""),
                    0.5,
                )?;
                if let Some(y) = e.target.change {
                    change.push(if (out.change >= 0.5) == y { 1.0 } else { 0.0 });
                }
                if let (Some(gold), true) = (&e.target.text, e.target.change == Some(true)) {
                    lines.push(line_f1(out.delta.as_deref().unwrap_or(""), gold));
                }
            }
            Task::Compose => {
                let roster: Vec<String> = Vec::new();
                let t = Instant::now();
                let c = model.compose(
                    e.lang,
                    question,
                    e.state.as_deref(),
                    e.draft.as_deref(),
                    &roster,
                    Default::default(),
                )?;
                t_compose.push(t.elapsed().as_secs_f64() * 1000.0);
                if let Some(gold) = &e.target.text {
                    f1.push(f64::from(crate::model::char_ngram_f1(&c.answer, gold)));
                }
            }
        }
    }
    report.gate_accuracy = mean(&gate);
    report.yes_no_accuracy = mean(&kind);
    report.change_accuracy = mean(&change);
    report.decision_accuracy = mean(&decide);
    report.decision_ece = ece(&calibration);
    report.abstain_recall = mean(&abstain);
    report.compose_char_f1 = mean(&f1);
    report.update_line_f1 = mean(&lines);
    report.ms_gate = mean(&t_gate);
    report.ms_decide = mean(&t_decide);
    report.ms_compose = mean(&t_compose);
    report.counts = seen
        .into_iter()
        .map(|(t, n)| (format!("{t:?}").to_lowercase(), n))
        .collect();
    Ok(report)
}

/// Fits the decision temperature on development examples by minimising their negative
/// log-likelihood over a grid, writes it into the config and re-records it in the manifest.
pub fn calibrate(dir: &Path, examples: &[Example], device: &Device) -> Result<f32> {
    let model = Lncm::load(dir, device)?;
    let dev: Vec<&Example> = examples
        .iter()
        .filter(|e| {
            e.split == Split::Dev
                && e.task == Task::Decide
                && e.target.choice.is_some()
                && e.options.len() <= model.config().max_options
        })
        .take(2000)
        .collect();
    if dev.is_empty() {
        return Err(LncmError::Data(
            "no development decisions to calibrate on".into(),
        ));
    }
    // Logits at temperature 1, once.
    let mut rows = Vec::with_capacity(dev.len());
    let base_temperature = model.config().decision_temperature;
    for e in &dev {
        let d = model.decide(
            e.lang,
            e.question.as_deref().unwrap_or(""),
            e.state.as_deref(),
            e.draft.as_deref(),
            &e.options,
        )?;
        let mut probs = d.probabilities.clone();
        probs.push(d.abstain);
        let gold = match e.target.choice.expect("filtered") {
            Choice::Option(i) => i,
            Choice::Abstain => probs.len() - 1,
        };
        // Recover logits up to a constant from the probabilities at the current temperature.
        let logits: Vec<f64> = probs
            .iter()
            .map(|p| f64::from(p.max(1e-12)).ln() * f64::from(base_temperature))
            .collect();
        rows.push((logits, gold));
    }
    let nll = |t: f64| -> f64 {
        rows.iter()
            .map(|(logits, gold)| {
                let scaled: Vec<f64> = logits.iter().map(|l| l / t).collect();
                let max = scaled.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let log_sum = scaled.iter().map(|s| (s - max).exp()).sum::<f64>().ln() + max;
                log_sum - scaled[*gold]
            })
            .sum::<f64>()
            / rows.len() as f64
    };
    let mut best = (
        f64::from(base_temperature),
        nll(f64::from(base_temperature)),
    );
    let mut t = 0.05;
    while t <= 20.0 {
        let v = nll(t);
        if v < best.1 {
            best = (t, v);
        }
        t *= 1.1;
    }
    let temperature = best.0 as f32;
    let mut config = model.config().clone();
    config.decision_temperature = temperature.clamp(0.05, 20.0);
    write_atomic(&dir.join(CONFIG_FILE), config.to_toml()?.as_bytes())?;
    let mut manifest = Manifest::load(dir)?;
    manifest.record(dir, CONFIG_FILE)?;
    manifest.stages.retain(|s| s.stage != "calibrate");
    let mut metrics = BTreeMap::new();
    metrics.insert(
        "decision_temperature".into(),
        f64::from(config.decision_temperature),
    );
    metrics.insert("dev_nll".into(), best.1);
    manifest.stages.push(StageRecord {
        stage: "calibrate".into(),
        steps: 0,
        data_sha256: Vec::new(),
        metrics,
        finished_at_ms: now_ms(),
    });
    manifest.save(dir)?;
    Ok(config.decision_temperature)
}

/// Greedy samples from a trained model, for reading what it says.
pub fn samples(model: &Lncm, examples: &[Example], n: usize) -> Result<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    for e in examples.iter().filter(|e| e.task == Task::Compose).take(n) {
        let c = model.compose(
            e.lang,
            e.question.as_deref().unwrap_or(""),
            e.state.as_deref(),
            e.draft.as_deref(),
            &[],
            Default::default(),
        )?;
        out.push(serde_json::json!({"id": e.id, "question": e.question, "plan": c.plan, "answer": c.answer, "gold": e.target.text}));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_file_stops_and_no_thermal_config_never_waits() {
        let dir = tempfile::tempdir().unwrap();
        let opts = TrainOptions::default();
        assert!(!pause_or_stop(dir.path(), &opts, 0).unwrap());
        std::fs::write(dir.path().join(STOP_FILE), "").unwrap();
        assert!(pause_or_stop(dir.path(), &opts, 0).unwrap());
    }

    #[test]
    fn accumulated_halves_give_the_whole_batch_gradient() {
        let w = Var::new(&[0.5f32, -1.0, 2.0], &Device::Cpu).unwrap();
        let rows = Tensor::new(
            &[
                [1f32, 2.0, 3.0],
                [0.5, -1.0, 4.0],
                [2.0, 0.0, -1.0],
                [1.0, 1.0, 1.0],
            ],
            &Device::Cpu,
        )
        .unwrap();
        let loss_of = |x: &Tensor| -> Tensor {
            x.broadcast_mul(w.as_tensor())
                .unwrap()
                .sum(1)
                .unwrap()
                .sqr()
                .unwrap()
                .mean_all()
                .unwrap()
        };
        let whole = loss_of(&rows).backward().unwrap();
        let half = |start: usize| {
            (loss_of(&rows.narrow(0, start, 2).unwrap()) / 2.0)
                .unwrap()
                .backward()
                .unwrap()
        };
        let summed = add_gradients(half(0), &half(2), &[w.clone()]).unwrap();
        let expect = whole.get(w.as_tensor()).unwrap().to_vec1::<f32>().unwrap();
        let got = summed.get(w.as_tensor()).unwrap().to_vec1::<f32>().unwrap();
        for (a, b) in expect.iter().zip(&got) {
            assert!((a - b).abs() < 1e-5, "{expect:?} vs {got:?}");
        }
    }

    #[test]
    fn schedule_warms_up_then_decays() {
        let opts = TrainOptions {
            steps: 100,
            warmup: 10,
            lr: 1.0,
            ..Default::default()
        };
        assert!((learning_rate(&opts, 0) - 0.1).abs() < 1e-9);
        assert!((learning_rate(&opts, 9) - 1.0).abs() < 1e-9);
        assert!(learning_rate(&opts, 50) < 1.0);
        assert!((learning_rate(&opts, 100) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn bce_matches_its_definition() {
        let x = Tensor::new(&[-30f32, -1.0, 0.0, 2.0, 30.0], &Device::Cpu).unwrap();
        let y = Tensor::new(&[0f32, 1.0, 1.0, 0.0, 1.0], &Device::Cpu).unwrap();
        let got = bce_with_logits(&x, &y).unwrap().to_vec1::<f32>().unwrap();
        let expect: Vec<f32> = [
            (-30f32, 0f32),
            (-1.0, 1.0),
            (0.0, 1.0),
            (2.0, 0.0),
            (30.0, 1.0),
        ]
        .iter()
        .map(|&(x, y)| {
            let p = 1.0 / (1.0 + (-x as f64).exp());
            (-(y as f64) * p.max(1e-300).ln() - (1.0 - y as f64) * (1.0 - p).max(1e-300).ln())
                as f32
        })
        .collect();
        for (g, e) in got.iter().zip(&expect) {
            assert!((g - e).abs() < 1e-4, "{g} vs {e}");
        }
    }

    #[test]
    fn calibration_error_and_line_f1() {
        assert!((ece(&[(0.9, true), (0.9, true)]).unwrap() - 0.1).abs() < 1e-6);
        assert_eq!(line_f1("a\nb", "a\nb"), 1.0);
        assert_eq!(line_f1("a", "b"), 0.0);
        assert!((line_f1("a\nb", "a") - 2.0 / 3.0).abs() < 1e-9);
    }
}
