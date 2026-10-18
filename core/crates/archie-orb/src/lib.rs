//! TFOG: a read-only projection of ArchieOS state into a persistent semantic field.
//!
//! Producers report observations through [`Signal`]. The orb never owns an executor,
//! policy lease, stage socket, microphone, or speaker. It cannot make an observation
//! true and it cannot make an action happen.

#![forbid(unsafe_code)]

pub mod control;
pub mod lumen;
mod model;
mod signal;
mod transport;

pub use model::{
    Morphotype, OrbConfig, OrbEdge, OrbField, OrbFrame, OrbModel, OrbNode, OrbState, Relation,
    ResourceProfile, WorkflowView,
};
pub use signal::{
    EntityKind, OrbEntity, Signal, SignalError, SignalKind, StageStatus, WorkflowComponent,
    WorkflowPhase, TFOG_SCHEMA, TOKEN_DIMENSIONS,
};
pub use transport::{SignalPublisher, SignalReceiver};

use std::sync::OnceLock;

/// Best-effort publication for in-process Archie producers. The datagram send is
/// non-blocking; a stopped observatory never delays speech, cognition or Stage.
pub fn publish(signal: &Signal) {
    static PUBLISHER: OnceLock<Option<SignalPublisher>> = OnceLock::new();
    let publisher = PUBLISHER.get_or_init(|| {
        let path = std::env::var_os("ARCHIE_ORB_SOCKET")
            .unwrap_or_else(|| "/run/archie-display/orb.sock".into());
        SignalPublisher::new(path).ok()
    });
    if let Some(publisher) = publisher {
        let _ = publisher.publish(signal);
    }
}

/// A timestamped signal in the common Archie clock domain.
pub fn signal(kind: SignalKind) -> Signal {
    Signal {
        schema: TFOG_SCHEMA,
        at_ms: archie_types::Timestamp::now().as_millis(),
        kind,
    }
}

/// Turn local words into a bounded signed lexical sketch. The source text never leaves
/// the caller; only this fixed-size vector is suitable for the Orb transport.
pub fn semantic_features(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0_f32; TOKEN_DIMENSIONS];
    let mut previous: Option<u32> = None;
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let mut hash = 0x811c9dc5_u32;
        for byte in word.to_lowercase().bytes() {
            hash = (hash ^ u32::from(byte)).wrapping_mul(0x01000193);
        }
        let index = (hash as usize) & (TOKEN_DIMENSIONS - 1);
        let sign = if hash & 0x8000_0000 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
        if let Some(previous) = previous {
            let bigram = hash.rotate_left(11) ^ previous.wrapping_mul(0x9e3779b9);
            let index = (bigram as usize) & (TOKEN_DIMENSIONS - 1);
            vector[index] += if bigram & 0x8000_0000 == 0 { 0.5 } else { -0.5 };
        }
        previous = Some(hash);
    }
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}
