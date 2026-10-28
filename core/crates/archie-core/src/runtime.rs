//! The Archie runtime: the boot sequence, and the live state it produces.
//!
//! [`Runtime::boot`] executes the canonical sequence documented in
//! `docs/BOOT_SEQUENCE.md` and returns a node that is either READY or explains exactly
//! why it is not.

use crate::boot::{stages, BootReport, BootStage};
use crate::config::{ArchieConfig, TransportKind};
use crate::control::{
    ControlRequest, ControlResponse, CortexTrajectoryView, DeviceConnectionError,
    DeviceConnectionIntent, DeviceOperationView, DevicePairingPlan, DevicePairingTransport,
    DeviceView, ModelView, ModuleView, PeerView, ProviderView, StatusView, TotalsView,
};
use crate::hardware::HardwareProbe;
use crate::hotplug::{apply_scoped, scoped_delta, CapabilityDelta};
use archie_fabric::{
    Fabric, FabricConfig, FabricEvent, LoopbackBus, NodeIdentity, Transport, UdpTransport,
};
use archie_observability::kernel::psi::PressureThresholds;
use archie_observability::kernel::CapabilityClass;
use archie_observability::Metrics;
use archie_policy::{PolicyConfig, PolicyEngine};
use archie_registry::{
    ModelRegistry, ModuleRegistry, NodeRecord, PermissionPolicy, ProviderContext, ProviderPlan,
    ProviderRegistry, ResourceGraph, Topology, TopologyContext,
};
use archie_router::Router;
use archie_scheduler::{Scheduler, SchedulerConfig};
use archie_state::{
    CortexCorrectionTrajectoryRecord, DeviceOperationRecord, DeviceRecord, StatePaths, StateStore,
    TrustState,
};
use archie_types::{
    ModulePermission, NodeDescriptor, NodeHealth, NodeId, Timestamp, ARCHIEOS_VERSION,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("node identity unavailable: {0}")]
    Identity(String),

    #[error("state store unavailable: {0}")]
    State(String),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

/// Failure to make an authenticated device connection intent durable.
///
/// This is separate from a policy lease because enrollment establishes identity only. A
/// paired record still cannot command hardware; any later physical operation must create its
/// own prepared id, ask policy, obtain a one-operation lease, and reconcile a receipt.
#[derive(Debug, Error)]
pub enum DeviceEnrollmentError {
    #[error("invalid device connection intent: {0}")]
    InvalidIntent(#[from] DeviceConnectionError),

    #[error("device pairing needs its own proof through {transport:?}: {explanation}")]
    DeviceProofRequired {
        transport: DevicePairingTransport,
        explanation: String,
    },

    #[error("could not persist device pairing: {0}")]
    State(String),

    #[error("paired device disappeared from local inventory")]
    MissingAfterPairing,
}

/// A booted Archie node.
pub struct Runtime {
    pub identity: Arc<NodeIdentity>,
    pub config: ArchieConfig,
    pub paths: StatePaths,
    pub state: Arc<StateStore>,
    pub fabric: Arc<Fabric>,
    pub metrics: Metrics,

    graph: RwLock<ResourceGraph>,
    modules: ModuleRegistry,
    topology: Topology,
    providers: ProviderRegistry,
    /// Resolved once, at boot, from `providers` and the node's own capabilities and
    /// configuration — none of which change without a restart, so there is nothing for
    /// a stale cache to get wrong here the way there would be for anything network-fed.
    provider_plan: ProviderPlan,
    models: RwLock<ModelRegistry>,
    scheduler: RwLock<Scheduler>,
    router: Router,
    /// The one live engine. AI dispatch reads it at every admission, dispatch and
    /// commit, so an emergency pause set here stops work already admitted.
    policy: Arc<std::sync::RwLock<PolicyEngine>>,

    descriptor: RwLock<NodeDescriptor>,
    probe_warnings: Vec<String>,
    boot_report: BootReport,
    started_at: Timestamp,
    capability_changes: Vec<String>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("node_id", &self.identity.node_id().to_string())
            .field("ready", &self.boot_report.is_ready())
            .finish()
    }
}

impl Runtime {
    /// Execute the boot sequence.
    pub async fn boot(paths: StatePaths, config: ArchieConfig) -> Result<Self, RuntimeError> {
        let transport = build_transport(&config).await;
        Self::boot_with_transport(paths, config, transport).await
    }

