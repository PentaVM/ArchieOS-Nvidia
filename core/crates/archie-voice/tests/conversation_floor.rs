//! Synthetic ASR/model/PCM at the external boundaries; the production conversation,
//! floor policy, cognitive adapter and response ledger run unchanged.
use archie_model_runtime::{SseAssembler, ToolChatRequest};
use archie_speech::{CognitiveModelAdapter, CognitiveRequest, SpeechBudget};
use archie_speech::{FloorPolicy, SessionPlan, SpeechCapabilities, SpeechSession};
use archie_voice::asr::{AsrStream, Heard, SpeechRecognizer};
use archie_voice::conversation::{converse, Hearing, Moment, Saying, Setup, StreamedVoice};
use archie_voice::nemotron::{NemotronCognitive, TurnSource, TurnStream, Update};
use archie_voice::voice::{Said, Speaks};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

struct Recognizer(Vec<&'static str>);
struct Stream {
    scheduled: VecDeque<String>,
    ready: VecDeque<Heard>,
    delayed: bool,
    waiting: Option<Heard>,
}
impl SpeechRecognizer for Recognizer {
    fn listen(&self, _: &str, _: &[&str], _: f32) -> Result<Box<dyn AsrStream + '_>, String> {
        Ok(Box::new(Stream {
            scheduled: self.0.iter().map(|text| text.to_string()).collect(),
            ready: VecDeque::new(),
            delayed: false,
            waiting: None,
        }))
    }
    fn recognize(&self, _: &[f32], _: u32) -> Result<String, String> {
        unreachable!()
    }
}
impl AsrStream for Stream {
    fn push(&mut self, samples: &[f32], _: u32) -> Result<(), String> {
        if self.delayed && samples.iter().all(|sample| *sample == 0.0) {
            if let Some(heard) = self.waiting.take() {
                self.ready.push_back(heard);
            }
        }
        if let Some(text) = samples
            .iter()
            .any(|sample| *sample != 0.0)
            .then(|| self.scheduled.pop_front())
            .flatten()
        {
            let heard = Heard {
                text,
                is_final: true,
            };
            if self.delayed && heard.text != "Archie, qué hora es?" {
                self.waiting = Some(heard);
            } else {
                self.ready.push_back(heard);
            }
        }
        Ok(())
    }
    fn next(&mut self) -> Result<Option<Heard>, String> {
        Ok(self.ready.pop_front())
    }
    fn end_utterance(&mut self) -> Result<(), String> {
        Ok(())
    }
}
struct DelayedRecognizer(Vec<&'static str>);
impl SpeechRecognizer for DelayedRecognizer {
    fn listen(&self, _: &str, _: &[&str], _: f32) -> Result<Box<dyn AsrStream + '_>, String> {
        Ok(Box::new(Stream {
            scheduled: self.0.iter().map(|text| text.to_string()).collect(),
            ready: VecDeque::new(),
            delayed: true,
            waiting: None,
        }))
    }
    fn recognize(&self, _: &[f32], _: u32) -> Result<String, String> {
        unreachable!()
    }
}
struct Model;
impl TurnSource for Model {
    fn start(&mut self, _: ToolChatRequest) -> TurnStream {
        let (send, updates) = mpsc::channel();
        let mut assembler = SseAssembler::new();
        for event in assembler.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"Listo.\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n").unwrap() {
            send.send(Update::Event(event)).unwrap();
        }
        TurnStream {
            updates,
            stop: Box::new(|| {}),
        }
    }
}
struct Voice;
impl Speaks for Voice {
    fn sample_rate(&self) -> u32 {
        16_000
    }
    fn say(&mut self, _: &str, hear: &mut dyn FnMut(&[i16]) -> bool) -> Result<Said, String> {
        let stopped = !hear(&[100; 320]);
        Ok(Said {
            sample_rate: 16_000,
            audio_s: 0.02,
            elapsed_s: 0.0,
            first_audio_ms: 0.0,
            stopped,
        })
    }
}
fn replay_with(
    words: Vec<&'static str>,
    voice: Box<dyn Speaks>,
) -> (
    Vec<Moment>,
    SpeechSession<NemotronCognitive<Model>>,
    Result<(), String>,
) {
    let plan = SessionPlan {
        language: "es".into(),
        listener: SpeechCapabilities {
            partial_transcript: true,
            languages: vec!["es".into()],
            ..Default::default()
        },
        voice: SpeechCapabilities {
            accepts_external_response_text: true,
            renders_external_response_verbatim: true,
            languages: vec!["es".into()],
            ..Default::default()
        },
    };
    let mut session = SpeechSession::new(
        plan,
        FloorPolicy::default(),
        NemotronCognitive::new(Model, "replay", vec!["es".into()], vec![]),
    )
    .unwrap();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "archie-floor-replay-{}-{}.wav",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut moments = Vec::new();
    let result = converse(
        &Setup::default(),
        &Recognizer(words),
        None,
        voice,
        Hearing::Recording {
            samples: vec![0.1; 16_000 / 2],
            rate: 16_000,
            at: 0,
            silence_left: 16_000 * 2,
        },
        Saying::Recording {
            path: path.clone(),
            samples: vec![],
            rate: 16_000,
        },
        &mut session,
        &mut |_| panic!("no tool was proposed"),
        &mut String::new,
        Arc::new(AtomicBool::new(false)),
        &mut |moment| moments.push(moment),
    );
    let _ = std::fs::remove_file(path);
    (moments, session, result)
}
fn replay(words: Vec<&'static str>) -> (Vec<Moment>, String) {
    let (moments, session, result) = replay_with(words, Box::new(Voice));
    result.unwrap();
    (moments, session.heard_text())
}

