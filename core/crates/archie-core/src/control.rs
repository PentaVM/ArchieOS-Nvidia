//! The control protocol between `archie` (CLI) and `archied` (daemon).
//!
//! # Wire format
//!
//! Newline-delimited JSON. One request per line, one response per line. Chosen over
//! anything more elaborate because it is debuggable with `socat` at three in the
//! morning, which is when a control plane actually needs inspecting.
//!
//! # Transport and authorisation
//!
//! On Linux the endpoint is a Unix domain socket at `/run/archie/archied.sock`, and
//! access is controlled by filesystem permissions — the socket is owned by the archie
//! group, so membership *is* the authorisation check. On Windows (development only) it
//! falls back to a loopback TCP port published in `/run/archie/archied.endpoint`.
//!
//! Read-only queries are unauthenticated beyond socket access. Anything that changes
//! state carries an explicit marker. Sentry mutations require an allowed kernel UID,
//! the same foreground terminal and a single-use review on the same Unix connection.
//! Windows loopback and the context-free dispatcher cannot provide that authority.

use crate::boot::BootReport;
use archie_observability::MetricSnapshot;
use archie_policy::{
    Action, CapabilityRequest, Constraints, Lease, LeaseRejection, Operation, Principal, Resource,
    Scope,
};
use archie_state::{DeviceClass, DeviceOperationRecord, DeviceOperationState, DeviceRecord};
use archie_types::{
    DeviceAdapterClass, DeviceAdapterManifest, ModuleManifest, TaskId, Timestamp, TraceId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Largest single control message accepted, to bound what a client can make the daemon
/// allocate.
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 1024 * 1024;
pub use archie_sentry::contracts::Command as SentryCommand;
pub use archie_sentry::contracts::PrintFormat as SentryPrintFormat;
pub use archie_sentry::contracts::RemoteKeyName as SentryRemoteKey;
pub use archie_sentry::engine::ExecutionStatus as SentryExecutionStatus;
/// The IRIS context-feed builders, re-exported so the CLI can run a feed pass
/// without a direct archie-sentry dependency.
pub use archie_sentry::feed as sentry_feed;

/// A bounded, operator-scoped Nmap survey profile. These are inventory operations, not
/// vulnerability exploitation, credential testing, arbitrary NSE script execution, or
/// a general command runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NmapSurveyProfile {
    /// ARP/ICMP-style host discovery only; does not enumerate ports.
    HostDiscovery,
    /// A low-rate, light service inventory of the common 100 TCP ports.
    ServiceInventory,
}

/// Fully structured planned Nmap invocation. It contains no shell fragment and is not
/// executable by itself; a later sandboxed runner must bind this plan to an authenticated
/// local-network engagement and persist its evidence before starting a process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NmapSurveyPlan {
    profile: NmapSurveyProfile,
    approved_private_cidrs: Vec<String>,
    arguments: Vec<String>,
    estimated_addresses: u64,
}

impl NmapSurveyPlan {
    /// Construct a defensive survey only for explicitly declared RFC1918 IPv4 CIDRs.
    /// The 65,536-address ceiling preserves a whole home/small-office network survey
    /// without allowing a mistaken `0.0.0.0/0` into an unattended background worker.
    pub fn build(
        profile: NmapSurveyProfile,
        approved_private_cidrs: Vec<String>,
    ) -> std::result::Result<Self, NetworkSurveyError> {
        if approved_private_cidrs.is_empty() || approved_private_cidrs.len() > 32 {
            return Err(NetworkSurveyError::InvalidScope);
        }
        let mut canonical_cidrs = Vec::with_capacity(approved_private_cidrs.len());
        let mut estimated_addresses = 0_u64;
        let mut unique = BTreeSet::new();
        for cidr in &approved_private_cidrs {
            let (address, prefix, addresses) = private_cidr(cidr)?;
            if !unique.insert((address, prefix)) {
                return Err(NetworkSurveyError::DuplicateScope);
            }
            estimated_addresses = estimated_addresses.saturating_add(addresses);
            canonical_cidrs.push(format!("{address}/{prefix}"));
        }
        if estimated_addresses > 65_536 {
            return Err(NetworkSurveyError::ScopeTooLarge);
        }

        let mut arguments = vec![
            "-n".into(),
            "--reason".into(),
            "-T2".into(),
            "--max-retries".into(),
            "1".into(),
            "--host-timeout".into(),
            "30s".into(),
            "--max-rate".into(),
            "50".into(),
        ];
        match profile {
            NmapSurveyProfile::HostDiscovery => arguments.push("-sn".into()),
            NmapSurveyProfile::ServiceInventory => arguments.extend([
                "-sV".into(),
                "--version-light".into(),
                "--top-ports".into(),
                "100".into(),
            ]),
        }
        arguments.push("--".into());
        arguments.extend(canonical_cidrs.iter().cloned());
        Ok(Self {
            profile,
            approved_private_cidrs: canonical_cidrs,
            arguments,
            estimated_addresses,
        })
    }

    pub fn profile(&self) -> NmapSurveyProfile {
        self.profile
    }

    pub fn approved_private_cidrs(&self) -> &[String] {
        &self.approved_private_cidrs
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub fn estimated_addresses(&self) -> u64 {
        self.estimated_addresses
    }

    /// Reject deserialized or otherwise tampered plans before their scope can be
    /// consumed. A future runner must call this as part of its engagement binding.
    pub fn validate(&self) -> std::result::Result<(), NetworkSurveyError> {
        let rebuilt = Self::build(self.profile, self.approved_private_cidrs.clone())?;
        if rebuilt.arguments != self.arguments
            || rebuilt.estimated_addresses != self.estimated_addresses
            || rebuilt.approved_private_cidrs != self.approved_private_cidrs
        {
            return Err(NetworkSurveyError::InvalidPlan);
        }
        Ok(())
    }

    /// Merge completed survey observations into deterministic candidate inventory
    /// results. This validates the complete scope again and cannot carry an observation
    /// from an outside address into a local-network result.
    pub fn classify_observations(
        &self,
        observations: &[NetworkAssetObservation],
    ) -> std::result::Result<Vec<ClassifiedNetworkAsset>, NetworkSurveyError> {
        self.validate()?;
        if observations.len() as u64 > self.estimated_addresses {
            return Err(NetworkSurveyError::TooManyObservations);
        }
        let scopes = self
            .approved_private_cidrs
            .iter()
            .map(|cidr| private_cidr(cidr))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut addresses = BTreeSet::new();
        let mut assets = Vec::with_capacity(observations.len());
        for observation in observations {
            if !addresses.insert(observation.ipv4_address) {
                return Err(NetworkSurveyError::DuplicateObservation);
            }
            if !scopes.iter().any(|(network, prefix, _)| {
                ipv4_cidr_contains(*network, *prefix, observation.ipv4_address)
            }) {
                return Err(NetworkSurveyError::ObservationOutsideScope);
            }
            assets.push(ClassifiedNetworkAsset {
                ipv4_address: observation.ipv4_address,
                candidates: analytically_classify_network_asset(
                    &self.validated_observation_evidence(&observation.evidence)?,
                ),
            });
        }
        assets.sort_by_key(|asset| asset.ipv4_address);
        Ok(assets)
    }

    /// Compare two valid observations of this exact declared network scope. A missing
    /// observation is reported as "no longer observed", never as proof that a device
    /// is offline or untrusted.
    pub fn compare_observations(
        &self,
        baseline: &[NetworkAssetObservation],
        current: &[NetworkAssetObservation],
    ) -> std::result::Result<Vec<NetworkInventoryChange>, NetworkSurveyError> {
        let baseline = self.classify_observations(baseline)?;
        let current = self.classify_observations(current)?;
        Ok(diff_network_inventory(&baseline, &current))
    }

    fn validated_observation_evidence(
        &self,
        evidence: &[SourcedNetworkEvidence],
    ) -> std::result::Result<Vec<NetworkEvidence>, NetworkSurveyError> {
        let mut normalized = Vec::with_capacity(evidence.len());
        for item in evidence {
            let allowed = matches!(
                (self.profile, item.source, item.evidence),
                (
                    NmapSurveyProfile::ServiceInventory,
                    NetworkEvidenceSource::NmapServiceInventory,
                    NetworkEvidence::OpenTcpPort(_),
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveMdns,
                    NetworkEvidence::MdnsRoku
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveMdns,
                    NetworkEvidence::MdnsGoogleCast
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveMdns,
                    NetworkEvidence::MdnsIpp
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveMdns,
                    NetworkEvidence::MdnsRtsp
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveMdns,
                    NetworkEvidence::MdnsHomeAssistant
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveSsdp,
                    NetworkEvidence::SsdpInternetGatewayDevice,
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveSsdp,
                    NetworkEvidence::SsdpMediaRenderer
                ) | (
                    _,
                    NetworkEvidenceSource::PassiveRouterAdvertisement,
                    NetworkEvidence::RouterAdvertisement,
                )
            );
            if !allowed {
                return Err(NetworkSurveyError::EvidenceSourceMismatch);
            }
            normalized.push(item.evidence);
        }
        Ok(normalized)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NetworkSurveyError {
    #[error("network survey requires between one and 32 approved private CIDRs")]
    InvalidScope,
    #[error("network survey CIDRs must be unique")]
    DuplicateScope,
    #[error("network survey scope contains a non-private or malformed IPv4 CIDR")]
    NonPrivateCidr,
    #[error("network survey exceeds 65,536 IPv4 addresses")]
    ScopeTooLarge,
    #[error("network survey plan was not constructed from its declared scope")]
    InvalidPlan,
    #[error("network survey received more observed addresses than its approved scope")]
    TooManyObservations,
    #[error("network survey observation is outside its approved scope")]
    ObservationOutsideScope,
    #[error("network survey contains duplicate address observations")]
    DuplicateObservation,
    #[error("network evidence does not match its declared collection source or survey profile")]
    EvidenceSourceMismatch,
}

/// A normalized observation emitted by a future passive collector or a completed
/// network survey. This is deliberately a finite vocabulary: raw packets, credentials,
/// endpoint URLs and arbitrary vendor strings do not become authority-bearing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkEvidence {
    RouterAdvertisement,
    MdnsRoku,
    MdnsGoogleCast,
    MdnsIpp,
    MdnsRtsp,
    MdnsHomeAssistant,
    SsdpInternetGatewayDevice,
    SsdpMediaRenderer,
    OpenTcpPort(u16),
}

/// Fixed collection provenance for a normalized observation. A future collector must
/// make this declaration before raw protocol material is discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkEvidenceSource {
    NmapServiceInventory,
    PassiveMdns,
    PassiveSsdp,
    PassiveRouterAdvertisement,
}

/// One normalized signal and the collection path that produced it. It is deliberately
/// separate from the classifier so source/profile consistency is enforced before any
/// device-class score is calculated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcedNetworkEvidence {
    pub source: NetworkEvidenceSource,
    pub evidence: NetworkEvidence,
}

/// Deterministic analytic result for a candidate inventory class. A classification is
/// only an observation: it cannot propose pairing, select an adapter, mint a lease or
/// enable any device transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyticDeviceClassification {
    pub class: DeviceClass,
    /// 0–100 evidence score. Only candidates at or above the conservative threshold
    /// are emitted.
    pub score: u8,
    pub supporting_signals: u8,
}

/// Bounded, normalized evidence associated with one responding IPv4 address. This is
/// inventory data only; the address is neither a pairing identity nor a transport
/// destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAssetObservation {
    pub ipv4_address: Ipv4Addr,
    pub evidence: Vec<SourcedNetworkEvidence>,
}

/// Reproducible candidate inventory view of one observation. An empty `candidates`
/// list means a host was observed but the analytic rules do not identify a device class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassifiedNetworkAsset {
    pub ipv4_address: Ipv4Addr,
    pub candidates: Vec<AnalyticDeviceClassification>,
}

/// Deterministic difference between two observations of the same declared network
/// scope. This is an analytic finding only: it does not mutate inventory, trust,
/// pairing, routing, firewall state or any device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NetworkInventoryChange {
    NewlyObserved {
        ipv4_address: Ipv4Addr,
        candidates: Vec<AnalyticDeviceClassification>,
    },
    NoLongerObserved {
        ipv4_address: Ipv4Addr,
        previous_candidates: Vec<AnalyticDeviceClassification>,
    },
    ClassificationChanged {
        ipv4_address: Ipv4Addr,
        previous_candidates: Vec<AnalyticDeviceClassification>,
        current_candidates: Vec<AnalyticDeviceClassification>,
    },
}

