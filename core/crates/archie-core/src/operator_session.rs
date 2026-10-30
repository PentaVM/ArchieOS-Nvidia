//! One authenticated connection owns one short-lived confirmation.
use crate::{
    control::{ControlRequest, ControlResponse},
    operator::KernelOperator,
    Runtime,
};
use archie_policy::{Action, CapabilityRequest, Constraints, Principal, Resource, Scope};
use archie_sentry::{
    adapters::{ipp::IppAdapter, process::ProcessRunner, roku::Roku, router::RouterAdapter},
    contracts::{Command, CommandKind, Intent},
    engine::{CoreAuthority, EdgeAdapter, EngineError, ExecutionGuard},
    journal::Journal,
    registry::{Registry, ResourceDescriptor},
};
use archie_types::{Timestamp, TraceId};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

struct SessionGuard {
    peer: KernelOperator,
    connected: Arc<AtomicBool>,
}
impl ExecutionGuard for SessionGuard {
    fn validate(&self, _: &Intent, _: &CoreAuthority) -> Result<(), EngineError> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(EngineError::Authority);
        }
        self.peer.check_present()
    }
}
struct OperatorSession {
    core: Arc<Runtime>,
    peer: Option<KernelOperator>,
    pending: Option<Intent>,
    review_started: Option<std::time::Instant>,
    connected: Arc<AtomicBool>,
}
impl Drop for OperatorSession {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::SeqCst);
        if let Some(intent) = self.pending.take() {
            let _ = self.core.state.cancel_device_operation(intent.operation_id);
        }
    }
}
impl OperatorSession {
    fn peer(&self) -> Result<&KernelOperator, EngineError> {
        let peer = self.peer.as_ref().ok_or(EngineError::Authority)?;
        peer.check_present()?;
        Ok(peer)
    }
    /// A configured resource is usable only while the local inventory holds a
    /// paired record of the expected device class for it. MQTT actuators reuse
    /// the HomeAssistant class: they are home-automation devices and the fixed
    /// inventory vocabulary has no more specific honest class for them.
    fn paired_device(
        &self,
        resource_id: &str,
        class: archie_state::DeviceClass,
    ) -> Result<bool, EngineError> {
        let device = self
            .core
            .state
            .get_device(resource_id)
            .map_err(|_| EngineError::Authority)?;
        Ok(device.is_some_and(|d| {
            d.pairing == archie_state::DevicePairingState::Paired && d.class == class
        }))
    }
    /// Registers every configured actuable resource with its owning adapter
    /// route. Telemetry resources (HA telemetry, ONVIF cameras)
    /// never appear here, so no intent can name them.
    fn service(&self) -> Result<crate::sentry::SentryService, EngineError> {
        let sentry = &self.core.config.sentry;
        let mut registry = Registry::default();
        if let Some(binding) = &sentry.home_assistant {
            for resource in &binding.resources {
                registry.register(ResourceDescriptor {
                    resource_id: resource.resource_id.clone(),
                    edge_id: "local-core".into(),
                    adapter_id: "home-assistant".into(),
                    paired: self.paired_device(
                        &resource.resource_id,
                        archie_state::DeviceClass::HomeAssistant,
                    )?,
                    capabilities: resource
                        .capabilities()
                        .map_err(|_| EngineError::Authority)?,
                })?;
            }
        }
        for roku in &sentry.roku {
            registry.register(ResourceDescriptor {
                resource_id: roku.resource_id.clone(),
                edge_id: "local-core".into(),
                adapter_id: "roku".into(),
                paired: self
                    .paired_device(&roku.resource_id, archie_state::DeviceClass::Television)?,
                capabilities: crate::sentry_config::RokuBinding::capabilities(),
            })?;
        }
        for cast in &sentry.chromecasts {
            registry.register(ResourceDescriptor {
                resource_id: cast.resource_id.clone(),
                edge_id: "local-core".into(),
                adapter_id: "cast".into(),
                paired: self
                    .paired_device(&cast.resource_id, archie_state::DeviceClass::Television)?,
                capabilities: archie_sentry::adapters::chromecast::Chromecast::CAPABILITIES
                    .to_vec(),
            })?;
        }
        for printer in &sentry.printers {
            if printer.spool_dir.is_none() {
                continue; // query-only printer; never actuable
            }
            registry.register(ResourceDescriptor {
                resource_id: printer.resource_id.clone(),
                edge_id: "local-core".into(),
                adapter_id: "ipp".into(),
                paired: self
                    .paired_device(&printer.resource_id, archie_state::DeviceClass::Printer)?,
                capabilities: vec![CommandKind::PrintSubmit],
            })?;
        }
        if let Some(mqtt) = &sentry.mqtt {
            for (resource_id, binding) in &mqtt.resources {
                registry.register(ResourceDescriptor {
                    resource_id: resource_id.clone(),
                    edge_id: "local-core".into(),
                    adapter_id: "mqtt".into(),
                    paired: self
                        .paired_device(resource_id, archie_state::DeviceClass::HomeAssistant)?,
                    capabilities: crate::sentry_config::mqtt_capabilities(binding),
                })?;
            }
        }
        if let Some(wol) = &sentry.wol {
            for node in &wol.nodes {
                registry.register(ResourceDescriptor {
                    resource_id: node.resource_id.clone(),
                    edge_id: "local-core".into(),
                    adapter_id: "wol".into(),
                    paired: self
                        .paired_device(&node.resource_id, archie_state::DeviceClass::Host)?,
                    capabilities: vec![CommandKind::WakeHost],
                })?;
            }
        }
        if let Some(android) = &sentry.android {
            for node in &android.nodes {
                registry.register(ResourceDescriptor {
                    resource_id: node.resource_id.clone(),
                    edge_id: "local-core".into(),
                    adapter_id: "adb".into(),
                    paired: self
                        .paired_device(&node.resource_id, archie_state::DeviceClass::Mobile)?,
                    capabilities: vec![
                        CommandKind::AppLaunch,
                        CommandKind::SessionLock,
                        CommandKind::VolumeSet,
                    ],
                })?;
            }
        }
        if let Some(router) = &sentry.router {
            if let Some((_, capabilities)) = sentry.actuable_route(&router.resource_id) {
                registry.register(ResourceDescriptor {
                    resource_id: router.resource_id.clone(),
                    edge_id: "local-core".into(),
                    adapter_id: "router".into(),
                    paired: self
                        .paired_device(&router.resource_id, archie_state::DeviceClass::Router)?,
                    capabilities,
                })?;
            }
        }
        Ok(self.core.sentry_service(Arc::new(Mutex::new(registry))))
    }
    /// Builds the owning adapter for one configured actuable resource, opening
    /// referenced secrets for this one operation only.
    async fn adapter_for(&self, resource_id: &str) -> Result<Box<dyn EdgeAdapter>, EngineError> {
        let sentry = &self.core.config.sentry;
        let (route, _) = sentry
            .actuable_route(resource_id)
            .ok_or(EngineError::Authority)?;
        let adapter: Box<dyn EdgeAdapter> = match route {
            "home-assistant" => Box::new(
                sentry
                    .home_assistant
                    .as_ref()
                    .ok_or(EngineError::Authority)?
                    .adapter()
                    .map_err(|_| EngineError::Authority)?,
            ),
            "roku" => Box::new(
                Roku::new(
                    "local-core",
                    "roku",
                    sentry
                        .roku
                        .iter()
                        .map(|r| (r.resource_id.clone(), r.endpoint.clone()))
                        .collect(),
                )
                .map_err(|_| EngineError::Authority)?,
            ),
            "cast" => Box::new(sentry.cast_adapter().map_err(|_| EngineError::Authority)?),
            "ipp" => {
                let printer = sentry
                    .printers
                    .iter()
                    .find(|p| p.resource_id == resource_id)
                    .ok_or(EngineError::Authority)?;
                let spool = printer.spool_dir.as_ref().ok_or(EngineError::Authority)?;
                Box::new(
                    IppAdapter::new(
                        "local-core",
                        "ipp",
                        &printer.endpoint,
                        &printer.resource_id,
                        PathBuf::from(spool),
                    )
                    .map_err(|_| EngineError::Authority)?,
                )
            }
            "mqtt" => Box::new(
                sentry
                    .mqtt
                    .as_ref()
                    .ok_or(EngineError::Authority)?
                    .adapter()
                    .map_err(|_| EngineError::Authority)?,
            ),
            "wol" => Box::new(
                sentry
                    .wol
                    .as_ref()
                    .ok_or(EngineError::Authority)?
                    .adapter()
                    .map_err(|_| EngineError::Authority)?,
            ),
            "adb" => Box::new(
                sentry
                    .android
                    .as_ref()
                    .ok_or(EngineError::Authority)?
                    .adapter()
                    .map_err(|_| EngineError::Authority)?,
            ),
            "router" => {
                let binding = sentry.router.as_ref().ok_or(EngineError::Authority)?;
                let runner: Arc<dyn archie_sentry::adapters::process::CommandRunner> =
                    Arc::new(ProcessRunner::default());
                let (config, capabilities) = binding
                    .probe_router_config(Arc::clone(&runner))
                    .await
                    .map_err(|_| EngineError::Authority)?;
                Box::new(
                    RouterAdapter::new(
                        "local-core",
                        "router",
                        &binding.resource_id,
                        config,
                        capabilities,
                        runner,
                    )
                    .map_err(|_| EngineError::Authority)?,
                )
            }
            _ => return Err(EngineError::Authority),
        };
        Ok(adapter)
    }
    fn prepare(
        &mut self,
        resource_id: String,
        command: Command,
    ) -> Result<ControlResponse, EngineError> {
        let uid = self.peer()?.uid();
        if self.pending.is_some() {
            return Err(EngineError::Authority);
        }
        let (_, capabilities) = self
            .core
            .config
            .sentry
            .actuable_route(&resource_id)
            .ok_or(EngineError::Authority)?;
        if !capabilities.contains(&command.capability()) {
            return Err(EngineError::Authority);
        }
        let now = Timestamp::now();
        let intent = self.service()?.prepare(Intent {
            schema_version: 1,
            intent_id: TraceId::new().to_string(),
            operation_id: 1,
            trace_id: TraceId::new(),
            resource_id,
            command,
            created_at: now,
            deadline_at: now.plus_millis(30000),
            max_observation_age_ms: 3000,
            verify: true,
        })?;
        let digest = intent.digest()?;
        self.pending = Some(intent.clone());
        self.review_started = Some(std::time::Instant::now());
        self.core
            .state
            .append_audit(
                &archie_observability::AuditEvent::new(
                    self.core.node_id(),
                    "device.sentry.prepare",
                    format!("human:unix-uid-{uid}"),
                    format!("device:{}", intent.resource_id),
                    archie_observability::AuditOutcome::PendingApproval,
                    "typed intention prepared; no dispatch authority granted",
                )
                .with_trace(intent.trace_id)
                .with_attribute("intent_digest", &digest),
            )
            .map_err(|_| EngineError::Authority)?;
        Ok(ControlResponse::SentryPrepared {
            intent: Box::new(intent),
            digest,
        })
    }
    async fn approve(
        &mut self,
        intent_id: String,
        digest: String,
    ) -> Result<ControlResponse, EngineError> {
        let intent = self.pending.take().ok_or(EngineError::Authority)?;
        let review_started = self.review_started.take().ok_or(EngineError::Authority)?;
        if intent.intent_id != intent_id
            || intent.digest()? != digest
            || Timestamp::now() > intent.deadline_at
            || review_started.elapsed() > std::time::Duration::from_secs(30)
        {
            let _ = self.core.state.cancel_device_operation(intent.operation_id);
            return Err(EngineError::Authority);
        }
        let peer = self.peer()?.clone();
        let principal = Principal::Human {
            id: format!("unix-uid-{}", peer.uid()),
            authenticated: true,
        };
        let request = CapabilityRequest {
            trace_id: intent.trace_id,
            task_id: None,
            principal: principal.clone(),
            resource: Resource::Device {
                device: intent.resource_id.clone(),
            },
            scope: Scope::OneOperation {
                operation: intent.operation_id,
            },
            constraints: Constraints {
                actions: [Action::Read, Action::Actuate].into_iter().collect(),
                max_uses: Some(1),
                expires_at: Some(intent.deadline_at),
                requires_user_presence: true,
            },
            justification:
                "exact typed intent confirmed on authenticated interactive local session".into(),
        };
        let decision = self.core.policy().decide(&request);
        self.core
            .state
            .append_audit(
                &decision
                    .to_audit(self.core.node_id(), &request)
                    .with_attribute("intent_digest", &digest)
                    .with_attribute("operation_id", intent.operation_id.to_string()),
            )
            .map_err(|_| EngineError::Authority)?;
        let lease = decision.lease.ok_or(EngineError::Authority)?;
        let authority = CoreAuthority::from_reviewed_intent(
            self.core.policy(),
            lease,
            principal,
            true,
            digest,
        )?;
        let mut adapter = self.adapter_for(&intent.resource_id).await?;
        let guard = Arc::new(SessionGuard {
            peer,
            connected: self.connected.clone(),
        });
        let result = self
            .service()?
            .execute_with_guard(&intent, &authority, adapter.as_mut(), guard)
            .await?;
        Ok(ControlResponse::SentryResult(Box::new(result)))
    }
    async fn handle(&mut self, request: ControlRequest) -> ControlResponse {
        let outcome = match request {
            ControlRequest::SentryPrepare {
                resource_id,
                command,
            } => self.prepare(resource_id, command),
            ControlRequest::SentryApprove { intent_id, digest } => {
                self.approve(intent_id, digest).await
            }
            ControlRequest::SentryCancel => {
                if self.peer().is_err() {
                    return ControlResponse::error("authenticated local operator session required");
                }
                if let Some(intent) = self.pending.take() {
                    if self
                        .core
                        .state
                        .cancel_device_operation(intent.operation_id)
                        .is_err()
                    {
                        return ControlResponse::error(
                            "Sentry cancellation could not be persisted",
                        );
                    }
                }
                self.review_started = None;
                Ok(ControlResponse::SentryCancelled)
            }
            ControlRequest::SentryReceipt { intent_id } => self.receipt(&intent_id),
            other => return self.core.handle_control(other).await,
        };
        outcome.unwrap_or_else(|_| {
            ControlResponse::error("Sentry operation refused or persistence unavailable")
        })
    }
    fn receipt(&self, intent_id: &str) -> Result<ControlResponse, EngineError> {
        self.peer()?;
        archie_sentry::contracts::validate_id(intent_id)?;
        let record = self
            .core
            .state
            .sentry_journal()?
            .load(intent_id)?
            .ok_or(EngineError::Authority)?;
        Ok(ControlResponse::SentryReceipt(Box::new(record)))
    }
}