struct FailedVoice;
impl Speaks for FailedVoice {
    fn sample_rate(&self) -> u32 {
        16_000
    }
    fn say(&mut self, _: &str, _: &mut dyn FnMut(&[i16]) -> bool) -> Result<Said, String> {
        Err("synthesis failed".into())
    }
}

#[test]
fn failed_synthesis_is_not_remembered_as_something_the_person_heard() {
    let (moments, mut session, result) =
        replay_with(vec!["Archie, qué hora es?"], Box::new(FailedVoice));
    assert!(
        result.is_err(),
        "a failed voice must cancel the remaining playback"
    );
    assert!(moments
        .iter()
        .any(|moment| matches!(moment, Moment::Alert(_))));
    assert!(session.heard_text().is_empty());
    session
        .cognitive_mut()
        .begin(CognitiveRequest {
            turn_id: "next".into(),
            transcript: "Archie, otra pregunta.".into(),
            language: "es".into(),
            budget: SpeechBudget::Full,
        })
        .unwrap();
    assert!(!session.cognitive().last_request().unwrap().messages.iter().any(|message| matches!(message, archie_model_runtime::ToolChatMessage::Assistant { content: Some(text), .. } if text == "Listo.")), "generated but unplayed text must not become heard history");
}

#[test]
fn early_asr_final_does_not_invent_silence_or_lose_the_request() {
    let (moments, heard) = replay(vec!["Archie, qué hora es?"]);
    let floors: Vec<_> = moments
        .iter()
        .filter_map(|moment| match moment {
            Moment::Floor { action, .. } => Some(action.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        floors.first(),
        Some(&"Silent"),
        "ASR final arrived during voiced audio"
    );
    assert!(
        floors.contains(&"TakeFloor"),
        "answer after measured quiet, without another ASR event"
    );
    assert_eq!(heard, "Listo.");
}

#[test]
fn unfinished_clause_survives_a_second_asr_endpoint() {
    let (moments, heard) = replay(vec!["Archie, abre el formulario y", "revisa los totales."]);
    assert!(
        moments
            .iter()
            .any(|moment| matches!(moment, Moment::Floor { action, .. } if action == "TakeFloor")),
        "retain addressed prefix across the endpoint"
    );
    assert_eq!(heard, "Listo.");
}

struct PacedVoice(mpsc::Sender<()>);
impl Speaks for PacedVoice {
    fn sample_rate(&self) -> u32 {
        16_000
    }
    fn say(&mut self, _: &str, hear: &mut dyn FnMut(&[i16]) -> bool) -> Result<Said, String> {
        let _ = self.0.send(());
        let mut stopped = false;
        for _ in 0..64 {
            if !hear(&[100; 320]) {
                stopped = true;
                break;
            }
        }
        Ok(Said {
            sample_rate: 16_000,
            audio_s: 1.28,
            elapsed_s: 0.0,
            first_audio_ms: 0.0,
            stopped,
        })
    }
}

fn overlapping(words: &'static str) -> Vec<Moment> {
    streaming(words, false).0
}
fn streaming(words: &'static str, stop_on_speech: bool) -> (Vec<Moment>, usize) {
    streaming_with(vec![words], stop_on_speech, false)
}
fn streaming_with(
    words: Vec<&'static str>,
    stop_on_speech: bool,
    delayed: bool,
) -> (Vec<Moment>, usize) {
    let mut script = vec!["Archie, qué hora es?"];
    script.extend(words.iter().copied());
    let recognizer: Box<dyn SpeechRecognizer> = if delayed {
        Box::new(DelayedRecognizer(script))
    } else {
        Box::new(Recognizer(script))
    };
    let (audio_in, audio) = mpsc::channel();
    let (began, voice_began) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        for _ in 0..8 {
            audio_in.send(vec![0.1; 320]).unwrap();
        }
        for _ in 0..40 {
            audio_in.send(vec![0.0; 320]).unwrap();
        }
        voice_began
            .recv_timeout(Duration::from_secs(5))
            .expect("the first answer starts");
        if stop_on_speech {
            return;
        }
        for _ in words {
            for _ in 0..if delayed { 8 } else { 20 } {
                audio_in.send(vec![0.1; 320]).unwrap();
            }
            for _ in 0..8 {
                audio_in.send(vec![0.0; 320]).unwrap();
            }
        }
        for _ in 0..120 {
            audio_in.send(vec![0.0; 320]).unwrap();
        }
    });
    let plan = SessionPlan {
        language: "es".into(),
        listener: SpeechCapabilities {
            partial_transcript: true,
            languages: vec!["es".into()],
            ..Default::default()
        },
        voice: SpeechCapabilities {
            accepts_external_response_text: true,
            renders_external_response_verbatim: true,
            languages: vec!["es".into()],
            ..Default::default()
        },
    };
    let mut session = SpeechSession::new(
        plan,
        FloorPolicy::default(),
        NemotronCognitive::new(Model, "replay", vec!["es".into()], vec![]),
    )
    .unwrap();
    let (out, playback) = mpsc::channel();
    let drain = std::thread::spawn(move || {
        let mut samples = 0;
        while let Ok(event) = playback.recv() {
            if let archie_voice::conversation::VoiceOut::Audio(pcm) = event {
                samples += pcm.len();
            }
        }
        samples
    });
    let mut moments = Vec::new();
    let stop = Arc::new(AtomicBool::new(false));
    converse(
        &Setup {
            headphones: true,
            ..Setup::default()
        },
        recognizer.as_ref(),
        None,
        Box::new(PacedVoice(began)),
        Hearing::Stream(audio),
        Saying::Stream(StreamedVoice::new(out, 16_000)),
        &mut session,
        &mut |_| panic!("no tool was proposed"),
        &mut String::new,
        stop.clone(),
        &mut |moment| {
            if stop_on_speech && matches!(moment, Moment::Speaking { .. }) {
                stop.store(true, Ordering::SeqCst);
            }
            moments.push(moment);
        },
    )
    .unwrap();
    writer.join().unwrap();
    let samples = drain.join().unwrap();
    (moments, samples)
}