/// The identity-bearing path by which a discovered device asks to join this node.
///
/// Discovery answers only "something at this address looks like a television". It is
/// deliberately not a pairing credential: mDNS and SSDP announcements are unauthenticated
/// LAN broadcast. An explicit operator intent ("connect to the living-room TV") may select
/// a discovery candidate, but it must still follow one of these pairing paths before the
/// candidate can become a durable paired device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePairingTransport {
    /// An Archie agent already authenticated by the signed local fabric. This is the one
    /// path that can complete from an operator's request alone: the agent proved its
    /// Ed25519 identity and the request names that exact key.
    ArchieFabric,
    /// Matter commissioning. The device's setup code, NFC tap, or on-device confirmation
    /// is a protocol requirement, not a configuration detail Archie may bypass.
    Matter,
    /// A Roku ECP advertisement identifies a useful control surface but supplies no
    /// cryptographic device identity.
    RokuEcp,
    /// Home Assistant needs a user-issued long-lived access token or its own pairing flow.
    HomeAssistant,
    /// Router, printer, camera and generic SSDP/mDNS advertisements have no common secure
    /// pairing primitive. A reviewed adapter must name their device-side proof first.
    VendorSpecific,
}

/// What Archie may do after the user says "connect to this device".
///
/// It is intentionally a plan, never a socket or a command. The caller can show a native
/// card only when the target firmware itself requires proof; users do not need to hunt down
/// an address, port, protocol, or adapter by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DevicePairingPlan {
    /// The exact agent identity is already authenticated by Archie fabric, so the local
    /// inventory may progress from proposed to paired without an extra manual link step.
    AutoEnroll {
        device_reference: String,
        class: DeviceClass,
        public_key: String,
    },
    /// The device was identified and an adapter path was selected, but the firmware's own
    /// proof must be completed. This never asks for a password in Archie state.
    AwaitDeviceProof {
        class: DeviceClass,
        transport: DevicePairingTransport,
        explanation: String,
    },
}

/// A bounded connection intent emitted by a voice/UI parser after it resolved the name the
/// owner used. The source address, raw discovery packet, vendor URL, and credentials are
/// intentionally absent: those belong to a later adapter process, never to control state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConnectionIntent {
    pub device_reference: String,
    pub class: DeviceClass,
    pub transport: DevicePairingTransport,
    /// Present only for an already-authenticated Archie agent. It is the Ed25519 key the
    /// inventory will pin; it is not a MAC address or an assertion from mDNS/SSDP.
    pub authenticated_public_key: Option<String>,
}

impl DeviceConnectionIntent {
    /// Turn an operator's natural "connect" intent into the exact permitted next step.
    ///
    /// A generic LAN advertisement can never take the `AutoEnroll` branch. That is the
    /// difference between no-manual-configuration UX and silently trusting every device on
    /// Wi-Fi: the former is convenient, the latter is a takeover primitive.
    pub fn plan(&self) -> std::result::Result<DevicePairingPlan, DeviceConnectionError> {
        validate_device_reference(&self.device_reference)
            .map_err(|_| DeviceConnectionError::InvalidDeviceReference)?;

        match self.transport {
            DevicePairingTransport::ArchieFabric => {
                let public_key = self
                    .authenticated_public_key
                    .as_deref()
                    .ok_or(DeviceConnectionError::MissingAuthenticatedKey)?;
                if !is_lowercase_sha256(public_key) {
                    return Err(DeviceConnectionError::InvalidAuthenticatedKey);
                }
                Ok(DevicePairingPlan::AutoEnroll {
                    device_reference: self.device_reference.clone(),
                    class: self.class,
                    public_key: public_key.to_string(),
                })
            }
            DevicePairingTransport::Matter => Ok(DevicePairingPlan::AwaitDeviceProof {
                class: self.class,
                transport: self.transport,
                explanation: "Matter requires the device setup code, NFC tap, or its own confirmation".into(),
            }),
            DevicePairingTransport::RokuEcp => Ok(DevicePairingPlan::AwaitDeviceProof {
                class: self.class,
                transport: self.transport,
                explanation: "Roku discovery has no cryptographic identity; confirm on the device before control".into(),
            }),
            DevicePairingTransport::HomeAssistant => Ok(DevicePairingPlan::AwaitDeviceProof {
                class: self.class,
                transport: self.transport,
                explanation: "Home Assistant requires its own user-issued pairing credential".into(),
            }),
            DevicePairingTransport::VendorSpecific => Ok(DevicePairingPlan::AwaitDeviceProof {
                class: self.class,
                transport: self.transport,
                explanation: "this firmware needs a reviewed adapter and its declared device-side proof".into(),
            }),
        }
    }
}

/// A connection intent is malformed before it reaches a pairing adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DeviceConnectionError {
    #[error("device connection intent has an invalid device reference")]
    InvalidDeviceReference,
    #[error("automatic enrollment requires an already-authenticated Archie device key")]
    MissingAuthenticatedKey,
    #[error("automatic enrollment device key must be a lower-case Ed25519 hex value")]
    InvalidAuthenticatedKey,
}

/// Classify finite, normalized network evidence before any optional generative
/// explanation. Duplicate observations cannot inflate a score, outcomes have a stable
/// order, and a lone common port is insufficient to create a candidate.
pub fn analytically_classify_network_asset(
    evidence: &[NetworkEvidence],
) -> Vec<AnalyticDeviceClassification> {
    let unique_evidence = evidence.iter().collect::<BTreeSet<_>>();
    let mut scores = Vec::new();
    for observation in unique_evidence {
        match observation {
            NetworkEvidence::RouterAdvertisement => {
                add_analytic_score(&mut scores, DeviceClass::Router, 80)
            }
            NetworkEvidence::MdnsRoku => {
                add_analytic_score(&mut scores, DeviceClass::Television, 90)
            }
            NetworkEvidence::MdnsGoogleCast => {
                add_analytic_score(&mut scores, DeviceClass::Display, 80)
            }
            NetworkEvidence::MdnsIpp => add_analytic_score(&mut scores, DeviceClass::Printer, 80),
            NetworkEvidence::MdnsRtsp => add_analytic_score(&mut scores, DeviceClass::Camera, 80),
            NetworkEvidence::MdnsHomeAssistant => {
                add_analytic_score(&mut scores, DeviceClass::HomeAssistant, 90)
            }
            NetworkEvidence::SsdpInternetGatewayDevice => {
                add_analytic_score(&mut scores, DeviceClass::Router, 90)
            }
            NetworkEvidence::SsdpMediaRenderer => {
                add_analytic_score(&mut scores, DeviceClass::Display, 70)
            }
            NetworkEvidence::OpenTcpPort(port) => match port {
                53 | 67 => add_analytic_score(&mut scores, DeviceClass::Router, 12),
                554 => add_analytic_score(&mut scores, DeviceClass::Camera, 40),
                631 | 9100 => add_analytic_score(&mut scores, DeviceClass::Printer, 40),
                8008 | 8009 => add_analytic_score(&mut scores, DeviceClass::Display, 30),
                8060 => add_analytic_score(&mut scores, DeviceClass::Television, 40),
                8123 => add_analytic_score(&mut scores, DeviceClass::HomeAssistant, 40),
                _ => {}
            },
        }
    }

    scores.retain(|candidate| candidate.score >= 70);
    scores.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.supporting_signals.cmp(&left.supporting_signals))
            .then_with(|| left.class.as_str().cmp(right.class.as_str()))
    });
    scores
}

fn add_analytic_score(
    scores: &mut Vec<AnalyticDeviceClassification>,
    class: DeviceClass,
    score: u8,
) {
    if let Some(candidate) = scores.iter_mut().find(|candidate| candidate.class == class) {
        candidate.score = candidate.score.saturating_add(score).min(100);
        candidate.supporting_signals = candidate.supporting_signals.saturating_add(1);
        return;
    }
    scores.push(AnalyticDeviceClassification {
        class,
        score,
        supporting_signals: 1,
    });
}

fn diff_network_inventory(
    baseline: &[ClassifiedNetworkAsset],
    current: &[ClassifiedNetworkAsset],
) -> Vec<NetworkInventoryChange> {
    let baseline = baseline
        .iter()
        .map(|asset| (asset.ipv4_address, &asset.candidates))
        .collect::<BTreeMap<_, _>>();
    let current = current
        .iter()
        .map(|asset| (asset.ipv4_address, &asset.candidates))
        .collect::<BTreeMap<_, _>>();
    let addresses = baseline
        .keys()
        .chain(current.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for address in addresses {
        match (baseline.get(&address), current.get(&address)) {
            (None, Some(candidates)) => changes.push(NetworkInventoryChange::NewlyObserved {
                ipv4_address: address,
                candidates: (*candidates).clone(),
            }),
            (Some(previous_candidates), None) => {
                changes.push(NetworkInventoryChange::NoLongerObserved {
                    ipv4_address: address,
                    previous_candidates: (*previous_candidates).clone(),
                });
            }
            (Some(previous_candidates), Some(current_candidates))
                if previous_candidates != current_candidates =>
            {
                changes.push(NetworkInventoryChange::ClassificationChanged {
                    ipv4_address: address,
                    previous_candidates: (*previous_candidates).clone(),
                    current_candidates: (*current_candidates).clone(),
                });
            }
            (None, None) | (Some(_), Some(_)) => {}
        }
    }
    changes
}

fn private_cidr(cidr: &str) -> std::result::Result<(Ipv4Addr, u8, u64), NetworkSurveyError> {
    let Some((address, prefix)) = cidr.split_once('/') else {
        return Err(NetworkSurveyError::NonPrivateCidr);
    };
    let Ok(address) = address.parse::<Ipv4Addr>() else {
        return Err(NetworkSurveyError::NonPrivateCidr);
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return Err(NetworkSurveyError::NonPrivateCidr);
    };
    if prefix > 32 || !address.is_private() {
        return Err(NetworkSurveyError::NonPrivateCidr);
    }
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(prefix))
    };
    if u32::from(address) & mask != u32::from(address) {
        return Err(NetworkSurveyError::NonPrivateCidr);
    }
    Ok((address, prefix, 1_u64 << (32 - prefix)))
}

fn ipv4_cidr_contains(network: Ipv4Addr, prefix: u8, address: Ipv4Addr) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(prefix))
    };
    u32::from(network) & mask == u32::from(address) & mask
}

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("control i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("malformed control message: {0}")]
    Malformed(String),

    #[error("control message exceeds {MAX_CONTROL_MESSAGE_BYTES} bytes")]
    TooLarge,

    #[error("daemon is not running, or its control socket is unreachable: {0}")]
    NotRunning(String),

    #[error("another archied instance already controls this node: {0}")]
    AlreadyRunning(String),
}

pub type Result<T> = std::result::Result<T, ControlError>;

/// A native device adapter contract bound to one currently paired local inventory record.
///
/// This is an admission check, not a connection or a dispatch. It deliberately contains
/// no endpoint, credential, payload, live lease or transport handle. A future dispatcher
/// must still bind one prepared operation, obtain an operator's one-operation lease and
/// re-check pairing immediately before asking a separately reviewed adapter to act.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDeviceAdapterBinding {
    pub adapter_id: String,
    pub module_id: String,
    pub device_reference: String,
    pub class: String,
}

impl PairedDeviceAdapterBinding {
    /// Admit a built-in adapter contract only for the paired device class it declares.
    pub fn admit(
        adapter: &DeviceAdapterManifest,
        module: &ModuleManifest,
        device: &DeviceRecord,
    ) -> std::result::Result<Self, DeviceAdapterBindingError> {
        adapter
            .validate_activation(module)
            .map_err(DeviceAdapterBindingError::InvalidContract)?;
        if !device.pairing.may_receive_commands() {
            return Err(DeviceAdapterBindingError::DeviceNotPaired);
        }
        if !adapter_supports_device_class(adapter, device.class) {
            return Err(DeviceAdapterBindingError::UnsupportedDeviceClass);
        }
        Ok(Self {
            adapter_id: adapter.adapter_id.clone(),
            module_id: module.id.to_string(),
            device_reference: device.device_reference.clone(),
            class: device.class.as_str().to_string(),
        })
    }
}

