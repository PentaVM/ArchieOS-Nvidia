//! # archie-core
//!
//! Archie Core: the single user-facing intelligence, and the boot sequence that
//! assembles it.
//!
//! This crate is where the other crates meet. It probes the machine, loads identity and
//! configuration, brings up the fabric, compiles the cognitive topology, and answers the
//! control protocol.
//!
//! ## Modules
//!
//! * [`config`] — operator configuration. Never model-generated.
//! * [`hardware`] — what this machine actually is.
//! * [`boot`] — the canonical boot sequence and its report.
//! * [`runtime`] — the booted node.
//! * [`control`] — the protocol `archie` speaks to `archied`.
//! * [`emergency`] — the record that keeps Core's emergency pause across a restart.

#![forbid(unsafe_code)]

mod ai_control;
pub mod boot;
pub mod cognitive_context;
pub mod config;
pub mod control;
pub mod cortex;
pub mod dispatch;
pub mod emergency;
pub mod endpoint;
pub mod execution;
pub mod hardware;
pub mod hotplug;
pub mod operator;
mod operator_session;
pub mod passive_network;
pub mod runtime;
pub mod sentry;
pub mod sentry_config;
pub mod task_service;
pub mod unify;
pub mod world_control;

pub use boot::{BootReport, BootStage, StageStatus};
pub use config::{ArchieConfig, ConfigError, TransportKind};
pub use control::{ControlRequest, ControlResponse, StatusView};
pub use endpoint::{ControlListener, ControlStream};
pub use hardware::HardwareProbe;
pub use runtime::{
    build_transport, DeviceEnrollmentError, Runtime, RuntimeError, TransportOutcome,
};

pub use archie_types::ARCHIEOS_VERSION;