#[test]
fn an_overlapping_acknowledgement_does_not_cancel_playback() {
    let moments = overlapping("mhm");
    assert!(!moments.contains(&Moment::Interrupted));
    assert!(moments
        .iter()
        .any(|moment| matches!(moment, Moment::Said { stopped: false, .. })));
}

#[test]
fn a_final_correction_during_old_audio_is_answered_after_yielding() {
    let moments = overlapping("Archie, no, revisa la otra cuenta.");
    assert!(moments.contains(&Moment::Interrupted));
    let answered = moments
        .iter()
        .filter(|moment| matches!(moment, Moment::Floor { action, .. } if action == "TakeFloor"))
        .count();
    assert_eq!(
        answered, 2,
        "the correction cannot disappear while stale audio drains"
    );
}

#[test]
fn shutdown_cancels_in_flight_speech_instead_of_playing_the_entire_queue() {
    let (_, samples) = streaming("unused", true);
    assert!(
        samples < 20_480,
        "shutdown let the full 1.28 s response play out"
    );
}

#[test]
fn repeated_acknowledgements_do_not_start_another_answer() {
    let moments = streaming_with(vec!["mhm", "mhm"], false, false).0;
    assert!(!moments.contains(&Moment::Interrupted));
    assert_eq!(
        moments
            .iter()
            .filter(
                |moment| matches!(moment, Moment::Floor { action, .. } if action == "TakeFloor")
            )
            .count(),
        1
    );
}