/// Reasons that a device-adapter contract cannot be admitted for a local target.
#[derive(Debug, Error)]
pub enum DeviceAdapterBindingError {
    #[error("device adapter contract is invalid: {0}")]
    InvalidContract(archie_types::TypeError),

    #[error("device inventory record is not locally paired")]
    DeviceNotPaired,

    #[error("device adapter does not declare this device class")]
    UnsupportedDeviceClass,
}

fn adapter_supports_device_class(adapter: &DeviceAdapterManifest, class: DeviceClass) -> bool {
    let expected = match class {
        DeviceClass::Television => DeviceAdapterClass::Television,
        DeviceClass::Router => DeviceAdapterClass::Router,
        DeviceClass::Camera => DeviceAdapterClass::Camera,
        DeviceClass::Display => DeviceAdapterClass::Display,
        DeviceClass::Mobile => DeviceAdapterClass::Mobile,
        DeviceClass::Host => DeviceAdapterClass::Host,
        DeviceClass::Printer => DeviceAdapterClass::Printer,
        DeviceClass::HomeAssistant => DeviceAdapterClass::HomeAssistant,
    };
    adapter.supported_classes.contains(&expected)
}

/// One declared adapter action, bound to the durable identifier an operator reviews.
///
/// It deliberately retains the canonical digest rather than a device payload. The native
/// adapter that created the digest must validate its own fixed payload vocabulary again
/// at the dispatch boundary; this shared layer makes it impossible to substitute the
/// target, adapter, operation kind, or one-operation lease identifier in transit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDeviceAdapterOperation {
    pub operation_id: u64,
    pub adapter_id: String,
    pub device_reference: String,
    pub operation: archie_types::DeviceAdapterOperation,
    pub operation_sha256: String,
}

impl PreparedDeviceAdapterOperation {
    /// Bind an admitted adapter operation to the durable record it must later prove.
    ///
    /// This performs no command dispatch. In particular, a `prepared` database record
    /// alone remains insufficient: the caller must subsequently obtain an exact,
    /// unexpired operator lease and re-check pairing at the physical boundary.
    pub fn from_durable(
        binding: &PairedDeviceAdapterBinding,
        adapter: &DeviceAdapterManifest,
        durable: &DeviceOperationRecord,
        operation: archie_types::DeviceAdapterOperation,
    ) -> std::result::Result<Self, DeviceAdapterOperationError> {
        if binding.adapter_id != adapter.adapter_id {
            return Err(DeviceAdapterOperationError::AdapterMismatch);
        }
        if !adapter.operations.contains(&operation) {
            return Err(DeviceAdapterOperationError::OperationNotDeclared);
        }
        if durable.state != DeviceOperationState::Prepared {
            return Err(DeviceAdapterOperationError::DurableOperationNotPrepared);
        }
        if durable.device_reference != binding.device_reference {
            return Err(DeviceAdapterOperationError::DeviceMismatch);
        }
        let prepared = Self {
            operation_id: durable.operation_id,
            adapter_id: binding.adapter_id.clone(),
            device_reference: binding.device_reference.clone(),
            operation,
            operation_sha256: durable.operation_sha256.clone(),
        };
        prepared.validate()?;
        Ok(prepared)
    }

    pub fn validate(&self) -> std::result::Result<(), DeviceAdapterOperationError> {
        if self.operation_id == 0 {
            return Err(DeviceAdapterOperationError::MissingOperationId);
        }
        if self.adapter_id.is_empty() {
            return Err(DeviceAdapterOperationError::MissingAdapterId);
        }
        validate_device_reference(&self.device_reference)
            .map_err(|_| DeviceAdapterOperationError::InvalidDeviceReference)?;
        if !is_lowercase_sha256(&self.operation_sha256) {
            return Err(DeviceAdapterOperationError::InvalidOperationDigest);
        }
        Ok(())
    }

    /// Build the exact policy request for this declared adapter action.
    ///
    /// Mutations become `Actuate` and therefore require the physical safety boundary,
    /// an authenticated human, presence and one-operation scope. Read-only observation
    /// remains a typed `Read` request; neither path establishes a device transport.
    pub fn capability_request(
        &self,
        trace_id: TraceId,
        task_id: Option<TaskId>,
        principal: Principal,
        justification: impl Into<String>,
    ) -> std::result::Result<CapabilityRequest, DeviceAdapterOperationError> {
        self.validate()?;
        let action = if self.operation.requires_actuation() {
            Action::Actuate
        } else {
            Action::Read
        };
        Ok(CapabilityRequest {
            trace_id,
            task_id,
            principal,
            resource: Resource::Device {
                device: self.device_reference.clone(),
            },
            scope: Scope::OneOperation {
                operation: self.operation_id,
            },
            constraints: Constraints::with_actions([action]),
            justification: justification.into(),
        })
    }

    /// Verify an operator lease against this exact durable operation at the later
    /// dispatch boundary. This never consumes a lease or sends a command itself.
    pub fn validate_lease(
        &self,
        lease: &Lease,
        principal: Principal,
        task_id: Option<TaskId>,
        user_present: bool,
        now: Timestamp,
    ) -> std::result::Result<(), DeviceAdapterLeaseError> {
        self.validate()
            .map_err(DeviceAdapterLeaseError::InvalidOperation)?;
        let action = if self.operation.requires_actuation() {
            Action::Actuate
        } else {
            Action::Read
        };
        lease
            .authorises_one_operation(
                &Operation {
                    principal,
                    resource: Resource::Device {
                        device: self.device_reference.clone(),
                    },
                    action,
                    task_id,
                    user_present,
                },
                self.operation_id,
                now,
            )
            .map_err(DeviceAdapterLeaseError::Lease)
    }
}

/// Reasons a generic adapter operation is invalid before a physical adapter can see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DeviceAdapterOperationError {
    #[error("prepared device adapter operation needs a non-zero identifier")]
    MissingOperationId,

    #[error("prepared device adapter operation needs an adapter identifier")]
    MissingAdapterId,

    #[error("device adapter operation has an invalid device reference")]
    InvalidDeviceReference,

    #[error("device adapter operation has an invalid canonical digest")]
    InvalidOperationDigest,

    #[error("admitted binding does not belong to this adapter")]
    AdapterMismatch,

    #[error("adapter did not declare this operation")]
    OperationNotDeclared,

    #[error("durable device operation is no longer prepared")]
    DurableOperationNotPrepared,

    #[error("durable device operation names a different paired device")]
    DeviceMismatch,
}

/// A generic adapter operation may fail before, or during, lease validation. Keeping
/// those failure domains distinct prevents a malformed adapter request from looking
/// like a revoked human decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DeviceAdapterLeaseError {
    #[error("prepared device adapter operation is invalid: {0}")]
    InvalidOperation(DeviceAdapterOperationError),

    #[error("device adapter operation lease was refused: {0}")]
    Lease(LeaseRejection),
}

/// A closed set of inputs that a paired television may expose.
///
/// This is deliberately not a URL, a shell command, or a vendor-specific protocol.
/// A future paired adapter translates this bounded vocabulary only after the control
/// plane has prepared, authorised, and dispatched the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TelevisionInput {
    Hdmi { port: u8 },
    Roku,
    Tuner,
}

impl TelevisionInput {
    fn validate(self) -> std::result::Result<(), TelevisionOperationError> {
        match self {
            Self::Hdmi { port: 1..=16 } | Self::Roku | Self::Tuner => Ok(()),
            Self::Hdmi { .. } => Err(TelevisionOperationError::InvalidHdmiPort),
        }
    }
}

/// A bounded physical operation for a paired television or projector path.
///
/// `SetTunerChannel` intentionally names the input switch and the requested channel as
/// one prepared operator-visible operation. A bare `StepTunerChannel` never changes
/// input implicitly, because a request such as "next channel" is ambiguous while Roku
/// is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TelevisionOperation {
    SelectInput { input: TelevisionInput },
    SetTunerChannel { channel: u16 },
    StepTunerChannel { direction: ChannelDirection },
}

impl TelevisionOperation {
    fn validate(self) -> std::result::Result<(), TelevisionOperationError> {
        match self {
            Self::SelectInput { input } => input.validate(),
            Self::SetTunerChannel { channel: 1..=9_999 } => Ok(()),
            Self::SetTunerChannel { .. } => Err(TelevisionOperationError::InvalidChannel),
            Self::StepTunerChannel { .. } => Ok(()),
        }
    }

    /// Canonical digest carried by the durable prepared-operation record. The operation
    /// vocabulary has no maps, free-form payload, endpoint, or vendor command, so its
    /// tagged JSON encoding is stable and can be reviewed without retaining speech or
    /// screen data in the state store.
    pub fn sha256(&self) -> std::result::Result<String, TelevisionOperationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| TelevisionOperationError::CanonicalEncodingFailed)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

/// The only relative channel operations that can reach a future paired adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelDirection {
    Previous,
    Next,
}

/// The source of an operation proposal. A voice transcript is represented only by its
/// digest so the control/audit path does not copy private speech into routine logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TelevisionProposalOrigin {
    AuthenticatedOperator,
    VoiceProposal { transcript_sha256: String },
}

impl TelevisionProposalOrigin {
    fn validate(&self) -> std::result::Result<(), TelevisionOperationError> {
        if let Self::VoiceProposal { transcript_sha256 } = self {
            if !is_lowercase_sha256(transcript_sha256) {
                return Err(TelevisionOperationError::InvalidTranscriptDigest);
            }
        }
        Ok(())
    }

    /// A voice recogniser can propose this operation but can never confirm it. The
    /// policy request below still needs an authenticated human, a safety boundary, and
    /// a one-operation lease.
    pub fn requires_operator_approval(&self) -> bool {
        matches!(self, Self::VoiceProposal { .. })
    }
}

/// A prepared, immutable device operation. It carries no authority and no transport.
///
/// Its `operation_id` is owned by the future durable dispatcher. The simulator uses it
/// only to exercise the exact one-operation lease boundary before a real adapter exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedTelevisionOperation {
    pub operation_id: u64,
    pub device_reference: String,
    pub operation: TelevisionOperation,
    pub origin: TelevisionProposalOrigin,
}

impl PreparedTelevisionOperation {
    pub fn prepare(
        operation_id: u64,
        device_reference: impl Into<String>,
        operation: TelevisionOperation,
        origin: TelevisionProposalOrigin,
    ) -> std::result::Result<Self, TelevisionOperationError> {
        let prepared = Self {
            operation_id,
            device_reference: device_reference.into(),
            operation,
            origin,
        };
        prepared.validate()?;
        Ok(prepared)
    }

    /// Bind a television operation to the durable id allocated by the local state
    /// store. A caller cannot substitute a different HDMI/input/channel payload under
    /// the same id: the canonical digest and current paired inventory are checked here
    /// before it can even become a policy request.
    pub fn from_durable_inventory_operation(
        durable: &DeviceOperationRecord,
        device: &DeviceRecord,
        operation: TelevisionOperation,
        origin: TelevisionProposalOrigin,
    ) -> std::result::Result<Self, TelevisionOperationError> {
        if durable.state != DeviceOperationState::Prepared {
            return Err(TelevisionOperationError::DurableOperationNotPrepared);
        }
        if durable.device_reference != device.device_reference {
            return Err(TelevisionOperationError::UnpairedDevice);
        }
        if device.class != DeviceClass::Television {
            return Err(TelevisionOperationError::UnexpectedDeviceClass);
        }
        if !device.pairing.may_receive_commands() {
            return Err(TelevisionOperationError::DeviceNotPaired);
        }
        if durable.operation_sha256 != operation.sha256()? {
            return Err(TelevisionOperationError::DurableOperationDigestMismatch);
        }
        Self::prepare(
            durable.operation_id,
            durable.device_reference.clone(),
            operation,
            origin,
        )
    }

    pub fn validate(&self) -> std::result::Result<(), TelevisionOperationError> {
        if self.operation_id == 0 {
            return Err(TelevisionOperationError::MissingOperationId);
        }
        validate_device_reference(&self.device_reference)?;
        self.operation.validate()?;
        self.origin.validate()
    }

