//! A spoken conversation: the ears, the floor, the model, the hands and the voice, in one
//! loop.
//!
//! ```text
//!  microphone ─► ears (Nemotron 3.5 ASR) ─► floor ─► Nemotron on Nebius ─► voice (Magpie) ─► speaker
//!                                               │            │
//!                                               │            └─► proposals ─► decide ─► hands ─┐
//!                                               └───────────────── results told back ◄─────────┘
//! ```
//!
//! The loop owns the listening and the deciding; the voice runs on its own thread, because
//! making speech takes longer than hearing it and the ears must not go deaf meanwhile.
//! Three things the loop is careful about:
//!
//! - **It answers what was said to it.** Whether to speak is the speech plane's
//!   [`FloorPolicy`](archie_speech::FloorPolicy), fed by [`crate::listen`]: an utterance
//!   that neither names Archie nor answers it is heard and left alone.
//! - **It does not hear itself.** Without echo cancellation a microphone near a speaker
//!   hears the voice, and a voice that answers itself never stops. Unless the person says
//!   they wear headphones, the ears are closed while Archie speaks.
//! - **It stops when spoken over** — with headphones, where it can tell. The queued audio
//!   is dropped, the turn is cancelled as an interruption, and what was not said stays in
//!   the commit log, unsaid.

use crate::actions::{ActionDispatcher, InlineActions, PendingAction};
use crate::asr::SpeechRecognizer;
use crate::audio::{loudness, AudioError, VoiceActivity};
#[cfg(feature = "pulse")]
use crate::audio::{Microphone, Speaker};
use crate::listen::{
    after_utterance_within, is_acknowledgement, names_archie, over_archie, Utterance,
};
use crate::nemotron::{NemotronCognitive, TurnSource};
use crate::voice::Speaks;
use crate::wav::{read_wav, write_wav};
use archie_orb::{publish, semantic_features, signal, SignalKind};
use archie_speech::{
    FloorAction, SessionEvent, SpeechBudget, SpeechSession, SpeechToolIntentCandidate,
};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Where the sound comes from.
pub enum Hearing {
    #[cfg(feature = "pulse")]
    Microphone(Microphone),
    /// Sound streamed from elsewhere — a browser page — as 16 kHz mono frames. The
    /// conversation ends when the sender is dropped.
    Stream(Receiver<Vec<f32>>),
    /// A recording, played in as if it were the room, then silence long enough for the
    /// conversation that follows it to finish.
    Recording {
        samples: Vec<f32>,
        rate: u32,
        at: usize,
        silence_left: usize,
    },
}

impl Hearing {
    /// A mono 16-bit PCM WAV file, after a second of quiet — a room is quiet before
    /// somebody speaks, and a streaming recogniser given words at its very first frame
    /// loses them.
    pub fn recording(path: &std::path::Path, then_silence: Duration) -> Result<Self, String> {
        let (spoken, rate) = read_wav(path)?;
        let mut samples = vec![0.0; rate as usize];
        samples.extend(spoken);
        Ok(Self::Recording {
            samples,
            rate,
            at: 0,
            silence_left: (then_silence.as_secs_f32() * rate as f32) as usize,
        })
    }

    /// The next 20 ms of sound and its rate, or `None` when a recording has run out.
    fn frame(&mut self) -> Result<Option<(Vec<f32>, u32)>, AudioError> {
        match self {
            #[cfg(feature = "pulse")]
            Self::Microphone(microphone) => Ok(Some((
                microphone.frame()?.to_vec(),
                crate::audio::CAPTURE_RATE,
            ))),
            // A network stalls; the loop must not. An empty frame keeps the voice and the
            // turn moving while no sound arrives, and advances no audio time.
            Self::Stream(frames) => match frames.recv_timeout(Duration::from_millis(40)) {
                Ok(frame) => {
                    // Reject malformed input before it enters VAD or the native ASR.
                    // Network chunks may vary, but one packet is at most one second.
                    if frame.len() > crate::audio::CAPTURE_RATE as usize
                        || frame.iter().any(|s| !s.is_finite() || s.abs() > 1.0)
                    {
                        return Err(AudioError::new(
                            "reading streamed audio",
                            "invalid PCM or chunk exceeds one second",
                        ));
                    }
                    Ok(Some((frame, crate::audio::CAPTURE_RATE)))
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    Ok(Some((Vec::new(), crate::audio::CAPTURE_RATE)))
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
            },
            Self::Recording {
                samples,
                rate,
                at,
                silence_left,
            } => {
                let step = (*rate / 50) as usize;
                if *at < samples.len() {
                    let end = (*at + step).min(samples.len());
                    let frame = samples[*at..end].to_vec();
                    *at = end;
                    return Ok(Some((frame, *rate)));
                }
                if *silence_left == 0 {
                    return Ok(None);
                }
                let n = step.min(*silence_left);
                *silence_left -= n;
                Ok(Some((vec![0.0; n], *rate)))
            }
        }
    }

    fn is_live(&self) -> bool {
        !matches!(self, Self::Recording { .. })
    }
}

/// What a streamed voice sends: audio to play, or "drop everything queued".
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceOut {
    Audio(Vec<i16>),
    Hush,
}

/// A voice sent over a stream, paced like a speaker: synthesis may run at most a quarter
/// of a second ahead of what has been heard, so the loop knows when Archie is still
/// talking and the floor can hand the turn back at the right moment.
pub struct StreamedVoice {
    out: Sender<VoiceOut>,
    rate: u32,
    since: Option<Instant>,
    sent: u64,
}

impl StreamedVoice {
    pub fn new(out: Sender<VoiceOut>, rate: u32) -> Self {
        Self {
            out,
            rate: rate.max(1),
            since: None,
            sent: 0,
        }
    }

    fn ahead(&self) -> Duration {
        let Some(since) = self.since else {
            return Duration::ZERO;
        };
        Duration::from_secs_f64(self.sent as f64 / self.rate as f64).saturating_sub(since.elapsed())
    }

