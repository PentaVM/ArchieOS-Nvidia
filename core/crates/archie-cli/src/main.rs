//! `archie` — the ArchieOS command-line interface.
//!
//! A thin client. Queries render daemon state; Sentry execution adds an interactive
//! review of a Core-prepared intent on the same connection before confirmation.
//!
//! Every command accepts `--json` for machine consumption.

use archie_core::control::{
    request_over, ControlRequest, ControlResponse, CortexTrajectoryView, DeviceOperationView,
    DeviceView, ModelView, ModuleView, PeerView, ProviderView, StatusView, TotalsView,
};
use archie_core::endpoint;
use archie_fabric::{plan_enrolment, Enrolment, FabricChoice, NodeIdentity};
use archie_state::StatePaths;
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

mod ai;
mod visual;
mod world;
mod world_operator;

#[derive(Debug, Parser)]
#[command(
    name = "archie",
    version,
    about = "Query and control the local ArchieOS node."
)]
struct Cli {
    /// Relocate the ArchieOS tree, matching `archied --root`.
    #[arg(long, value_name = "DIR", global = true)]
    root: Option<String>,

    /// Emit raw JSON instead of a table.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Explicit geographic exploration through the private world daemon.
    World {
        #[command(subcommand)]
        command: world::WorldCommand,
    },
    /// Run admitted AI inference, scoped memory and the pinned Harness2 engine.
    Ai {
        #[command(subcommand)]
        command: ai::AiCommand,
    },
    /// Validate the local workstation boundary and optional runtimes.
    Doctor,
    /// Create the local runtime directories and show first-run state.
    Setup,
    /// Start the ArchieOS workstation target.
    Start,
    /// Stop the ArchieOS workstation target.
    Stop,
    /// Node identity, hardware, capabilities, peers, modules and health.
    Status,
    /// The capability and resource graph across the fabric.
    Topology,
    /// Installed modules, and why each did or did not load.
    Modules,
    /// Installed data-provider manifests, and why each does or does not run.
    Providers,
    /// Fabric peers.
    Peers,
    /// Locally recorded device pairing inventory (read-only; no adapter connection).
    Devices,
    /// Query one configured printer through Sentry (read-only; never prints).
    SentryPrinter {
        /// Resource ID from the node's Sentry configuration.
        resource_id: String,
    },
    /// Read a configured home entity's cached report without commanding it.
    SentryHomeStatus { resource_id: String },
    /// Analyze an offline TShark mDNS export. Never captures or scans the network.
    SentryNetworkAnalyze {
        #[arg(long, required = true)]
        scope: Vec<String>,
        #[arg(long)]
        input: std::path::PathBuf,
        #[arg(long)]
        baseline: Option<std::path::PathBuf>,
    },
    /// One view of every device Sentry knows on the LAN: scan, watch and passive
    /// import merged by hardware address, each source's claims kept under its name.
    ///
    /// Purely local and read-only: it reads the three files and merges them. No
    /// config section names default paths for them yet, so at least one flag is
    /// required; the sources given are the sources merged.
    SentryDevices {
        /// `archie-lan-inventory-v1` JSON, as `archie-lan scan` / `watch --out` writes.
        #[arg(long)]
        inventory: Option<std::path::PathBuf>,
        /// `archie-netwatch-state-v1` JSON (`state.json` in the netwatch state dir).
        #[arg(long)]
        netwatch_state: Option<std::path::PathBuf>,
        /// Offline TShark mDNS export (ip.src, udp.srcport, dns.ptr.domain_name).
        #[arg(long)]
        passive: Option<std::path::PathBuf>,
    },
    /// Review and execute one paired device operation in an interactive Linux session.
    SentryExecute {
        resource_id: String,
        #[command(subcommand)]
        action: SentryAction,
    },
    /// Sentry IoT operations: remote keys, app launches, printing, printer diagnosis.
    ///
    /// Keys, launches and prints go through the same reviewed-intent
    /// prepare/approve flow as `sentry-execute`; there is no bypass.
    Sentry {
        #[command(subcommand)]
        action: SentryIoT,
    },
    /// Act on a configured node: wake it (WoL), lock/unlock its session, set
    /// its volume, or launch an app on it (SSH/ADB per node type).
    ///
    /// Every action is an intent that goes through the same reviewed
    /// prepare/approve flow as `sentry-execute`, with the adapter's measured
    /// postcondition; there is no bypass.
    SentryNode {
        #[command(subcommand)]
        action: SentryNodeAction,
    },
    /// Follow the netwatch alerts file: every new alert is validated, deduped
    /// against the vault's content-derived references, sealed at Restricted and
    /// considered in one Cerebra pass per batch, with its trace.
    ///
    /// Entirely local. On a node with no local model the restricted pass is
    /// refused and the refusal is reported, which is the designed outcome.
    /// The state directory falls back to `[sentry.netwatch]` when the flag is
    /// absent.
    SentryFollow {
        /// Netwatch state directory (holds `alerts.jsonl` and `told.json`).
        #[arg(long)]
        state_dir: Option<std::path::PathBuf>,
        /// One pass instead of a poll loop.
        #[arg(long)]
        once: bool,
        /// Poll interval when looping (500..=60000 ms; default 5000).
        #[arg(long)]
        poll_ms: Option<u64>,
    },
    /// One pass of the unified feeder: every netwatch alert, device change and
    /// Wanda record already sealed in the local vault, filtered and ordered by
    /// a focus profile (`pantalla-1`, `trabajo`, `personal`, `red`), then one
    /// restricted Cerebra pass. Local; never crosses personal/work domains.
    Focus {
        /// Profile name.
        profile: String,
        /// Context domain: `personal/<owner>` or `work/<organisation>/<project>`.
        #[arg(long, default_value = "personal/owner")]
        domain: String,
        /// Only records sealed at or after this time (ms since the epoch).
        #[arg(long, default_value_t = 0)]
        since_ms: i64,
    },
    /// The owner notice board: what Sentry told the owner, pending first.
    /// `sentry-follow` posts to it every alert at or above the floor
    /// (`[sentry.response] min_severity`, default `warning`); the voice says
    /// each one's fixed sentence once, and the Observatory shows them. A notice
    /// changes nothing on the network. Acknowledging ends it.
    SentryNotices {
        /// Show acknowledged notices too.
        #[arg(long)]
        all: bool,
        /// Acknowledge one notice by id (or a unique prefix of 6+ characters).
        #[arg(long, conflicts_with = "ack_all")]
        ack: Option<String>,
        /// Acknowledge every pending notice.
        #[arg(long)]
        ack_all: bool,
        /// For the voice: print the next notice to say (pending, unsaid, most
        /// serious first) as JSON, and mark it said. Prints `null` when there is
        /// none. Say its `spoken` field and nothing else.
        #[arg(long, conflicts_with_all = ["ack", "ack_all", "all"])]
        claim: bool,
    },
    /// Ingest one chat export file into Wanda's context: WhatsApp `.txt`,
    /// Instagram `.json` or notification-dump `.jsonl`. Local and idempotent:
    /// a file whose content is already ingested is skipped.
    WandaIngest {
        /// The export file.
        file: std::path::PathBuf,
        /// Sender-hash salt; defaults to `[sentry.chat_exports]` salt.
        #[arg(long)]
        salt: Option<String>,
    },
    /// Ingest every chat export in the watch directory into Wanda's context.
    /// The directory and salt fall back to `[sentry.chat_exports]`.
    /// Seal the owner's documents (`.md`, `.txt`) from a directory into the
    /// vault as the Vault pillar: briefs, restrictions, versions and notes,
    /// each in the one domain given. Local and idempotent; nothing leaves the
    /// machine, and `archie focus trabajo` then weighs them.
    VaultDocs {
        /// Directory of documents.
        #[arg(long)]
        dir: std::path::PathBuf,
        /// Context domain: `personal/<owner>` or `work/<organisation>/<project>`.
        #[arg(long, default_value = "personal/owner")]
        domain: String,
        /// One pass instead of a poll loop.
        #[arg(long)]
        once: bool,
        /// Poll interval when looping (500..=60000 ms; default 5000).
        #[arg(long)]
        poll_ms: Option<u64>,
    },
    WandaWatch {
        /// Directory of `.txt` / `.json` / `.jsonl` exports.
        #[arg(long)]
        dir: Option<std::path::PathBuf>,
        /// Sender-hash salt; defaults to `[sentry.chat_exports]` salt.
        #[arg(long)]
        salt: Option<String>,
        /// One pass instead of a poll loop.
        #[arg(long)]
        once: bool,
        /// Poll interval when looping (500..=60000 ms; default 5000).
        #[arg(long)]
        poll_ms: Option<u64>,
    },
    /// Build the local IRIS context feed from watchdog and offline exports.
    ///
    /// Works with the daemon down: reads the router watchdog directory, optional
    /// LAN inventory / passive analysis JSON files, and appends typed events to
    /// JSONL segments in the output directory. Directories fall back to the
    /// `[sentry.context_feed]` configuration when the flags are absent.
    ContextFeed {
        /// Directory of rotated TShark JSONL exports from the router watchdog.
        #[arg(long)]
        watchdog_dir: Option<std::path::PathBuf>,
        /// Output directory for feed segments and the cursor state file.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Optional `archie-lan-inventory-v1` JSON document to ingest.
        #[arg(long)]
        lan: Option<std::path::PathBuf>,
        /// Optional `sentry-network-analyze --json` report to ingest.
        #[arg(long)]
        passive: Option<std::path::PathBuf>,
        /// Poll interval when looping (500..=60000 ms; default from config or 5000).
        #[arg(long)]
        poll_ms: Option<u64>,
        /// One pass instead of a poll loop.
        #[arg(long)]
        once: bool,
    },
    /// Read a durable outcome or ambiguous dispatch fence; never retries an operation.
    SentryReceipt { intent_id: String },
    /// Recent prepared/cancelled physical-operation records (read-only; never dispatches).
    DeviceOperations {
        /// How many records to show.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Recent correction/replay metadata (read-only; no private payloads or graph writes).
    CortexTrajectories {
        /// How many records to show.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Registered model providers.
    Models,
    /// The boot report from the running daemon.
    Boot,
    /// Metrics snapshot.
    Metrics,
    /// Recent audit records.
    Audit {
        /// How many records to show.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Check that the daemon is alive.
    Ping,

    /// Put this node into a fabric.
    ///
    /// The only subcommand that writes rather than asks, and the only one that works
    /// with the daemon down — which is the state it exists for. A node that belongs to
    /// no fabric boots, runs, and reports a degraded stage saying so; this is what that
    /// message tells the operator to run.
    ///
    /// With no arguments it starts a new fabric owned by this machine. There is no
    /// guest flag: isolation is what happens when nobody does anything special, and
    /// joining is the step that takes an action.
    Enrol {
        /// Join the fabric this owner key belongs to, instead of starting a new one.
        ///
        /// The private key, because admitting a node means issuing a certificate and
        /// only the owner key can do that. A design where a fingerprint sufficed would
        /// be one where knowing a public value let you join.
        #[arg(long, value_name = "HEX")]
        owner_key: Option<String>,
    },
    /// Inspect or configure inference providers.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Inspect, add, authenticate or probe external harnesses.
    Harness {
        #[command(subcommand)]
        command: HarnessCommand,
    },
    /// Inspect the local sealed Vault.
    Vault {
        #[command(subcommand)]
        command: VaultCommand,
    },
    /// Inspect the local ASR model.
    Speech {
        #[command(subcommand)]
        command: SpeechCommand,
    },
    /// Control the Observatory: scenes and look presets (no overlays).
    Visual {
        #[command(subcommand)]
        command: visual::VisualCommand,
    },
    /// Test the local audio bridge.
    Voice {
        #[command(subcommand)]
        command: VoiceCommand,
    },
    /// Inspect professional tool adapters.
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
    /// Manage controlled workspaces.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Inspect the locally deployed Iris visual-reflex model.
    Iris {
        #[command(subcommand)]
        command: IrisCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ProviderCommand {
    List,
    Add { id: String },
    Test { id: String },
}

#[derive(Debug, Subcommand)]
enum HarnessCommand {
    List,
    Add { id: String },
    Login { id: String },
    Test { id: String },
}

#[derive(Debug, Subcommand)]
enum VaultCommand {
    Status,
}
#[derive(Debug, Subcommand)]
enum SpeechCommand {
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
}
#[derive(Debug, Subcommand)]
enum ModelCommand {
    Status,
}
#[derive(Debug, Subcommand)]
enum VoiceCommand {
    Test,
}
#[derive(Debug, Subcommand)]
enum ToolsCommand {
    List,
    Test { id: String },
}
#[derive(Debug, Subcommand)]
enum WorkspaceCommand {
    Create { id: String },
    List,
}
#[derive(Debug, Subcommand)]
enum IrisCommand {
    /// Verify the manifest, checkpoint digest and fixed-point tensor bounds.
    VerifyModel {
        /// Deployment manifest; defaults to /archie/models/iris/champion/model-manifest.json.
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// INT8 checkpoint; defaults to /archie/models/iris/champion/model.int8.json.
        #[arg(long)]
        checkpoint: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum SentryAction {
    SwitchOn,
    SwitchOff,
    Pause,
    Play,
    Brightness { percent: u8 },
    Temperature { milli_celsius: i32 },
}
impl SentryAction {
    fn command(&self) -> archie_core::control::SentryCommand {
        use archie_core::control::SentryCommand;
        match *self {
            Self::SwitchOn => SentryCommand::SwitchSet { on: true },
            Self::SwitchOff => SentryCommand::SwitchSet { on: false },
            Self::Pause => SentryCommand::MediaPause,
            Self::Play => SentryCommand::MediaPlay,
            Self::Brightness { percent } => SentryCommand::LightSetBrightness { percent },
            Self::Temperature { milli_celsius } => {
                SentryCommand::ClimateSetTemperature { milli_celsius }
            }
        }
    }
}

/// Remote keys with a measurable postcondition; the set is exactly the one the
/// Sentry contracts accept, so an unknown key name fails at the CLI.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[value(rename_all = "snake_case")]
enum RemoteKeyArg {
    PowerOn,
    PowerOff,
    Home,
    Play,
    Pause,
}
impl RemoteKeyArg {
    fn name(&self) -> archie_core::control::SentryRemoteKey {
        use archie_core::control::SentryRemoteKey;
        match self {
            Self::PowerOn => SentryRemoteKey::PowerOn,
            Self::PowerOff => SentryRemoteKey::PowerOff,
            Self::Home => SentryRemoteKey::Home,
            Self::Play => SentryRemoteKey::Play,
            Self::Pause => SentryRemoteKey::Pause,
        }
    }
}

/// Node actions with a measurable postcondition; each maps to exactly one
/// Sentry contract command, so an invalid one fails at the CLI.
#[derive(Debug, Subcommand)]
enum SentryNodeAction {
    /// Wake a configured WoL node with a magic packet (verified by the
    /// neighbor cache afterwards — the packet itself proves nothing).
    Wake {
        /// Resource ID from the node's Sentry configuration (`[sentry.wol]`).
        host: String,
    },
    /// Lock a configured Android node's session (verified against `dumpsys`).
    Lock {
        /// Resource ID from `[sentry.android]`.
        host: String,
    },
    /// Unlock a configured Android node's session (verified against `dumpsys`).
    Unlock {
        /// Resource ID from `[sentry.android]`.
        host: String,
    },
    /// Set a configured Android node's output volume (verified against
    /// `dumpsys audio` afterwards).
    Volume {
        /// Resource ID from `[sentry.android]`.
        host: String,
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: u8,
    },
    /// Launch an application on a configured Android node (verified against
    /// the foreground process the device reports).
    Launch {
        /// Resource ID from `[sentry.android]`.
        host: String,
        /// Application package name, e.g. `com.example.app`.
        app: String,
    },
    /// Cut a device off at the router: a FORWARD drop rule matched by its
    /// hardware address in every firewall the router answers with (verified by
    /// `iptables -C` afterwards). Addresses in `protected_macs` are refused.
    Block {
        /// Resource ID of `[sentry.router]` (needs its SSH channel).
        router: String,
        /// The device's hardware address, lowercase `aa:bb:cc:dd:ee:ff`.
        mac: String,
    },
    /// Restore a blocked device: removes this system's drop rule for it, and
    /// only that (verified by `iptables -C` afterwards).
    Unblock {
        /// Resource ID of `[sentry.router]`.
        router: String,
        /// The device's hardware address, lowercase `aa:bb:cc:dd:ee:ff`.
        mac: String,
    },
}
impl SentryNodeAction {
    fn command(&self) -> archie_core::control::SentryCommand {
        use archie_core::control::SentryCommand;
        match self {
            Self::Wake { .. } => SentryCommand::WakeHost,
            Self::Lock { .. } => SentryCommand::SessionLock { locked: true },
            Self::Unlock { .. } => SentryCommand::SessionLock { locked: false },
            Self::Volume { percent, .. } => SentryCommand::VolumeSet { percent: *percent },
            Self::Launch { app, .. } => SentryCommand::AppLaunch {
                app_id: app.clone(),
            },
            Self::Block { mac, .. } => SentryCommand::DeviceBlock {
                mac: mac.clone(),
                blocked: true,
            },
            Self::Unblock { mac, .. } => SentryCommand::DeviceBlock {
                mac: mac.clone(),
                blocked: false,
            },
        }
    }
    fn host(&self) -> &str {
        match self {
            Self::Wake { host }
            | Self::Lock { host }
            | Self::Unlock { host }
            | Self::Volume { host, .. }
            | Self::Launch { host, .. }
            | Self::Block { router: host, .. }
            | Self::Unblock { router: host, .. } => host,
        }
    }
}

#[derive(Debug, Subcommand)]
enum SentryIoT {
    /// Send one verifiable remote key to a paired Roku resource (reviewed flow).
    Key {
        /// Resource ID from the node's Sentry configuration.
        resource_id: String,
        #[arg(value_enum)]
        key: RemoteKeyArg,
    },
    /// Launch an application on a paired Roku or Chromecast resource
    /// (reviewed flow, verified against the device's own status).
    Launch {
        resource_id: String,
        /// ECP application ID (e.g. `12` for Netflix) or Cast app ID (e.g. `CC1AD845`).
        app_id: String,
    },
    /// Stop the running application on a paired Chromecast (reviewed flow,
    /// verified when RECEIVER_STATUS shows only the idle screen).
    Stop { resource_id: String },
    /// Set a paired Chromecast's volume (reviewed flow, verified against
    /// RECEIVER_STATUS; a receiver that rounds the level diverges).
    Volume {
        resource_id: String,
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: u8,
    },
    /// Mute a paired Chromecast (reviewed flow, verified against RECEIVER_STATUS).
    Mute { resource_id: String },
    /// Unmute a paired Chromecast (reviewed flow, verified against RECEIVER_STATUS).
    Unmute { resource_id: String },
    /// Complete one TLS handshake with a Chromecast and print the SHA-256 of
    /// the certificate it presents, for the operator to compare and pin as
    /// `certificate_sha256`. Sends no CAST message; trusts nothing by itself.
    CastProbe {
        /// Device address (loopback or private).
        address: std::net::IpAddr,
        #[arg(long, default_value_t = 8009)]
        port: u16,
    },
    /// Hash, stage and submit one document to a configured printer (reviewed flow).
    ///
    /// The file is hashed (SHA-256), its format is sniffed and refused when not
    /// printable, and it is staged atomically into the printer's configured
    /// spool directory before any intent is prepared.
    Print {
        resource_id: String,
        /// Local document to print.
        file: std::path::PathBuf,
        /// Declared page count, 1..=9999.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u16).range(1..=9999))]
        pages: u16,
    },
    /// Probe one configured printer and print its driver plan. Read-only: the
    /// planned command line is data for the operator and is never executed.
    PrinterDoctor { resource_id: String },
}

impl Command {
    /// The daemon request this command makes, or `None` when it makes none.
    ///
    /// `Option` rather than a panic on the local arm: a command that never reaches the
    /// daemon is a real kind of command, not a mistake, and encoding it in the type
    /// means adding another cannot produce an `unreachable!` that is one day reached.
    fn to_request(&self) -> Option<ControlRequest> {
        Some(match self {
            Self::Ai { .. }
            | Self::World { .. }
            | Self::Doctor
            | Self::Setup
            | Self::Start
            | Self::Stop
            | Self::Provider { .. }
            | Self::Harness { .. }
            | Self::Vault { .. }
            | Self::Speech { .. }
            | Self::Voice { .. }
            | Self::Tools { .. }
            | Self::Workspace { .. }
            | Self::Iris { .. } => return None,
            Self::Status => ControlRequest::Status,
            Self::Topology => ControlRequest::Topology,
            Self::Modules => ControlRequest::Modules,
            Self::Providers => ControlRequest::Providers,
            Self::Peers => ControlRequest::Peers,
            Self::Devices => ControlRequest::Devices,
            Self::SentryPrinter { resource_id } => ControlRequest::SentryPrinter {
                resource_id: resource_id.clone(),
            },
            Self::SentryHomeStatus { resource_id } => ControlRequest::SentryHomeStatus {
                resource_id: resource_id.clone(),
            },
            Self::SentryExecute {
                resource_id,
                action,
            } => ControlRequest::SentryPrepare {
                resource_id: resource_id.clone(),
                command: action.command(),
            },
            Self::Sentry { action } => match action {
                SentryIoT::Key { resource_id, key } => ControlRequest::SentryPrepare {
                    resource_id: resource_id.clone(),
                    command: archie_core::control::SentryCommand::RemoteKey { key: key.name() },
                },
                SentryIoT::Launch {
                    resource_id,
                    app_id,
                } => ControlRequest::SentryPrepare {
                    resource_id: resource_id.clone(),
                    command: archie_core::control::SentryCommand::AppLaunch {
                        app_id: app_id.clone(),
                    },
                },
                SentryIoT::Stop { resource_id } => ControlRequest::SentryPrepare {
                    resource_id: resource_id.clone(),
                    command: archie_core::control::SentryCommand::AppStop,
                },
                SentryIoT::Volume {
                    resource_id,
                    percent,
                } => ControlRequest::SentryPrepare {
                    resource_id: resource_id.clone(),
                    command: archie_core::control::SentryCommand::VolumeSet { percent: *percent },
                },
                SentryIoT::Mute { resource_id } | SentryIoT::Unmute { resource_id } => {
                    ControlRequest::SentryPrepare {
                        resource_id: resource_id.clone(),
                        command: archie_core::control::SentryCommand::MuteSet {
                            muted: matches!(action, SentryIoT::Mute { .. }),
                        },
                    }
                }
                // A handshake from this machine; no daemon, no intent.
                SentryIoT::CastProbe { .. } => return None,
                SentryIoT::PrinterDoctor { resource_id } => ControlRequest::SentryPrinterDoctor {
                    resource_id: resource_id.clone(),
                },
                // `print` needs local staging before the intent exists; main()
                // builds its request after staging, so there is none here.
                SentryIoT::Print { .. } => return None,
            },
            Self::SentryReceipt { intent_id } => ControlRequest::SentryReceipt {
                intent_id: intent_id.clone(),
            },
            Self::SentryNode { action } => ControlRequest::SentryPrepare {
                resource_id: action.host().to_owned(),
                command: action.command(),
            },
            Self::DeviceOperations { limit } => ControlRequest::DeviceOperations { limit: *limit },
            Self::CortexTrajectories { limit } => {
                ControlRequest::CortexTrajectories { limit: *limit }
            }
            Self::Models => ControlRequest::Models,
            Self::Boot => ControlRequest::Boot,
            Self::Metrics => ControlRequest::Metrics,
            Self::Audit { limit } => ControlRequest::Audit { limit: *limit },
            Self::Ping => ControlRequest::Ping,
            Self::Enrol { .. }
            | Self::SentryNetworkAnalyze { .. }
            | Self::SentryDevices { .. }
            | Self::ContextFeed { .. }
            | Self::SentryFollow { .. }
            | Self::Focus { .. }
            | Self::SentryNotices { .. }
            | Self::WandaIngest { .. }
            | Self::WandaWatch { .. }
            | Self::VaultDocs { .. }
            // Answered by the display in `main`, before any daemon request is built.
            | Self::Visual { .. } => return None,
        })
    }
}

/// Print the certificate fingerprint a Chromecast presents. The operator
/// compares it (for example with the device's own settings or a second probe
/// from another machine) before writing it into `[[sentry.chromecasts]]`.
async fn cast_probe(address: std::net::IpAddr, port: u16, json: bool) -> std::process::ExitCode {
    match archie_core::sentry_config::probe_cast_certificate(address, port).await {
        Ok(fingerprint) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"address": address, "port": port, "certificate_sha256": fingerprint})
                );
            } else {
                println!("certificate_sha256 = \"{fingerprint}\"");
                println!("Nothing was trusted or stored. Pin it only after comparing it.");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("archie: {address}:{port}: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn workstation_root(cli_root: Option<&str>) -> PathBuf {
    cli_root
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/archie"))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn run_process(command: &str, args: &[&str]) -> std::process::ExitCode {
    match ProcessCommand::new(command)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) if status.success() => std::process::ExitCode::SUCCESS,
        Ok(_) | Err(_) => std::process::ExitCode::FAILURE,
    }
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err(format!("{} is not a bounded model file", path.display()));
    }
    std::fs::read(path).map_err(|error| error.to_string())
}

async fn local_command(
    command: &Command,
    root: &Path,
    paths: &StatePaths,
    json: bool,
) -> std::process::ExitCode {
    use archie_workstation::harness::{HarnessKind, HarnessSpec};
    match command {
        Command::Doctor => {
            let checks = archie_workstation::doctor();
            if json {
                println!("{}", serde_json::to_string_pretty(&checks).unwrap());
            } else {
                for c in &checks {
                    println!("{:<5} {:<20} {}", c.status, c.name, c.detail);
                }
            }
            if checks.iter().all(|c| c.status.is_healthy()) {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Command::Setup => {
            for directory in ["runtime", "workspaces", "cache", "models", "state", "logs"] {
                if let Err(error) = std::fs::create_dir_all(root.join(directory)) {
                    eprintln!("archie: {error}");
                    return std::process::ExitCode::FAILURE;
                }
            }
            println!("workstation directories ready at {}", root.display());
            std::process::ExitCode::SUCCESS
        }
        Command::Start => run_process("sudo", &["systemctl", "start", "archie-workstation.target"]),
        Command::Stop => run_process("sudo", &["systemctl", "stop", "archie-workstation.target"]),
        Command::Provider { command } => {
            let path = paths.inference_providers();
            match command {
                ProviderCommand::List => {
                    match archie_workstation::provider::load_providers(&path) {
                        Ok(items) => {
                            for (id, p) in items {
                                println!("{id}\t{:?}\t{}", p.locality, p.base_url);
                            }
                            std::process::ExitCode::SUCCESS
                        }
                        Err(e) => {
                            eprintln!("archie: {e}");
                            std::process::ExitCode::FAILURE
                        }
                    }
                }
                ProviderCommand::Add { id } => {
                    match archie_workstation::provider::builtin_provider(id)
                        .ok_or("unknown provider")
                        .and_then(|p| {
                            archie_workstation::provider::save_provider(&path, p)
                                .map_err(|_| "invalid provider")
                        }) {
                        Ok(()) => {
                            println!(
                                "provider {id} configured; credentials remain environment-backed"
                            );
                            std::process::ExitCode::SUCCESS
                        }
                        Err(e) => {
                            eprintln!("archie: {e}");
                            std::process::ExitCode::FAILURE
                        }
                    }
                }
                ProviderCommand::Test { id } => {
                    let providers =
                        archie_workstation::provider::load_providers(&path).unwrap_or_default();
                    let Some(p) = providers.get(id) else {
                        eprintln!("archie: provider not configured");
                        return std::process::ExitCode::FAILURE;
                    };
                    let endpoint = match p.endpoint() {
                        Ok(value) => value,
                        Err(e) => {
                            eprintln!("archie: {e}");
                            return std::process::ExitCode::FAILURE;
                        }
                    };
                    let client = match archie_model_runtime::InferenceClient::new(endpoint) {
                        Ok(value) => value,
                        Err(e) => {
                            eprintln!("archie: {e}");
                            return std::process::ExitCode::FAILURE;
                        }
                    };
                    match client.models().await {
                        Ok(models) => {
                            let mut updated = p.clone();
                            updated.models = models.data.into_iter().map(|m| m.id).collect();
                            if let Err(error) =
                                archie_workstation::provider::save_provider(&path, updated)
                            {
                                eprintln!("archie: {error}");
                                return std::process::ExitCode::FAILURE;
                            }
                            println!("provider {id}: model catalog saved; inference entitlement remains untested");
                            std::process::ExitCode::SUCCESS
                        }
                        Err(e) => {
                            eprintln!("archie: {e}");
                            std::process::ExitCode::FAILURE
                        }
                    }
                }
            }
        }
        Command::Harness { command } => {
            let config_path = paths.inference_harnesses();
            let kind = |id: &str| match id {
                "codex" => Some(HarnessKind::Codex),
                "claude" => Some(HarnessKind::Claude),
                "hermes" => Some(HarnessKind::Hermes),
                "uhp" => Some(HarnessKind::Uhp),
                _ => None,
            };
            match command {
                HarnessCommand::List => {
                    for harness in archie_workstation::harness::load_harnesses(&config_path)
                        .unwrap_or_default()
                    {
                        let available = harness.probe().unwrap_or_else(|_| "not installed".into());
                        println!("{}\t{available}", harness.id);
                    }
                    std::process::ExitCode::SUCCESS
                }
                HarnessCommand::Add { id } => {
                    let Some(harness) = kind(id).map(HarnessSpec::official) else {
                        eprintln!("archie: unknown harness");
                        return std::process::ExitCode::FAILURE;
                    };
                    match archie_workstation::harness::save_harness(&config_path, harness) {
                        Ok(()) => {
                            println!("harness {id} registered; install remains first-run and license-controlled");
                            std::process::ExitCode::SUCCESS
                        }
                        Err(e) => {
                            eprintln!("archie: {e}");
                            std::process::ExitCode::FAILURE
                        }
                    }
                }
                HarnessCommand::Login { id } => match kind(id) {
                    Some(HarnessKind::Codex) => run_process("codex", &["login"]),
                    Some(HarnessKind::Claude) => run_process("claude", &[]),
                    Some(HarnessKind::Hermes) => run_process("hermes", &["setup"]),
                    _ => std::process::ExitCode::FAILURE,
                },
                HarnessCommand::Test { id } => match kind(id)
                    .map(HarnessSpec::official)
                    .and_then(|h| h.probe().ok())
                {
                    Some(version) => {
                        println!("{id}: {version}");
                        std::process::ExitCode::SUCCESS
                    }
                    None => std::process::ExitCode::FAILURE,
                },
            }
        }
        Command::Vault {
            command: VaultCommand::Status,
        } => {
            let vault = Path::new("/vault");
            println!(
                "vault={} mode=sealed references=opaque",
                if vault.exists() {
                    "ready"
                } else {
                    "unavailable"
                }
            );
            if vault.exists() || cfg!(windows) {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Command::Speech {
            command:
                SpeechCommand::Model {
                    command: ModelCommand::Status,
                },
        } => {
            let model = root.join("models/nemotron-3.5-asr-streaming-0.6b.q8_0.gguf");
            println!(
                "model={} path={}",
                if model.is_file() { "ready" } else { "missing" },
                model.display()
            );
            if model.is_file() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Command::Voice {
            command: VoiceCommand::Test,
        } => run_process("nemo-speech", &["--help"]),
        Command::Tools {
            command: ToolsCommand::List,
        } => {
            println!("freecad\tworkspace-scoped\tnetwork=false");
            std::process::ExitCode::SUCCESS
        }
        Command::Tools {
            command: ToolsCommand::Test { id },
        } if id == "freecad" => run_process("/usr/lib/archie/freecad-smoke-test", &[]),
        Command::Tools { .. } => std::process::ExitCode::FAILURE,
        Command::Workspace {
            command: WorkspaceCommand::Create { id },
        } => {
            if !valid_id(id) {
                return std::process::ExitCode::FAILURE;
            }
            match std::fs::create_dir(root.join("workspaces").join(id)) {
                Ok(()) => {
                    println!("workspace {id} created");
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("archie: {e}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Command::Workspace {
            command: WorkspaceCommand::List,
        } => {
            if let Ok(entries) = std::fs::read_dir(root.join("workspaces")) {
                for entry in entries.flatten().filter(|e| e.path().is_dir()) {
                    println!("{}", entry.file_name().to_string_lossy());
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Command::Iris {
            command:
                IrisCommand::VerifyModel {
                    manifest,
                    checkpoint,
                },
        } => {
            use archie_iris::{FoundryReflexModel, VisualReflexModel};
            let mutable_directory = root.join("models/iris/champion");
            let packaged_directory = PathBuf::from("/usr/share/archie/models/iris/champion");
            let directory = if mutable_directory.join("model-manifest.json").is_file()
                && mutable_directory.join("model.int8.json").is_file()
            {
                mutable_directory
            } else {
                packaged_directory
            };
            let manifest_path = manifest
                .clone()
                .unwrap_or_else(|| directory.join("model-manifest.json"));
            let checkpoint_path = checkpoint
                .clone()
                .unwrap_or_else(|| directory.join("model.int8.json"));
            let loaded = read_bounded(&manifest_path, 64 * 1024)
                .and_then(|manifest_json| {
                    read_bounded(&checkpoint_path, 1024 * 1024)
                        .map(|checkpoint_json| (manifest_json, checkpoint_json))
                })
                .and_then(|(manifest_json, checkpoint_json)| {
                    FoundryReflexModel::load(&manifest_json, &checkpoint_json)
                        .map_err(|error| error.to_string())
                });
            match loaded {
                Ok(model) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(model.manifest()).unwrap()
                        );
                    } else {
                        println!(
                            "iris model={} revision={} quantization={} checkpoint={}",
                            model.manifest().model_id,
                            model.manifest().revision,
                            model.manifest().quantization.as_deref().unwrap_or("none"),
                            model.manifest().checkpoint_sha256
                        );
                    }
                    std::process::ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("archie: {error}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        _ => std::process::ExitCode::FAILURE,
    }
}

/// Put this node into a fabric.
///
/// The same decision the installer makes, through the same code, because there should be
/// one of it: a machine enrolled after the fact must end up in exactly the state a
/// machine enrolled during installation is in, or the two paths drift and only one of
/// them is the one anybody tests.
///
/// Refuses to re-enrol. Moving a machine between fabrics silently would orphan whatever
/// it holds and give no sign it had happened; a person who means it can remove the
/// membership file, which is an action rather than a flag.
fn enrol(paths: &StatePaths, owner_key: Option<&str>, json: bool) -> std::process::ExitCode {
    let node = match NodeIdentity::load_or_create(paths.identity_key()) {
        Ok(node) => node,
        Err(e) => {
            eprintln!("archie: cannot read this node's key: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    match Enrolment::load(&node.public_key_hex(), paths.membership()) {
        Ok(existing) if existing.is_enrolled() => {
            let fingerprint = existing.fabric_fingerprint().unwrap_or_default();
            eprintln!("archie: this node already belongs to fabric {fingerprint}.");
            eprintln!("Enrolling again would move it, orphaning anything it holds there.");
            eprintln!(
                "If you mean it, remove {} first.",
                paths.membership().display()
            );
            return std::process::ExitCode::FAILURE;
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("archie: the existing membership file is unusable: {e}");
            return std::process::ExitCode::FAILURE;
        }
    }

    let choice = match owner_key {
        Some(hex) => FabricChoice::JoinWithOwnerKey {
            owner_private_key_hex: hex.to_string(),
        },
        None => FabricChoice::StartNew,
    };

    let now_ms = archie_types::Timestamp::now().as_millis();
    let plan = match plan_enrolment(&node, &choice, now_ms) {
        Ok(plan) => plan,
        Err(e) => {
            eprintln!("archie: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    if let Err(e) = plan.enrolment.save(paths.membership()) {
        eprintln!("archie: cannot write the membership: {e}");
        return std::process::ExitCode::FAILURE;
    }
    // Only when this machine made the fabric. A machine joining one was lent the key long
    // enough to issue a certificate for itself and is not given a copy.
    if let Some(owner) = &plan.owner {
        if let Err(e) = owner.save(paths.owner_key()) {
            eprintln!("archie: cannot write the owner key: {e}");
            return std::process::ExitCode::FAILURE;
        }
    }

    if json {
        let record = serde_json::json!({
            "fabric": plan.fabric_fingerprint(),
            "started_new_fabric": plan.started_new_fabric,
            "membership": paths.membership().display().to_string(),
            "owner_key": plan.owner.as_ref().map(|_| paths.owner_key().display().to_string()),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&record).expect("serialisable")
        );
    } else {
        println!("{}", plan.summary());
        if plan.owner.is_some() {
            println!();
            println!(
                "The key that admits other machines to it is at {}.",
                paths.owner_key().display()
            );
            println!("Copy it somewhere safe: the fabric keeps working without it, but nothing");
            println!("new can join until it comes back.");
        }
        // The daemon reads its enrolment once, at boot. Saying so is cheaper than
        // somebody watching an empty peer list and concluding the fabric is broken.
        println!();
        println!("Restart archied for this to take effect.");
    }
    std::process::ExitCode::SUCCESS
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    if let Command::World { command } = &cli.command {
        return world::run(command, cli.json).await;
    }

    if let Command::Visual { command } = &cli.command {
        return visual::run(command, cli.json).await;
    }

    let paths = match &cli.root {
        Some(root) => StatePaths::rooted(root),
        None => StatePaths::from_env(),
    };

    if let Command::Ai { command } = &cli.command {
        return match ai::run(command, &paths, cli.json).await {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("archie: {error}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    // `sentry print` stages the document locally before its intent can exist:
    // the SHA-256 of the staged bytes is part of the prepared command.
    let request = match &cli.command {
        Command::Sentry {
            action:
                SentryIoT::Print {
                    resource_id,
                    file,
                    pages,
                },
        } => match prepare_print_request(&paths, resource_id, file, *pages) {
            Ok(request) => Some(request),
            Err(message) => {
                eprintln!("archie: {message}");
                return std::process::ExitCode::FAILURE;
            }
        },
        command => command.to_request(),
    };

    // Local commands never reach the daemon, and `enrol` in particular must work with
    // it down: a node that belongs to no fabric is one whose daemon is reporting a
    // degraded boot stage, and telling somebody to start the daemon first would be
    // telling them to fix the thing they are trying to fix.
    let Some(request) = request else {
        return match &cli.command {
            Command::Enrol { owner_key } => enrol(&paths, owner_key.as_deref(), cli.json),
            Command::SentryNetworkAnalyze {
                scope,
                input,
                baseline,
            } => analyze_network(scope, input, baseline.as_deref(), cli.json),
            Command::SentryDevices {
                inventory,
                netwatch_state,
                passive,
            } => sentry_devices(
                inventory.as_deref(),
                netwatch_state.as_deref(),
                passive.as_deref(),
                cli.json,
            ),
            Command::ContextFeed {
                watchdog_dir,
                out,
                lan,
                passive,
                poll_ms,
                once,
            } => context_feed(
                &paths,
                watchdog_dir.as_deref(),
                out.as_deref(),
                lan.as_deref(),
                passive.as_deref(),
                *poll_ms,
                *once,
                cli.json,
            ),
            Command::SentryFollow {
                state_dir,
                once,
                poll_ms,
            } => sentry_follow(&paths, state_dir.as_deref(), *poll_ms, *once, cli.json),
            Command::Sentry {
                action: SentryIoT::CastProbe { address, port },
            } => cast_probe(*address, *port, cli.json).await,
            Command::SentryNotices {
                all,
                ack,
                ack_all,
                claim,
            } => sentry_notices(&paths, *all, ack.as_deref(), *ack_all, *claim, cli.json),
            Command::Focus {
                profile,
                domain,
                since_ms,
            } => focus(&paths, profile, domain, *since_ms, cli.json),
            Command::WandaIngest { file, salt } => {
                wanda_ingest(&paths, file, salt.as_deref(), cli.json)
            }
            Command::VaultDocs {
                dir,
                domain,
                once,
                poll_ms,
            } => vault_docs(&paths, dir, domain, *poll_ms, *once, cli.json),
            Command::WandaWatch {
                dir,
                salt,
                once,
                poll_ms,
            } => wanda_watch(
                &paths,
                dir.as_deref(),
                salt.as_deref(),
                *poll_ms,
                *once,
                cli.json,
            ),
            other => {
                local_command(
                    other,
                    &workstation_root(cli.root.as_deref()),
                    &paths,
                    cli.json,
                )
                .await
            }
        };
    };

    let mut stream = match endpoint::connect(&paths).await {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("archie: {e}");
            return std::process::ExitCode::from(69); // EX_UNAVAILABLE
        }
    };

    if matches!(
        cli.command,
        Command::SentryExecute { .. }
            | Command::SentryNode { .. }
            | Command::Sentry {
                action: SentryIoT::Key { .. }
                    | SentryIoT::Launch { .. }
                    | SentryIoT::Print { .. }
                    | SentryIoT::Stop { .. }
                    | SentryIoT::Volume { .. }
                    | SentryIoT::Mute { .. }
                    | SentryIoT::Unmute { .. },
            }
    ) {
        return review_and_execute(&mut stream, &request, cli.json).await;
    }

    let response = match request_over(&mut stream, &request).await {
        Ok(response) => response,
        Err(e) => {
            eprintln!("archie: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    if cli.json {
        match serde_json::to_string_pretty(&response) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("archie: cannot render JSON: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
        return exit_code(&response);
    }

    print!("{}", render(&response));
    exit_code(&response)
}

fn analyze_network(
    scope: &[String],
    input: &std::path::Path,
    baseline: Option<&std::path::Path>,
    json: bool,
) -> std::process::ExitCode {
    use std::io::Read;
    fn read(path: &std::path::Path) -> Result<String, String> {
        let file =
            std::fs::File::open(path).map_err(|_| "cannot open network export".to_owned())?;
        if !file
            .metadata()
            .map_err(|_| "cannot inspect network export")?
            .is_file()
        {
            return Err("network export must be a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take((archie_core::passive_network::MAX_EXPORT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read network export")?;
        if bytes.len() > archie_core::passive_network::MAX_EXPORT_BYTES {
            return Err("network export too large".into());
        }
        String::from_utf8(bytes).map_err(|_| "network export must be UTF-8".into())
    }
    let result = (|| {
        let current = read(input)?;
        let previous = baseline.map(read).transpose()?;
        archie_core::passive_network::analyze(scope.to_vec(), &current, previous.as_deref())
    })();
    match result {
        Ok((assets, changes)) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"source":"offline_mdns_metadata",
                    "trusted":false,"assets":assets,"changes":changes})
                );
            } else {
                println!("Passive mDNS metadata: {} observed hosts; {} changes. Advertisements are untrusted.", assets.len(), changes.len());
                for asset in assets {
                    println!("{}: {:?}", asset.ipv4_address, asset.candidates);
                }
                for change in changes {
                    println!("{change:?}");
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("archie: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Merge the three device-knowledge files into one view. Entirely local: the
/// inventory, the audit state and the TShark export are files, and the merge is
/// [`archie_core::unify`]. Returns the rendered output so tests see what the
/// operator would.
fn sentry_devices_render(
    inventory: Option<&std::path::Path>,
    netwatch_state: Option<&std::path::Path>,
    passive: Option<&std::path::Path>,
    json: bool,
) -> Result<String, String> {
    use std::io::Read;
    fn read(path: &std::path::Path, what: &str, maximum: usize) -> Result<String, String> {
        let file = std::fs::File::open(path).map_err(|_| format!("cannot open {what}"))?;
        if !file
            .metadata()
            .map_err(|_| format!("cannot inspect {what}"))?
            .is_file()
        {
            return Err(format!("{what} must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(maximum as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| format!("cannot read {what}"))?;
        if bytes.len() > maximum {
            return Err(format!("{what} too large"));
        }
        String::from_utf8(bytes).map_err(|_| format!("{what} must be UTF-8"))
    }
    if inventory.is_none() && netwatch_state.is_none() && passive.is_none() {
        return Err("no sources: pass --inventory, --netwatch-state and/or --passive".to_owned());
    }
    let inventory = inventory
        .map(|path| {
            read(
                path,
                "lan inventory",
                archie_core::unify::MAX_INVENTORY_BYTES,
            )
        })
        .transpose()?
        .map(|text| {
            archie_core::unify::parse_inventory(&text)
                .map_err(|e| format!("lan inventory rejected: {e}"))
        })
        .transpose()?;
    let netwatch = netwatch_state
        .map(|path| read(path, "netwatch state", archie_core::unify::MAX_STATE_BYTES))
        .transpose()?
        .map(|text| {
            archie_core::unify::parse_netwatch_state(&text)
                .map_err(|e| format!("netwatch state rejected: {e}"))
        })
        .transpose()?;
    let passive = passive
        .map(|path| {
            read(
                path,
                "passive network export",
                archie_core::passive_network::MAX_EXPORT_BYTES,
            )
        })
        .transpose()?
        .map(|text| archie_core::passive_network::observations(&text).map_err(str::to_owned))
        .transpose()?
        .unwrap_or_default();
    let generated_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let view = archie_core::unify::unify(
        inventory.as_ref(),
        netwatch.as_ref(),
        &passive,
        generated_unix,
    )
    .map_err(|e| format!("cannot unify the sources: {e}"))?;
    if json {
        return serde_json::to_string_pretty(&view).map_err(|e| format!("cannot render JSON: {e}"));
    }
    let keyed = view.devices.iter().filter(|d| d.mac.is_some()).count();
    let mut out = format!(
        "devices: {} ({} by hardware address, {} by address only)\n",
        view.devices.len(),
        keyed,
        view.devices.len() - keyed
    );
    for device in &view.devices {
        let ips = device
            .ips
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let name = device.names.first().map(String::as_str).unwrap_or("-");
        let sources = device
            .sources
            .keys()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("+");
        let ips = if ips.is_empty() { "-".to_owned() } else { ips };
        out.push_str(&format!("{}  {ips}  {name}  {sources}\n", device.identity));
    }
    Ok(out)
}

fn sentry_devices(
    inventory: Option<&std::path::Path>,
    netwatch_state: Option<&std::path::Path>,
    passive: Option<&std::path::Path>,
    json: bool,
) -> std::process::ExitCode {
    match sentry_devices_render(inventory, netwatch_state, passive, json) {
        Ok(output) => {
            print!("{output}");
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("archie: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Stage the document into the printer's configured spool directory, then build
/// the prepare request for the reviewed flow. The daemon decides authority; the
/// CLI only ever hands it a typed intent whose bytes are already in place.
fn prepare_print_request(
    paths: &StatePaths,
    resource_id: &str,
    file: &std::path::Path,
    pages: u16,
) -> Result<ControlRequest, String> {
    let config = archie_core::config::ArchieConfig::load(paths.main_config())
        .map_err(|e| format!("cannot load configuration: {e}"))?;
    let printer = config
        .sentry
        .printers
        .iter()
        .find(|p| p.resource_id == resource_id)
        .ok_or_else(|| "printer resource is not configured".to_owned())?;
    let spool = printer
        .spool_dir
        .as_deref()
        .ok_or_else(|| "printer resource has no spool_dir configured".to_owned())?;
    let (sha256, format) =
        archie_core::sentry::stage_print_document(std::path::Path::new(spool), file)?;
    Ok(ControlRequest::SentryPrepare {
        resource_id: resource_id.to_owned(),
        command: archie_core::control::SentryCommand::PrintSubmit {
            sha256,
            format,
            pages,
        },
    })
}

/// One pass (or a poll loop) of context-feed construction. Entirely local: the
/// watchdog directory, the optional exports and the output segments are files.
/// Sentry execution receipts would need the daemon's journal, which has no
/// offline reader; that source is reported as skipped rather than faked.
#[allow(clippy::too_many_arguments)]
fn context_feed(
    paths: &StatePaths,
    watchdog_dir: Option<&std::path::Path>,
    out: Option<&std::path::Path>,
    lan: Option<&std::path::Path>,
    passive: Option<&std::path::Path>,
    poll_ms: Option<u64>,
    once: bool,
    json: bool,
) -> std::process::ExitCode {
    use archie_core::control::sentry_feed::{
        dedupe_events, from_lan_inventory, from_passive_network, FeedWriter, WatchdogReader,
        MAX_INPUT_BYTES,
    };
    use std::io::Read;

    fn read_bounded(path: &std::path::Path, what: &str) -> Result<String, String> {
        let file = std::fs::File::open(path).map_err(|_| format!("cannot open {what}"))?;
        if !file
            .metadata()
            .map_err(|_| format!("cannot inspect {what}"))?
            .is_file()
        {
            return Err(format!("{what} must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take((MAX_INPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| format!("cannot read {what}"))?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(format!("{what} too large"));
        }
        String::from_utf8(bytes).map_err(|_| format!("{what} must be UTF-8"))
    }

    let run = || -> Result<(), String> {
        // Flags win; absent flags fall back to [sentry.context_feed] config.
        let configured = if watchdog_dir.is_none() || out.is_none() || poll_ms.is_none() {
            Some(
                archie_core::config::ArchieConfig::load(paths.main_config())
                    .map_err(|e| format!("cannot load configuration: {e}"))?,
            )
        } else {
            None
        };
        let feed_config = configured
            .as_ref()
            .and_then(|config| config.sentry.context_feed.as_ref());
        let watchdog_dir = watchdog_dir
            .map(std::path::PathBuf::from)
            .or_else(|| feed_config.map(|c| std::path::PathBuf::from(&c.watchdog_dir)))
            .ok_or_else(|| {
                "no watchdog directory: pass --watchdog-dir or configure [sentry.context_feed]"
                    .to_owned()
            })?;
        let out = out
            .map(std::path::PathBuf::from)
            .or_else(|| feed_config.map(|c| std::path::PathBuf::from(&c.out_dir)))
            .ok_or_else(|| {
                "no output directory: pass --out or configure [sentry.context_feed]".to_owned()
            })?;
        let poll_ms = poll_ms.or(feed_config.map(|c| c.poll_ms)).unwrap_or(5000);
        if !(500..=60000).contains(&poll_ms) {
            return Err("poll_ms must be within 500..=60000".to_owned());
        }
        let lan_text = lan
            .map(|path| read_bounded(path, "lan inventory"))
            .transpose()?;
        let passive_text = passive
            .map(|path| read_bounded(path, "passive network report"))
            .transpose()?;

        let mut reader = WatchdogReader::new(&watchdog_dir);
        let mut writer = FeedWriter::open(&out)
            .map_err(|_| "cannot open the feed output directory".to_owned())?;
        // Cross-run replay suppression. WatchdogReader cursors are in-memory,
        // so a restarted process re-reads unconsumed files; the persisted id
        // set drops events whose content is identical to one already written.
        // Seeding file offsets across processes would require feed.rs changes,
        // which are out of scope.
        let state_path = out.join("context-feed-state.json");
        const MAX_REMEMBERED_IDS: usize = 8192;
        let mut remembered: std::collections::VecDeque<String> =
            std::fs::read_to_string(&state_path)
                .ok()
                .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
                .unwrap_or_default()
                .into_iter()
                .take(MAX_REMEMBERED_IDS)
                .collect();
        let mut known: std::collections::BTreeSet<String> = remembered.iter().cloned().collect();
        let persist_state = |remembered: &std::collections::VecDeque<String>| {
            let tmp = state_path.with_extension("json.tmp");
            if let Ok(text) = serde_json::to_string(&remembered) {
                if std::fs::write(&tmp, text).is_ok() {
                    let _ = std::fs::rename(&tmp, &state_path);
                }
            }
        };
        loop {
            let mut events = reader
                .poll()
                .map_err(|_| "cannot read the watchdog directory".to_owned())?;
            if let Some(text) = &lan_text {
                events.extend(
                    from_lan_inventory(text).map_err(|e| format!("lan inventory rejected: {e}"))?,
                );
            }
            if let Some(text) = &passive_text {
                events.extend(
                    from_passive_network(text)
                        .map_err(|e| format!("passive network report rejected: {e}"))?,
                );
            }
            let mut written = 0u64;
            for event in dedupe_events(events) {
                if !known.insert(event.event_id.clone()) {
                    continue;
                }
                writer
                    .append(&event)
                    .map_err(|e| format!("cannot append a feed event: {e}"))?;
                remembered.push_back(event.event_id.clone());
                if remembered.len() > MAX_REMEMBERED_IDS {
                    if let Some(evicted) = remembered.pop_front() {
                        known.remove(&evicted);
                    }
                }
                written += 1;
            }
            persist_state(&remembered);
            let malformed = reader.malformed_lines();
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "events_written": written,
                        "malformed_lines": malformed,
                        "segment": writer.segment_index(),
                        "out": out.display().to_string(),
                        "execution_receipts": "skipped: the Sentry journal has no offline reader",
                    })
                );
            } else {
                println!(
                    "context feed: {written} events written, {malformed} malformed lines skipped (segment {:06} in {})",
                    writer.segment_index(),
                    out.display()
                );
                println!("sentry execution receipts: skipped (the journal has no offline reader)");
            }
            if once {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(poll_ms));
        }
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("archie: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Resolve the `[sentry.netwatch]` / `[sentry.chat_exports]` fallbacks once.
fn load_sentry_config(paths: &StatePaths) -> Result<archie_core::config::ArchieConfig, String> {
    archie_core::config::ArchieConfig::load(paths.main_config())
        .map_err(|e| format!("cannot load configuration: {e}"))
}

/// The notice floor from `[sentry.response]`, or the default (`warning`).
fn notice_policy(
    config: Option<&archie_core::config::ArchieConfig>,
) -> archie_workstation::live::notices::Policy {
    use archie_workstation::live::notices::{Policy, Severity};
    config
        .and_then(|c| c.sentry.response.as_ref())
        .and_then(|r| Severity::parse(&r.min_severity))
        .map_or_else(Policy::default, |min_severity| Policy { min_severity })
}

/// Lists the owner notice board, or acknowledges notices on it.
fn sentry_notices(
    paths: &StatePaths,
    all: bool,
    ack: Option<&str>,
    ack_all: bool,
    claim: bool,
    json: bool,
) -> std::process::ExitCode {
    use archie_workstation::live::notices::Board;
    let run = || -> Result<(), String> {
        let vault_dir = archie_workstation::live::cerebra_vault_dir(&paths.state_dir);
        let path = archie_workstation::live::notice_board_path(&vault_dir);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        if claim {
            let mut said = None;
            Board::update(&path, |board| {
                said = board.claim_unspoken(now_ms);
                said.is_some()
            })
            .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&said).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
        if ack.is_some() || ack_all {
            let mut changed = 0;
            Board::update(&path, |board| {
                changed = match ack {
                    Some(id) => board.acknowledge(id, now_ms),
                    None => board.acknowledge_all(now_ms),
                };
                changed > 0
            })
            .map_err(|e| e.to_string())?;
            if json {
                println!("{}", serde_json::json!({ "acknowledged": changed }));
            } else if changed == 0 {
                return Err(match ack {
                    Some(id) => format!("no single pending notice matches {id:?}"),
                    None => "nothing pending".to_owned(),
                });
            } else {
                println!("acknowledged {changed}");
            }
            return Ok(());
        }
        let board = Board::load(&path).map_err(|e| e.to_string())?;
        let shown: Vec<_> = board
            .notices
            .iter()
            .filter(|n| all || n.pending())
            .collect();
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&shown).map_err(|e| e.to_string())?
            );
        } else if shown.is_empty() {
            println!("no pending notices");
        } else {
            for notice in shown {
                let state = match (notice.acknowledged_ms, notice.spoken_ms) {
                    (Some(_), _) => "acknowledged",
                    (None, Some(_)) => "said",
                    (None, None) => "pending",
                };
                println!(
                    "{}  {:<7} {:<18} {:<12} {}  [{}]",
                    &notice.id[..12],
                    notice.severity.as_str(),
                    notice.rule,
                    state,
                    notice.detail,
                    notice.device,
                );
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("archie: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// One pass (or a poll loop) of the live alert follower. Entirely local: the
/// netwatch state directory and the follower vault are files. The pass runs
/// against an empty model registry, so on a node without a local model every
/// batch's refusal is `NoLocalModel` — the designed outcome, reported as such.
fn sentry_follow(
    paths: &StatePaths,
    state_dir: Option<&std::path::Path>,
    poll_ms: Option<u64>,
    once: bool,
    json: bool,
) -> std::process::ExitCode {
    let run = || -> Result<(), String> {
        // The config is needed for the state directory only when the flag is absent;
        // with the flag, a config that does not load costs only the notice floor.
        let config = match (state_dir, load_sentry_config(paths)) {
            (_, Ok(config)) => Some(config),
            (None, Err(e)) => return Err(e),
            (Some(_), Err(e)) => {
                eprintln!("archie: {e}; using the default notice floor (warning)");
                None
            }
        };
        let state_dir = match state_dir.map(std::path::PathBuf::from) {
            Some(dir) => dir,
            None => config
                .as_ref()
                .and_then(|c| c.sentry.netwatch.as_ref())
                .map(|n| std::path::PathBuf::from(&n.state_dir))
                .ok_or_else(|| {
                    "no netwatch state directory: pass --state-dir or configure [sentry.netwatch]"
                        .to_owned()
                })?,
        };
        let notice_policy = notice_policy(config.as_ref());
        let poll_ms = poll_ms.unwrap_or(5000);
        if !(500..=60000).contains(&poll_ms) {
            return Err("poll_ms must be within 500..=60000".to_owned());
        }
        let vault_dir = archie_workstation::live::cerebra_vault_dir(&paths.state_dir);
        loop {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64);
            let report = archie_workstation::live::follow_alerts_once(
                &state_dir,
                &vault_dir,
                notice_policy,
                now_ms,
            )
            .map_err(|e| format!("alert follower failed: {e}"))?;
            let refused = report.refused_no_local_model();
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "lines_read": report.lines_read,
                        "malformed_lines": report.malformed_lines,
                        "alerts_fed": report.alerts_fed,
                        "alerts_rejected": report.alerts_rejected,
                        "passes": report.passes.len(),
                        "refused_no_local_model": refused,
                        "journal_reset": report.journal_reset,
                        "rotation_reset": report.rotation_reset,
                        "notices_posted": report.notices_posted,
                        "notices_reset": report.notices_reset,
                    })
                );
            } else {
                println!(
                    "sentry follow: {} alerts fed ({} malformed lines, {} rejected), {} passes, {} refused without a local model, {} notices posted",
                    report.alerts_fed,
                    report.malformed_lines,
                    report.alerts_rejected,
                    report.passes.len(),
                    refused,
                    report.notices_posted,
                );
                if report.notices_reset {
                    eprintln!(
                        "archie: the notice board did not parse; it was set aside as notices.bad"
                    );
                }
            }
            if once {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(poll_ms));
        }
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("archie: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn focus(
    paths: &StatePaths,
    profile: &str,
    domain: &str,
    since_ms: i64,
    json: bool,
) -> std::process::ExitCode {
    let vault_dir = archie_workstation::live::cerebra_vault_dir(&paths.state_dir);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    match archie_workstation::live::focus_once(&vault_dir, profile, domain, since_ms, now_ms) {
        Ok(report) => {
            if json {
                println!("{}", serde_json::to_string(&report).unwrap_or_default());
            } else {
                println!(
                    "focus {} in {}: {} admitted, dropped {:?}, outcome {}",
                    report.profile,
                    report.domain,
                    report.admitted.len(),
                    report.dropped,
                    report.outcome
                );
                for item in &report.admitted {
                    println!(
                        "  p{:<3} w{:<3} {:<18} {}",
                        item.priority, item.weight, item.source, item.reference
                    );
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("archie: focus: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn wanda_salt(paths: &StatePaths, salt: Option<&str>) -> Result<String, String> {
    match salt {
        Some(salt) if !salt.is_empty() && salt.len() <= 128 => Ok(salt.to_owned()),
        Some(_) => Err("the salt must be 1..=128 bytes".to_owned()),
        None => load_sentry_config(paths)?
            .sentry
            .chat_exports
            .map(|c| c.salt)
            .ok_or_else(|| "no salt: pass --salt or configure [sentry.chat_exports]".to_owned()),
    }
}

fn print_watch_report(report: &archie_workstation::live::WatchReport, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "files_seen": report.files_seen,
                "files_ingested": report.files_ingested,
                "files_already_known": report.files_already_known,
                "messages_recorded": report.messages_recorded,
                "chats_recorded": report.chats_recorded,
                "failures": report.failures,
            })
        );
    } else {
        println!(
            "wanda ingest: {} files ingested ({} already known), {} messages, {} chat aggregates",
            report.files_ingested,
            report.files_already_known,
            report.messages_recorded,
            report.chats_recorded,
        );
        for failure in &report.failures {
            println!("  failed: {failure}");
        }
    }
}

/// Manually ingest one chat export file into Wanda's context. Local and
/// idempotent: the content-hash journal skips what is already ingested.
fn wanda_ingest(
    paths: &StatePaths,
    file: &std::path::Path,
    salt: Option<&str>,
    json: bool,
) -> std::process::ExitCode {
    let run = || -> Result<(), String> {
        let salt = wanda_salt(paths, salt)?;
        let vault_dir = archie_workstation::live::cerebra_vault_dir(&paths.state_dir);
        let report = archie_workstation::live::ingest_export_file(file, &vault_dir, &salt)
            .map_err(|e| format!("wanda ingest failed: {e}"))?;
        print_watch_report(&report, json);
        if report.files_ingested == 0 && report.files_already_known == 0 {
            return Err("nothing ingested".to_owned());
        }
        Ok(())
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("archie: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// One pass (or a poll loop) over the chat-export watch directory.
fn wanda_watch(
    paths: &StatePaths,
    dir: Option<&std::path::Path>,
    salt: Option<&str>,
    poll_ms: Option<u64>,
    once: bool,
    json: bool,
) -> std::process::ExitCode {
    let run = || -> Result<(), String> {
        let configured = if dir.is_none() || salt.is_none() {
            Some(load_sentry_config(paths)?)
        } else {
            None
        };
        let chat = configured
            .as_ref()
            .and_then(|c| c.sentry.chat_exports.as_ref());
        let dir = dir
            .map(std::path::PathBuf::from)
            .or_else(|| chat.map(|c| std::path::PathBuf::from(&c.watch_dir)))
            .ok_or_else(|| {
                "no watch directory: pass --dir or configure [sentry.chat_exports]".to_owned()
            })?;
        let salt = match salt {
            Some(salt) if !salt.is_empty() && salt.len() <= 128 => salt.to_owned(),
            Some(_) => return Err("the salt must be 1..=128 bytes".to_owned()),
            None => chat.map(|c| c.salt.clone()).ok_or_else(|| {
                "no salt: pass --salt or configure [sentry.chat_exports]".to_owned()
            })?,
        };
        let poll_ms = poll_ms.unwrap_or(5000);
        if !(500..=60000).contains(&poll_ms) {
            return Err("poll_ms must be within 500..=60000".to_owned());
        }
        let vault_dir = archie_workstation::live::cerebra_vault_dir(&paths.state_dir);
        loop {
            let report = archie_workstation::live::watch_exports_once(&dir, &vault_dir, &salt)
                .map_err(|e| format!("wanda watch failed: {e}"))?;
            print_watch_report(&report, json);
            if once {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(poll_ms));
        }
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("archie: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn vault_docs(
    paths: &StatePaths,
    dir: &std::path::Path,
    domain: &str,
    poll_ms: Option<u64>,
    once: bool,
    json: bool,
) -> std::process::ExitCode {
    let run = || -> Result<(), String> {
        let poll_ms = poll_ms.unwrap_or(5000);
        if !(500..=60000).contains(&poll_ms) {
            return Err("poll_ms must be within 500..=60000".to_owned());
        }
        let vault_dir = archie_workstation::live::cerebra_vault_dir(&paths.state_dir);
        loop {
            let report = archie_workstation::live::watch_documents_once(dir, &vault_dir, domain)
                .map_err(|e| format!("vault documents failed: {e}"))?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&report).map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "vault docs: {} recorded ({} briefs, {} restrictions, {} versions, {} notes), {} already known, {} failed",
                    report.documents_recorded,
                    report.briefs,
                    report.restrictions,
                    report.versions,
                    report.notes,
                    report.files_already_known,
                    report.failures.len()
                );
                for failure in &report.failures {
                    eprintln!("archie: {failure}");
                }
            }
            if once {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(poll_ms));
        }
    };
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("archie: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn review_and_execute(
    stream: &mut endpoint::ControlStream,
    request: &ControlRequest,
    json: bool,
) -> std::process::ExitCode {
    use std::io::{BufRead, IsTerminal, Read, Write};
    if !cfg!(target_os = "linux") || !std::io::stdin().is_terminal() {
        eprintln!("archie: Sentry execution requires an interactive Linux terminal; there is no unattended approval flag");
        return std::process::ExitCode::FAILURE;
    }
    let prepared = match request_over(stream, request).await {
        Ok(response) => response,
        Err(_) => {
            eprintln!("archie: preparation failed");
            return std::process::ExitCode::FAILURE;
        }
    };
    let ControlResponse::SentryPrepared { intent, digest } = prepared else {
        eprint!("{}", render(&prepared));
        return std::process::ExitCode::FAILURE;
    };
    if intent.digest().ok().as_deref() != Some(&digest) {
        eprintln!("archie: preparation digest mismatch");
        return std::process::ExitCode::FAILURE;
    }
    eprintln!("Review this exact operation (expires in at most 30 seconds):\n{}\nType the complete digest to approve, or Enter to cancel:\n{digest}", serde_json::to_string_pretty(&intent).unwrap_or_default());
    let _ = std::io::stderr().flush();
    let mut input = String::new();
    if std::io::stdin()
        .lock()
        .take(130)
        .read_line(&mut input)
        .is_err()
        || input.trim() != digest
    {
        let _ = request_over(stream, &ControlRequest::SentryCancel).await;
        eprintln!("archie: operation not approved");
        return std::process::ExitCode::FAILURE;
    }
    let response = match request_over(
        stream,
        &ControlRequest::SentryApprove {
            intent_id: intent.intent_id,
            digest,
        },
    )
    .await
    {
        Ok(response) => response,
        Err(_) => {
            eprintln!("archie: result unavailable; do not resend an ambiguous operation");
            return std::process::ExitCode::FAILURE;
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&response).unwrap_or_default()
        );
    } else {
        print!("{}", render(&response));
    }
    exit_code(&response)
}

fn exit_code(response: &ControlResponse) -> std::process::ExitCode {
    let unverified = matches!(response, ControlResponse::SentryResult(result) if result.status != archie_core::control::SentryExecutionStatus::Verified);
    let uncertain_receipt = matches!(response, ControlResponse::SentryReceipt(record) if record.result.as_ref().is_none_or(|result| result.status != archie_core::control::SentryExecutionStatus::Verified));
    if response.is_error() || unverified || uncertain_receipt {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

fn render(response: &ControlResponse) -> String {
    match response {
        ControlResponse::Status(status) => render_status(status),
        ControlResponse::Topology {
            capabilities,
            nodes,
            totals,
        } => render_topology(capabilities, nodes, totals),
        ControlResponse::Modules {
            modules,
            load_order,
        } => render_modules(modules, load_order),
        ControlResponse::Providers { providers, active } => render_providers(providers, active),
        ControlResponse::Peers { peers } => render_peers(peers),
        ControlResponse::Devices { devices } => render_devices(devices),
        ControlResponse::SentryPrinter { resource_id, snapshot } => format!(
            "printer: {resource_id}\nstate: {:?}\nUUID: {}\nobserved: {}\nidentity: matches configured UUID (self-reported)\n",
            snapshot.state, snapshot.uuid.as_deref().unwrap_or("unreported"), snapshot.observed_at.as_millis()
        ),
        ControlResponse::SentryHomeStatus { snapshot } => format!(
            "home: {}\nstate: {} {}\navailable: {}\nprovider reported: {}\nsource: HA cache (freshness not guaranteed)\n",
            snapshot.resource_id, snapshot.state, snapshot.unit.as_deref().unwrap_or(""),
            snapshot.available, snapshot.reported_at.as_millis()),
        ControlResponse::SentryPrinterDoctor {
            resource_id,
            tier,
            detail,
            planned_command_line,
        } => format!(
            "printer-doctor: {resource_id}\ntier: {tier}\ndetail: {detail}\nplan: {planned_command_line}\n(read-only probe; nothing was installed or executed)\n"
        ),
        ControlResponse::SentryPrepared { intent, digest } => format!("prepared {} for {}\ndigest: {digest}\n", intent.intent_id, intent.resource_id),
        ControlResponse::SentryResult(result) => format!("operation {}: {:?}\nreceipt: {}\n", result.operation_id, result.status, result.intent_id),
        ControlResponse::SentryCancelled => "Sentry operation cancelled\n".into(),
        ControlResponse::SentryReceipt(record) => match &record.result {
            Some(result) => format!("receipt {}: {:?}\noperation: {}\n", record.intent_id, result.status, record.operation_id),
            None => format!("receipt {}: Indeterminate\noperation: {}\nDispatch is durably fenced; inspect the device before issuing a new intent. This query never retries.\n", record.intent_id, record.operation_id),
        },
        ControlResponse::DeviceOperations { operations } => render_device_operations(operations),
        ControlResponse::CortexTrajectories { trajectories } => {
            render_cortex_trajectories(trajectories)
        }
        ControlResponse::Models { models } => render_models(models),
        ControlResponse::Boot(report) => report.render(),
        ControlResponse::Metrics(snapshot) => {
            let mut out = String::new();
            for (name, value) in &snapshot.counters {
                out.push_str(&format!("{name:<40} {value}\n"));
            }
            for (name, value) in &snapshot.gauges {
                out.push_str(&format!("{name:<40} {value}\n"));
            }
            if out.is_empty() {
                out.push_str("no metrics recorded yet\n");
            }
            out
        }
        ControlResponse::Audit { events } => {
            if events.is_empty() {
                return "no audit records\n".to_string();
            }
            let mut out = String::new();
            for event in events {
                let action = event.get("action").and_then(|v| v.as_str()).unwrap_or("?");
                let outcome = event.get("outcome").and_then(|v| v.as_str()).unwrap_or("?");
                let principal = event
                    .get("principal")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let resource = event
                    .get("resource")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let explanation = event
                    .get("explanation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                out.push_str(&format!(
                    "{outcome:<16} {action:<24} {principal} -> {resource}\n    {explanation}\n"
                ));
            }
            out
        }
        ControlResponse::Pong { node_id, version } => {
            format!("archied {version} is running (node {})\n", short(node_id))
        }
        ControlResponse::WorldResult { value } => format!("{value}\n"),
        ControlResponse::AiResult { outcome } => format!("{}\nreceipt={} status={:?}\n", outcome.text.as_deref().unwrap_or(""), outcome.receipt.receipt_id, outcome.receipt.status),
        ControlResponse::AiHarnessResult { outcome } => format!("Harness2 {}: {} stages; promotion={} (improvement_verified={})\n", outcome.result.status, outcome.receipts.len(), outcome.result.promotion.status, outcome.result.promotion.improvement_verified),
        ControlResponse::AiPolicy { paused, recorded } => {
            format!("emergency_pause={paused} recorded={recorded}\n")
        }
        ControlResponse::Error { message } => format!("error: {message}\n"),
    }
}

fn render_status(status: &StatusView) -> String {
    let mut out = String::new();
    let field = |out: &mut String, label: &str, value: &str| {
        out.push_str(&format!("{label:<22} {value}\n"));
    };

    out.push_str("ARCHIE OS\n\n");
    field(&mut out, "Node ID", &status.node_id);
    field(&mut out, "Hostname", &status.hostname);
    field(&mut out, "Profile", &status.profile);
    field(&mut out, "ArchieOS version", &status.archieos_version);
    field(
        &mut out,
        "System",
        &format!("{} {} ({})", status.os, status.kernel, status.architecture),
    );
    field(&mut out, "Uptime", &format_duration(status.uptime_ms));
    out.push('\n');

    field(
        &mut out,
        "CPU",
        &format!(
            "{} ({} physical / {} logical)",
            status.cpu_model, status.physical_cores, status.logical_cores
        ),
    );
    field(
        &mut out,
        "RAM",
        &format!(
            "{} free of {}",
            gib(status.memory_available_bytes),
            gib(status.memory_total_bytes)
        ),
    );
    field(
        &mut out,
        "Accelerators",
        &if status.accelerators.is_empty() {
            "none detected".to_string()
        } else {
            status.accelerators.join("\n                       ")
        },
    );
    field(
        &mut out,
        "Storage",
        &if status.storage.is_empty() {
            "none reported".to_string()
        } else {
            status.storage.join("\n                       ")
        },
    );
    field(
        &mut out,
        "Network",
        &if status.networks.is_empty() {
            "none reported".to_string()
        } else {
            status.networks.join("\n                       ")
        },
    );
    out.push('\n');

    field(
        &mut out,
        "Capabilities",
        &format!("{} advertised", status.capabilities.len()),
    );
    for capability in &status.capabilities {
        out.push_str(&format!("                       {capability}\n"));
    }
    if !status.capability_changes.is_empty() {
        field(
            &mut out,
            "Changed since boot",
            &status.capability_changes.join(" "),
        );
    }
    out.push('\n');

    field(
        &mut out,
        "Fabric",
        &format!("{} transport", status.fabric_transport),
    );
    field(
        &mut out,
        "Peers",
        &format!(
            "{} known, {} reachable",
            status.peers, status.reachable_peers
        ),
    );
    field(
        &mut out,
        "Modules",
        &format!(
            "{} loaded of {} installed",
            status.modules_loaded, status.modules_installed
        ),
    );
    field(
        &mut out,
        "Models",
        &if status.models_registered == 0 {
            "none registered".to_string()
        } else {
            format!("{} registered", status.models_registered)
        },
    );
    field(&mut out, "Tasks", &status.active_tasks.to_string());
    field(&mut out, "Health", &status.health);
    out
}

fn render_topology(
    capabilities: &BTreeMap<String, Vec<String>>,
    nodes: &[PeerView],
    totals: &TotalsView,
) -> String {
    let mut out = String::from("RESOURCE TOPOLOGY\n\n");

    out.push_str(&format!(
        "{} nodes ({} schedulable), {} logical cores, {} RAM",
        totals.nodes,
        totals.schedulable_nodes,
        totals.logical_cores,
        gib(totals.memory_bytes)
    ));
    if totals.accelerators > 0 {
        out.push_str(&format!(
            ", {} accelerators ({})",
            totals.accelerators,
            gib(totals.accelerator_bytes)
        ));
    }
    out.push_str("\n\nNODES\n");
    for node in nodes {
        out.push_str(&format!(
            "  {}  {:<20} {:<14} {:<10} {}\n",
            short(&node.node_id),
            node.hostname,
            node.profile,
            node.trust,
            if node.reachable {
                node.health.clone()
            } else {
                "unreachable".to_string()
            }
        ));
    }

    out.push_str("\nCAPABILITIES\n");
    if capabilities.is_empty() {
        out.push_str("  none advertised\n");
    }
    for (capability, providers) in capabilities {
        out.push_str(&format!("  {capability}\n"));
        for provider in providers {
            out.push_str(&format!("      {}\n", short(provider)));
        }
    }
    out
}

fn render_modules(modules: &[ModuleView], load_order: &[String]) -> String {
    if modules.is_empty() {
        return "no modules installed\n".to_string();
    }
    let mut out = String::from("MODULES\n\n");
    let width = modules
        .iter()
        .map(|m| m.id.len())
        .max()
        .unwrap_or(20)
        .max(20);

    for module in modules {
        out.push_str(&format!(
            "  {:<width$}  {:<10}  {:<14}  {}\n",
            module.id,
            module.version,
            module.state,
            module.explanation,
            width = width
        ));
    }

    if !load_order.is_empty() {
        out.push_str("\nLOAD ORDER\n");
        for (index, id) in load_order.iter().enumerate() {
            out.push_str(&format!("  {}. {id}\n", index + 1));
        }
    }
    out
}

fn render_providers(providers: &[ProviderView], active: &[String]) -> String {
    if providers.is_empty() {
        return "no data providers installed\n".to_string();
    }
    let mut out = String::from("DATA PROVIDERS\n\n");
    let width = providers
        .iter()
        .map(|p| p.id.len())
        .max()
        .unwrap_or(20)
        .max(20);

    for provider in providers {
        out.push_str(&format!(
            "  {:<width$}  {:<14}  {:<16}  {}\n",
            provider.id,
            provider.status,
            provider.capability,
            provider.explanation,
            width = width
        ));
    }

    out.push_str(&format!(
        "\n{} of {} will actually run; --json shows records and documented blind spots\n",
        active.len(),
        providers.len()
    ));
    out
}

fn render_peers(peers: &[PeerView]) -> String {
    if peers.is_empty() {
        // A lone node is a normal state, not an error, and the message should not read
        // like one.
        return "no peers discovered\n\nThis node is running alone on the fabric.\n".to_string();
    }
    let mut out = String::from("PEERS\n\n");
    for peer in peers {
        out.push_str(&format!(
            "  {}  {:<20} {:<14} trust={:<10} {}\n",
            short(&peer.node_id),
            peer.hostname,
            peer.profile,
            peer.trust,
            if peer.reachable {
                peer.health.clone()
            } else {
                "unreachable".to_string()
            }
        ));
        out.push_str(&format!(
            "      capabilities: {}\n",
            peer.capabilities.join(", ")
        ));
    }
    out
}

fn render_devices(devices: &[DeviceView]) -> String {
    if devices.is_empty() {
        return "no paired-device inventory records\n\nNo device adapter or command channel is active.\n"
            .to_string();
    }

    let mut out = String::from("DEVICE INVENTORY\n\n");
    for device in devices {
        out.push_str(&format!(
            "  {:<24} {:<16} pairing={:<10} key={}\n",
            device.device_reference, device.class, device.pairing, device.public_key_fingerprint,
        ));
        out.push_str(&format!(
            "      first seen={}  last seen={}\n",
            device.first_seen.as_millis(),
            device.last_seen.as_millis(),
        ));
    }
    out
}

fn render_device_operations(operations: &[DeviceOperationView]) -> String {
    if operations.is_empty() {
        return "no durable device operations\n\nThis view does not create, approve, cancel, or dispatch a command.\n"
            .to_string();
    }

    let mut out = String::from("DEVICE OPERATIONS (READ-ONLY)\n\n");
    for operation in operations {
        out.push_str(&format!(
            "  #{:<6} {:<24} state={:<10} trace={} digest={}\n",
            operation.operation_id,
            operation.device_reference,
            operation.state,
            operation.trace_fingerprint,
            operation.operation_sha256_fingerprint,
        ));
        out.push_str(&format!(
            "      prepared={}\n",
            operation.prepared_at.as_millis()
        ));
    }
    out
}

fn render_cortex_trajectories(trajectories: &[CortexTrajectoryView]) -> String {
    if trajectories.is_empty() {
        return "no Cortex correction trajectories\n\nThe graph writer/parser and offline trainer are not active from this view.\n"
            .to_string();
    }
    let mut out = String::from("CORTEX TRAJECTORIES (READ-ONLY)\n\n");
    for trajectory in trajectories {
        out.push_str(&format!(
            "  {:<24} {:<28} model={} payload={}\n",
            trajectory.trajectory_id,
            trajectory.correction_kind,
            trajectory.model_id,
            trajectory.payload_sha256_fingerprint,
        ));
        out.push_str(&format!(
            "      graph {} -> {}  selected={} omitted={} captured={}\n",
            trajectory.source_graph_snapshot_id,
            trajectory.resulting_graph_snapshot_id,
            trajectory.selected_context_count,
            trajectory.omitted_candidate_count,
            trajectory.captured_at.as_millis(),
        ));
    }
    out
}

fn render_models(models: &[ModelView]) -> String {
    if models.is_empty() {
        return "no models installed\n\nModel manifests are read from the node's models \
                directory at boot, and `archie provider add` connects a local or cloud \
                backend. See docs/ROUTER.md.\n"
            .to_string();
    }
    let mut out = String::from("MODELS\n\n");
    for model in models {
        out.push_str(&format!(
            "  {:<28} {:<18} {:<10} {:<10} ctx={} privacy<={}\n",
            model.id,
            model.provider,
            model.locality,
            model.availability,
            model.context_window,
            model.max_privacy_grade
        ));
    }
    out
}

/// First 12 hex characters of a node id — enough to be unambiguous in a table, short
/// enough to read.
fn short(node_id: &str) -> String {
    node_id.chars().take(12).collect()
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1u64 << 30) as f64)
}

fn format_duration(millis: i64) -> String {
    if millis < 0 {
        return "unknown".to_string();
    }
    let seconds = millis / 1000;
    let (days, hours, minutes) = (
        seconds / 86_400,
        (seconds % 86_400) / 3600,
        (seconds % 3600) / 60,
    );
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {}s", seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> StatusView {
        StatusView {
            node_id: "a".repeat(64),
            hostname: "brain-01".into(),
            profile: "brain-node".into(),
            archieos_version: "0.1.0".into(),
            os: "Arch Linux".into(),
            kernel: "6.10.0".into(),
            architecture: "x86_64".into(),
            uptime_ms: 3_725_000,
            cpu_model: "AMD Ryzen 9".into(),
            logical_cores: 16,
            physical_cores: 8,
            memory_total_bytes: 32 << 30,
            memory_available_bytes: 20 << 30,
            accelerators: vec!["NVIDIA RTX 3070 (8.0 GiB)".into()],
            storage: vec!["/ btrfs 400/900 GiB free".into()],
            networks: vec!["enp5s0 Lan 10.0.0.2".into()],
            capabilities: vec!["compute.cpu".into(), "compute.cuda".into()],
            peers: 2,
            reachable_peers: 1,
            modules_loaded: 3,
            modules_installed: 5,
            models_registered: 0,
            active_tasks: 0,
            health: "ok".into(),
            fabric_transport: "udp".into(),
            capability_changes: vec!["-compute.cuda".into()],
            // The rendering does not use it yet; a node with no stated place is the
            // ordinary case and the one worth having in the fixture.
            location: None,
        }
    }

    #[test]
    fn status_renders_every_field_the_spec_requires() {
        let rendered = render_status(&status());
        for expected in [
            "Node ID",
            "Hostname",
            "Profile",
            "ArchieOS version",
            "CPU",
            "RAM",
            "Accelerators",
            "Storage",
            "Network",
            "Capabilities",
            "Peers",
            "Modules",
            "Models",
            "Health",
        ] {
            assert!(rendered.contains(expected), "status is missing {expected}");
        }
        assert!(rendered.contains("RTX 3070"));
        assert!(rendered.contains("2 known, 1 reachable"));
        assert!(rendered.contains("3 loaded of 5 installed"));
    }

    #[test]
    fn status_surfaces_hardware_that_disappeared() {
        let rendered = render_status(&status());
        assert!(
            rendered.contains("-compute.cuda"),
            "a GPU that vanished since last boot must be visible"
        );
    }

    #[test]
    fn an_empty_peer_list_reads_as_normal_not_broken() {
        let rendered = render_peers(&[]);
        assert!(rendered.contains("running alone"));
        assert!(!rendered.to_lowercase().contains("error"));
    }

    #[test]
    fn an_empty_model_list_says_what_to_do_next() {
        let rendered = render_models(&[]);
        assert!(rendered.contains("no models installed"));
        assert!(rendered.contains("docs/ROUTER.md"));
    }

    #[test]
    fn device_inventory_is_visible_without_implying_a_live_adapter() {
        let rendered = render_devices(&[DeviceView {
            device_reference: "living-room-tv".into(),
            class: "television".into(),
            pairing: "paired".into(),
            public_key_fingerprint: "a1b2c3d4e5f60708".into(),
            first_seen: archie_types::Timestamp::from_millis(1),
            last_seen: archie_types::Timestamp::from_millis(2),
        }]);
        assert!(rendered.contains("living-room-tv"));
        assert!(rendered.contains("pairing=paired"));
        assert!(rendered.contains("a1b2c3d4e5f60708"));

        let empty = render_devices(&[]);
        assert!(empty.contains("No device adapter or command channel is active"));
    }

    #[test]
    fn device_operations_render_as_real_history_without_a_command_channel() {
        let rendered = render_device_operations(&[DeviceOperationView {
            operation_id: 42,
            device_reference: "living-room-tv".into(),
            state: "cancelled".into(),
            trace_fingerprint: "e1f2a3b4c5d6".into(),
            operation_sha256_fingerprint: "bbbbbbbbbbbbbbbb".into(),
            prepared_at: archie_types::Timestamp::from_millis(1),
        }]);
        assert!(rendered.contains("#42"));
        assert!(rendered.contains("state=cancelled"));
        assert!(rendered.contains("READ-ONLY"));

        let empty = render_device_operations(&[]);
        assert!(empty.contains("does not create, approve, cancel, or dispatch"));
    }

    #[test]
    fn modules_render_their_exclusion_reason() {
        let modules = vec![ModuleView {
            id: "science.physics".into(),
            version: "0.1.0".into(),
            state: "unsatisfied".into(),
            explanation: "node does not provide: compute.cuda".into(),
            dependencies: vec!["core.memory".into()],
        }];
        let rendered = render_modules(&modules, &[]);
        assert!(rendered.contains("science.physics"));
        assert!(
            rendered.contains("compute.cuda"),
            "an operator must be able to see WHY a module did not load"
        );
    }

    #[test]
    fn providers_render_their_exclusion_reason() {
        let providers = vec![
            ProviderView {
                id: "usgs.earthquakes".into(),
                title: "USGS Earthquake Hazards Program".into(),
                capability: "world.seismic".into(),
                status: "available".into(),
                explanation: "available".into(),
                records: vec!["observation".into(), "event".into()],
                known_gaps: vec![],
            },
            ProviderView {
                id: "nasa.firms".into(),
                title: "NASA FIRMS active fire detections".into(),
                capability: "world.wildfire".into(),
                status: "no-credentials".into(),
                explanation: "secret \"nasa.firms.map_key\" is not configured".into(),
                records: vec!["observation".into()],
                known_gaps: vec!["a detection is a thermal anomaly, not a confirmed fire".into()],
            },
        ];
        let rendered = render_providers(&providers, &["usgs.earthquakes".into()]);
        assert!(rendered.contains("usgs.earthquakes"));
        assert!(rendered.contains("nasa.firms"));
        assert!(
            rendered.contains("nasa.firms.map_key"),
            "an operator must be able to see WHY a provider is not running"
        );
        assert!(rendered.contains("1 of 2 will actually run"));
    }

    #[test]
    fn no_providers_installed_says_so_plainly() {
        assert_eq!(render_providers(&[], &[]), "no data providers installed\n");
    }

    #[test]
    fn module_load_order_is_numbered() {
        let rendered = render_modules(
            &[ModuleView {
                id: "core.archie".into(),
                version: "0.1.0".into(),
                state: "loaded".into(),
                explanation: "loaded".into(),
                dependencies: vec![],
            }],
            &["core.memory".into(), "core.archie".into()],
        );
        assert!(rendered.contains("1. core.memory"));
        assert!(rendered.contains("2. core.archie"));
    }

    #[test]
    fn topology_reports_totals_and_the_capability_index() {
        let mut capabilities = BTreeMap::new();
        capabilities.insert("compute.cuda".to_string(), vec!["b".repeat(64)]);
        let rendered = render_topology(
            &capabilities,
            &[],
            &TotalsView {
                nodes: 3,
                schedulable_nodes: 2,
                logical_cores: 48,
                memory_bytes: 96 << 30,
                accelerator_bytes: 24 << 30,
                accelerators: 3,
            },
        );
        assert!(rendered.contains("3 nodes (2 schedulable)"));
        assert!(rendered.contains("48 logical cores"));
        assert!(rendered.contains("compute.cuda"));
        assert!(rendered.contains("3 accelerators"));
    }

    #[test]
    fn node_ids_are_shortened_consistently() {
        assert_eq!(short(&"a".repeat(64)).len(), 12);
        assert_eq!(short("short"), "short");
    }

    #[test]
    fn durations_render_at_a_sensible_scale() {
        assert_eq!(format_duration(45_000), "45s");
        assert_eq!(format_duration(125_000), "2m 5s");
        assert_eq!(format_duration(3_725_000), "1h 2m");
        assert_eq!(format_duration(90_061_000), "1d 1h 1m");
        assert_eq!(format_duration(-1), "unknown");
    }

    #[test]
    fn an_error_response_renders_and_fails() {
        let response = ControlResponse::error("something went wrong");
        assert!(render(&response).contains("something went wrong"));
    }

    #[test]
    fn every_command_maps_to_a_request() {
        for (command, expected) in [
            (Command::Status, "status"),
            (Command::Topology, "topology"),
            (Command::Modules, "modules"),
            (Command::Providers, "providers"),
            (Command::Peers, "peers"),
            (Command::Devices, "devices"),
            (Command::DeviceOperations { limit: 5 }, "device-operations"),
            (
                Command::CortexTrajectories { limit: 5 },
                "cortex-trajectories",
            ),
            (Command::Models, "models"),
            (Command::Boot, "boot"),
            (Command::Metrics, "metrics"),
            (Command::Ping, "ping"),
            (Command::Audit { limit: 5 }, "audit"),
        ] {
            assert_eq!(
                command
                    .to_request()
                    .expect("this command talks to the daemon")
                    .name(),
                expected
            );
        }
    }

    /// `enrol` writes files and must work with the daemon down, because a node that
    /// belongs to no fabric is one whose daemon is reporting a degraded boot stage.
    /// Routing it through the control socket would mean telling somebody to start the
    /// thing they are trying to fix.
    #[test]
    fn enrol_asks_the_daemon_for_nothing() {
        assert!(Command::Enrol { owner_key: None }.to_request().is_none());
        assert!(Command::Enrol {
            owner_key: Some("aabb".into())
        }
        .to_request()
        .is_none());
    }

    #[test]
    fn workstation_commands_are_local() {
        for command in [
            Command::Doctor,
            Command::Setup,
            Command::Start,
            Command::Stop,
        ] {
            assert!(command.to_request().is_none());
        }
    }

    #[test]
    fn iris_model_verification_is_local() {
        assert!(Command::Iris {
            command: IrisCommand::VerifyModel {
                manifest: None,
                checkpoint: None,
            },
        }
        .to_request()
        .is_none());
    }

    /// The IoT key/launch commands prepare typed intents; they never carry an
    /// endpoint and never bypass the reviewed flow.
    #[test]
    fn sentry_iot_commands_map_to_typed_intents() {
        use archie_core::control::SentryCommand;
        let key = Command::Sentry {
            action: SentryIoT::Key {
                resource_id: "living-room-tv".into(),
                key: RemoteKeyArg::PowerOff,
            },
        }
        .to_request()
        .unwrap();
        assert_eq!(key.name(), "sentry-prepare");
        assert!(key.is_mutating());
        assert_eq!(
            key,
            ControlRequest::SentryPrepare {
                resource_id: "living-room-tv".into(),
                command: SentryCommand::RemoteKey {
                    key: archie_core::control::SentryRemoteKey::PowerOff,
                },
            }
        );
        let launch = Command::Sentry {
            action: SentryIoT::Launch {
                resource_id: "living-room-tv".into(),
                app_id: "12".into(),
            },
        }
        .to_request()
        .unwrap();
        assert_eq!(
            launch,
            ControlRequest::SentryPrepare {
                resource_id: "living-room-tv".into(),
                command: SentryCommand::AppLaunch {
                    app_id: "12".into()
                },
            }
        );
        let doctor = Command::Sentry {
            action: SentryIoT::PrinterDoctor {
                resource_id: "office-printer".into(),
            },
        }
        .to_request()
        .unwrap();
        assert_eq!(doctor.name(), "sentry-printer-doctor");
        assert!(!doctor.is_mutating(), "printer-doctor is a read-only probe");
    }

    /// `print` stages locally before its intent exists, and `context-feed` is a
    /// purely local command: neither maps directly onto a daemon request.
    #[test]
    fn print_and_context_feed_are_not_plain_requests() {
        assert!(Command::Sentry {
            action: SentryIoT::Print {
                resource_id: "office-printer".into(),
                file: "doc.pdf".into(),
                pages: 1,
            },
        }
        .to_request()
        .is_none());
        assert!(Command::ContextFeed {
            watchdog_dir: None,
            out: None,
            lan: None,
            passive: None,
            poll_ms: None,
            once: true,
        }
        .to_request()
        .is_none());
    }

    /// The device view reads files and nothing else; it never reaches the daemon.
    #[test]
    fn sentry_devices_is_not_a_daemon_request() {
        assert!(Command::SentryDevices {
            inventory: None,
            netwatch_state: None,
            passive: None,
        }
        .to_request()
        .is_none());
    }

    /// Offline smoke test: fixture files only (the three sources describing one
    /// device are fixtures, not a real network), merged without any daemon.
    #[test]
    fn sentry_devices_merges_fixture_files_into_one_device() {
        let dir =
            std::env::temp_dir().join(format!("archie-sentry-devices-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inventory = dir.join("lan.json");
        let state = dir.join("state.json");
        let passive = dir.join("passive.tsv");
        std::fs::write(
            &inventory,
            r#"{"format":"archie-lan-inventory-v1","scanned_unix":1790000000,"devices":[{"address":"192.168.1.40","class":"television","name":"Living","manufacturer":"Roku","model":"3941X","services":["_airplay._tcp"],"seen_by":["ssdp","mdns"],"state":{"showing":"Netflix"}}]}"#,
        )
        .unwrap();
        std::fs::write(
            &state,
            r#"{"format":"archie-netwatch-state-v1","started_ms":1789999000000,"devices":{"54:4e:f0:07:3d:e8":{"first_seen_ms":1789999000000,"last_seen_ms":1790000005000,"addresses":["192.168.1.40"],"hostname":"roku-living","frames":12,"bytes":3456,"asked":["api.roku.com"],"sent_to":{"api.roku.com":3456}}},"owners":{"192.168.1.40":"54:4e:f0:07:3d:e8"}}"#,
        )
        .unwrap();
        std::fs::write(
            &passive,
            "192.168.1.40\t5353\tLiving._roku-ecp._tcp.local\n",
        )
        .unwrap();

        let json =
            sentry_devices_render(Some(&inventory), Some(&state), Some(&passive), true).unwrap();
        let view: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            view["devices"].as_array().unwrap().len(),
            1,
            "one device, told by three sources"
        );
        let device = &view["devices"][0];
        assert_eq!(device["mac"], "54:4e:f0:07:3d:e8");
        let sources: Vec<&str> = device["sources"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(sources, ["lan-scan", "netwatch", "passive-import"]);

        let human =
            sentry_devices_render(Some(&inventory), Some(&state), Some(&passive), false).unwrap();
        assert!(human.contains("devices: 1 (1 by hardware address, 0 by address only)"));
        assert!(human
            .contains("54:4e:f0:07:3d:e8  192.168.1.40  Living  lan-scan+netwatch+passive-import"));

        assert!(
            sentry_devices_render(None, None, None, false).is_err(),
            "at least one source is required"
        );
        assert!(sentry_devices_render(Some(&dir.join("missing.json")), None, None, false).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The new live-wiring verbs parse to the exact typed commands they mean.
    #[test]
    fn sentry_node_verbs_parse_to_typed_commands() {
        use archie_core::control::{ControlRequest, SentryCommand};

        let cli = Cli::try_parse_from(["archie", "sentry-node", "wake", "living-pc"]).unwrap();
        let Some(ControlRequest::SentryPrepare {
            resource_id,
            command,
        }) = cli.command.to_request()
        else {
            panic!("wake prepares an intent");
        };
        assert_eq!(resource_id, "living-pc");
        assert_eq!(command, SentryCommand::WakeHost);

        let cli = Cli::try_parse_from(["archie", "sentry-node", "lock", "pixel"]).unwrap();
        let Some(ControlRequest::SentryPrepare { command, .. }) = cli.command.to_request() else {
            panic!("lock prepares an intent");
        };
        assert_eq!(command, SentryCommand::SessionLock { locked: true });

        let cli = Cli::try_parse_from(["archie", "sentry-node", "unlock", "pixel"]).unwrap();
        let Some(ControlRequest::SentryPrepare { command, .. }) = cli.command.to_request() else {
            panic!("unlock prepares an intent");
        };
        assert_eq!(command, SentryCommand::SessionLock { locked: false });

        let cli = Cli::try_parse_from(["archie", "sentry-node", "volume", "pixel", "40"]).unwrap();
        let Some(ControlRequest::SentryPrepare { command, .. }) = cli.command.to_request() else {
            panic!("volume prepares an intent");
        };
        assert_eq!(command, SentryCommand::VolumeSet { percent: 40 });

        for (verb, blocked) in [("block", true), ("unblock", false)] {
            let cli = Cli::try_parse_from([
                "archie",
                "sentry-node",
                verb,
                "home-router",
                "de:ad:be:ef:00:02",
            ])
            .unwrap();
            let Some(ControlRequest::SentryPrepare {
                resource_id,
                command,
            }) = cli.command.to_request()
            else {
                panic!("{verb} prepares an intent");
            };
            assert_eq!(resource_id, "home-router");
            assert_eq!(
                command,
                SentryCommand::DeviceBlock {
                    mac: "de:ad:be:ef:00:02".into(),
                    blocked,
                }
            );
        }

        let cli = Cli::try_parse_from([
            "archie",
            "sentry-node",
            "launch",
            "pixel",
            "com.example.app",
        ])
        .unwrap();
        let Some(ControlRequest::SentryPrepare { command, .. }) = cli.command.to_request() else {
            panic!("launch prepares an intent");
        };
        assert_eq!(
            command,
            SentryCommand::AppLaunch {
                app_id: "com.example.app".into()
            }
        );

        // Volume is bounded at the CLI, before any intent exists.
        assert!(Cli::try_parse_from(["archie", "sentry-node", "volume", "pixel", "101"]).is_err());
        assert!(Cli::try_parse_from(["archie", "sentry-node", "wake"]).is_err());
    }

    /// `focus` is a local command over the vault; it never reaches the daemon.
    #[test]
    fn focus_is_local_and_defaults_to_the_personal_domain() {
        let cli = Cli::try_parse_from(["archie", "focus", "pantalla-1"]).unwrap();
        assert!(cli.command.to_request().is_none());
        let Command::Focus {
            profile,
            domain,
            since_ms,
        } = cli.command
        else {
            panic!("focus parses to Focus");
        };
        assert_eq!(
            (profile.as_str(), domain.as_str(), since_ms),
            ("pantalla-1", "personal/owner", 0)
        );
    }

    /// Chromecast verbs prepare reviewed intents; the probe is local.
    #[test]
    fn cast_verbs_parse_to_typed_commands_and_the_probe_stays_local() {
        use archie_core::control::{ControlRequest, SentryCommand};
        for (argv, expected) in [
            (vec!["stop", "living-cast"], SentryCommand::AppStop),
            (
                vec!["volume", "living-cast", "35"],
                SentryCommand::VolumeSet { percent: 35 },
            ),
            (
                vec!["mute", "living-cast"],
                SentryCommand::MuteSet { muted: true },
            ),
            (
                vec!["unmute", "living-cast"],
                SentryCommand::MuteSet { muted: false },
            ),
            (
                vec!["launch", "living-cast", "CC1AD845"],
                SentryCommand::AppLaunch {
                    app_id: "CC1AD845".into(),
                },
            ),
        ] {
            let cli = Cli::try_parse_from(["archie", "sentry"].into_iter().chain(argv)).unwrap();
            let Some(ControlRequest::SentryPrepare {
                resource_id,
                command,
            }) = cli.command.to_request()
            else {
                panic!("cast verbs prepare an intent");
            };
            assert_eq!(resource_id, "living-cast");
            assert_eq!(command, expected);
        }
        assert!(Cli::try_parse_from(["archie", "sentry", "volume", "c", "101"]).is_err());
        let probe =
            Cli::try_parse_from(["archie", "sentry", "cast-probe", "192.168.1.30"]).unwrap();
        assert!(probe.command.to_request().is_none());
    }

    /// The follower and Wanda verbs are local commands: they never reach the
    /// daemon, like `context-feed` and `sentry-devices`.
    #[test]
    fn follower_and_wanda_verbs_are_local_and_parse() {
        let cli = Cli::try_parse_from(["archie", "sentry-follow", "--once"]).unwrap();
        assert!(cli.command.to_request().is_none());

        let cli = Cli::try_parse_from(["archie", "wanda-ingest", "chat.txt"]).unwrap();
        assert!(cli.command.to_request().is_none());

        let cli = Cli::try_parse_from([
            "archie",
            "wanda-watch",
            "--dir",
            "/tmp/exports",
            "--salt",
            "fixture-salt",
            "--poll-ms",
            "1000",
        ])
        .unwrap();
        assert!(cli.command.to_request().is_none());

        // wanda-ingest needs the file argument.
        assert!(Cli::try_parse_from(["archie", "wanda-ingest"]).is_err());
    }
}