    /// Build the only policy request a television operation can use. This is a request,
    /// not a grant: a caller must pass it through `PolicyEngine::decide` and validate
    /// the returned lease immediately before an eventual device dispatch.
    pub fn capability_request(
        &self,
        trace_id: TraceId,
        task_id: Option<TaskId>,
        principal: Principal,
        justification: impl Into<String>,
    ) -> std::result::Result<CapabilityRequest, TelevisionOperationError> {
        self.validate()?;
        Ok(CapabilityRequest {
            trace_id,
            task_id,
            principal,
            resource: Resource::Device {
                device: self.device_reference.clone(),
            },
            scope: Scope::OneOperation {
                operation: self.operation_id,
            },
            constraints: Constraints::with_actions([Action::Actuate]),
            justification: justification.into(),
        })
    }

    /// Verify a policy lease at the irreversible boundary. The generic lease checker
    /// purposely cannot validate `OneOperation`; this is the exact prepared id it needs.
    pub fn validate_lease(
        &self,
        lease: &Lease,
        principal: Principal,
        task_id: Option<TaskId>,
        user_present: bool,
        now: Timestamp,
    ) -> std::result::Result<(), TelevisionLeaseError> {
        self.validate()
            .map_err(TelevisionLeaseError::InvalidOperation)?;
        lease
            .authorises_one_operation(
                &Operation {
                    principal,
                    resource: Resource::Device {
                        device: self.device_reference.clone(),
                    },
                    action: Action::Actuate,
                    task_id,
                    user_present,
                },
                self.operation_id,
                now,
            )
            .map_err(TelevisionLeaseError::Lease)
    }
}

/// Known state of a paired television, supplied by its future signed inventory and
/// receipt stream. For now it makes dry-runs deterministic without opening a network
/// connection or pretending the target has been controlled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelevisionState {
    pub device_reference: String,
    pub available_inputs: BTreeSet<TelevisionInput>,
    pub active_input: TelevisionInput,
    pub tuner_channel: Option<u16>,
}

impl TelevisionState {
    pub fn validate(&self) -> std::result::Result<(), TelevisionOperationError> {
        validate_device_reference(&self.device_reference)?;
        if self.available_inputs.is_empty() || !self.available_inputs.contains(&self.active_input) {
            return Err(TelevisionOperationError::InvalidSimulationState);
        }
        for input in &self.available_inputs {
            input.validate()?;
        }
        if let Some(channel) = self.tuner_channel {
            if !(1..=9_999).contains(&channel) {
                return Err(TelevisionOperationError::InvalidSimulationState);
            }
        }
        Ok(())
    }
}

/// The receipt shape a future adapter must reconcile with reality. `dry_run` stays true
/// in the current implementation, so neither preparing nor previewing can touch HDMI,
/// Roku, CEC, Wi-Fi, or a television.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelevisionSimulationReceipt {
    pub operation_id: u64,
    pub device_reference: String,
    pub operation: TelevisionOperation,
    pub before: TelevisionState,
    pub predicted_after: TelevisionState,
    pub dry_run: bool,
}

/// Pure deterministic simulator for the paired-television contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelevisionSimulator {
    state: TelevisionState,
}

impl TelevisionSimulator {
    /// Build the simulator from the durable inventory path a future adapter must use.
    ///
    /// This checks pairing and device class before accepting the separately reported
    /// input state. It still produces only a dry-run receipt: a paired record does not
    /// grant authority, establish a transport, or send a command.
    pub fn from_paired_inventory(
        device: &DeviceRecord,
        state: TelevisionState,
    ) -> std::result::Result<Self, TelevisionOperationError> {
        if device.class != DeviceClass::Television {
            return Err(TelevisionOperationError::UnexpectedDeviceClass);
        }
        if !device.pairing.may_receive_commands() {
            return Err(TelevisionOperationError::DeviceNotPaired);
        }
        if device.device_reference != state.device_reference {
            return Err(TelevisionOperationError::UnpairedDevice);
        }
        Self::new(state)
    }

    /// Construct a pure fixture simulator without an inventory record. Production
    /// adapter integration must use [`Self::from_paired_inventory`] instead.
    pub fn new(state: TelevisionState) -> std::result::Result<Self, TelevisionOperationError> {
        state.validate()?;
        Ok(Self { state })
    }

    pub fn preview(
        &self,
        prepared: &PreparedTelevisionOperation,
    ) -> std::result::Result<TelevisionSimulationReceipt, TelevisionOperationError> {
        prepared.validate()?;
        if prepared.device_reference != self.state.device_reference {
            return Err(TelevisionOperationError::UnpairedDevice);
        }

        let mut predicted_after = self.state.clone();
        match prepared.operation {
            TelevisionOperation::SelectInput { input } => {
                if !predicted_after.available_inputs.contains(&input) {
                    return Err(TelevisionOperationError::UnavailableInput);
                }
                predicted_after.active_input = input;
            }
            TelevisionOperation::SetTunerChannel { channel } => {
                if !predicted_after
                    .available_inputs
                    .contains(&TelevisionInput::Tuner)
                {
                    return Err(TelevisionOperationError::UnavailableInput);
                }
                predicted_after.active_input = TelevisionInput::Tuner;
                predicted_after.tuner_channel = Some(channel);
            }
            TelevisionOperation::StepTunerChannel { direction } => {
                if predicted_after.active_input != TelevisionInput::Tuner {
                    return Err(TelevisionOperationError::TunerNotActive);
                }
                let channel = predicted_after
                    .tuner_channel
                    .ok_or(TelevisionOperationError::UnknownTunerChannel)?;
                let next = match direction {
                    ChannelDirection::Previous => channel.saturating_sub(1).max(1),
                    ChannelDirection::Next => channel.saturating_add(1).min(9_999),
                };
                predicted_after.tuner_channel = Some(next);
            }
        }

        Ok(TelevisionSimulationReceipt {
            operation_id: prepared.operation_id,
            device_reference: prepared.device_reference.clone(),
            operation: prepared.operation,
            before: self.state.clone(),
            predicted_after,
            dry_run: true,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TelevisionOperationError {
    #[error("prepared device operations need a non-zero identifier")]
    MissingOperationId,

    #[error(
        "device reference must be 1..=128 ASCII letters, digits, dots, underscores, or hyphens"
    )]
    InvalidDeviceReference,

    #[error("HDMI port must be in the bounded range 1..=16")]
    InvalidHdmiPort,

    #[error("television channel must be in the bounded range 1..=9999")]
    InvalidChannel,

    #[error("could not encode the bounded television operation canonically")]
    CanonicalEncodingFailed,

    #[error("voice proposal must identify its transcript with a lower-case SHA-256 digest")]
    InvalidTranscriptDigest,

    #[error("television simulation state is internally inconsistent")]
    InvalidSimulationState,

    #[error("device inventory record is not a television")]
    UnexpectedDeviceClass,

    #[error("device inventory record is not locally paired")]
    DeviceNotPaired,

    #[error("durable device operation is no longer prepared")]
    DurableOperationNotPrepared,

    #[error("durable operation digest does not match the reviewed television operation")]
    DurableOperationDigestMismatch,

    #[error("operation names a device that is not paired with this simulator")]
    UnpairedDevice,

    #[error("requested input is not in the paired device inventory")]
    UnavailableInput,

    #[error("relative channel changes require the tuner to already be active")]
    TunerNotActive,

    #[error("relative channel changes require a known tuner channel")]
    UnknownTunerChannel,
}

/// A prepared television operation may fail before, or during, lease validation. Keeping
/// those failure domains distinct makes a malformed proposal impossible to mistake for
/// an expired or revoked operator grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TelevisionLeaseError {
    #[error("prepared television operation is invalid: {0}")]
    InvalidOperation(TelevisionOperationError),

    #[error("television operation lease was refused: {0}")]
    Lease(LeaseRejection),
}

fn validate_device_reference(
    device_reference: &str,
) -> std::result::Result<(), TelevisionOperationError> {
    if device_reference.is_empty()
        || device_reference.len() > 128
        || !device_reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(TelevisionOperationError::InvalidDeviceReference);
    }
    Ok(())
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// A job-local control action. The typed action carries no authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldJobAction {
    Pause,
    Resume,
    Cancel,
}

/// What an operator client can ask Core to admit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlRequest {
    WorldNavigate {
        #[serde(rename = "request_body")]
        request: archie_worldd::navigation::NavigationRequest,
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    WorldNavigationStatus {
        id: String,
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    WorldNavigationStop {
        id: String,
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    WorldJob {
        action: WorldJobAction,
        id: String,
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    WorldPlan {
        #[serde(rename = "request_body")]
        request: archie_geofabric::exploration::ExplorationRequest,
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    WorldExplore {
        #[serde(rename = "request_body")]
        request: archie_geofabric::exploration::ExplorationRequest,
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    WorldResolve {
        query: String,
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    WorldStatus {
        domain: archie_vault::ContextDomain,
        privacy: archie_types::PrivacyGrade,
    },
    /// Bounded text proposal. Only a kernel-authenticated local operator may
    /// invoke this; the wire cannot configure executables or grant authority.
    AiInfer {
        #[serde(rename = "request_body")]
        request: archie_workstation::ai::InferenceRequest,
    },
    /// Run the configured pinned Harness² worker through the same admitted
    /// runtime. Worker configuration and state paths never come from this wire.
    AiHarness {
        #[serde(rename = "request_body")]
        request: archie_workstation::harness_runtime::HarnessRunRequest,
    },
    /// Recall exactly one committed, attributed result under its original scope.
    AiRecall {
        binding: archie_memory::inference::InferenceBinding,
        receipt_id: String,
    },
    /// Set Core's emergency pause: every AI dispatch is refused from now on, including
    /// one already admitted, and so is every other mutating lease. Only the operator
    /// profile's kernel-identified owner may ask, and the pause outlives a restart.
    AiPause,
    /// Lift the emergency pause the same owner set.
    AiResume,
    /// Whether the emergency pause is set and recorded.
    AiStatus,
    /// Everything `archie status` shows.
    Status,
    /// The capability/resource graph.
    Topology,
    /// Installed modules and why each did or did not load.
    Modules,
    /// Installed data-provider manifests and why each does or does not run.
    Providers,
    /// Fabric peers.
    Peers,
    /// Locally recorded device pairing inventory. This is read-only and has no adapter
    /// endpoint or command channel.
    Devices,
    /// Recent durable physical-operation records. This is read-only and cannot approve,
    /// cancel, send, or otherwise dispatch an operation.
    DeviceOperations {
        limit: u32,
    },
    /// Read-only IPP query for a printer in operator configuration. No caller URL.
    SentryPrinter {
        resource_id: String,
    },
    /// Read configured HA cache telemetry; no refresh, URL or actuation.
    SentryHomeStatus {
        resource_id: String,
    },
    /// Read-only Get-Printer-Attributes probe plus driver planning for one
    /// configured printer. Never installs or executes anything.
    SentryPrinterDoctor {
        resource_id: String,
    },
    /// Prepare one typed operation in an authenticated local operator session.
    SentryPrepare {
        resource_id: String,
        command: archie_sentry::contracts::Command,
    },
    /// Confirm the exact digest returned on this same connection.
    SentryApprove {
        intent_id: String,
        digest: String,
    },
    SentryCancel,
    /// Query a durable outcome/fence through an authenticated local session.
    SentryReceipt {
        intent_id: String,
    },
    /// Recent Cortex learning trajectories. This is a privacy-minimised read-only
    /// projection; it cannot parse a correction, write the graph or promote a model.
    CortexTrajectories {
        limit: u32,
    },
    /// Registered models.
    Models,
    /// The boot report from this daemon's start.
    Boot,
    /// Metrics snapshot.
    Metrics,
    /// Recent audit records.
    Audit {
        limit: u32,
    },
    /// Liveness check.
    Ping,
}

impl ControlRequest {
    /// Whether servicing this request changes daemon state.
    ///
    /// Sentry preparation/approval/cancellation require the operator dispatcher.
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Self::SentryPrepare { .. }
                | Self::WorldNavigate { .. }
                | Self::WorldNavigationStop { .. }
                | Self::WorldExplore { .. }
                | Self::WorldResolve { .. }
                | Self::WorldJob { .. }
                | Self::SentryApprove { .. }
                | Self::SentryCancel
                | Self::AiInfer { .. }
                | Self::AiRecall { .. }
                | Self::AiHarness { .. }
                | Self::AiPause
                | Self::AiResume
        )
    }

    /// These requests can write inference state, disclose admitted Vault text, or stop
    /// and resume AI dispatch. All of them need the kernel-identified operator.
    pub fn is_ai(&self) -> bool {
        matches!(
            self,
            Self::AiInfer { .. }
                | Self::AiRecall { .. }
                | Self::AiHarness { .. }
                | Self::AiPause
                | Self::AiResume
                | Self::AiStatus
        )
    }

    /// World requests require kernel identity even when the operation is local/read-only.
    pub fn is_world(&self) -> bool {
        matches!(
            self,
            Self::WorldNavigate { .. }
                | Self::WorldNavigationStatus { .. }
                | Self::WorldNavigationStop { .. }
                | Self::WorldPlan { .. }
                | Self::WorldJob { .. }
                | Self::WorldExplore { .. }
                | Self::WorldResolve { .. }
                | Self::WorldStatus { .. }
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::WorldNavigate { .. } => "world-navigate",
            Self::WorldNavigationStatus { .. } => "world-navigation-status",
            Self::WorldNavigationStop { .. } => "world-navigation-stop",
            Self::WorldJob { .. } => "world-job",
            Self::WorldPlan { .. } => "world-plan",
            Self::WorldExplore { .. } => "world-explore",
            Self::WorldResolve { .. } => "world-resolve",
            Self::WorldStatus { .. } => "world-status",
            Self::AiInfer { .. } => "ai-infer",
            Self::AiHarness { .. } => "ai-harness",
            Self::AiRecall { .. } => "ai-recall",
            Self::AiPause => "ai-pause",
            Self::AiResume => "ai-resume",
            Self::AiStatus => "ai-status",
            Self::Status => "status",
            Self::Topology => "topology",
            Self::Modules => "modules",
            Self::Providers => "providers",
            Self::Peers => "peers",
            Self::Devices => "devices",
            Self::DeviceOperations { .. } => "device-operations",
            Self::SentryPrinter { .. } => "sentry-printer",
            Self::SentryHomeStatus { .. } => "sentry-home-status",
            Self::SentryPrinterDoctor { .. } => "sentry-printer-doctor",
            Self::SentryPrepare { .. } => "sentry-prepare",
            Self::SentryApprove { .. } => "sentry-approve",
            Self::SentryCancel => "sentry-cancel",
            Self::SentryReceipt { .. } => "sentry-receipt",
            Self::CortexTrajectories { .. } => "cortex-trajectories",
            Self::Models => "models",
            Self::Boot => "boot",
            Self::Metrics => "metrics",
            Self::Audit { .. } => "audit",
            Self::Ping => "ping",
        }
    }
}

/// A node as reported to the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerView {
    pub node_id: String,
    pub hostname: String,
    pub profile: String,
    pub trust: String,
    pub health: String,
    pub reachable: bool,
    pub capabilities: Vec<String>,
    pub last_seen: Timestamp,
}

/// A privacy-minimised device inventory projection for native operator tools.
///
/// It intentionally contains neither the full public key nor an endpoint. Seeing a
/// record does not imply a connected adapter, and a `paired` state remains distinct
/// from a policy lease for a concrete physical operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceView {
    pub device_reference: String,
    pub class: String,
    pub pairing: String,
    pub public_key_fingerprint: String,
    pub first_seen: Timestamp,
    pub last_seen: Timestamp,
}

/// A privacy-minimised projection of one durable prepared/cancelled operation.
///
/// It deliberately omits the payload, full canonical digest, any live lease, endpoint
/// and receipt. It tells native operator tools which reviewed operation exists without
/// making that record an authority to reach a physical device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceOperationView {
    pub operation_id: u64,
    pub device_reference: String,
    pub state: String,
    pub trace_fingerprint: String,
    pub operation_sha256_fingerprint: String,
    pub prepared_at: Timestamp,
}

/// Privacy-minimised replay/audit view for one Cortex correction trajectory.
///
/// The underlying record contains opaque vault/provenance references. They are not
/// exposed here: seeing a trajectory must not grant the CLI, display or another worker
/// permission to dereference a private voice, conversation or model-response record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CortexTrajectoryView {
    pub trajectory_id: String,
    pub correction_kind: String,
    pub source_graph_snapshot_id: String,
    pub resulting_graph_snapshot_id: String,
    pub selected_context_count: u32,
    pub omitted_candidate_count: u32,
    pub model_id: String,
    pub payload_sha256_fingerprint: String,
    pub captured_at: Timestamp,
}