    /// Boot against a supplied transport.
    ///
    /// This is what makes multi-node integration testing possible: several runtimes can
    /// share one [`LoopbackBus`] inside a single test process and genuinely discover
    /// each other, with no sockets and no timing races.
    pub async fn boot_with_transport(
        paths: StatePaths,
        config: ArchieConfig,
        transport: TransportOutcome,
    ) -> Result<Self, RuntimeError> {
        let started_at = Timestamp::now();
        let mut report = BootReport::new();

        // ---- 1. directories and identity -----------------------------------------
        paths.ensure_directories()?;

        let identity = match NodeIdentity::load_or_create(paths.identity_key()) {
            Ok(identity) => {
                report.push(BootStage::ready(stages::IDENTITY, true));
                Arc::new(identity)
            }
            Err(e) => {
                // Without an identity the node cannot sign anything, so it cannot
                // participate in the fabric at all. This is genuinely fatal.
                return Err(RuntimeError::Identity(e.to_string()));
            }
        };
        let node_id = identity.node_id();
        info!(node_id = %node_id, "node identity loaded");

        // ---- 2. hardware ----------------------------------------------------------
        let probe = HardwareProbe::run();
        let probe_warnings = probe.warnings.clone();
        for warning in &probe_warnings {
            report.warn(warning.clone());
        }
        report.push(BootStage::ready(stages::HARDWARE, true));

        // ---- 3. capability manifest ----------------------------------------------
        let descriptor = probe.to_descriptor(
            node_id,
            identity.public_key_hex(),
            config.node.profile,
            config.extra_capabilities(),
        );
        let problems = descriptor.self_consistency_problems();
        if problems.is_empty() {
            report.push(BootStage::ready(stages::CAPABILITIES, true));
        } else {
            // Our own descriptor failing the check every peer applies means no peer
            // would accept us. Better to say so loudly than to announce and be ignored.
            report.push(BootStage::failed(
                stages::CAPABILITIES,
                true,
                problems.join("; "),
            ));
        }

        // ---- 4. state store -------------------------------------------------------
        let state = match StateStore::open(paths.database()) {
            Ok(state) => {
                report.push(BootStage::ready(stages::STATE, true));
                Arc::new(state)
            }
            Err(e) => return Err(RuntimeError::State(e.to_string())),
        };

        let mut capability_changes = Vec::new();
        match state.record_boot(&descriptor) {
            Ok(delta) => {
                for capability in &delta.appeared {
                    capability_changes.push(format!("+{capability}"));
                }
                for capability in &delta.disappeared {
                    capability_changes.push(format!("-{capability}"));
                }
                if delta.had_previous_boot && !delta.is_unchanged() {
                    // Hardware that vanished is worth an operator's attention: a dead
                    // GPU presents exactly this way.
                    report.warn(format!(
                        "hardware changed since last boot: {}",
                        capability_changes.join(" ")
                    ));
                }
            }
            Err(e) => report.warn(format!("could not record boot: {e}")),
        }

        // ---- 5. fabric ------------------------------------------------------------
        let metrics = Metrics::new();
        let fabric_config = FabricConfig {
            announce_interval_ms: config.fabric.announce_interval_ms,
            peer_timeout_ms: config.fabric.peer_timeout_ms,
            peer_forget_ms: config.fabric.peer_forget_ms,
        };

        let TransportOutcome { transport, note } = transport;
        match &note {
            Some(detail) => report.push(BootStage::degraded(stages::FABRIC, false, detail.clone())),
            None => report.push(BootStage::ready(stages::FABRIC, true)),
        }

        // Which fabric this node belongs to, if any.
        //
        // Reported rather than assumed. A node that belongs to nothing runs perfectly
        // well and talks to nobody, and an operator who cannot see which of those two
        // states they are in will read an empty peer list as a network problem.
        let enrolment =
            match archie_fabric::Enrolment::load(&identity.public_key_hex(), paths.membership()) {
                Ok(enrolment) => {
                    match enrolment.fabric_fingerprint() {
                        // Which fabric is surfaced through `Fabric::fabric_fingerprint` for
                        // `archie status`; a ready stage carries no detail and widening
                        // StageStatus for one string would ripple into every projection of
                        // the boot report.
                        Some(_) => report.push(BootStage::ready(stages::MEMBERSHIP, true)),
                        None => report.push(BootStage::degraded(
                            stages::MEMBERSHIP,
                            false,
                            "this node belongs to no fabric and will not talk to any peer; \
                         enrol it with `archie enrol`"
                                .to_string(),
                        )),
                    }
                    enrolment
                }
                Err(e) => {
                    // Somebody put a membership file here and meant something by it.
                    report.warn(format!("ignoring unusable membership: {e}"));
                    report.push(BootStage::degraded(
                        stages::MEMBERSHIP,
                        false,
                        format!("membership present but unusable: {e}"),
                    ));
                    archie_fabric::Enrolment::none()
                }
            };

        let fabric = Arc::new(Fabric::new(
            identity.clone(),
            enrolment,
            transport,
            descriptor.clone(),
            fabric_config,
            metrics.clone(),
        ));

        // ---- 6/7. peers -----------------------------------------------------------
        // No peers are known at t=0; discovery is continuous, not a boot-time barrier.
        // Blocking boot on peer discovery would make a single-node install hang.
        report.push(BootStage::skipped(
            stages::PEERS,
            "discovery is continuous; none seen yet",
        ));

        // ---- 8. resource topology -------------------------------------------------
        let mut graph = ResourceGraph::new();
        graph.upsert(NodeRecord::local(descriptor.clone()));
        for peer in state.list_peers().unwrap_or_default() {
            if let Some(peer_descriptor) = peer.descriptor {
                graph.upsert(NodeRecord {
                    descriptor: peer_descriptor,
                    origin: archie_registry::NodeOrigin::Peer,
                    trust: peer.trust,
                    // Known from a previous boot but not yet heard from.
                    reachable: false,
                    last_seen: peer.last_seen,
                });
            }
        }
        report.push(BootStage::ready(stages::TOPOLOGY, true));

        // ---- 9/10/11. modules and cognitive topology ------------------------------
        let modules = match ModuleRegistry::load_from_dir(paths.modules()) {
            Ok(registry) => {
                if registry.is_empty() {
                    report.push(BootStage::skipped(stages::MODULES, "none installed"));
                } else {
                    report.push(BootStage::ready(stages::MODULES, true));
                }
                registry
            }
            Err(e) => {
                report.push(BootStage::failed(stages::MODULES, false, e.to_string()));
                ModuleRegistry::new()
            }
        };

        let mut permissions = PermissionPolicy::default_safe();
        for name in &config.modules.grant_permissions {
            if let Some(permission) = parse_permission(name) {
                permissions = permissions.grant(permission);
            } else {
                report.warn(format!("unknown permission in configuration: {name}"));
            }
        }

        let mut context = TopologyContext::new(
            descriptor.capabilities.clone(),
            descriptor.architecture.clone(),
            config.node.profile.as_str(),
        )
        .with_permissions(permissions);
        for id in config.disabled_modules() {
            context = context.disable(id);
        }

        let topology = modules.compile_topology(&context);
        archie_orb::publish(&archie_orb::signal(archie_orb::SignalKind::Workflow {
            component: archie_orb::WorkflowComponent::Cortex,
            phase: archie_orb::WorkflowPhase::Graph,
            active: true,
            count: 0,
        }));
        match crate::cortex::refresh_module_graph(
            state.as_ref(),
            &modules,
            Timestamp::now().as_millis(),
        ) {
            Ok(snapshot) => {
                archie_orb::publish(&archie_orb::signal(archie_orb::SignalKind::Workflow {
                    component: archie_orb::WorkflowComponent::Cortex,
                    phase: archie_orb::WorkflowPhase::Graph,
                    active: false,
                    count: snapshot.nodes.len().min(4096) as u16,
                }));
                #[cfg(unix)]
                if let Err(error) = crate::cortex::export_module_graph(&paths, &snapshot) {
                    report.warn(format!("Cortex snapshot export unavailable: {error}"));
                }
                #[cfg(not(unix))]
                let _ = snapshot;
            }
            Err(error) => {
                archie_orb::publish(&archie_orb::signal(archie_orb::SignalKind::Workflow {
                    component: archie_orb::WorkflowComponent::Cortex,
                    phase: archie_orb::WorkflowPhase::Error,
                    active: false,
                    count: 0,
                }));
                report.warn(format!("Cortex module projection unavailable: {error}"));
            }
        }
        if !topology.cycles.is_empty() {
            report.push(BootStage::failed(
                stages::COGNITIVE,
                false,
                format!("module dependency cycle: {:?}", topology.cycles),
            ));
        } else if topology.loaded_count() == 0 && modules.is_empty() {
            report.push(BootStage::skipped(stages::COGNITIVE, "no modules to load"));
        } else if topology.loaded_count() == 0 {
            report.push(BootStage::degraded(
                stages::COGNITIVE,
                false,
                format!("{} modules installed, none loadable", modules.len()),
            ));
        } else {
            report.push(BootStage::ready(stages::COGNITIVE, true));
        }

        // ---- data providers ---------------------------------------------------------
        // Declared external data feeds — seismic, aviation, orbital, cyber threat
        // intel — distinct from the models below: these feed the world model, not the
        // router. The image stages `providers/data/*.toml` into `paths.providers()`; a
        // node with none installed is a valid offline node rather than an error.
        let providers = match ProviderRegistry::load_from_dir(paths.providers()) {
            Ok(registry) => {
                if registry.is_empty() {
                    report.push(BootStage::skipped(stages::PROVIDERS, "none installed"));
                } else {
                    report.push(BootStage::ready(stages::PROVIDERS, true));
                }
                registry
            }
            Err(e) => {
                report.push(BootStage::failed(stages::PROVIDERS, false, e.to_string()));
                ProviderRegistry::new()
            }
        };

        // No secret store exists yet anywhere in this codebase — `available_secrets` is
        // honestly empty, not a placeholder for one. Every provider that authenticates
        // therefore and correctly reports `NoCredentials` today; that is the true state
        // of a fresh install, not a bug this boot sequence works around.
        let mut provider_context = ProviderContext::new(descriptor.capabilities.clone());
        for id in config.disabled_providers() {
            provider_context = provider_context.disable(id.clone());
        }
        for family in config.authorized_provider_families() {
            provider_context = provider_context.authorize_family(family.clone());
        }
        let provider_plan = providers.resolve(&provider_context);

        // ---- 12. models -----------------------------------------------------------
        // The manifests staged into `paths.models()`: IRIS, and whatever local or cloud
        // model the operator has added. A node with none cannot think yet and says so;
        // a manifest that does not load is a failure, because a typo that dropped the
        // only local model would later look like private requests being refused for no
        // reason.
        let models = match archie_model_runtime::provider::load_catalog(
            paths.models(),
            paths.inference_providers(),
        ) {
            Ok(registry) => {
                if registry.is_empty() {
                    report.push(BootStage::skipped(stages::MODELS, "no models installed"));
                } else if registry.has_local_model() {
                    report.push(BootStage::ready(stages::MODELS, false));
                } else {
                    report.push(BootStage::degraded(
                        stages::MODELS,
                        false,
                        format!(
                            "{} model(s) installed, none on this machine: restricted data cannot be answered",
                            registry.len()
                        ),
                    ));
                }
                registry
            }
            Err(e) => {
                report.push(BootStage::failed(stages::MODELS, false, e.to_string()));
                ModelRegistry::new()
            }
        };

        // ---- 13. workers ----------------------------------------------------------
        if topology.loaded_count() == 0 {
            report.push(BootStage::skipped(stages::WORKERS, "no modules loaded"));
        } else {
            report.push(BootStage::degraded(
                stages::WORKERS,
                false,
                format!(
                    "{} modules resolved; worker execution lands in v0.2",
                    topology.loaded_count()
                ),
            ));
        }

        let scheduler = Scheduler::new(SchedulerConfig {
            slots_per_node: config.scheduler.slots_per_node,
            reserved_active_slots: config.scheduler.reserved_active_slots,
            pressure: PressureThresholds::default(),
        });
        // A pause the owner recorded survives the restart; only the owner lifts it.
        let emergency_pause = crate::emergency::recorded(&paths);
        if emergency_pause {
            warn!(node_id = %node_id, "booting in emergency pause: AI dispatch and mutating leases are refused");
        }
        let policy = Arc::new(std::sync::RwLock::new(PolicyEngine::new(PolicyConfig {
            safety_boundary_present: config.policy.safety_boundary_present,
            max_lease_lifetime_ms: config.policy.max_lease_lifetime_ms,
            emergency_pause,
        })));

        if report.is_ready() {
            info!(node_id = %node_id, "ARCHIE READY");
        } else {
            warn!(node_id = %node_id, "boot completed with fatal stage failures");
        }

        Ok(Self {
            identity,
            config,
            paths,
            state,
            fabric,
            metrics,
            graph: RwLock::new(graph),
            modules,
            topology,
            providers,
            provider_plan,
            models: RwLock::new(models),
            scheduler: RwLock::new(scheduler),
            router: Router::default(),
            policy,
            descriptor: RwLock::new(descriptor),
            probe_warnings,
            boot_report: report,
            started_at,
            capability_changes,
        })
    }