    fn play(&mut self, pcm: &[i16]) -> Result<(), AudioError> {
        self.since.get_or_insert_with(Instant::now);
        self.out
            .send(VoiceOut::Audio(pcm.to_vec()))
            .map_err(|_| AudioError::new("playing the voice", "the listener is gone"))?;
        self.sent += pcm.len() as u64;
        let ahead = self.ahead();
        if ahead > Duration::from_millis(250) {
            std::thread::sleep(ahead - Duration::from_millis(250));
        }
        Ok(())
    }

    fn settle(&mut self, current: &AtomicU64, generation: u64) -> bool {
        while self.ahead() > Duration::ZERO {
            if current.load(Ordering::SeqCst) != generation {
                self.hush();
                return false;
            }
            std::thread::sleep(self.ahead().min(Duration::from_millis(20)));
        }
        self.since = None;
        self.sent = 0;
        true
    }

    fn hush(&mut self) {
        let _ = self.out.send(VoiceOut::Hush);
        self.since = None;
        self.sent = 0;
    }
}

/// Where the voice goes.
pub enum Saying {
    #[cfg(feature = "pulse")]
    Speaker(Speaker),
    Stream(StreamedVoice),
    Recording {
        path: PathBuf,
        samples: Vec<i16>,
        rate: u32,
    },
}

impl Saying {
    fn play(&mut self, pcm: &[i16]) -> Result<(), AudioError> {
        match self {
            #[cfg(feature = "pulse")]
            Self::Speaker(speaker) => speaker.play(pcm),
            Self::Stream(voice) => voice.play(pcm),
            Self::Recording { samples, .. } => {
                samples.extend_from_slice(pcm);
                Ok(())
            }
        }
    }

    fn settle(&mut self, current: &AtomicU64, generation: u64) -> Result<bool, AudioError> {
        if current.load(Ordering::SeqCst) != generation {
            self.hush();
            return Ok(false);
        }
        match self {
            #[cfg(feature = "pulse")]
            Self::Speaker(speaker) => {
                speaker.drain()?;
            }
            Self::Stream(voice) => return Ok(voice.settle(current, generation)),
            Self::Recording {
                path,
                samples,
                rate,
            } => {
                write_wav(path, samples, *rate)
                    .map_err(|error| AudioError::new("saving the voice", error.to_string()))?;
            }
        }
        Ok(current.load(Ordering::SeqCst) == generation)
    }

    fn hush(&mut self) {
        match self {
            #[cfg(feature = "pulse")]
            Self::Speaker(speaker) => {
                let _ = speaker.stop();
            }
            Self::Stream(voice) => voice.hush(),
            Self::Recording { .. } => {}
        }
    }
}

/// How a conversation is set up.
pub struct Setup {
    /// The locale the ears transcribe (`es-US`, `es-ES`, or `auto`).
    pub listen_language: String,
    /// Answer without being named. For a person alone with their computer.
    pub always: bool,
    /// The person wears headphones: the ears stay open while Archie speaks, so being
    /// spoken over can stop it.
    pub headphones: bool,
    /// How long a silence after words ends the utterance, when the ears have not.
    pub end_after: Duration,
    /// How long after Archie spoke a reply needs no name.
    pub follow_up: Duration,
    /// The most steps one utterance may take through tools.
    pub steps: usize,
    /// Words the ears should favour, and how strongly: the name first of all.
    pub phrases: Vec<String>,
    pub phrase_boost: f32,
    /// Where Sentry's owner notice board (`notices.json`) lives, if the voice should
    /// speak pending unspoken notices when the room is quiet.
    pub notices: Option<PathBuf>,
    /// What Archie tells on its own while it works (a drawing under way), said when the
    /// room is quiet, before any notice.
    pub narration: Option<crate::narration::Narration>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            listen_language: "es-US".into(),
            always: false,
            headphones: false,
            end_after: Duration::from_millis(800),
            follow_up: crate::listen::FOLLOW_UP_WINDOW,
            steps: 6,
            phrases: vec!["Archie".into()],
            // As `--name-boost`: more than ~2 makes a boosting GGUF hear the phrases anywhere.
            phrase_boost: 1.5,
            notices: None,
            narration: None,
        }
    }
}

/// What the loop reports as it goes: printed by the command line, kept by a test.
#[derive(Debug, Clone, PartialEq)]
pub enum Moment {
    /// Monotonic time since this turn was admitted, not an audible latency claim.
    Timing {
        turn_id: String,
        phase: &'static str,
        elapsed_ms: u128,
        queued_segments: usize,
    },
    Heard {
        text: String,
        at_ms: u128,
    },
    Floor {
        action: String,
        reason: String,
        evidence: Vec<String>,
    },
    Said {
        text: String,
        first_audio_ms: f64,
        audio_s: f64,
        stopped: bool,
    },
    Proposed {
        tool: String,
        arguments: String,
    },
    Did {
        report: String,
    },
    Interrupted,
    Alert(String),
    /// About ten times a second: how loud the room is, and whether that is a voice.
    Level {
        level: f32,
        voiced: bool,
    },
    /// The voice began saying this.
    Speaking {
        text: String,
    },
    /// Nothing is being answered or said: back to listening.
    Ready,
}

enum ToVoice {
    /// Said only while `generation` is still current.
    Say {
        generation: u64,
        turn_id: String,
        sequence: u32,
        text: String,
        semantic_features: Vec<f32>,
    },
    /// Everything queued so far is dropped.
    Hush,
    Finish,
}

enum FromVoice {
    Began {
        text: String,
    },
    Spoke {
        turn_id: String,
        sequence: u32,
        text: String,
        first_audio_ms: f64,
        audio_s: f64,
        stopped: bool,
    },
    Failed(String),
}

