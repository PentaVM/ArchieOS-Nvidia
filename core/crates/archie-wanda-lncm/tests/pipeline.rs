//! The whole training pipeline on a CPU with the tiny preset: synthetic data, codec,
//! tokenizer, generator, adapter, calibration, evaluation, use and integrity checks.
//!
//! This proves the code paths run and that weights round-trip; it says nothing about how good
//! a trained model is. A tiny model trained for a few dozen steps is not.

use archie_wanda_lncm::data::{self, Choice, Split};
use archie_wanda_lncm::model::{ComposeOptions, Lncm, Manifest};
use archie_wanda_lncm::reader::{self, LncmInterpreter, ReaderConfig, Turn, TurnKind};
use archie_wanda_lncm::state::SocialState;
use archie_wanda_lncm::synth::{self, SynthConfig};
use archie_wanda_lncm::train::{self, TaskMix, TrainOptions};
use archie_wanda_lncm::{Lang, Task};
use candle_core::Device;

fn log_values(dir: &std::path::Path, stage: &str, key: &str) -> Vec<f64> {
    std::fs::read_to_string(dir.join(train::LOG_FILE))
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["stage"] == stage)
        .filter_map(|v| v[key].as_f64())
        .collect()
}

#[test]
fn trains_and_serves_end_to_end_on_cpu() {
    let tmp = tempfile::tempdir().unwrap();
    let mut out = synth::generate(&SynthConfig {
        worlds: 5,
        min_turns: 40,
        max_turns: 60,
        ..Default::default()
    })
    .unwrap();
    for e in out
        .examples
        .iter_mut()
        .filter(|e| e.lang == Lang::Es)
        .take(60)
    {
        e.adapter = Some("personal-test".into());
    }
    // Hold one world out for development, whatever the hash split gave.
    for e in out.examples.iter_mut().filter(|e| e.id.contains("-4-en")) {
        e.split = Split::Dev;
    }
    let examples_path = tmp.path().join("examples.jsonl");
    data::write_jsonl(&examples_path, &out.examples).unwrap();
    let examples = data::load_examples(&examples_path).unwrap();
    let data_files = vec![examples_path.clone()];

    let dir = tmp.path().join("ckpt");
    let config = train::init(&dir, "tiny", 640, &examples).unwrap();
    assert!(config.vocab_size > 272, "a BPE codec was trained");
    assert!(
        train::init(&dir, "tiny", 640, &examples).is_err(),
        "init never overwrites"
    );

    let opts = TrainOptions {
        steps: 60,
        batch: 4,
        lr: 3e-3,
        warmup: 5,
        save_every: 30,
        eval_every: 0,
        eval_limit: 6,
        ..Default::default()
    };
    let tokenizer =
        train::train_tokenizer(&dir, &examples, &data_files, &opts, &Device::Cpu).unwrap();
    assert_eq!(tokenizer.utilisation.len(), config.levels.len());
    let reconstruction = log_values(&dir, "tokenizer", "reconstruction");
    assert!(
        reconstruction.last().unwrap() < reconstruction.first().unwrap(),
        "{reconstruction:?}"
    );

    let report = train::train_generator(
        &dir,
        &examples,
        &data_files,
        &opts,
        &TaskMix::default(),
        &Device::Cpu,
    )
    .unwrap();
    for task in ["gate", "decide", "update", "compose"] {
        assert!(
            report.counts.get(task).copied().unwrap_or(0) > 0,
            "{task} was evaluated: {report:?}"
        );
    }
    assert!(report.decision_accuracy.is_some() && report.ms_compose.is_some());

    // Two micro-batches per step: the accumulation path runs end to end.
    let adapter_opts = TrainOptions {
        steps: 10,
        accumulate: 2,
        ..opts.clone()
    };
    train::train_adapter(
        &dir,
        "personal-test",
        &examples,
        &data_files,
        &adapter_opts,
        &TaskMix::default(),
        &Device::Cpu,
    )
    .unwrap();
    let t = train::calibrate(&dir, &examples, &Device::Cpu).unwrap();
    assert!((0.05..=20.0).contains(&t));

    let manifest = Manifest::load(&dir).unwrap();
    let stages: Vec<&str> = manifest.stages.iter().map(|s| s.stage.as_str()).collect();
    assert_eq!(
        stages,
        vec![
            "tokenizer",
            "generator",
            "adapter-personal-test",
            "calibrate"
        ]
    );
    assert!(manifest.adapters.contains_key("personal-test"));

    // Use: every entry point answers in range.
    let model = Lncm::load(&dir, &Device::Cpu).unwrap();
    let gate = model
        .gate(Lang::Es, "¿Sabe Pedro que el viaje es en julio?")
        .unwrap();
    assert!((0.0..=1.0).contains(&gate.social) && (0.0..=1.0).contains(&gate.yes_no));
    let options = vec!["sí".to_string(), "no".to_string()];
    let decision = model
        .decide(
            Lang::Es,
            "¿Sabe Pedro lo del viaje?",
            Some("Personas: Pedro"),
            None,
            &options,
        )
        .unwrap();
    let total: f32 = decision.probabilities.iter().sum::<f32>() + decision.abstain;
    assert!((total - 1.0).abs() < 1e-3, "{decision:?}");
    let composed = model
        .compose(
            Lang::Es,
            "¿Tengo que avisar a Pedro?",
            Some("Personas: Pedro"),
            Some("Creo que no lo sabe."),
            &["Pedro".to_string()],
            ComposeOptions {
                candidates: 3,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(composed.seconds >= 0.0);
    let with_adapter = model.with_adapter("personal-test").unwrap();
    assert_eq!(with_adapter.adapter(), Some("personal-test"));
    assert!(model.with_adapter("nobody").is_err());
    let _ = with_adapter
        .quick(Lang::Es, "¿Llegará tarde Ana?", None, None)
        .unwrap();

    // Reading with the trained model as interpreter keeps the state valid.
    let mut state = SocialState::new("owner", "Yo").unwrap();
    let turns: Vec<Turn> = (1..=12u64)
        .map(|t| Turn {
            turn: t,
            at_ms: t as i64 * 60_000,
            chat: "grupo".into(),
            kind: TurnKind::Message {
                speaker: if t % 2 == 0 {
                    "ana".into()
                } else {
                    "pedro".into()
                },
                text: "Perdón, voy diez minutos tarde.".into(),
            },
            evidence: format!("ev-{t}"),
            label: None,
        })
        .collect();
    let interpreter = LncmInterpreter {
        model: &model,
        lang: Lang::Es,
        threshold: 0.0,
    };
    let read = reader::read(&mut state, &turns, &interpreter, &ReaderConfig::default()).unwrap();
    assert!(read.chunks >= 1);
    state.validate().unwrap();

    // Synthetic decisions include the right to abstain.
    assert!(examples
        .iter()
        .any(|e| e.task == Task::Decide && e.target.choice == Some(Choice::Abstain)));

    // Integrity: a changed weight file is refused, and so is the adapter trained on the original.
    let generator = dir.join("generator.safetensors");
    let mut bytes = std::fs::read(&generator).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&generator, bytes).unwrap();
    assert!(Lncm::load(&dir, &Device::Cpu).is_err());
}
