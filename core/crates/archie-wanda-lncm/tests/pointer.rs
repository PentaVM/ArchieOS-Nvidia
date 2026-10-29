//! The copy head and the n-gram index through the whole path on a CPU, tiny preset: trained on
//! the frozen generator, recorded in the manifest, bound to the generator's digest, served by
//! `compose` and `update`, and refused when either no longer matches.
//!
//! This proves the code paths, not the head's quality: a tiny model trained for a few steps
//! copies little and well only by chance.

use archie_wanda_lncm::data::{self, Split};
use archie_wanda_lncm::model::{ComposeOptions, Lncm, Manifest};
use archie_wanda_lncm::ngram::NGRAM_FILE;
use archie_wanda_lncm::pointer::{POINTER_BASE_FILE, POINTER_FILE};
use archie_wanda_lncm::synth::{self, SynthConfig};
use archie_wanda_lncm::train::{self, TaskMix, TrainOptions};
use archie_wanda_lncm::{Lang, Task};
use candle_core::Device;

#[test]
fn the_copy_head_and_the_index_train_load_serve_and_refuse_tampering() {
    let tmp = tempfile::tempdir().unwrap();
    let mut out = synth::generate(&SynthConfig {
        worlds: 4,
        min_turns: 40,
        max_turns: 60,
        ..Default::default()
    })
    .unwrap();
    for e in out.examples.iter_mut().filter(|e| e.id.contains("-3-")) {
        e.split = Split::Dev;
    }
    let path = tmp.path().join("examples.jsonl");
    data::write_jsonl(&path, &out.examples).unwrap();
    let examples = data::load_examples(&path).unwrap();
    let files = vec![path.clone()];
    let dir = tmp.path().join("ckpt");
    train::init(&dir, "tiny", 640, &examples).unwrap();
    let opts = TrainOptions {
        steps: 30,
        batch: 4,
        lr: 3e-3,
        warmup: 5,
        save_every: 15,
        eval_every: 0,
        eval_limit: 6,
        ..Default::default()
    };
    train::train_tokenizer(&dir, &examples, &files, &opts, &Device::Cpu).unwrap();
    train::train_generator(
        &dir,
        &examples,
        &files,
        &opts,
        &TaskMix::default(),
        &Device::Cpu,
    )
    .unwrap();

    let pointer_opts = TrainOptions {
        steps: 20,
        accumulate: 2,
        ..opts.clone()
    };
    let report =
        train::train_pointer(&dir, &examples, &files, &pointer_opts, &Device::Cpu).unwrap();
    assert!(report.loss.is_finite() && report.examples > 0, "{report:?}");
    assert!(
        report.copy_share > 0.0,
        "some answer tokens are copies: {report:?}"
    );
    let built = train::build_ngram(&dir, &examples, &files).unwrap();
    assert!(built["answers"].as_u64().unwrap() > 0);

    let manifest = Manifest::load(&dir).unwrap();
    for file in [POINTER_FILE, POINTER_BASE_FILE, NGRAM_FILE] {
        assert!(manifest.files.contains_key(file), "{file} is recorded");
    }
    let model = Lncm::load(&dir, &Device::Cpu).unwrap();
    assert!(model.has_pointer());

    let e = examples
        .iter()
        .find(|e| e.task == Task::Compose && e.split == Split::Dev)
        .unwrap();
    let composed = model
        .compose(
            e.lang,
            e.question.as_deref().unwrap(),
            e.state.as_deref(),
            None,
            &[],
            ComposeOptions {
                candidates: 3,
                ..Default::default()
            },
        )
        .unwrap();
    // Either it passed the checks, or it says why not.
    assert!(
        composed.verified || !composed.problems.is_empty(),
        "{composed:?}"
    );
    let u = examples.iter().find(|e| e.task == Task::Update).unwrap();
    model
        .update(
            Lang::Es,
            u.state.as_deref().unwrap_or(""),
            u.chunk.as_deref().unwrap(),
            0.0,
        )
        .unwrap();

    // A head that names another generator is refused, and so is a changed index.
    let base = std::fs::read_to_string(dir.join(POINTER_BASE_FILE)).unwrap();
    // Another generator: the hash's first digit changed. (Swapping every '0' for a '1' left
    // the file as it was whenever the hash had no '0', about one training in sixty.)
    let at = base.find("\"generator_sha256\"").unwrap();
    let digit = at + base[at..].find(": \"").unwrap() + 3;
    let other = if &base[digit..digit + 1] == "0" {
        "1"
    } else {
        "0"
    };
    let named_other = format!("{}{other}{}", &base[..digit], &base[digit + 1..]);
    assert_ne!(named_other, base);
    std::fs::write(dir.join(POINTER_BASE_FILE), named_other).unwrap();
    assert!(Lncm::load(&dir, &Device::Cpu).is_err());
    std::fs::write(dir.join(POINTER_BASE_FILE), &base).unwrap();
    let mut index = std::fs::read(dir.join(NGRAM_FILE)).unwrap();
    let last = index.len() - 2;
    index[last] ^= 1;
    std::fs::write(dir.join(NGRAM_FILE), &index).unwrap();
    assert!(Lncm::load(&dir, &Device::Cpu).is_err());
}