/// Context is captured before splitting. EOF/pipelining during a mutation drops
/// execution; a reservation already written remains a durable dispatch fence.
pub async fn serve_operator_connection(
    stream: crate::endpoint::ControlStream,
    core: Arc<Runtime>,
) -> crate::control::Result<()> {
    use crate::control::{decode, encode, MAX_CONTROL_MESSAGE_BYTES};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let peer = KernelOperator::from_stream(&stream, &core.config.sentry.operator_uids).ok();
    let ai_peer = crate::ai_control::AiOperator::from_stream(&stream);
    let world_peer = crate::world_control::WorldOperator::from_stream(&stream);
    let mut session = OperatorSession {
        core,
        peer,
        pending: None,
        review_started: None,
        connected: Arc::new(AtomicBool::new(true)),
    };
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    loop {
        let mut line = String::new();
        let count = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            (&mut reader)
                .take(MAX_CONTROL_MESSAGE_BYTES as u64 + 1)
                .read_line(&mut line),
        )
        .await
        .map_err(|_| {
            crate::control::ControlError::Malformed("control session timed out".into())
        })??;
        if count == 0 {
            return Ok(());
        }
        if count > MAX_CONTROL_MESSAGE_BYTES {
            return Err(crate::control::ControlError::TooLarge);
        }
        if line.trim().is_empty() {
            continue;
        }
        let response = match decode::<ControlRequest>(&line) {
            Ok(request) if request.is_world() => {
                tokio::select! { biased;
                    _ = reader.fill_buf() => { return Ok(()); }
                    response = session.core.handle_operator_world(world_peer.as_ref(), request) => response,
                }
            }
            Ok(request) if request.is_ai() => {
                // EOF or pipelined input cancels this exact inference. Give the
                // attributed service a bounded chance to persist cancellation;
                // dropping the future also drops its owned model transport.
                let (cancel, cancelled) = tokio::sync::watch::channel(false);
                let operation =
                    session
                        .core
                        .handle_operator_ai(ai_peer.as_ref(), request, cancelled);
                tokio::pin!(operation);
                tokio::select! { biased;
                    _ = reader.fill_buf() => {
                        let _ = cancel.send(true);
                        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), operation).await;
                        return Ok(());
                    }
                    response = &mut operation => response,
                }
            }
            Ok(request) if request.is_mutating() => {
                tokio::select! { biased;
                    _ = reader.fill_buf() => { return Ok(()); }
                    response = session.handle(request) => response,
                }
            }
            Ok(request) => session.handle(request).await,
            Err(_) => ControlResponse::error("malformed control request"),
        };
        write_half.write_all(&encode(&response)?).await?;
        write_half.flush().await?;
    }
}