/// A module as reported to the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleView {
    pub id: String,
    pub version: String,
    pub state: String,
    /// Why it is in that state. Always populated, including for loaded modules.
    pub explanation: String,
    /// Module ids this one depends on.
    ///
    /// Carried so the observatory can draw dependency edges that correspond to real
    /// declared relationships rather than to a layout algorithm's idea of proximity.
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// A data-provider manifest as reported to the CLI — enough to see what it is and
/// whether it can run, not the full capability record `archie-registry` enforces the
/// real rules from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderView {
    pub id: String,
    pub title: String,
    /// The Archie capability this provider satisfies, e.g. `world.seismic`.
    pub capability: String,
    /// `available`, `no-credentials`, `not-authorized`, `no-capability`, `disabled` or
    /// `invalid` — see [`archie_registry::providers::ProviderStatus::label`].
    pub status: String,
    /// Why it is in that state. Always populated, including for available providers.
    pub explanation: String,
    /// The record kinds this provider yields, e.g. `["observation", "track"]`.
    pub records: Vec<String>,
    /// Documented blind spots. Rendered so an empty region reads as unobserved rather
    /// than as calm.
    #[serde(default)]
    pub known_gaps: Vec<String>,
}

/// A model as reported to the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelView {
    pub id: String,
    pub provider: String,
    pub locality: String,
    pub availability: String,
    pub max_privacy_grade: String,
    pub context_window: u32,
}

/// Aggregate resources, as reported to the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TotalsView {
    pub nodes: u32,
    pub schedulable_nodes: u32,
    pub logical_cores: u64,
    pub memory_bytes: u64,
    pub accelerator_bytes: u64,
    pub accelerators: u32,
}

/// The full status payload.
///
/// `PartialEq` but not `Eq`: it carries a location, and a coordinate is a float.
/// That is the right restriction rather than a nuisance — exact equality on a
/// latitude is not a question worth being able to ask, and `geo.hpp` compares places
/// with a tolerance for the same reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusView {
    pub node_id: String,
    pub hostname: String,
    pub profile: String,
    pub archieos_version: String,
    pub os: String,
    pub kernel: String,
    pub architecture: String,
    pub uptime_ms: i64,

    pub cpu_model: String,
    pub logical_cores: u32,
    pub physical_cores: u32,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub accelerators: Vec<String>,
    pub storage: Vec<String>,
    pub networks: Vec<String>,

    pub capabilities: Vec<String>,
    pub peers: usize,
    pub reachable_peers: usize,
    pub modules_loaded: usize,
    pub modules_installed: usize,
    pub models_registered: usize,
    pub active_tasks: u64,
    pub health: String,
    pub fabric_transport: String,
    /// Hardware that appeared or disappeared since the previous boot.
    pub capability_changes: Vec<String>,
    /// Where the operator said this machine is. `None` when nobody has said.
    ///
    /// Carried so the observatory can put the globe where the machine is instead of
    /// on a default meridian. It is configuration rather than measurement, and the
    /// absence is meaningful: a display that centres on somewhere plausible is worse
    /// than one that centres on nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<crate::config::NodeLocation>,
}

/// The daemon's reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum ControlResponse {
    WorldResult {
        value: serde_json::Value,
    },
    AiResult {
        outcome: Box<archie_workstation::ai::InferenceOutcome>,
    },
    AiHarnessResult {
        outcome: Box<archie_workstation::harness_runtime::HarnessRunOutcome>,
    },
    /// Core's emergency pause as it stands after the request.
    AiPolicy {
        /// Whether the live engine refuses AI dispatch now.
        paused: bool,
        /// Whether a pause is recorded beside Core state, so a restart keeps it.
        recorded: bool,
    },
    Status(Box<StatusView>),
    Topology {
        /// capability → node ids providing it.
        capabilities: BTreeMap<String, Vec<String>>,
        nodes: Vec<PeerView>,
        totals: TotalsView,
    },
    Modules {
        modules: Vec<ModuleView>,
        load_order: Vec<String>,
    },
    Providers {
        providers: Vec<ProviderView>,
        /// Ids that will actually run, in id order.
        active: Vec<String>,
    },
    Peers {
        peers: Vec<PeerView>,
    },
    Devices {
        devices: Vec<DeviceView>,
    },
    DeviceOperations {
        operations: Vec<DeviceOperationView>,
    },
    SentryPrinter {
        resource_id: String,
        snapshot: archie_sentry::adapters::ipp::PrinterSnapshot,
    },
    SentryHomeStatus {
        snapshot: archie_sentry::adapters::home_assistant::HomeSnapshot,
    },
    /// Driver-planning outcome for one configured printer. The command line is
    /// rendered data for the operator; nothing here executes it.
    SentryPrinterDoctor {
        resource_id: String,
        tier: String,
        detail: String,
        planned_command_line: String,
    },
    SentryPrepared {
        intent: Box<archie_sentry::contracts::Intent>,
        digest: String,
    },
    SentryResult(Box<archie_sentry::engine::ExecutionResult>),
    SentryCancelled,
    SentryReceipt(Box<archie_sentry::journal::JournalRecord>),
    CortexTrajectories {
        trajectories: Vec<CortexTrajectoryView>,
    },
    Models {
        models: Vec<ModelView>,
    },
    Boot(Box<BootReport>),
    Metrics(Box<MetricSnapshot>),
    Audit {
        events: Vec<serde_json::Value>,
    },
    Pong {
        node_id: String,
        version: String,
    },
    Error {
        message: String,
    },
}

impl ControlResponse {
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }
}

/// Encode one message as a protocol line.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(message)
        .map_err(|e| ControlError::Malformed(format!("cannot encode: {e}")))?;
    if bytes.len() > MAX_CONTROL_MESSAGE_BYTES {
        return Err(ControlError::TooLarge);
    }
    // The newline is the frame delimiter, so the payload must not contain one.
    // `serde_json::to_vec` never emits a bare newline, which is why this is safe.
    bytes.push(b'\n');
    Ok(bytes)
}

/// Decode one protocol line.
pub fn decode<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T> {
    if line.len() > MAX_CONTROL_MESSAGE_BYTES {
        return Err(ControlError::TooLarge);
    }
    serde_json::from_str(line.trim_end())
        .map_err(|e| ControlError::Malformed(format!("cannot decode: {e}")))
}

/// Serve requests on one already-accepted connection.
///
/// Generic over the stream so the same code path serves a Unix socket on Linux and a
/// TCP socket on a Windows development host — and so it can be tested over an in-memory
/// duplex with no socket at all.
pub async fn serve_connection<S, F, Fut>(stream: S, mut handler: F) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    F: FnMut(ControlRequest) -> Fut,
    Fut: std::future::Future<Output = ControlResponse>,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        // Bound the read itself, not the decoded result.
        //
        // `read_line` grows its buffer until it meets a newline or the end of input, so a
        // client that opens this socket and writes without ever sending a delimiter makes
        // the daemon allocate until the kernel kills it. The size check inside `decode` is
        // too late to prevent that — by the time it runs, the memory is already spent. The
        // control socket is group-accessible, so this does not even require root.
        //
        // One byte past the limit is enough to tell "at the limit" from "over" it.
        let read = {
            let mut limited = (&mut reader).take(MAX_CONTROL_MESSAGE_BYTES as u64 + 1);
            limited.read_line(&mut line).await?
        };
        if read == 0 {
            return Ok(()); // client hung up
        }
        if read > MAX_CONTROL_MESSAGE_BYTES {
            // Answer, then close. There is no resynchronising from here: the rest of the
            // oversized frame is still queued, and reading on would treat its tail as the
            // frames that follow. Closing is the only honest recovery.
            let response = ControlResponse::error(ControlError::TooLarge.to_string());
            let _ = write_half.write_all(&encode(&response)?).await;
            let _ = write_half.flush().await;
            return Ok(());
        }
        if line.trim().is_empty() {
            continue;
        }

        // A malformed request must produce an error response, not close the connection:
        // one bad line from a script should not take down the operator's session.
        let response = match decode::<ControlRequest>(&line) {
            Ok(request) => handler(request).await,
            Err(e) => ControlResponse::error(e.to_string()),
        };

        write_half.write_all(&encode(&response)?).await?;
        write_half.flush().await?;
    }
}