/// Runs the conversation until a recording runs out, or `stop` is set.
///
/// `act` carries out one proposal and says what came of it; `context` says what the model
/// should know right now (the time, the application in front). `rehear`, when given, is an
/// offline model (Parakeet TDT) that transcribes each finished utterance again from its
/// audio, whole: the streaming ears decide when the person stopped, and the second
/// hearing decides what they said.
#[allow(clippy::too_many_arguments)]
pub fn converse<S: TurnSource>(
    setup: &Setup,
    ears: &dyn SpeechRecognizer,
    rehear: Option<&dyn SpeechRecognizer>,
    voice: Box<dyn Speaks>,
    hearing: Hearing,
    saying: Saying,
    session: &mut SpeechSession<NemotronCognitive<S>>,
    act: &mut dyn FnMut(&SpeechToolIntentCandidate) -> String,
    context: &mut dyn FnMut() -> String,
    stop: Arc<AtomicBool>,
    report: &mut dyn FnMut(Moment),
) -> Result<(), String> {
    converse_with_actions(
        setup,
        ears,
        rehear,
        voice,
        hearing,
        saying,
        session,
        &mut InlineActions(act),
        context,
        stop,
        report,
    )
}

/// Interactive entry point: pending tool futures do not block audio or floor decisions.
/// A superseded turn drops its pending future and its queued proposals together.
#[allow(clippy::too_many_arguments)]
pub fn converse_with_actions<S: TurnSource>(
    setup: &Setup,
    ears: &dyn SpeechRecognizer,
    rehear: Option<&dyn SpeechRecognizer>,
    voice: Box<dyn Speaks>,
    mut hearing: Hearing,
    saying: Saying,
    session: &mut SpeechSession<NemotronCognitive<S>>,
    act: &mut dyn ActionDispatcher,
    context: &mut dyn FnMut() -> String,
    stop: Arc<AtomicBool>,
    report: &mut dyn FnMut(Moment),
) -> Result<(), String> {
    let started = Instant::now();
    let phrases: Vec<&str> = setup.phrases.iter().map(String::as_str).collect();
    let mut listening = ears
        .listen(&setup.listen_language, &phrases, setup.phrase_boost)
        .map_err(|e| e.to_string())?;
    // Bumped on every interruption: audio of an older generation is never played.
    let generation = Arc::new(AtomicU64::new(0));
    let (to_voice, voice_inbox) = mpsc::channel::<ToVoice>();
    let (voice_outbox, from_voice) = mpsc::channel::<FromVoice>();
    let voice_thread = {
        let generation = generation.clone();
        std::thread::Builder::new()
            .name("archie-voice".into())
            .spawn(move || speak(voice, saying, voice_inbox, voice_outbox, generation))
            .map_err(|error| error.to_string())?
    };

    publish(&signal(SignalKind::Listen {
        active: true,
        audio_envelope: 0.0,
    }));
    let mut activity = VoiceActivity::default();
    let mut partial = String::new();
    let mut audio_ms: u64 = 0;
    let mut last_words_ms: u64 = 0;
    let mut speaking = 0usize; // segments given to the voice and not yet reported
    let mut last_spoke: Option<Instant> = None;
    let mut turn_number = 0u32;
    let mut turn: Option<Answering> = None;
    let mut deferred: Option<PendingUtterance> = None;
    let mut forced = false;
    let mut frames = 0u64;
    let mut was_busy = false;
    let mut overlap_ms = 0u32;
    let mut overlap_detected = false;
    let mut yielded = false;
    let mut overlap_words = String::new();
    let trace = std::env::var_os("ARCHIE_VOICE_TRACE").is_some();
    // Audio goes to the ears a model step at a time: asking after every 20 ms frame made
    // the same recogniser three times slower than the runtime's own benchmark.
    let mut pending: Vec<f32> = Vec::new();
    // The sound of the utterance under way, for the second hearing: a few seconds of what
    // came before the first word (the streaming ears are late to notice it), then all of it.
    let mut utterance: Vec<f32> = Vec::new();
    let mut notice_alerted = false;
    let mut narrated = 0_u64;

    let result = (|| -> Result<(), String> {
        while !stop.load(Ordering::Relaxed) {
            let Some((frame, rate)) = hearing.frame().map_err(|e| e.to_string())? else {
                // A recording ran out: let what is under way finish, then stop.
                if turn.is_none() && speaking == 0 {
                    if partial.is_empty() {
                        turn = begin_pending(
                            &mut deferred,
                            setup,
                            session,
                            activity.quiet_ms(),
                            last_spoke,
                            &mut turn_number,
                            context,
                            report,
                        )?;
                    }
                    if turn.is_none()
                        && partial.is_empty()
                        && deferred.is_none()
                        && activity.quiet_ms() as u64 >= setup.end_after.as_millis() as u64
                    {
                        let _ = speak_next_narration(
                            setup.narration.as_ref(),
                            &to_voice,
                            &generation,
                            &mut speaking,
                            &mut narrated,
                        ) || speak_next_notice(
                            setup.notices.as_deref(),
                            &to_voice,
                            &generation,
                            &mut speaking,
                            &mut notice_alerted,
                            report,
                        );
                    }
                    if turn.is_none() && speaking == 0 {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
                pump_voice(
                    &from_voice,
                    session,
                    setup.notices.as_deref(),
                    &mut speaking,
                    &mut last_spoke,
                    report,
                )?;
                advance(
                    session,
                    &mut turn,
                    setup,
                    act,
                    context,
                    &to_voice,
                    &generation,
                    &mut speaking,
                    report,
                )?;
                if turn.is_none() && speaking == 0 && partial.is_empty() {
                    turn = begin_pending(
                        &mut deferred,
                        setup,
                        session,
                        activity.quiet_ms(),
                        last_spoke,
                        &mut turn_number,
                        context,
                        report,
                    )?;
                }
                if turn.is_none()
                    && speaking == 0
                    && partial.is_empty()
                    && deferred.is_none()
                    && activity.quiet_ms() as u64 >= setup.end_after.as_millis() as u64
                {
                    let _ = speak_next_narration(
                        setup.narration.as_ref(),
                        &to_voice,
                        &generation,
                        &mut speaking,
                        &mut narrated,
                    ) || speak_next_notice(
                        setup.notices.as_deref(),
                        &to_voice,
                        &generation,
                        &mut speaking,
                        &mut notice_alerted,
                        report,
                    );
                }
                continue;
            };
            audio_ms += (frame.len() as u64 * 1000) / rate.max(1) as u64;
            let level = loudness(&frame);
            // An empty frame is a stream that has sent nothing yet, not a quiet room.
            let voiced = activity.hear_audio(&frame, rate);
            frames += 1;
            if frames % 5 == 0 {
                report(Moment::Level { level, voiced });
            }

            let archie_talking = speaking > 0;
            if !archie_talking || activity.quiet_ms() >= 300 {
                overlap_words.clear();
            }
            // Closed ears while the voice plays through a speaker: otherwise it hears itself.
            if !archie_talking || setup.headphones {
                pending.extend_from_slice(&frame);
                if rehear.is_some() {
                    utterance.extend_from_slice(&frame);
                    let keep = if partial.is_empty() { 3 } else { 30 } * rate as usize;
                    if utterance.len() > keep {
                        utterance.drain(..utterance.len() - keep);
                    }
                }
            }
            let step = (rate as usize * 160) / 1000;
            let ready = pending.len() >= step;
            if ready {
                listening.push(&pending, rate).map_err(|e| e.to_string())?;
                pending.clear();
            }

            // What the ears have made of it.
            while let Some(heard) = if ready || forced {
                listening.next().map_err(|e| e.to_string())?
            } else {
                None
            } {
                if trace && (heard.is_final || heard.text.trim() != partial) {
                    eprintln!(
                        "[ears {audio_ms} ms] {} {:?}",
                        if heard.is_final { "final  " } else { "interim" },
                        heard.text
                    );
                }
                let text = heard.text.trim().to_string();
                if archie_talking && !text.is_empty() {
                    overlap_words = text.clone();
                }
                if text.len() > 16 * 1024 {
                    partial.clear();
                    deferred = None;
                    report(Moment::Alert(
                        "ASR transcript exceeds the conversation limit".into(),
                    ));
                    continue;
                }
                if !text.is_empty() && text != partial {
                    partial = text.clone();
                    last_words_ms = audio_ms;
                }
                if heard.is_final && !partial.is_empty() {
                    let streamed = tidy(&std::mem::take(&mut partial));
                    let sound = std::mem::take(&mut utterance);
                    let said = match rehear {
                        Some(rehear) if sound.len() >= rate as usize / 4 => {
                            match rehear.recognize(&sound, rate) {
                                Ok(again) if !again.trim().is_empty() => {
                                    if trace {
                                        eprintln!("[rehear] {again:?} (streamed {streamed:?})");
                                    }
                                    tidy(&again)
                                }
                                Ok(_) => streamed.clone(),
                                Err(error) => {
                                    report(Moment::Alert(format!("second hearing: {error}")));
                                    streamed.clone()
                                }
                            }
                        }
                        _ => streamed.clone(),
                    };
                    report(Moment::Heard {
                        text: said.clone(),
                        at_ms: started.elapsed().as_millis(),
                    });
                    forced = false;
                    // Each completed acknowledgement stands alone, even while playback
                    // prevents another utterance from being admitted to the floor.
                    if is_acknowledgement(&said) {
                        continue;
                    }
                    // A named correction while the model/tool is thinking has no audio
                    // to overlap. It still supersedes the old turn immediately.
                    let named_now = names_archie(&streamed) || names_archie(&said);
                    if turn.is_some() && speaking == 0 && (named_now || setup.always) {
                        if let Some(old) = &turn {
                            report(old.timing("cancelled", speaking));
                        }
                        session.cancel(archie_speech::CancelReason::Superseded);
                        generation.fetch_add(1, Ordering::SeqCst);
                        turn = None; // abort owned future; queued proposals never start
                        report(Moment::Interrupted);
                    }
                    let previous = deferred.take();
                    let named =
                        named_now || previous.as_ref().is_some_and(|utterance| utterance.named);
                    let joined = match previous {
                        Some(previous) => format!("{} {said}", previous.text),
                        None => said,
                    };
                    if joined.len() <= 16 * 1024 {
                        deferred = Some(PendingUtterance {
                            text: joined,
                            named,
                            last_action: None,
                        });
                    } else {
                        report(Moment::Alert(
                            "unfinished utterance exceeds the conversation limit".into(),
                        ));
                    }
                }
            }
            // Give streaming ASR one bounded interval to distinguish a listening noise
            // from a correction. Never repeatedly cancel the same playback generation.
            if archie_talking && setup.headphones && voiced {
                overlap_detected = true;
                overlap_ms = overlap_ms
                    .saturating_add((frame.len() as u64 * 1000 / rate.max(1) as u64) as u32);
            } else {
                overlap_ms = 0;
            }
            if !archie_talking {
                overlap_detected = false;
                yielded = false;
            }
            // ASR may finish after the overlapping sound has ended. The acoustic
            // evidence belongs to this playback, not only to the current frame.
            if archie_talking
                && setup.headphones
                && overlap_detected
                && !yielded
                && (!overlap_words.trim().is_empty() || overlap_ms >= 320)
            {
                let decision = session.floor_moved(&over_archie(&overlap_words));
                if decision.action == FloorAction::Yield {
                    yielded = true;
                    generation.fetch_add(1, Ordering::SeqCst);
                    let _ = to_voice.send(ToVoice::Hush);
                    report(Moment::Interrupted);
                    publish(&signal(SignalKind::Think {
                        active: false,
                        trace_id: format!("voz-{turn_number}"),
                        working_set: BTreeMap::new(),
                    }));
                    if let Some(old) = &turn {
                        report(old.timing("cancelled", speaking));
                    }
                    turn = None;
                }
            }
            // The ears did not call it, but the person has plainly stopped.
            if !forced
                && !partial.is_empty()
                && activity.quiet_ms() as u64 >= setup.end_after.as_millis() as u64
                && audio_ms.saturating_sub(last_words_ms) >= setup.end_after.as_millis() as u64
            {
                listening.end_utterance().map_err(|e| e.to_string())?;
                forced = true;
            }

            pump_voice(
                &from_voice,
                session,
                setup.notices.as_deref(),
                &mut speaking,
                &mut last_spoke,
                report,
            )?;
            if turn.is_none() && speaking == 0 && partial.is_empty() {
                turn = begin_pending(
                    &mut deferred,
                    setup,
                    session,
                    activity.quiet_ms(),
                    last_spoke,
                    &mut turn_number,
                    context,
                    report,
                )?;
            }
            if turn.is_none()
                && speaking == 0
                && partial.is_empty()
                && deferred.is_none()
                && activity.quiet_ms() as u64 >= setup.end_after.as_millis() as u64
            {
                let _ = speak_next_narration(
                    setup.narration.as_ref(),
                    &to_voice,
                    &generation,
                    &mut speaking,
                    &mut narrated,
                ) || speak_next_notice(
                    setup.notices.as_deref(),
                    &to_voice,
                    &generation,
                    &mut speaking,
                    &mut notice_alerted,
                    report,
                );
            }
            advance(
                session,
                &mut turn,
                setup,
                act,
                context,
                &to_voice,
                &generation,
                &mut speaking,
                report,
            )?;
            let busy = turn.is_some() || speaking > 0;
            if was_busy && !busy {
                report(Moment::Ready);
            }
            was_busy = busy;

            if !hearing.is_live() {
                // A recording is read faster than real time; the voice thread still gets
                // the processor it needs.
                std::thread::yield_now();
            }
        }
        Ok(())
    })();

    if stop.load(Ordering::Relaxed) || result.is_err() {
        if let Some(old) = &turn {
            report(old.timing("cancelled", speaking));
        }
        generation.fetch_add(1, Ordering::SeqCst);
        session.cancel(archie_speech::CancelReason::Shutdown);
        drop(turn.take());
        let _ = to_voice.send(ToVoice::Hush);
    }
    let _ = to_voice.send(ToVoice::Finish);
    let joined = voice_thread.join();
    publish(&signal(SignalKind::Listen {
        active: false,
        audio_envelope: 0.0,
    }));
    publish(&signal(SignalKind::Think {
        active: false,
        trace_id: format!("voz-{turn_number}"),
        working_set: BTreeMap::new(),
    }));
    if joined.is_err() {
        return Err("the voice worker panicked".into());
    }
    result
}

/// The answer being given to one utterance.
struct Answering {
    id: String,
    heard: String,
    budget: SpeechBudget,
    steps: usize,
    results: Vec<(String, String)>,
    finished: bool,
    model_finished: bool,
    proposed: usize,
    queued: VecDeque<SpeechToolIntentCandidate>,
    action: Option<(String, PendingAction)>,
    started: Instant,
    first_segment: bool,
}

impl Answering {
    fn timing(&self, phase: &'static str, queued_segments: usize) -> Moment {
        Moment::Timing {
            turn_id: self.id.clone(),
            phase,
            elapsed_ms: self.started.elapsed().as_millis(),
            queued_segments,
        }
    }
}

struct PendingUtterance {
    text: String,
    named: bool,
    last_action: Option<FloorAction>,
}

#[allow(clippy::too_many_arguments)]
fn begin_pending<S: TurnSource>(
    pending: &mut Option<PendingUtterance>,
    setup: &Setup,
    session: &mut SpeechSession<NemotronCognitive<S>>,
    quiet_ms: u32,
    last_spoke: Option<Instant>,
    turn_number: &mut u32,
    context: &mut dyn FnMut() -> String,
    report: &mut dyn FnMut(Moment),
) -> Result<Option<Answering>, String> {
    let Some(utterance) = pending.as_mut() else {
        return Ok(None);
    };
    let mut state = after_utterance_within(
        &Utterance {
            text: utterance.text.clone(),
            final_by_recogniser: true,
            silence_ms: quiet_ms,
        },
        last_spoke.map(|at| at.elapsed()),
        setup.follow_up,
    );
    if utterance.named || setup.always {
        state.response_utility = archie_speech::Probability::new(0.9);
        state.evidence.push(
            if setup.always {
                "answering everything: --always"
            } else {
                "the streaming ears heard the name"
            }
            .into(),
        );
    }
    let decision = session.floor_decision(&state);
    let started = Instant::now();
    let id = format!("voz-{}", *turn_number + 1);
    let decision = if matches!(decision.budget, SpeechBudget::Sentence | SpeechBudget::Full) {
        session.cognitive_mut().set_context(context());
        session
            .heard(&id, &utterance.text, &state)
            .map_err(|error| error.to_string())?
    } else {
        decision
    };
    if utterance.last_action != Some(decision.action) {
        report(Moment::Floor {
            action: format!("{:?}", decision.action),
            reason: decision.reason.clone(),
            evidence: state.evidence.clone(),
        });
        utterance.last_action = Some(decision.action);
    }
    if !matches!(decision.budget, SpeechBudget::Sentence | SpeechBudget::Full) {
        if quiet_ms >= 300
            && state.speaker_hold.get() < 0.5
            && state.thought_completion.get() >= 0.5
        {
            *pending = None;
        }
        return Ok(None);
    }
    let utterance = pending.take().expect("pending utterance");
    *turn_number += 1;
    report(Moment::Timing {
        turn_id: id.clone(),
        phase: "request",
        elapsed_ms: started.elapsed().as_millis(),
        queued_segments: 0,
    });
    publish(&signal(SignalKind::Think {
        active: true,
        trace_id: id.clone(),
        working_set: BTreeMap::new(),
    }));
    Ok(Some(Answering {
        id,
        heard: utterance.text,
        budget: decision.budget,
        steps: 1,
        results: Vec::new(),
        finished: false,
        model_finished: false,
        proposed: 0,
        queued: VecDeque::new(),
        action: None,
        started,
        first_segment: false,
    }))
}

/// Moves the answer along: what the model says goes to the voice, what it proposes is
/// carried out, and when a step ends with proposals, the next step is asked for.
#[allow(clippy::too_many_arguments)]
fn advance<S: TurnSource>(
    session: &mut SpeechSession<NemotronCognitive<S>>,
    turn: &mut Option<Answering>,
    setup: &Setup,
    act: &mut dyn ActionDispatcher,
    context: &mut dyn FnMut() -> String,
    to_voice: &Sender<ToVoice>,
    generation: &AtomicU64,
    speaking: &mut usize,
    report: &mut dyn FnMut(Moment),
) -> Result<(), String> {
    let Some(answering) = turn.as_mut() else {
        return Ok(());
    };
    if answering.finished {
        if *speaking == 0 {
            report(answering.timing("complete", 0));
            let heard_from_archie = session.heard_text();
            let heard = answering.heard.clone();
            session.cognitive_mut().remember(&heard, &heard_from_archie);
            *turn = None;
        }
        return Ok(());
    }
    if let Some((id, pending)) = answering.action.as_mut() {
        if let Some(outcome) = pending.poll() {
            report(Moment::Did {
                report: outcome.clone(),
            });
            answering.results.push((id.clone(), outcome));
            answering.action = None;
            report(answering.timing("tool_finished", *speaking));
        }
    }
    if answering.action.is_none() {
        if let Some(intent) = answering.queued.pop_front() {
            let id = intent.correlation_id.clone();
            answering.action = Some((id, act.start(intent)));
            report(answering.timing("tool_started", *speaking));
        }
    }
    // At most two released segments may be queued/playing. Polling also has a
    // bounded work budget, so a fast model cannot monopolize the hearing loop.
    let capacity = 2usize.saturating_sub(*speaking);
    for event in session.pump_up_to(capacity) {
        match event {
            SessionEvent::Speak(segment) => {
                *speaking += 1;
                if !answering.first_segment {
                    answering.first_segment = true;
                    report(answering.timing("first_segment", *speaking));
                }
                let features = semantic_features(&segment.text);
                let _ = to_voice.send(ToVoice::Say {
                    generation: generation.load(Ordering::SeqCst),
                    turn_id: segment.turn_id,
                    sequence: segment.sequence,
                    text: segment.text,
                    semantic_features: features,
                });
            }
            SessionEvent::ToolProposal(intent) => {
                report(Moment::Proposed {
                    tool: intent.tool_name.clone(),
                    arguments: intent.raw_arguments.clone(),
                });
                answering.proposed += 1;
                if answering.proposed <= 16 {
                    answering.queued.push_back(intent);
                } else {
                    answering.results.push((
                        intent.correlation_id,
                        "No se hizo: se alcanzó el límite de herramientas de este turno.".into(),
                    ));
                }
            }
            SessionEvent::Withheld { .. } => {}
            SessionEvent::Alert(message) => {
                report(Moment::Alert(message));
                if session.turn_finished() {
                    finish_thinking(&answering.id);
                    answering.finished = true;
                    answering.queued.clear();
                    answering.action = None;
                    report(answering.timing("failed", *speaking));
                    return Ok(());
                }
            }
            SessionEvent::Finished => {
                answering.model_finished = true;
            }
        }
    }
    if answering.model_finished && answering.action.is_none() && answering.queued.is_empty() {
        if session.cognitive().awaiting_results() && answering.steps < setup.steps {
            answering.model_finished = false;
            answering.steps += 1;
            let results = std::mem::take(&mut answering.results);
            let budget = answering.budget;
            let id = answering.id.clone();
            let facts = context();
            let cognitive = session.cognitive_mut();
            cognitive.set_context(facts);
            cognitive
                .follow_up(id, &results, budget)
                .map_err(|error| error.to_string())?;
        } else {
            // The model is done; history is committed only after playback receipts.
            finish_thinking(&answering.id);
            answering.finished = true;
            return Ok(());
        }
    }
    Ok(())
}

fn finish_thinking(turn_id: &str) {
    publish(&signal(SignalKind::Think {
        active: false,
        trace_id: turn_id.to_string(),
        working_set: BTreeMap::new(),
    }));
}

/// If the room is quiet and nothing is under way, says the next line Archie tells on its
/// own about what it is doing — its own words, never what anyone said — and shows it as a
/// subtitle while it is said.
fn speak_next_narration(
    narration: Option<&crate::narration::Narration>,
    to_voice: &Sender<ToVoice>,
    generation: &AtomicU64,
    speaking: &mut usize,
    told: &mut u64,
) -> bool {
    let Some(said) = narration.and_then(|narration| narration.next()) else {
        return false;
    };
    if let Some(narration) = narration {
        narration.subtitle(&said.text, said.lang.code());
    }
    *told += 1;
    let features = semantic_features(&said.text);
    *speaking += 1;
    let _ = to_voice.send(ToVoice::Say {
        generation: generation.load(Ordering::SeqCst),
        turn_id: format!("narration:{told}"),
        sequence: 0,
        text: said.text,
        semantic_features: features,
    });
    true
}

/// If the room is quiet and nothing is under way, reads the next unspoken Sentry notice
/// (`notice.spoken` verbatim, never `detail` and never through the model) and hands it to
/// the voice. `spoken_ms` is recorded only when playback finishes in [`pump_voice`].
fn speak_next_notice(
    notices: Option<&std::path::Path>,
    to_voice: &Sender<ToVoice>,
    generation: &AtomicU64,
    speaking: &mut usize,
    alerted: &mut bool,
    report: &mut dyn FnMut(Moment),
) -> bool {
    let Some(path) = notices else {
        return false;
    };
    let board = match archie_notices::Board::load(path) {
        Ok(board) => {
            *alerted = false;
            board
        }
        Err(error) => {
            if !*alerted {
                *alerted = true;
                report(Moment::Alert(format!("notice board: {error}")));
            }
            return false;
        }
    };
    let Some(notice) = board.unspoken().into_iter().next() else {
        return false;
    };
    let text = notice.spoken.clone();
    let features = semantic_features(&text);
    *speaking += 1;
    let _ = to_voice.send(ToVoice::Say {
        generation: generation.load(Ordering::SeqCst),
        turn_id: format!("notice:{}", notice.id),
        sequence: 0,
        text,
        semantic_features: features,
    });
    true
}

fn pump_voice<C: archie_speech::CognitiveModelAdapter>(
    from_voice: &Receiver<FromVoice>,
    session: &mut SpeechSession<C>,
    notices: Option<&std::path::Path>,
    speaking: &mut usize,
    last_spoke: &mut Option<Instant>,
    report: &mut dyn FnMut(Moment),
) -> Result<(), String> {
    loop {
        match from_voice.try_recv() {
            Ok(FromVoice::Spoke {
                turn_id,
                sequence,
                text,
                first_audio_ms,
                audio_s,
                stopped,
            }) => {
                *speaking = speaking.saturating_sub(1);
                *last_spoke = Some(Instant::now());
                if !stopped {
                    if let Some(id) = turn_id.strip_prefix("notice:") {
                        if let Some(path) = notices {
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0, |d| d.as_millis() as u64)
                                .max(1);
                            if let Err(error) = archie_notices::Board::update(path, |board| {
                                board.mark_spoken(id, now_ms)
                            }) {
                                report(Moment::Alert(format!("notice board: {error}")));
                            }
                        }
                    } else if session
                        .commit_log()
                        .is_some_and(|log| log.turn_id() == turn_id)
                    {
                        // Only what was said to the end counts as heard.
                        let _ = session.spoke(sequence);
                    }
                }
                report(Moment::Said {
                    text,
                    first_audio_ms,
                    audio_s,
                    stopped,
                });
            }
            Ok(FromVoice::Began { text }) => report(Moment::Speaking { text }),
            Ok(FromVoice::Failed(message)) => {
                *speaking = speaking.saturating_sub(1);
                report(Moment::Alert(message.clone()));
                return Err(message);
            }
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => return Err("the voice stopped".into()),
        }
    }
}

/// The voice's thread: says each piece as it arrives, in order, and says when it is done.
fn speak(
    mut voice: Box<dyn Speaks>,
    mut saying: Saying,
    inbox: Receiver<ToVoice>,
    outbox: Sender<FromVoice>,
    current: Arc<AtomicU64>,
) {
    while let Ok(message) = inbox.recv() {
        match message {
            ToVoice::Say {
                generation,
                turn_id,
                sequence,
                text,
                semantic_features,
            } => {
                if current.load(Ordering::SeqCst) != generation {
                    let _ = outbox.send(FromVoice::Spoke {
                        turn_id,
                        sequence,
                        text,
                        first_audio_ms: 0.0,
                        audio_s: 0.0,
                        stopped: true,
                    });
                    continue;
                }
                let _ = outbox.send(FromVoice::Began { text: text.clone() });
                let mut failed = None;
                let signal_turn_id = format!("{turn_id}-{sequence}");
                let mut last_signal = Instant::now() - Duration::from_secs(1);
                let mut published_active = false;
                let result = voice.say(&text, &mut |pcm| {
                    if current.load(Ordering::SeqCst) != generation {
                        return false;
                    }
                    if let Err(error) = saying.play(pcm) {
                        failed = Some(error.to_string());
                        return false;
                    }
                    if !published_active || last_signal.elapsed() >= Duration::from_millis(100) {
                        let envelope = if pcm.is_empty() {
                            0.0
                        } else {
                            (pcm.iter()
                                .map(|sample| {
                                    let value = *sample as f32 / i16::MAX as f32;
                                    value * value
                                })
                                .sum::<f32>()
                                / pcm.len() as f32)
                                .sqrt()
                                .clamp(0.0, 1.0)
                        };
                        publish(&signal(SignalKind::Speak {
                            active: true,
                            turn_id: signal_turn_id.clone(),
                            semantic_features: semantic_features.clone(),
                            audio_envelope: envelope,
                        }));
                        last_signal = Instant::now();
                        published_active = true;
                    }
                    true
                });
                if published_active {
                    publish(&signal(SignalKind::Speak {
                        active: false,
                        turn_id: signal_turn_id,
                        semantic_features: Vec::new(),
                        audio_envelope: 0.0,
                    }));
                }
                match (result, failed) {
                    (_, Some(error)) => {
                        current.fetch_add(1, Ordering::SeqCst);
                        saying.hush();
                        let _ = outbox.send(FromVoice::Failed(error));
                        break;
                    }
                    (Ok(said), None) => {
                        let played = if said.stopped {
                            Ok(false)
                        } else {
                            saying.settle(&current, generation)
                        };
                        let played = match played {
                            Ok(played) => played,
                            Err(error) => {
                                current.fetch_add(1, Ordering::SeqCst);
                                saying.hush();
                                let _ = outbox.send(FromVoice::Failed(error.to_string()));
                                break;
                            }
                        };
                        if !played {
                            saying.hush();
                        }
                        let _ = outbox.send(FromVoice::Spoke {
                            turn_id,
                            sequence,
                            text,
                            first_audio_ms: said.first_audio_ms,
                            audio_s: said.audio_s,
                            stopped: !played,
                        });
                    }
                    (Err(error), None) => {
                        current.fetch_add(1, Ordering::SeqCst);
                        saying.hush();
                        let _ = outbox.send(FromVoice::Failed(error.to_string()));
                        break;
                    }
                }
            }
            ToVoice::Hush => {
                saying.hush();
                // Every queued Say reports its own Spoke, including cancellation.
                // An unscoped Idle could arrive after new speech was enqueued and
                // incorrectly erase that generation's outstanding segment count.
            }
            ToVoice::Finish => break,
        }
    }
    let _ = saying.settle(&current, current.load(Ordering::SeqCst));
}

/// What the ears wrote, as a sentence: a run of punctuation — the recogniser adds full
/// stops through a long silence — becomes its first mark.
pub fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_mark = false;
    for c in text.trim().chars() {
        let mark = matches!(c, '.' | '?' | '!' | ',' | ';' | ':');
        if mark && last_was_mark {
            continue;
        }
        last_was_mark = mark;
        out.push(c);
    }
    out
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    use archie_speech::{
        FloorPolicy, ScriptedCognitiveModel, ScriptedReply, SessionPlan, SpeechCapabilities,
    };

    struct FirstFailure(Arc<AtomicU64>);

    #[test]
    fn a_delayed_hush_has_no_unscoped_receipt_to_clear_new_speech() {
        let (send, inbox) = mpsc::channel();
        send.send(ToVoice::Hush).unwrap();
        send.send(ToVoice::Say {
            generation: 1,
            turn_id: "new".into(),
            sequence: 0,
            text: "Hola.".into(),
            semantic_features: vec![],
        })
        .unwrap();
        send.send(ToVoice::Finish).unwrap();
        let (outbox, events) = mpsc::channel();
        speak(
            Box::new(ShortVoice),
            Saying::Recording {
                path: std::env::temp_dir().join("archie-hush-unused.wav"),
                samples: vec![],
                rate: 16_000,
            },
            inbox,
            outbox,
            Arc::new(AtomicU64::new(1)),
        );
        let events: Vec<_> = events.try_iter().collect();
        assert_eq!(
            events.len(),
            2,
            "only Began and the turn-scoped Spoke may change playback accounting"
        );
        assert!(matches!(&events[0], FromVoice::Began { .. }));
        assert!(
            matches!(&events[1], FromVoice::Spoke { turn_id, stopped: false, .. } if turn_id == "new")
        );
    }
    impl Speaks for FirstFailure {
        fn sample_rate(&self) -> u32 {
            16_000
        }
        fn say(
            &mut self,
            _: &str,
            hear: &mut dyn FnMut(&[i16]) -> bool,
        ) -> Result<crate::voice::Said, String> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err("first segment failed".into());
            }
            ShortVoice.say("queued", hear)
        }
    }

    #[test]
    fn synthesis_failure_stops_prequeued_segments_without_waiting_for_main() {
        let calls = Arc::new(AtomicU64::new(0));
        let (send, inbox) = mpsc::channel();
        for sequence in 0..2 {
            send.send(ToVoice::Say {
                generation: 0,
                turn_id: "test".into(),
                sequence,
                text: "queued".into(),
                semantic_features: Vec::new(),
            })
            .unwrap();
        }
        send.send(ToVoice::Finish).unwrap();
        let (outbox, events) = mpsc::channel();
        let (audio, playback) = mpsc::channel();
        speak(
            Box::new(FirstFailure(calls.clone())),
            Saying::Stream(StreamedVoice::new(audio, 16_000)),
            inbox,
            outbox,
            Arc::new(AtomicU64::new(0)),
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the voice must fence the queue before notifying main"
        );
        assert!(events
            .try_iter()
            .any(|event| matches!(event, FromVoice::Failed(_))));
        assert!(!playback
            .try_iter()
            .any(|event| matches!(event, VoiceOut::Audio(_))));
    }

    #[test]
    fn stale_receipt_cannot_mark_the_new_turn_as_heard() {
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
            ScriptedCognitiveModel::new(&["es"])
                .answering("Archie, hola.", ScriptedReply::saying(&["Hola."])),
        )
        .unwrap();
        let state = after_utterance_within(
            &Utterance {
                text: "Archie, hola.".into(),
                final_by_recogniser: true,
                silence_ms: 800,
            },
            None,
            Duration::from_secs(10),
        );
        session.heard("old", "Archie, hola.", &state).unwrap();
        session.pump();
        session.heard("new", "Archie, hola.", &state).unwrap();
        session.pump();
        let (send, recv) = mpsc::channel();
        send.send(FromVoice::Spoke {
            turn_id: "old".into(),
            sequence: 0,
            text: "Hola.".into(),
            first_audio_ms: 0.0,
            audio_s: 0.1,
            stopped: false,
        })
        .unwrap();
        pump_voice(&recv, &mut session, None, &mut 1, &mut None, &mut |_| {}).unwrap();
        assert!(
            session.heard_text().is_empty(),
            "sequence zero belongs to a different turn"
        );
    }

    struct ShortVoice;
    impl Speaks for ShortVoice {
        fn sample_rate(&self) -> u32 {
            16_000
        }
        fn say(
            &mut self,
            _: &str,
            hear: &mut dyn FnMut(&[i16]) -> bool,
        ) -> Result<crate::voice::Said, String> {
            Ok(crate::voice::Said {
                sample_rate: 16_000,
                audio_s: 0.04,
                elapsed_s: 0.0,
                first_audio_ms: 0.0,
                stopped: !hear(&[100; 640]),
            })
        }
    }

    #[test]
    fn playback_receipt_waits_for_the_sink_not_only_synthesis() {
        let (input, inbox) = mpsc::channel();
        let (output, events) = mpsc::channel();
        let (audio, _played) = mpsc::channel();
        input
            .send(ToVoice::Say {
                generation: 0,
                turn_id: "t".into(),
                sequence: 0,
                text: "Hola.".into(),
                semantic_features: vec![],
            })
            .unwrap();
        input.send(ToVoice::Finish).unwrap();
        let at = Instant::now();
        let worker = std::thread::spawn(move || {
            speak(
                Box::new(ShortVoice),
                Saying::Stream(StreamedVoice::new(audio, 16_000)),
                inbox,
                output,
                Arc::new(AtomicU64::new(0)),
            )
        });
        let mut receipt = None;
        while let Ok(event) = events.recv_timeout(Duration::from_secs(2)) {
            if matches!(event, FromVoice::Spoke { .. }) {
                receipt = Some(at.elapsed());
                break;
            }
        }
        worker.join().unwrap();
        assert!(
            receipt.unwrap() >= Duration::from_millis(30),
            "the queued tail was not played yet"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_audio_rejects_invalid_or_oversized_chunks_before_asr() {
        for frame in [
            vec![f32::NAN],
            vec![f32::INFINITY],
            vec![1.1],
            vec![0.0; 16_001],
        ] {
            let (send, recv) = mpsc::channel();
            send.send(frame).unwrap();
            assert!(Hearing::Stream(recv).frame().is_err());
        }
        let (send, recv) = mpsc::channel();
        send.send(vec![]).unwrap();
        assert!(Hearing::Stream(recv).frame().unwrap().unwrap().0.is_empty());
    }

    #[test]
    fn a_run_of_punctuation_through_silence_becomes_one_mark() {
        assert_eq!(
            tidy("Oye Archi, ¿qué hora es?.............."),
            "Oye Archi, ¿qué hora es?"
        );
        assert_eq!(tidy("  hola.  "), "hola.");
    }
}
