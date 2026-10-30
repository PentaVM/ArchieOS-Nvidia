//! Archie's voice: what stands behind the speech plane.
//!
//! [`archie_speech`] decides who has the floor, when to speak and how much. It does not
//! decide what to say, and it does not act. This crate supplies those backends:
//!
//! - [`nemotron::NemotronCognitive`] — the cognitive plane, a Nemotron model on Nebius that
//!   answers in speakable pieces and proposes tools.
//! - [`tools::os_control`] — the tools it may propose: the stage's own vocabulary, so every
//!   proposal maps to an action the stage already checks and can refuse.
//! - [`segment::Segmenter`] — streamed text cut where a voice can stop.
//! - [`act::decide`] and [`hands::Hands`] — a proposal read strictly and either refused in
//!   words or carried out through the stage gate, with what came of it told to the model.
//! - [`listen`] — what the ears heard, turned into the floor state the speech plane decides
//!   on: was it said to Archie, was it only "ya", does the thought sound finished.
//! - [`addressee`] — whether it was said to Archie when nobody named it: alone or among
//!   others, looking at Archie or away, a friend answering, a call.
//! - With `--features speech`: `native` (Nemotron 3.5 ASR and Magpie TTS through NVIDIA
//!   NeMo-Speech.cpp's C ABI), `audio` (microphone and speaker through PulseAudio) and
//!   `conversation`, the loop that joins them.
//!
//! The shape follows the voice agents Hugging Face runs in production: listening, thinking
//! and speaking are separate stages rather than one network, so that the thinking can be a
//! model that uses tools and speaks Spanish, and so that each stage can be replaced without
//! the others noticing.

pub mod act;
pub mod actions;
pub mod addressee;
pub mod asr;
pub mod audio;
pub mod conversation;
pub mod draw;
pub mod hands;
pub mod language;
pub mod listen;
#[cfg(feature = "live")]
pub mod live;
pub mod narration;
#[cfg(feature = "nemo")]
pub mod native;
pub mod nemotron;
pub mod piper;
#[cfg(feature = "rvc")]
pub mod rvc;
pub mod segment;
pub mod tools;
#[cfg(any(feature = "live", test))]
mod visual;
pub mod voice;
pub mod wanda;
pub mod wav;
#[cfg(any(feature = "live", test))]
mod world_response;

pub use act::{decide, Plan, Refusal};
pub use hands::{Grounding, Hands, Launch, Performed};
pub use nemotron::{
    NebiusSource, NemotronCognitive, RoutedSource, TurnSource, TurnStream, Update, PERSONA_ES,
};
pub use segment::{Segmenter, Spoken};