/// Send one request over an established stream and read the reply.
pub async fn request_over<S>(stream: &mut S, request: &ControlRequest) -> Result<ControlResponse>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    stream.write_all(&encode(request)?).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    // Bounded for the same reason as the server's read: a daemon that is wedged, buggy or
    // impersonated must not be able to grow the CLI's memory without limit either.
    let read = {
        let mut limited = (&mut reader).take(MAX_CONTROL_MESSAGE_BYTES as u64 + 1);
        limited.read_line(&mut line).await?
    };
    if read == 0 {
        return Err(ControlError::NotRunning(
            "daemon closed the connection without replying".into(),
        ));
    }
    if read > MAX_CONTROL_MESSAGE_BYTES {
        return Err(ControlError::TooLarge);
    }
    decode(&line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_authenticated_archie_agent_can_enroll_from_one_connect_intent() {
        let intent = DeviceConnectionIntent {
            device_reference: "mama-phone".into(),
            class: DeviceClass::Mobile,
            transport: DevicePairingTransport::ArchieFabric,
            authenticated_public_key: Some("a".repeat(64)),
        };

        assert_eq!(
            intent.plan().unwrap(),
            DevicePairingPlan::AutoEnroll {
                device_reference: "mama-phone".into(),
                class: DeviceClass::Mobile,
                public_key: "a".repeat(64),
            }
        );
    }

    #[test]
    fn a_lan_advertisement_cannot_impersonate_an_authenticated_agent() {
        let missing_key = DeviceConnectionIntent {
            device_reference: "living-room-tv".into(),
            class: DeviceClass::Television,
            transport: DevicePairingTransport::ArchieFabric,
            authenticated_public_key: None,
        };
        assert_eq!(
            missing_key.plan().unwrap_err(),
            DeviceConnectionError::MissingAuthenticatedKey
        );

        let malformed_key = DeviceConnectionIntent {
            authenticated_public_key: Some("not-a-key".into()),
            ..missing_key
        };
        assert_eq!(
            malformed_key.plan().unwrap_err(),
            DeviceConnectionError::InvalidAuthenticatedKey
        );
    }

    #[test]
    fn vendor_firmware_is_discovered_without_being_silently_trusted() {
        for transport in [
            DevicePairingTransport::Matter,
            DevicePairingTransport::RokuEcp,
            DevicePairingTransport::HomeAssistant,
            DevicePairingTransport::VendorSpecific,
        ] {
            let intent = DeviceConnectionIntent {
                device_reference: "living-room-tv".into(),
                class: DeviceClass::Television,
                transport,
                authenticated_public_key: Some("b".repeat(64)),
            };
            assert!(matches!(
                intent.plan().unwrap(),
                DevicePairingPlan::AwaitDeviceProof {
                    transport: actual,
                    ..
                } if actual == transport
            ));
        }
    }

    #[test]
    fn nmap_host_discovery_is_structured_and_private_network_only() {
        let plan = NmapSurveyPlan::build(
            NmapSurveyProfile::HostDiscovery,
            vec!["192.168.1.0/24".into(), "10.20.0.0/24".into()],
        )
        .unwrap();
        assert_eq!(plan.estimated_addresses(), 512);
        assert_eq!(
            plan.approved_private_cidrs(),
            ["192.168.1.0/24", "10.20.0.0/24"]
        );
        assert_eq!(plan.profile(), NmapSurveyProfile::HostDiscovery);
        assert!(plan.arguments().contains(&"-sn".into()));
        assert!(plan.arguments().contains(&"--".into()));
        assert!(!plan.arguments().iter().any(|arg| arg.contains("script")));
        assert!(!plan.arguments().iter().any(|arg| arg.contains("password")));

        assert!(matches!(
            NmapSurveyPlan::build(NmapSurveyProfile::HostDiscovery, vec!["8.8.8.0/24".into()]),
            Err(NetworkSurveyError::NonPrivateCidr)
        ));
        assert!(matches!(
            NmapSurveyPlan::build(
                NmapSurveyProfile::HostDiscovery,
                vec!["192.168.1.5/24".into()],
            ),
            Err(NetworkSurveyError::NonPrivateCidr)
        ));
    }

    #[test]
    fn nmap_service_inventory_is_rate_limited_and_rejects_overscope() {
        let plan = NmapSurveyPlan::build(
            NmapSurveyProfile::ServiceInventory,
            vec!["192.168.20.0/24".into()],
        )
        .unwrap();
        assert!(plan.arguments().contains(&"-sV".into()));
        assert!(plan.arguments().contains(&"--version-light".into()));
        assert!(plan.arguments().contains(&"--max-rate".into()));
        assert!(plan.arguments().contains(&"50".into()));

        assert!(matches!(
            NmapSurveyPlan::build(NmapSurveyProfile::HostDiscovery, vec!["10.0.0.0/8".into()]),
            Err(NetworkSurveyError::ScopeTooLarge)
        ));
    }

    #[test]
    fn nmap_survey_never_silently_deduplicates_or_expands_scope() {
        assert!(matches!(
            NmapSurveyPlan::build(
                NmapSurveyProfile::HostDiscovery,
                vec!["192.168.1.0/24".into(), "192.168.1.0/024".into()],
            ),
            Err(NetworkSurveyError::DuplicateScope)
        ));
        assert!(matches!(
            NmapSurveyPlan::build(NmapSurveyProfile::HostDiscovery, vec![]),
            Err(NetworkSurveyError::InvalidScope)
        ));
    }

    #[test]
    fn analytic_inventory_classification_is_deterministic_and_deduplicated() {
        let camera = analytically_classify_network_asset(&[
            NetworkEvidence::MdnsRtsp,
            NetworkEvidence::OpenTcpPort(554),
            NetworkEvidence::MdnsRtsp,
        ]);
        assert_eq!(
            camera,
            vec![AnalyticDeviceClassification {
                class: DeviceClass::Camera,
                score: 100,
                supporting_signals: 2,
            }]
        );

        let television = analytically_classify_network_asset(&[
            NetworkEvidence::OpenTcpPort(8060),
            NetworkEvidence::MdnsRoku,
        ]);
        assert_eq!(
            television,
            vec![AnalyticDeviceClassification {
                class: DeviceClass::Television,
                score: 100,
                supporting_signals: 2,
            }]
        );
    }

    #[test]
    fn analytic_inventory_never_treats_one_common_port_as_an_identity() {
        assert!(
            analytically_classify_network_asset(&[NetworkEvidence::OpenTcpPort(554)]).is_empty()
        );
        assert!(
            analytically_classify_network_asset(&[NetworkEvidence::OpenTcpPort(8123)]).is_empty()
        );
    }

    #[test]
    fn a_survey_classifies_only_unique_assets_inside_its_declared_private_scope() {
        let plan = NmapSurveyPlan::build(
            NmapSurveyProfile::ServiceInventory,
            vec!["192.168.42.0/24".into()],
        )
        .unwrap();
        let assets = plan
            .classify_observations(&[
                NetworkAssetObservation {
                    ipv4_address: "192.168.42.20".parse().unwrap(),
                    evidence: vec![
                        SourcedNetworkEvidence {
                            source: NetworkEvidenceSource::PassiveMdns,
                            evidence: NetworkEvidence::MdnsRoku,
                        },
                        SourcedNetworkEvidence {
                            source: NetworkEvidenceSource::NmapServiceInventory,
                            evidence: NetworkEvidence::OpenTcpPort(8060),
                        },
                    ],
                },
                NetworkAssetObservation {
                    ipv4_address: "192.168.42.10".parse().unwrap(),
                    evidence: vec![SourcedNetworkEvidence {
                        source: NetworkEvidenceSource::NmapServiceInventory,
                        evidence: NetworkEvidence::OpenTcpPort(22),
                    }],
                },
            ])
            .unwrap();
        assert_eq!(
            assets[0].ipv4_address,
            "192.168.42.10".parse::<Ipv4Addr>().unwrap()
        );
        assert!(assets[0].candidates.is_empty());
        assert_eq!(
            assets[1].ipv4_address,
            "192.168.42.20".parse::<Ipv4Addr>().unwrap()
        );
        assert_eq!(assets[1].candidates[0].class, DeviceClass::Television);

        assert!(matches!(
            plan.classify_observations(&[NetworkAssetObservation {
                ipv4_address: "10.0.0.7".parse().unwrap(),
                evidence: vec![SourcedNetworkEvidence {
                    source: NetworkEvidenceSource::PassiveMdns,
                    evidence: NetworkEvidence::MdnsRtsp,
                }],
            }]),
            Err(NetworkSurveyError::ObservationOutsideScope)
        ));
    }

    #[test]
    fn a_survey_rejects_duplicate_observations_and_tampered_plan_fields() {
        let plan = NmapSurveyPlan::build(
            NmapSurveyProfile::HostDiscovery,
            vec!["192.168.42.0/24".into()],
        )
        .unwrap();
        let duplicate = NetworkAssetObservation {
            ipv4_address: "192.168.42.7".parse().unwrap(),
            evidence: vec![],
        };
        assert!(matches!(
            plan.classify_observations(&[duplicate.clone(), duplicate]),
            Err(NetworkSurveyError::DuplicateObservation)
        ));

        let mut tampered = plan;
        tampered.arguments.clear();
        assert_eq!(tampered.validate(), Err(NetworkSurveyError::InvalidPlan));
    }

    #[test]
    fn survey_evidence_must_match_its_declared_collector_and_profile() {
        let host_plan = NmapSurveyPlan::build(
            NmapSurveyProfile::HostDiscovery,
            vec!["192.168.42.0/24".into()],
        )
        .unwrap();
        assert!(matches!(
            host_plan.classify_observations(&[NetworkAssetObservation {
                ipv4_address: "192.168.42.7".parse().unwrap(),
                evidence: vec![SourcedNetworkEvidence {
                    source: NetworkEvidenceSource::NmapServiceInventory,
                    evidence: NetworkEvidence::OpenTcpPort(554),
                }],
            }]),
            Err(NetworkSurveyError::EvidenceSourceMismatch)
        ));

        let service_plan = NmapSurveyPlan::build(
            NmapSurveyProfile::ServiceInventory,
            vec!["192.168.42.0/24".into()],
        )
        .unwrap();
        assert!(matches!(
            service_plan.classify_observations(&[NetworkAssetObservation {
                ipv4_address: "192.168.42.7".parse().unwrap(),
                evidence: vec![SourcedNetworkEvidence {
                    source: NetworkEvidenceSource::PassiveMdns,
                    evidence: NetworkEvidence::OpenTcpPort(554),
                }],
            }]),
            Err(NetworkSurveyError::EvidenceSourceMismatch)
        ));
    }

    #[test]
    fn survey_inventory_delta_is_scope_bound_deterministic_and_not_an_offline_claim() {
        let plan = NmapSurveyPlan::build(
            NmapSurveyProfile::ServiceInventory,
            vec!["192.168.42.0/24".into()],
        )
        .unwrap();
        let changes = plan
            .compare_observations(
                &[
                    NetworkAssetObservation {
                        ipv4_address: "192.168.42.10".parse().unwrap(),
                        evidence: vec![SourcedNetworkEvidence {
                            source: NetworkEvidenceSource::PassiveMdns,
                            evidence: NetworkEvidence::MdnsRoku,
                        }],
                    },
                    NetworkAssetObservation {
                        ipv4_address: "192.168.42.20".parse().unwrap(),
                        evidence: vec![],
                    },
                ],
                &[
                    NetworkAssetObservation {
                        ipv4_address: "192.168.42.10".parse().unwrap(),
                        evidence: vec![SourcedNetworkEvidence {
                            source: NetworkEvidenceSource::PassiveMdns,
                            evidence: NetworkEvidence::MdnsGoogleCast,
                        }],
                    },
                    NetworkAssetObservation {
                        ipv4_address: "192.168.42.30".parse().unwrap(),
                        evidence: vec![SourcedNetworkEvidence {
                            source: NetworkEvidenceSource::PassiveMdns,
                            evidence: NetworkEvidence::MdnsRtsp,
                        }],
                    },
                ],
            )
            .unwrap();
        assert_eq!(changes.len(), 3);
        assert!(matches!(
            changes[0],
            NetworkInventoryChange::ClassificationChanged { ipv4_address, .. }
                if ipv4_address == "192.168.42.10".parse::<Ipv4Addr>().unwrap()
        ));
        assert!(matches!(
            changes[1],
            NetworkInventoryChange::NoLongerObserved { ipv4_address, .. }
                if ipv4_address == "192.168.42.20".parse::<Ipv4Addr>().unwrap()
        ));
        assert!(matches!(
            changes[2],
            NetworkInventoryChange::NewlyObserved { ipv4_address, .. }
                if ipv4_address == "192.168.42.30".parse::<Ipv4Addr>().unwrap()
        ));
    }

    #[test]
    fn requests_round_trip() {
        for request in [
            ControlRequest::Status,
            ControlRequest::Topology,
            ControlRequest::Modules,
            ControlRequest::Providers,
            ControlRequest::Peers,
            ControlRequest::Devices,
            ControlRequest::DeviceOperations { limit: 10 },
            ControlRequest::CortexTrajectories { limit: 10 },
            ControlRequest::Models,
            ControlRequest::Boot,
            ControlRequest::Metrics,
            ControlRequest::Audit { limit: 10 },
            ControlRequest::Ping,
        ] {
            let encoded = encode(&request).unwrap();
            let line = String::from_utf8(encoded).unwrap();
            assert!(
                line.ends_with('\n'),
                "every message must be one framed line"
            );
            assert_eq!(
                decode::<ControlRequest>(&line).unwrap(),
                request,
                "{} did not round trip",
                request.name()
            );
        }
    }

    #[test]
    fn an_encoded_message_contains_exactly_one_newline() {
        // The newline is the frame delimiter; an embedded one would desynchronise the
        // stream for every subsequent message.
        let response = ControlResponse::error("a message\nwith an embedded newline");
        let encoded = encode(&response).unwrap();
        assert_eq!(encoded.iter().filter(|b| **b == b'\n').count(), 1);
        let line = String::from_utf8(encoded).unwrap();
        assert_eq!(decode::<ControlResponse>(&line).unwrap(), response);
    }

    #[test]
    fn garbage_decodes_to_an_error_not_a_panic() {
        for junk in ["", "{", "not json", "[]", "{\"request\":\"nonexistent\"}"] {
            assert!(decode::<ControlRequest>(junk).is_err());
        }
    }

    #[test]
    fn an_oversized_message_is_refused() {
        let huge = "x".repeat(MAX_CONTROL_MESSAGE_BYTES + 1);
        assert!(matches!(
            decode::<ControlRequest>(&huge).unwrap_err(),
            ControlError::TooLarge
        ));
    }

    #[test]
    fn every_v0_1_request_is_read_only() {
        // If this ever fails, the dispatcher needs an authorisation branch before the
        // new request is served.
        for request in [
            ControlRequest::Status,
            ControlRequest::Topology,
            ControlRequest::Devices,
            ControlRequest::DeviceOperations { limit: 10 },
            ControlRequest::CortexTrajectories { limit: 10 },
            ControlRequest::Ping,
        ] {
            assert!(!request.is_mutating());
        }
    }

    #[tokio::test]
    async fn a_request_gets_its_response_over_a_stream() {
        let (client, server) = tokio::io::duplex(64 * 1024);

        tokio::spawn(async move {
            serve_connection(server, |request| async move {
                match request {
                    ControlRequest::Ping => ControlResponse::Pong {
                        node_id: "abc123".into(),
                        version: "0.1.0".into(),
                    },
                    other => ControlResponse::error(format!("unhandled {}", other.name())),
                }
            })
            .await
            .ok();
        });

        let mut client = client;
        let response = request_over(&mut client, &ControlRequest::Ping)
            .await
            .unwrap();
        assert_eq!(
            response,
            ControlResponse::Pong {
                node_id: "abc123".into(),
                version: "0.1.0".into()
            }
        );
    }

    #[tokio::test]
    async fn a_malformed_line_does_not_kill_the_connection() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            serve_connection(server, |_| async {
                ControlResponse::Pong {
                    node_id: "n".into(),
                    version: "0.1.0".into(),
                }
            })
            .await
            .ok();
        });

        let mut client = client;
        // Send junk...
        client.write_all(b"this is not json\n").await.unwrap();
        client.flush().await.unwrap();

        let mut reader = BufReader::new(&mut client);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(decode::<ControlResponse>(&line).unwrap().is_error());

        // ...then a valid request on the same connection still works.
        let mut line = String::new();
        client
            .write_all(&encode(&ControlRequest::Ping).unwrap())
            .await
            .unwrap();
        client.flush().await.unwrap();
        let mut reader = BufReader::new(&mut client);
        reader.read_line(&mut line).await.unwrap();
        assert!(!decode::<ControlResponse>(&line).unwrap().is_error());
    }

    #[tokio::test]
    async fn several_requests_share_one_connection() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            serve_connection(server, |request| async move {
                ControlResponse::Pong {
                    node_id: request.name().into(),
                    version: "0.1.0".into(),
                }
            })
            .await
            .ok();
        });

        let mut client = client;
        for request in [
            ControlRequest::Ping,
            ControlRequest::Status,
            ControlRequest::Peers,
        ] {
            let response = request_over(&mut client, &request).await.unwrap();
            match response {
                ControlResponse::Pong { node_id, .. } => assert_eq!(node_id, request.name()),
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn responses_round_trip() {
        let response = ControlResponse::Modules {
            modules: vec![ModuleView {
                id: "cognition.critic".into(),
                version: "0.1.0".into(),
                state: "unsatisfied".into(),
                explanation: "node does not provide: compute.cuda".into(),
                dependencies: vec!["core.memory".into()],
            }],
            load_order: vec!["core.archie".into()],
        };
        let line = String::from_utf8(encode(&response).unwrap()).unwrap();
        assert_eq!(decode::<ControlResponse>(&line).unwrap(), response);
    }

    #[test]
    fn device_inventory_response_round_trips_without_a_full_public_key() {
        let response = ControlResponse::Devices {
            devices: vec![DeviceView {
                device_reference: "living-room-tv".into(),
                class: "television".into(),
                pairing: "paired".into(),
                public_key_fingerprint: "a1b2c3d4e5f60708".into(),
                first_seen: Timestamp::from_millis(1),
                last_seen: Timestamp::from_millis(2),
            }],
        };
        let line = String::from_utf8(encode(&response).unwrap()).unwrap();
        assert_eq!(decode::<ControlResponse>(&line).unwrap(), response);
        assert!(!line.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn device_operation_response_round_trips_without_a_payload_or_full_digest() {
        let response = ControlResponse::DeviceOperations {
            operations: vec![DeviceOperationView {
                operation_id: 42,
                device_reference: "living-room-tv".into(),
                state: "prepared".into(),
                trace_fingerprint: "e1f2a3b4c5d6".into(),
                operation_sha256_fingerprint: "bbbbbbbbbbbbbbbb".into(),
                prepared_at: Timestamp::from_millis(1),
            }],
        };
        let line = String::from_utf8(encode(&response).unwrap()).unwrap();
        assert_eq!(decode::<ControlResponse>(&line).unwrap(), response);
        assert!(!line.contains(&"b".repeat(64)));
    }

    #[test]
    fn cortex_trajectory_response_omits_private_vault_references() {
        let response = ControlResponse::CortexTrajectories {
            trajectories: vec![CortexTrajectoryView {
                trajectory_id: "trajectory:1".into(),
                correction_kind: "missing_context_label".into(),
                source_graph_snapshot_id: "graph:before".into(),
                resulting_graph_snapshot_id: "graph:after".into(),
                selected_context_count: 2,
                omitted_candidate_count: 1,
                model_id: "deterministic-v1".into(),
                payload_sha256_fingerprint: "aaaaaaaaaaaaaaaa".into(),
                captured_at: Timestamp::from_millis(1),
            }],
        };
        let line = String::from_utf8(encode(&response).unwrap()).unwrap();
        assert_eq!(decode::<ControlResponse>(&line).unwrap(), response);
        for private_field in [
            "vault",
            "response_ref",
            "rationale_ref",
            "raw_correction_ref",
        ] {
            assert!(
                !line.contains(private_field),
                "control leaked {private_field}"
            );
        }
    }

    fn tv_state(input: TelevisionInput) -> TelevisionState {
        TelevisionState {
            device_reference: "living-room-tv".into(),
            available_inputs: [
                TelevisionInput::Hdmi { port: 1 },
                TelevisionInput::Roku,
                TelevisionInput::Tuner,
            ]
            .into_iter()
            .collect(),
            active_input: input,
            tuner_channel: Some(7),
        }
    }

    fn television_inventory(pairing: archie_state::DevicePairingState) -> DeviceRecord {
        DeviceRecord {
            device_reference: "living-room-tv".into(),
            public_key: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            class: DeviceClass::Television,
            first_seen: Timestamp::now(),
            last_seen: Timestamp::now(),
            pairing,
        }
    }

    fn durable_operation(
        operation_id: u64,
        operation: TelevisionOperation,
        state: DeviceOperationState,
    ) -> DeviceOperationRecord {
        DeviceOperationRecord {
            operation_id,
            device_reference: "living-room-tv".into(),
            trace_id: TraceId::new(),
            operation_sha256: operation.sha256().unwrap(),
            prepared_at: Timestamp::now(),
            state,
        }
    }

    fn native_television_adapter_module() -> ModuleManifest {
        ModuleManifest {
            id: archie_types::ModuleId::parse("devices.television").unwrap(),
            version: "0.1.0".into(),
            description: "reviewed native television adapter".into(),
            required_capabilities: vec![],
            optional_capabilities: vec![],
            dependencies: vec![],
            permissions: vec![
                archie_types::ModulePermission::DeviceCommunicate,
                archie_types::ModulePermission::DeviceActuate,
            ],
            architectures: vec![],
            profiles: vec![],
            priority: 100,
            worker: archie_types::WorkerImpl::Builtin {
                symbol: "archie_television_adapter".into(),
            },
            health_check: archie_types::HealthCheck::None,
            config_schema: None,
            labels: BTreeMap::new(),
        }
    }

    fn native_television_adapter_contract() -> DeviceAdapterManifest {
        DeviceAdapterManifest {
            adapter_id: "living-room-tv".into(),
            module_id: archie_types::ModuleId::parse("devices.television").unwrap(),
            origin: archie_types::DeviceAdapterOrigin::BuiltIntoIso,
            supported_classes: vec![DeviceAdapterClass::Television],
            operations: vec![
                archie_types::DeviceAdapterOperation::ObserveState,
                archie_types::DeviceAdapterOperation::ApplyDesiredState,
            ],
            resource_budget: archie_types::DeviceAdapterResourceBudget {
                max_resident_memory_mib: 64,
                max_cpu_millis_per_second: 100,
                max_gpu_memory_mib: 0,
            },
            dry_run_required: true,
            receipt_required: true,
        }
    }

    fn prepared(operation_id: u64, operation: TelevisionOperation) -> PreparedTelevisionOperation {
        PreparedTelevisionOperation::prepare(
            operation_id,
            "living-room-tv",
            operation,
            TelevisionProposalOrigin::AuthenticatedOperator,
        )
        .unwrap()
    }

    #[test]
    fn hdmi_projection_is_a_bounded_prepared_operation() {
        let operation = prepared(
            42,
            TelevisionOperation::SelectInput {
                input: TelevisionInput::Hdmi { port: 1 },
            },
        );
        let request = operation
            .capability_request(
                TraceId::new(),
                None,
                Principal::Human {
                    id: "operator".into(),
                    authenticated: true,
                },
                "project the native observatory on HDMI 1",
            )
            .unwrap();

        assert_eq!(
            request.resource,
            Resource::Device {
                device: "living-room-tv".into(),
            }
        );
        assert_eq!(request.scope, Scope::OneOperation { operation: 42 });
        assert_eq!(
            request.constraints.actions,
            [Action::Actuate].into_iter().collect()
        );
    }

    #[test]
    fn voice_can_propose_but_not_bypass_physical_policy() {
        use archie_policy::{Decision, PolicyEngine};

        let operation = PreparedTelevisionOperation::prepare(
            43,
            "living-room-tv",
            TelevisionOperation::SetTunerChannel { channel: 13 },
            TelevisionProposalOrigin::VoiceProposal {
                transcript_sha256:
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            },
        )
        .unwrap();
        assert!(operation.origin.requires_operator_approval());

        let request = operation
            .capability_request(
                TraceId::new(),
                None,
                Principal::Agent {
                    agent: archie_types::AgentId::new(),
                },
                "voice proposal: change the television channel",
            )
            .unwrap();
        let decision = PolicyEngine::default().decide(&request);
        assert_eq!(decision.decision, Decision::HardStop);
        assert!(decision.lease.is_none());
    }

    #[test]
    fn simulator_predicts_hdmi_projection_without_mutating_a_device() {
        let simulator = TelevisionSimulator::new(tv_state(TelevisionInput::Roku)).unwrap();
        let operation = prepared(
            44,
            TelevisionOperation::SelectInput {
                input: TelevisionInput::Hdmi { port: 1 },
            },
        );

        let receipt = simulator.preview(&operation).unwrap();
        assert!(receipt.dry_run);
        assert_eq!(receipt.before.active_input, TelevisionInput::Roku);
        assert_eq!(
            receipt.predicted_after.active_input,
            TelevisionInput::Hdmi { port: 1 }
        );
        assert_eq!(
            simulator,
            TelevisionSimulator::new(tv_state(TelevisionInput::Roku)).unwrap()
        );
    }

    #[test]
    fn inventory_backed_simulation_requires_a_paired_television_record() {
        let proposed = television_inventory(archie_state::DevicePairingState::Proposed);
        assert_eq!(
            TelevisionSimulator::from_paired_inventory(&proposed, tv_state(TelevisionInput::Roku))
                .unwrap_err(),
            TelevisionOperationError::DeviceNotPaired
        );

        let mut wrong_class = television_inventory(archie_state::DevicePairingState::Paired);
        wrong_class.class = DeviceClass::Router;
        assert_eq!(
            TelevisionSimulator::from_paired_inventory(
                &wrong_class,
                tv_state(TelevisionInput::Roku)
            )
            .unwrap_err(),
            TelevisionOperationError::UnexpectedDeviceClass
        );

        let paired = television_inventory(archie_state::DevicePairingState::Paired);
        let simulator =
            TelevisionSimulator::from_paired_inventory(&paired, tv_state(TelevisionInput::Roku))
                .unwrap();
        assert_eq!(
            simulator,
            TelevisionSimulator::new(tv_state(TelevisionInput::Roku)).unwrap()
        );
    }

    #[test]
    fn native_adapter_admission_requires_a_paired_supported_device() {
        let adapter = native_television_adapter_contract();
        let module = native_television_adapter_module();
        let paired = television_inventory(archie_state::DevicePairingState::Paired);

        let binding = PairedDeviceAdapterBinding::admit(&adapter, &module, &paired).unwrap();
        assert_eq!(binding.adapter_id, "living-room-tv");
        assert_eq!(binding.module_id, "devices.television");
        assert_eq!(binding.device_reference, "living-room-tv");
        assert_eq!(binding.class, "television");

        let proposed = television_inventory(archie_state::DevicePairingState::Proposed);
        assert!(matches!(
            PairedDeviceAdapterBinding::admit(&adapter, &module, &proposed),
            Err(DeviceAdapterBindingError::DeviceNotPaired)
        ));

        let mut wrong_class = television_inventory(archie_state::DevicePairingState::Paired);
        wrong_class.class = DeviceClass::Router;
        assert!(matches!(
            PairedDeviceAdapterBinding::admit(&adapter, &module, &wrong_class),
            Err(DeviceAdapterBindingError::UnsupportedDeviceClass)
        ));
    }

    #[test]
    fn a_generated_adapter_proposal_cannot_bind_to_a_paired_device() {
        let mut adapter = native_television_adapter_contract();
        adapter.origin = archie_types::DeviceAdapterOrigin::GeneratedProposal;
        let module = native_television_adapter_module();
        let paired = television_inventory(archie_state::DevicePairingState::Paired);

        assert!(matches!(
            PairedDeviceAdapterBinding::admit(&adapter, &module, &paired),
            Err(DeviceAdapterBindingError::InvalidContract(_))
        ));
    }

    #[test]
    fn generic_adapter_mutation_requires_the_exact_human_one_operation_lease() {
        let adapter = native_television_adapter_contract();
        let module = native_television_adapter_module();
        let device = television_inventory(archie_state::DevicePairingState::Paired);
        let binding = PairedDeviceAdapterBinding::admit(&adapter, &module, &device).unwrap();
        let durable = DeviceOperationRecord {
            operation_id: 91,
            device_reference: "living-room-tv".into(),
            trace_id: TraceId::new(),
            operation_sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .into(),
            prepared_at: Timestamp::now(),
            state: DeviceOperationState::Prepared,
        };
        let prepared = PreparedDeviceAdapterOperation::from_durable(
            &binding,
            &adapter,
            &durable,
            archie_types::DeviceAdapterOperation::ApplyDesiredState,
        )
        .unwrap();
        let principal = Principal::Human {
            id: "operator".into(),
            authenticated: true,
        };
        let request = prepared
            .capability_request(
                TraceId::new(),
                None,
                principal.clone(),
                "switch the paired display input",
            )
            .unwrap();
        assert_eq!(request.scope, Scope::OneOperation { operation: 91 });
        assert_eq!(
            request.constraints.actions,
            [Action::Actuate].into_iter().collect()
        );

        let policy = archie_policy::PolicyEngine::new(archie_policy::PolicyConfig {
            safety_boundary_present: true,
            ..Default::default()
        });
        let lease = policy.decide(&request).lease.unwrap();
        prepared
            .validate_lease(&lease, principal.clone(), None, true, Timestamp::now())
            .unwrap();

        let mut substituted = prepared.clone();
        substituted.operation_id = 92;
        assert!(matches!(
            substituted.validate_lease(&lease, principal, None, true, Timestamp::now()),
            Err(DeviceAdapterLeaseError::Lease(
                LeaseRejection::OperationMismatch
            ))
        ));
    }

    #[test]
    fn generic_adapter_observation_stays_typed_and_cannot_name_an_undeclared_action() {
        let adapter = native_television_adapter_contract();
        let module = native_television_adapter_module();
        let device = television_inventory(archie_state::DevicePairingState::Paired);
        let binding = PairedDeviceAdapterBinding::admit(&adapter, &module, &device).unwrap();
        let durable = DeviceOperationRecord {
            operation_id: 92,
            device_reference: "living-room-tv".into(),
            trace_id: TraceId::new(),
            operation_sha256: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                .into(),
            prepared_at: Timestamp::now(),
            state: DeviceOperationState::Prepared,
        };
        let observation = PreparedDeviceAdapterOperation::from_durable(
            &binding,
            &adapter,
            &durable,
            archie_types::DeviceAdapterOperation::ObserveState,
        )
        .unwrap();
        let request = observation
            .capability_request(
                TraceId::new(),
                None,
                Principal::Human {
                    id: "operator".into(),
                    authenticated: true,
                },
                "read the paired display state",
            )
            .unwrap();
        assert_eq!(
            request.constraints.actions,
            [Action::Read].into_iter().collect()
        );

        assert!(matches!(
            PreparedDeviceAdapterOperation::from_durable(
                &binding,
                &adapter,
                &durable,
                archie_types::DeviceAdapterOperation::FirmwareApply,
            ),
            Err(DeviceAdapterOperationError::OperationNotDeclared)
        ));

        let mut cancelled = durable;
        cancelled.state = DeviceOperationState::Cancelled;
        assert!(matches!(
            PreparedDeviceAdapterOperation::from_durable(
                &binding,
                &adapter,
                &cancelled,
                archie_types::DeviceAdapterOperation::ObserveState,
            ),
            Err(DeviceAdapterOperationError::DurableOperationNotPrepared)
        ));
    }

    #[test]
    fn durable_operation_binds_one_reviewed_television_payload_to_one_id() {
        let operation = TelevisionOperation::SelectInput {
            input: TelevisionInput::Hdmi { port: 1 },
        };
        let durable = durable_operation(57, operation, DeviceOperationState::Prepared);
        let device = television_inventory(archie_state::DevicePairingState::Paired);
        let prepared = PreparedTelevisionOperation::from_durable_inventory_operation(
            &durable,
            &device,
            operation,
            TelevisionProposalOrigin::AuthenticatedOperator,
        )
        .unwrap();
        assert_eq!(prepared.operation_id, 57);
        assert_eq!(prepared.device_reference, "living-room-tv");

        let changed_payload = TelevisionOperation::SelectInput {
            input: TelevisionInput::Roku,
        };
        assert_eq!(
            PreparedTelevisionOperation::from_durable_inventory_operation(
                &durable,
                &device,
                changed_payload,
                TelevisionProposalOrigin::AuthenticatedOperator,
            )
            .unwrap_err(),
            TelevisionOperationError::DurableOperationDigestMismatch
        );

        let cancelled = durable_operation(57, operation, DeviceOperationState::Cancelled);
        assert_eq!(
            PreparedTelevisionOperation::from_durable_inventory_operation(
                &cancelled,
                &device,
                operation,
                TelevisionProposalOrigin::AuthenticatedOperator,
            )
            .unwrap_err(),
            TelevisionOperationError::DurableOperationNotPrepared
        );
    }

    #[test]
    fn setting_a_tuner_channel_is_an_explicit_input_switch_and_channel_change() {
        let simulator = TelevisionSimulator::new(tv_state(TelevisionInput::Roku)).unwrap();
        let operation = prepared(45, TelevisionOperation::SetTunerChannel { channel: 13 });

        let receipt = simulator.preview(&operation).unwrap();
        assert_eq!(receipt.predicted_after.active_input, TelevisionInput::Tuner);
        assert_eq!(receipt.predicted_after.tuner_channel, Some(13));
    }

    #[test]
    fn an_ambiguous_relative_channel_request_never_switches_from_roku() {
        let simulator = TelevisionSimulator::new(tv_state(TelevisionInput::Roku)).unwrap();
        let operation = prepared(
            46,
            TelevisionOperation::StepTunerChannel {
                direction: ChannelDirection::Next,
            },
        );

        assert_eq!(
            simulator.preview(&operation).unwrap_err(),
            TelevisionOperationError::TunerNotActive
        );
    }

    #[test]
    fn prepared_operation_validates_the_exact_one_operation_lease() {
        use archie_policy::{PolicyConfig, PolicyEngine};

        let operation = prepared(
            47,
            TelevisionOperation::SelectInput {
                input: TelevisionInput::Tuner,
            },
        );
        let principal = Principal::Human {
            id: "operator".into(),
            authenticated: true,
        };
        let request = operation
            .capability_request(
                TraceId::new(),
                None,
                principal.clone(),
                "switch to television tuner",
            )
            .unwrap();
        let lease = PolicyEngine::new(PolicyConfig {
            safety_boundary_present: true,
            ..Default::default()
        })
        .decide(&request)
        .lease
        .unwrap();

        operation
            .validate_lease(&lease, principal.clone(), None, true, Timestamp::now())
            .unwrap();

        let other = prepared(
            48,
            TelevisionOperation::SelectInput {
                input: TelevisionInput::Tuner,
            },
        );
        assert_eq!(
            other
                .validate_lease(&lease, principal, None, true, Timestamp::now())
                .unwrap_err(),
            TelevisionLeaseError::Lease(LeaseRejection::OperationMismatch)
        );
    }

    #[test]
    fn invalid_vendor_or_voice_data_is_refused_before_it_becomes_an_operation() {
        assert_eq!(
            PreparedTelevisionOperation::prepare(
                1,
                "living room tv; curl bad.example",
                TelevisionOperation::SelectInput {
                    input: TelevisionInput::Hdmi { port: 1 },
                },
                TelevisionProposalOrigin::AuthenticatedOperator,
            )
            .unwrap_err(),
            TelevisionOperationError::InvalidDeviceReference
        );
        assert_eq!(
            PreparedTelevisionOperation::prepare(
                1,
                "living-room-tv",
                TelevisionOperation::SelectInput {
                    input: TelevisionInput::Hdmi { port: 0 },
                },
                TelevisionProposalOrigin::AuthenticatedOperator,
            )
            .unwrap_err(),
            TelevisionOperationError::InvalidHdmiPort
        );
        assert_eq!(
            PreparedTelevisionOperation::prepare(
                1,
                "living-room-tv",
                TelevisionOperation::SetTunerChannel { channel: 13 },
                TelevisionProposalOrigin::VoiceProposal {
                    transcript_sha256: "not-a-digest".into(),
                },
            )
            .unwrap_err(),
            TelevisionOperationError::InvalidTranscriptDigest
        );
    }
}