#[test]
fn a_short_correction_recognized_after_voicing_stops_still_interrupts() {
    let moments = streaming_with(vec!["Archie, no, revisa la otra cuenta."], false, true).0;
    assert!(moments.contains(&Moment::Interrupted));
    assert_eq!(
        moments
            .iter()
            .filter(
                |moment| matches!(moment, Moment::Floor { action, .. } if action == "TakeFloor")
            )
            .count(),
        2
    );
}

struct SilentModel;
impl TurnSource for SilentModel {
    fn start(&mut self, _: ToolChatRequest) -> TurnStream {
        panic!("a notice is spoken verbatim and never sent to the cognitive model");
    }
}

fn replay_notices(
    board_path: std::path::PathBuf,
    voice: Box<dyn Speaks>,
) -> (Vec<Moment>, Result<(), String>) {
    let plan = SessionPlan {
        language: "es".into(),
        listener: SpeechCapabilities {
            partial_transcript: true,
            languages: vec!["es".into()],
            ..Default::default()
        },
        voice: SpeechCapabilities {
            accepts_external_response_text: true,
            renders_external_response_verbatim: true,
            languages: vec!["es".into()],
            ..Default::default()
        },
    };
    let mut session = SpeechSession::new(
        plan,
        FloorPolicy::default(),
        NemotronCognitive::new(SilentModel, "replay", vec!["es".into()], vec![]),
    )
    .unwrap();
    static NEXT_NOTICE: AtomicU64 = AtomicU64::new(0);
    let wav_path = std::env::temp_dir().join(format!(
        "archie-floor-notice-{}-{}.wav",
        std::process::id(),
        NEXT_NOTICE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut moments = Vec::new();
    let result = converse(
        &Setup {
            notices: Some(board_path),
            ..Setup::default()
        },
        &Recognizer(vec![]),
        None,
        voice,
        Hearing::Recording {
            samples: vec![0.0; 16_000],
            rate: 16_000,
            at: 0,
            silence_left: 16_000,
        },
        Saying::Recording {
            path: wav_path.clone(),
            samples: vec![],
            rate: 16_000,
        },
        &mut session,
        &mut |_| panic!("no tool was proposed"),
        &mut String::new,
        Arc::new(AtomicBool::new(false)),
        &mut |moment| moments.push(moment),
    );
    let _ = std::fs::remove_file(wav_path);
    (moments, result)
}

#[test]
fn unspoken_notices_are_said_in_severity_order_during_silence_and_never_leak_detail() {
    use archie_notices::{Board, Policy, Raised, Severity};

    let dir = std::env::temp_dir().join(format!("archie-voice-notices-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notices.json");
    let policy = Policy {
        min_severity: Severity::Notice,
    };
    Board::update(&path, |board| {
        board.post(
            &Raised {
                at_ms: 1,
                severity: Severity::Warning,
                rule: "dns-tunnel-suspect",
                device: "de:ad:be:ef:00:01",
                detail: "secret-clinic.example queried by laptop-personal",
            },
            policy,
        );
        board.post(
            &Raised {
                at_ms: 2,
                severity: Severity::High,
                rule: "address-moved",
                device: "de:ad:be:ef:00:02",
                detail: "192.168.1.1 claimed by rogue-laptop",
            },
            policy,
        );
        board.post(
            &Raised {
                at_ms: 3,
                severity: Severity::High,
                rule: "port-scan",
                device: "de:ad:be:ef:00:03",
                detail: "already acknowledged scan",
            },
            policy,
        );
        let ack_id = board.notices.last().unwrap().id.clone();
        board.acknowledge(&ack_id, 10);
        true
    })
    .unwrap();

    let (moments, result) = replay_notices(path.clone(), Box::new(Voice));
    result.unwrap();
    let said: Vec<String> = moments
        .iter()
        .filter_map(|moment| match moment {
            Moment::Said {
                text,
                stopped: false,
                ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        said,
        vec![
            archie_notices::spoken_for("address-moved", Severity::High).to_string(),
            archie_notices::spoken_for("dns-tunnel-suspect", Severity::Warning).to_string(),
        ],
        "the voice speaks unacknowledged notices most serious first, using only fixed spoken text"
    );
    for text in &said {
        assert!(!text.contains("secret-clinic"));
        assert!(!text.contains("rogue-laptop"));
        assert!(!text.contains("192.168.1.1"));
        assert!(!text.contains("de:ad"));
    }

    let board = Board::load(&path).unwrap();
    assert!(
        board.unspoken().is_empty(),
        "both pending notices are marked spoken on disk"
    );
    assert_eq!(
        board.pending().count(),
        2,
        "speaking a notice does not acknowledge it"
    );

    // A second pass over the same board says nothing twice.
    let (again, result) = replay_notices(path, Box::new(Voice));
    result.unwrap();
    assert!(
        !again
            .iter()
            .any(|moment| matches!(moment, Moment::Said { .. })),
        "an already spoken notice is never repeated"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_synthesis_leaves_a_notice_unspoken() {
    use archie_notices::{Board, Policy, Raised, Severity};

    let dir =
        std::env::temp_dir().join(format!("archie-voice-notices-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notices.json");
    Board::update(&path, |board| {
        board.post(
            &Raised {
                at_ms: 1,
                severity: Severity::High,
                rule: "address-moved",
                device: "de:ad:be:ef:00:02",
                detail: "192.168.1.1 claimed",
            },
            Policy::default(),
        )
    })
    .unwrap();

    let (_moments, result) = replay_notices(path.clone(), Box::new(FailedVoice));
    assert!(result.is_err());
    let board = Board::load(&path).unwrap();
    assert_eq!(
        board.unspoken().len(),
        1,
        "a notice whose synthesis failed must stay unspoken"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_drawings_narration_is_said_in_the_quiet_shown_as_it_is_said_and_never_sent_to_the_model() {
    use archie_voice::draw::{Lang, Said as Line};
    use archie_voice::narration::Narration;

    let dir = std::env::temp_dir().join(format!("archie-voice-narration-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let subtitles = dir.join("narration.json");
    let narration = Narration::new(Some(subtitles.clone()));
    let line = |text: &str, progress: bool| Line {
        text: text.into(),
        progress,
        lang: Lang::En,
    };
    narration.push(line("Opening Photopea.", false));
    narration.push(line("A quarter done.", true));
    narration.push(line("Halfway.", true));

    let plan = SessionPlan {
        language: "en".into(),
        listener: SpeechCapabilities {
            partial_transcript: true,
            languages: vec!["en".into()],
            ..Default::default()
        },
        voice: SpeechCapabilities {
            accepts_external_response_text: true,
            renders_external_response_verbatim: true,
            languages: vec!["en".into()],
            ..Default::default()
        },
    };
    let mut session = SpeechSession::new(
        plan,
        FloorPolicy::default(),
        NemotronCognitive::new(SilentModel, "replay", vec!["en".into()], vec![]),
    )
    .unwrap();
    let wav_path = dir.join("said.wav");
    let mut moments = Vec::new();
    converse(
        &Setup {
            narration: Some(narration.clone()),
            ..Setup::default()
        },
        &Recognizer(vec![]),
        None,
        Box::new(Voice),
        Hearing::Recording {
            samples: vec![0.0; 16_000],
            rate: 16_000,
            at: 0,
            silence_left: 16_000,
        },
        Saying::Recording {
            path: wav_path,
            samples: vec![],
            rate: 16_000,
        },
        &mut session,
        &mut |_| panic!("no tool was proposed"),
        &mut String::new,
        Arc::new(AtomicBool::new(false)),
        &mut |moment| moments.push(moment),
    )
    .unwrap();
    let said: Vec<String> = moments
        .iter()
        .filter_map(|moment| match moment {
            Moment::Said {
                text,
                stopped: false,
                ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        said,
        ["Opening Photopea.", "Halfway."],
        "every milestone, and only the progress that is still true"
    );
    assert!(narration.is_empty());
    let shown: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&subtitles).unwrap()).unwrap();
    assert_eq!(
        shown["text"], "Halfway.",
        "the subtitle is the line said last"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