    pub fn node_id(&self) -> NodeId {
        self.identity.node_id()
    }

    pub fn boot_report(&self) -> &BootReport {
        &self.boot_report
    }

    pub fn is_ready(&self) -> bool {
        self.boot_report.is_ready()
    }

    pub fn topology(&self) -> &Topology {
        &self.topology
    }

    pub fn router(&self) -> &Router {
        &self.router
    }

    /// The engine as it stands now, for one decision. A later pause does not reach a
    /// copy taken earlier; whatever must stop mid-flight holds [`Self::live_policy`].
    pub fn policy(&self) -> PolicyEngine {
        match self.policy.read() {
            Ok(engine) => engine.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The live engine itself, shared with every AI dispatch.
    pub fn live_policy(&self) -> Arc<std::sync::RwLock<PolicyEngine>> {
        self.policy.clone()
    }

    /// Persist an automatically eligible Archie-agent enrollment.
    ///
    /// The network/fabric handler is responsible for admitting only a device that actually
    /// authenticated this intent's key. This method then pins that exact key and advances the
    /// normal local inventory transition in one short transaction sequence. It deliberately
    /// refuses Matter, Roku, Home Assistant and vendor paths until their device-side proof is
    /// complete; a user saying "connect" chooses the flow but does not forge firmware proof.
    pub fn enroll_authenticated_device(
        &self,
        intent: &DeviceConnectionIntent,
    ) -> Result<DeviceRecord, DeviceEnrollmentError> {
        match intent.plan()? {
            DevicePairingPlan::AutoEnroll {
                device_reference,
                class,
                public_key,
            } => {
                self.state
                    .propose_device_pairing(&device_reference, &public_key, class)
                    .map_err(|error| DeviceEnrollmentError::State(error.to_string()))?;
                self.state
                    .confirm_device_pairing(&device_reference)
                    .map_err(|error| DeviceEnrollmentError::State(error.to_string()))?;
                let record = self
                    .state
                    .get_device(&device_reference)
                    .map_err(|error| DeviceEnrollmentError::State(error.to_string()))?
                    .ok_or(DeviceEnrollmentError::MissingAfterPairing)?;
                self.metrics.increment("device.pairing.auto_enrolled", 1);
                Ok(record)
            }
            DevicePairingPlan::AwaitDeviceProof {
                transport,
                explanation,
                ..
            } => Err(DeviceEnrollmentError::DeviceProofRequired {
                transport,
                explanation,
            }),
        }
    }

    pub fn probe_warnings(&self) -> &[String] {
        &self.probe_warnings
    }

    /// Apply a fabric event to the runtime's view of the world.
    ///
    /// Discovery updates two places: the resource graph (so the peer becomes
    /// schedulable) and the durable peer table (so its key stays pinned across
    /// reboots). Trust comes from the store, never from the peer's own claim.
    pub async fn apply_fabric_event(&self, event: &FabricEvent) {
        match event {
            FabricEvent::PeerDiscovered(entry) | FabricEvent::PeerUpdated(entry) => {
                let trust = match self.state.observe_peer(
                    &entry.node_id,
                    &entry.public_key,
                    Some(&entry.descriptor),
                ) {
                    Ok(record) => record.trust,
                    Err(e) => {
                        // A key change on a known id is exactly the case where we must
                        // not quietly proceed.
                        warn!(peer = %entry.node_id, error = %e, "refusing peer");
                        self.metrics.count("fabric.peer.pin_conflict");
                        return;
                    }
                };

                let mut graph = self.graph.write().await;
                graph.upsert(NodeRecord {
                    descriptor: entry.descriptor.clone(),
                    origin: archie_registry::NodeOrigin::Peer,
                    trust,
                    reachable: true,
                    last_seen: entry.last_seen,
                });
                self.metrics.set_gauge("topology.nodes", graph.len() as i64);
            }
            FabricEvent::PeerDeparted { node_id, .. } => {
                let mut graph = self.graph.write().await;
                if let Some(record) = graph.get(node_id).cloned() {
                    // Keep the node in the graph but mark it unreachable: a rebooting
                    // peer should not have to be rediscovered from nothing.
                    graph.upsert(NodeRecord {
                        reachable: false,
                        ..record
                    });
                }
            }
            _ => {}
        }
    }

    /// Refresh the local node's hardware view and re-announce.
    pub async fn refresh_local_descriptor(&self) {
        let probe = HardwareProbe::run();
        let descriptor = probe.to_descriptor(
            self.node_id(),
            self.identity.public_key_hex(),
            self.config.node.profile,
            self.config.extra_capabilities(),
        );
        self.graph
            .write()
            .await
            .upsert(NodeRecord::local(descriptor.clone()));
        self.fabric.update_descriptor(descriptor.clone()).await;
        *self.descriptor.write().await = descriptor;
    }

    /// Re-probe after a hardware event, letting it change only the class it was about.
    ///
    /// This is the hotplug path, and it is deliberately narrower than
    /// [`Self::refresh_local_descriptor`]. A DRM `bind` says a GPU now has a driver. It
    /// says nothing about the network — but a wholesale replace would let any momentary
    /// weakness in an unrelated probe rewrite the announcement, so an interface list that
    /// came back empty during a NIC reset would withdraw `network.lan` *because a GPU was
    /// plugged in*. Refreshes used to be rare enough for that to be a theoretical
    /// problem; on a hotplug path it would be a routine one.
    ///
    /// Returns what actually changed. When nothing did — which is the common case, since
    /// most device events do not alter what a node can do — **nothing is announced**. The
    /// fabric does not need a broadcast per uevent.
    pub async fn refresh_capability_class(&self, class: CapabilityClass) -> CapabilityDelta {
        let probe = HardwareProbe::run();
        let fresh = probe.to_descriptor(
            self.node_id(),
            self.identity.public_key_hex(),
            self.config.node.profile,
            self.config.extra_capabilities(),
        );

        let previous = self.descriptor.read().await.clone();
        let delta = scoped_delta(&previous.capabilities, &fresh.capabilities, class);
        if delta.is_empty() {
            return delta;
        }

        let mut updated = previous.clone();
        updated.capabilities = apply_scoped(&previous.capabilities, &fresh.capabilities, class);
        // Carry across the hardware detail for this class too, so the manifest does not
        // claim `compute.gpu` while listing no accelerator. Same rule: only this class.
        match class {
            CapabilityClass::Compute => updated.accelerators = fresh.accelerators,
            CapabilityClass::Network => updated.networks = fresh.networks,
            CapabilityClass::Storage => updated.storage = fresh.storage,
            // Sensors, buses and radios have no dedicated field on the descriptor yet;
            // their capabilities are the whole of what is advertised.
            CapabilityClass::Sensor | CapabilityClass::Bus | CapabilityClass::Radio => {}
        }

        if let Some(summary) = delta.summary() {
            info!(class = ?class, change = %summary, "capability set changed after hardware event");
        }

        self.graph
            .write()
            .await
            .upsert(NodeRecord::local(updated.clone()));
        self.fabric.update_descriptor(updated.clone()).await;
        *self.descriptor.write().await = updated;
        self.metrics.increment("hardware.capability_change", 1);

        delta
    }

    /// Answer a control request.
    pub async fn handle_control(&self, request: ControlRequest) -> ControlResponse {
        self.metrics
            .increment(&format!("control.request.{}", request.name()), 1);

        match request {
            ControlRequest::WorldNavigate { .. }
            | ControlRequest::WorldNavigationStatus { .. }
            | ControlRequest::WorldNavigationStop { .. }
            | ControlRequest::WorldPlan { .. }
            | ControlRequest::WorldJob { .. }
            | ControlRequest::WorldExplore { .. }
            | ControlRequest::WorldResolve { .. }
            | ControlRequest::WorldStatus { .. } => {
                ControlResponse::error("authenticated local operator session required")
            }
            ControlRequest::AiInfer { .. }
            | ControlRequest::AiRecall { .. }
            | ControlRequest::AiHarness { .. }
            | ControlRequest::AiPause
            | ControlRequest::AiResume
            | ControlRequest::AiStatus => {
                ControlResponse::error("authenticated local operator session required")
            }
            ControlRequest::Ping => ControlResponse::Pong {
                node_id: self.node_id().to_string(),
                version: ARCHIEOS_VERSION.to_string(),
            },
            ControlRequest::SentryPrepare { .. }
            | ControlRequest::SentryApprove { .. }
            | ControlRequest::SentryCancel
            | ControlRequest::SentryReceipt { .. } => {
                ControlResponse::error("authenticated local operator session required")
            }
            ControlRequest::Status => ControlResponse::Status(Box::new(self.status_view().await)),
            ControlRequest::Boot => ControlResponse::Boot(Box::new(self.boot_report.clone())),
            ControlRequest::Metrics => ControlResponse::Metrics(Box::new(self.metrics.snapshot())),
            ControlRequest::Topology => {
                let graph = self.graph.read().await;
                let capabilities = graph
                    .capability_index()
                    .into_iter()
                    .map(|(capability, nodes)| {
                        (
                            capability.to_string(),
                            nodes.into_iter().map(|n| n.to_string()).collect(),
                        )
                    })
                    .collect();
                let totals = graph.totals();
                ControlResponse::Topology {
                    capabilities,
                    nodes: graph.nodes().map(node_view).collect(),
                    totals: TotalsView {
                        nodes: totals.nodes,
                        schedulable_nodes: totals.schedulable_nodes,
                        logical_cores: totals.logical_cores,
                        memory_bytes: totals.memory_bytes,
                        accelerator_bytes: totals.accelerator_bytes,
                        accelerators: totals.accelerators,
                    },
                }
            }
            ControlRequest::Peers => {
                let graph = self.graph.read().await;
                ControlResponse::Peers {
                    peers: graph
                        .nodes()
                        .filter(|n| !n.is_local())
                        .map(node_view)
                        .collect(),
                }
            }
            ControlRequest::Devices => match self.state.list_devices() {
                Ok(devices) => ControlResponse::Devices {
                    devices: devices.iter().map(device_view).collect(),
                },
                Err(e) => ControlResponse::error(e.to_string()),
            },
            ControlRequest::SentryPrinter { resource_id } => {
                self.sentry_printer(&resource_id).await
            }
            ControlRequest::SentryHomeStatus { resource_id } => {
                self.sentry_home_status(&resource_id).await
            }
            ControlRequest::SentryPrinterDoctor { resource_id } => {
                self.sentry_printer_doctor(&resource_id).await
            }
            ControlRequest::DeviceOperations { limit } => {
                match self.state.recent_device_operations(limit.min(1000)) {
                    Ok(operations) => ControlResponse::DeviceOperations {
                        operations: operations.iter().map(device_operation_view).collect(),
                    },
                    Err(e) => ControlResponse::error(e.to_string()),
                }
            }
            ControlRequest::CortexTrajectories { limit } => {
                match self
                    .state
                    .recent_cortex_correction_trajectories(limit.min(1000))
                {
                    Ok(trajectories) => ControlResponse::CortexTrajectories {
                        trajectories: trajectories.iter().map(cortex_trajectory_view).collect(),
                    },
                    Err(e) => ControlResponse::error(e.to_string()),
                }
            }
            ControlRequest::Modules => {
                let modules = self
                    .topology
                    .states
                    .iter()
                    .map(|(id, state)| {
                        let manifest = self.modules.get(id);
                        ModuleView {
                            id: id.to_string(),
                            version: manifest
                                .map(|m| m.version.clone())
                                .unwrap_or_else(|| "?".into()),
                            state: state.label().to_string(),
                            explanation: state.explain(),
                            dependencies: manifest
                                .map(|m| m.dependencies.iter().map(|d| d.to_string()).collect())
                                .unwrap_or_default(),
                        }
                    })
                    .collect();
                ControlResponse::Modules {
                    modules,
                    load_order: self
                        .topology
                        .load_order
                        .iter()
                        .map(|m| m.to_string())
                        .collect(),
                }
            }
            ControlRequest::Providers => {
                let providers = self
                    .providers
                    .manifests()
                    .map(|manifest| {
                        let status = self
                            .provider_plan
                            .statuses
                            .get(&manifest.id)
                            .expect("resolve() covers every registered manifest");
                        ProviderView {
                            id: manifest.id.clone(),
                            title: manifest.title.clone(),
                            capability: manifest.capability.to_string(),
                            status: status.label().to_string(),
                            explanation: status.explain(),
                            // Via the wire form rather than `{:?}`, so this says
                            // "observation" the same way the manifest's own TOML and
                            // JSON do, not "Observation" from a Debug derive nobody
                            // chose as a display format.
                            records: manifest
                                .records
                                .iter()
                                .map(|r| {
                                    serde_json::to_value(r)
                                        .ok()
                                        .and_then(|v| v.as_str().map(str::to_string))
                                        .unwrap_or_else(|| "unknown".into())
                                })
                                .collect(),
                            known_gaps: manifest.coverage.known_gaps.clone(),
                        }
                    })
                    .collect();
                ControlResponse::Providers {
                    providers,
                    active: self.provider_plan.active.clone(),
                }
            }
            ControlRequest::Models => {
                let models = self.models.read().await;
                ControlResponse::Models {
                    models: models
                        .iter()
                        .map(|m| ModelView {
                            id: m.id.to_string(),
                            provider: m.provider.to_string(),
                            locality: format!("{:?}", m.locality).to_lowercase(),
                            availability: format!("{:?}", m.availability).to_lowercase(),
                            max_privacy_grade: m.max_privacy_grade.to_string(),
                            context_window: m.context_window,
                        })
                        .collect(),
                }
            }
            ControlRequest::Audit { limit } => match self.state.recent_audit(limit.min(1000)) {
                Ok(events) => ControlResponse::Audit {
                    events: events
                        .into_iter()
                        .filter_map(|e| serde_json::to_value(e).ok())
                        .collect(),
                },
                Err(e) => ControlResponse::error(e.to_string()),
            },
        }
    }

    async fn status_view(&self) -> StatusView {
        let descriptor = self.descriptor.read().await.clone();
        let graph = self.graph.read().await;
        let models = self.models.read().await;

        let peers: Vec<_> = graph.nodes().filter(|n| !n.is_local()).collect();
        let reachable = peers.iter().filter(|n| n.schedulable()).count();

        StatusView {
            // Straight from configuration, unaltered. Nothing here derives or
            // guesses a place.
            location: self.config.node.location,
            node_id: descriptor.node_id.to_string(),
            hostname: descriptor.hostname.clone(),
            profile: descriptor.profile.to_string(),
            archieos_version: descriptor.archieos_version.clone(),
            os: descriptor.os.clone(),
            kernel: descriptor.kernel.clone(),
            architecture: descriptor.architecture.clone(),
            uptime_ms: self.started_at.elapsed_to(Timestamp::now()),

            cpu_model: descriptor.cpu.model.clone(),
            logical_cores: descriptor.cpu.logical_cores,
            physical_cores: descriptor.cpu.physical_cores,
            memory_total_bytes: descriptor.memory.total_bytes,
            memory_available_bytes: descriptor.memory.available_bytes,
            accelerators: descriptor
                .accelerators
                .iter()
                .map(|a| {
                    let memory = a
                        .memory_bytes
                        .map(|b| format!("{:.1} GiB", b as f64 / (1 << 30) as f64))
                        .unwrap_or_else(|| "memory unknown".into());
                    format!("{} {} ({memory})", a.vendor, a.model)
                })
                .collect(),
            storage: descriptor
                .storage
                .iter()
                .map(|v| {
                    format!(
                        "{} {} {:.0}/{:.0} GiB free",
                        v.mount_point,
                        v.filesystem,
                        v.available_bytes as f64 / (1u64 << 30) as f64,
                        v.total_bytes as f64 / (1u64 << 30) as f64
                    )
                })
                .collect(),
            networks: descriptor
                .networks
                .iter()
                .map(|n| format!("{} {:?} {}", n.name, n.link_class, n.addresses.join(",")))
                .collect(),

            capabilities: descriptor
                .capabilities
                .iter()
                .map(|c| c.to_string())
                .collect(),
            peers: peers.len(),
            reachable_peers: reachable,
            modules_loaded: self.topology.loaded_count(),
            modules_installed: self.modules.len(),
            models_registered: models.len(),
            active_tasks: self.state.task_count().unwrap_or(0),
            health: if self.is_ready() {
                NodeHealth::Ok.to_string()
            } else {
                NodeHealth::Degraded.to_string()
            },
            fabric_transport: self.fabric.transport_name().to_string(),
            capability_changes: self.capability_changes.clone(),
        }
    }

    /// A snapshot of the resource graph, for callers that need to inspect it.
    pub async fn resource_graph(&self) -> ResourceGraph {
        self.graph.read().await.clone()
    }

    pub async fn scheduler_running_count(&self) -> usize {
        self.scheduler.read().await.running_count()
    }

    /// Register a model provider at runtime.
    pub async fn register_model(&self, model: archie_types::ModelDescriptor) -> Result<(), String> {
        self.models
            .write()
            .await
            .upsert(model)
            .map_err(|e| e.to_string())
    }
}

fn node_view(record: &NodeRecord) -> PeerView {
    PeerView {
        node_id: record.node_id().to_string(),
        hostname: record.descriptor.hostname.clone(),
        profile: record.descriptor.profile.to_string(),
        trust: record.trust.as_str().to_string(),
        health: record.descriptor.health.to_string(),
        reachable: record.reachable,
        capabilities: record
            .descriptor
            .capabilities
            .iter()
            .map(|c| c.to_string())
            .collect(),
        last_seen: record.last_seen,
    }
}

/// Convert the durable inventory record into the deliberately smaller local control
/// projection. The full key stays in the state database; native observatory/CLI views
/// get just enough fingerprint to distinguish a key rotation from the record they saw.
fn device_view(record: &DeviceRecord) -> DeviceView {
    DeviceView {
        device_reference: record.device_reference.clone(),
        class: record.class.as_str().to_string(),
        pairing: record.pairing.as_str().to_string(),
        public_key_fingerprint: record.public_key.chars().take(16).collect(),
        first_seen: record.first_seen,
        last_seen: record.last_seen,
    }
}

/// Project a durable control record without exposing its reviewed payload or turning it
/// into a command. The complete digest stays inside the state store and must be checked
/// again alongside a fresh one-operation lease at any future dispatch boundary.
fn device_operation_view(record: &DeviceOperationRecord) -> DeviceOperationView {
    DeviceOperationView {
        operation_id: record.operation_id,
        device_reference: record.device_reference.clone(),
        state: record.state.as_str().to_string(),
        trace_fingerprint: record.trace_id.to_string().chars().take(12).collect(),
        operation_sha256_fingerprint: record.operation_sha256.chars().take(16).collect(),
        prepared_at: record.prepared_at,
    }
}

fn cortex_trajectory_view(record: &CortexCorrectionTrajectoryRecord) -> CortexTrajectoryView {
    let trajectory = &record.trajectory;
    CortexTrajectoryView {
        trajectory_id: trajectory.trajectory_id.clone(),
        correction_kind: trajectory.correction.kind.as_str().to_string(),
        source_graph_snapshot_id: trajectory.graph_update.source_graph_snapshot_id.clone(),
        resulting_graph_snapshot_id: trajectory.graph_update.resulting_graph_snapshot_id.clone(),
        selected_context_count: trajectory.selected_context_ids.len() as u32,
        omitted_candidate_count: trajectory.omitted_candidate_ids.len() as u32,
        model_id: trajectory.model_id.clone(),
        payload_sha256_fingerprint: record.payload_sha256.chars().take(16).collect(),
        captured_at: Timestamp::from_millis(trajectory.captured_at_ms),
    }
}

fn parse_permission(name: &str) -> Option<ModulePermission> {
    Some(match name {
        "read-knowledge" => ModulePermission::ReadKnowledge,
        "write-knowledge" => ModulePermission::WriteKnowledge,
        "network-internet" => ModulePermission::NetworkInternet,
        "network-lan" => ModulePermission::NetworkLan,
        "spawn-workers" => ModulePermission::SpawnWorkers,
        "read-telemetry" => ModulePermission::ReadTelemetry,
        "device-communicate" => ModulePermission::DeviceCommunicate,
        "device-actuate" => ModulePermission::DeviceActuate,
        "manage-models" => ModulePermission::ManageModels,
        "read-secrets" => ModulePermission::ReadSecrets,
        _ => return None,
    })
}

/// A transport plus an optional note explaining any degradation.
pub struct TransportOutcome {
    pub transport: Arc<dyn Transport>,
    /// `None` means fully operational.
    pub note: Option<String>,
}

impl TransportOutcome {
    pub fn ready(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            note: None,
        }
    }

    pub fn degraded(transport: Arc<dyn Transport>, note: impl Into<String>) -> Self {
        Self {
            transport,
            note: Some(note.into()),
        }
    }
}

/// Build the transport the configuration asks for.
///
/// Falls back to an isolated loopback endpoint rather than failing the boot: a node
/// that cannot reach the network is isolated, not broken, and an operator is better
/// served by a running daemon reporting the problem than by no daemon at all.
pub async fn build_transport(config: &ArchieConfig) -> TransportOutcome {
    match config.fabric.transport {
        TransportKind::Loopback => TransportOutcome::degraded(
            Arc::new(LoopbackBus::new().endpoint()),
            "loopback transport: this node discovers no peers",
        ),
        TransportKind::Udp => {
            let Ok(bind) = config.fabric.bind.parse::<std::net::SocketAddr>() else {
                return TransportOutcome::degraded(
                    Arc::new(LoopbackBus::new().endpoint()),
                    format!(
                        "invalid fabric.bind {:?}; fabric isolated",
                        config.fabric.bind
                    ),
                );
            };

            let mut targets = config.seed_addresses();
            if let Some(group) = &config.fabric.multicast_group {
                if let Ok(addr) = group.parse::<std::net::Ipv4Addr>() {
                    let port = bind.port();
                    targets.push(std::net::SocketAddr::from((addr, port)));
                }
            }

            match UdpTransport::bind(bind, targets).await {
                Ok(transport) => {
                    let mut note = None;
                    if let Some(group) = &config.fabric.multicast_group {
                        if let Ok(addr) = group.parse::<std::net::Ipv4Addr>() {
                            if let Err(e) =
                                transport.join_multicast(addr, std::net::Ipv4Addr::UNSPECIFIED)
                            {
                                // Multicast is filtered on many networks. Seed-based
                                // discovery still works, so this is a note, not a failure.
                                note = Some(format!(
                                    "multicast unavailable ({e}); using {} seed peers",
                                    config.fabric.seeds.len()
                                ));
                            }
                        }
                    }
                    if transport.targets().is_empty() {
                        note = Some(
                            "no seeds and no multicast group: this node announces to nobody".into(),
                        );
                    }
                    match note {
                        Some(note) => TransportOutcome::degraded(Arc::new(transport), note),
                        None => TransportOutcome::ready(Arc::new(transport)),
                    }
                }
                Err(e) => TransportOutcome::degraded(
                    Arc::new(LoopbackBus::new().endpoint()),
                    format!("could not bind {bind}: {e}; fabric isolated"),
                ),
            }
        }
    }
}

/// Trust a peer, as an operator action.
pub fn set_peer_trust(
    state: &StateStore,
    node_id: &NodeId,
    trust: TrustState,
) -> Result<(), String> {
    state
        .set_peer_trust(node_id, trust)
        .map_err(|e| e.to_string())
}

/// Capability index rendered for display.
pub fn render_capability_index(index: &BTreeMap<String, Vec<String>>) -> String {
    let mut out = String::new();
    for (capability, nodes) in index {
        out.push_str(&format!("{capability}\n"));
        for node in nodes {
            out.push_str(&format!("    {}\n", &node[..node.len().min(12)]));
        }
    }
    out
}
