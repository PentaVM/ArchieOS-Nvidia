//! Kernel-admitted inference over the daemon's existing state and authority.
//! Process configuration is read only from operator-owned files, never the wire.
use crate::{
    control::{ControlRequest, ControlResponse},
    Runtime,
};
use archie_model_runtime::routed::Routed;
use archie_workstation::{
    ai::{InferenceStorage, RunnerChoice, UnifiedRuntime},
    ai_policy::RuntimeAuthority,
    codex::CodexClient,
    harness::{load_harnesses, HarnessKind},
    harness_runtime::{load_program, run_harness},
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tracing::warn;

mod metrics;
use metrics::record_inference_metrics;

/// Only this module can construct an AI peer, from a live kernel-authenticated
/// socket before it is split. Serialized requests cannot mint one.
#[derive(Debug)]
pub(crate) struct AiOperator {
    uid: u32,
    _pid: u32,
}
impl AiOperator {
    #[cfg(target_os = "linux")]
    pub(crate) fn from_stream(stream: &crate::endpoint::ControlStream) -> Option<Self> {
        let peer = stream.peer_cred().ok()?;
        Some(Self {
            uid: peer.uid(),
            _pid: u32::try_from(peer.pid()?).ok()?,
        })
    }
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn from_stream(_: &crate::endpoint::ControlStream) -> Option<Self> {
        None
    }
}

impl Runtime {
    /// This entry point is crate-private and requires the non-serializable peer
    /// captured from this exact connection. The public dispatcher always denies AI.
    pub(crate) async fn handle_operator_ai(
        &self,
        peer: Option<&AiOperator>,
        request: ControlRequest,
        cancel: watch::Receiver<bool>,
    ) -> ControlResponse {
        self.metrics
            .increment(&format!("control.request.{}", request.name()), 1);
        let Some(peer) = peer else {
            return ControlResponse::error("authenticated local operator session required");
        };
        match self.dispatch_ai(peer, request, cancel).await {
            Ok(response) => response,
            Err(error) => ControlResponse::error(error),
        }
    }

    /// The operator profile under the live Core engine: what every AI dispatch here
    /// admits, dispatches and commits against.
    pub fn ai_authority(&self) -> RuntimeAuthority {
        RuntimeAuthority::new(
            self.paths.config_dir.join("inference.toml"),
            self.live_policy(),
        )
    }

    /// Core's emergency pause, set and lifted by the profile's owner. The engine is
    /// held for the whole change, so a pause and a resume never interleave. A pause
    /// takes effect before it is recorded or audited; a resume takes effect last, so
    /// one whose record cannot be removed or whose audit fails leaves the node paused.
    fn emergency_control(
        &self,
        authority: &RuntimeAuthority,
        peer: &AiOperator,
        request: &ControlRequest,
    ) -> Result<ControlResponse, String> {
        authority.admit_profile_owner(peer.uid)?;
        let live = self.live_policy();
        // A poisoned engine still pauses: the stop must not depend on an earlier panic.
        let mut engine = live
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let audit = |action: &str, explanation: &str| {
            self.state
                .append_audit(&archie_observability::AuditEvent::new(
                    self.node_id(),
                    action,
                    format!("human:unix-uid-{}", peer.uid),
                    "node:ai-dispatch",
                    archie_observability::AuditOutcome::Allowed,
                    explanation,
                ))
        };
        match request {
            ControlRequest::AiPause => {
                // Stop first: recording and auditing must not delay the pause itself.
                engine.set_emergency_pause(true);
                if let Err(error) = crate::emergency::record(&self.paths) {
                    warn!(%error, "emergency pause is live but not recorded; a restart would lift it");
                }
                if audit(
                    "policy.emergency_pause",
                    "operator set Core's emergency pause; AI dispatch is refused",
                )
                .is_err()
                {
                    warn!("emergency pause is live but its audit record failed");
                }
                self.metrics.increment("policy.emergency_pause.set", 1);
            }
            ControlRequest::AiResume => {
                crate::emergency::clear(&self.paths)
                    .map_err(|_| "pause record cannot be removed; the node stays paused")?;
                if audit(
                    "policy.emergency_resume",
                    "operator lifted Core's emergency pause",
                )
                .is_err()
                {
                    let _ = crate::emergency::record(&self.paths);
                    return Err("resume cannot be audited; the node stays paused".into());
                }
                engine.set_emergency_pause(false);
                self.metrics.increment("policy.emergency_pause.lifted", 1);
            }
            ControlRequest::AiStatus => {}
            _ => return Err("invalid emergency control".into()),
        }
        Ok(ControlResponse::AiPolicy {
            paused: engine.config().emergency_pause,
            recorded: crate::emergency::recorded(&self.paths),
        })
    }

    async fn dispatch_ai(
        &self,
        peer: &AiOperator,
        request: ControlRequest,
        cancel: watch::Receiver<bool>,
    ) -> Result<ControlResponse, String> {
        // The live Core engine, never a copy: an emergency pause set on another
        // connection reaches this request's dispatch and its commit.
        let authority = self.ai_authority();
        if matches!(
            request,
            ControlRequest::AiPause | ControlRequest::AiResume | ControlRequest::AiStatus
        ) {
            return self.emergency_control(&authority, peer, &request);
        }
        authority.admit_operator_uid(peer.uid)?;
        let (profile, _) = authority.profile()?;
        let (domain, runner, duration_ms) = match &request {
            ControlRequest::AiInfer { request } => (
                &request.domain,
                Some(&request.runner),
                profile.max_duration_ms,
            ),
            ControlRequest::AiRecall { binding, .. } => {
                (&binding.domain, None, profile.max_duration_ms)
            }
            ControlRequest::AiHarness { request } => {
                if !(100..=300000).contains(&request.duration_ms)
                    || request.prompt.is_empty()
                    || request.prompt.len() > profile.max_prompt_bytes
                    || matches!(request.runner, RunnerChoice::Api)
                {
                    return Err("invalid Harness² task, pinned runner or duration".into());
                }
                InferenceStorage::focus_id(&request.domain, &request.focus)?;
                (&request.domain, Some(&request.runner), request.duration_ms)
            }
            _ => return Err("invalid inference dispatch".into()),
        };
        let domain = domain.clone();
        if !profile.allowed_domains.contains(&domain) {
            return Err("domain is outside operator scope".into());
        }
        let runner_id = runner.map(|runner| {
            if matches!(runner, RunnerChoice::Codex { .. }) {
                "codex"
            } else {
                "api"
            }
        });
        if runner_id.is_some_and(|runner| {
            !profile
                .allowed_runners
                .iter()
                .any(|allowed| allowed == runner)
        }) {
            return Err("runner is outside operator scope".into());
        }
        let deadline = Duration::from_millis(duration_ms.saturating_add(3000));
        let catalog = if runner.is_some() {
            Routed::from_paths(self.paths.models(), self.paths.inference_providers())
                .map_err(|_| "configured inference catalog is invalid")?
        } else {
            Routed::new(archie_registry::ModelRegistry::new())
        };
        let codex = if matches!(runner, Some(RunnerChoice::Codex { .. })) {
            // The full Harness run may outlast one stage's profile lease. Each
            // actual model call retains the independent per-stage limit.
            Some(configured_codex(
                self,
                peer.uid,
                Duration::from_millis(profile.max_duration_ms),
            )?)
        } else {
            None
        };
        let program = if matches!(request, ControlRequest::AiHarness { .. }) {
            Some(load_program(
                &self.paths.state_dir.join("inference/harness2.toml"),
                self.paths.state_dir.join("inference/harness2"),
                Some(peer.uid),
            )?)
        } else {
            None
        };
        let mut service = UnifiedRuntime {
            storage: Arc::new(InferenceStorage {
                store: self.state.clone(),
                authority,
                vault_dir: self.paths.state_dir.join("inference/vault"),
            }),
            catalog,
            codex,
        };
        let outcome = tokio::time::timeout(deadline, async {
            match request {
                ControlRequest::AiInfer { request } => {
                    let started = Instant::now();
                    let result = service.execute(&request, Some(cancel)).await;
                    record_inference_metrics(
                        &self.metrics,
                        result
                            .as_ref()
                            .ok()
                            .and_then(|outcome| outcome.receipt.usage.output_tokens),
                        Some(started.elapsed()),
                        archie_types::Timestamp::now().as_millis(),
                    );
                    result.map(|outcome| ControlResponse::AiResult {
                        outcome: Box::new(outcome),
                    })
                }
                ControlRequest::AiRecall {
                    binding,
                    receipt_id,
                } => service.recall(binding, receipt_id).await.map(|outcome| {
                    ControlResponse::AiResult {
                        outcome: Box::new(outcome),
                    }
                }),
                ControlRequest::AiHarness { request } => run_harness(
                    &mut service,
                    program
                        .as_ref()
                        .ok_or("configured Harness² worker is required")?,
                    &request,
                    Some(cancel),
                )
                .await
                .map(|outcome| ControlResponse::AiHarnessResult {
                    outcome: Box::new(outcome),
                }),
                _ => Err("invalid inference dispatch".into()),
            }
        })
        .await
        .map_err(|_| "daemon inference deadline expired")??;
        // A profile removed or revoked during the request cannot disclose a result.
        service.storage.authority.admit_operator_uid(peer.uid)?;
        let (fresh, _) = service.storage.authority.profile()?;
        if !fresh.allowed_domains.contains(&domain)
            || runner_id.is_some_and(|runner| {
                !fresh
                    .allowed_runners
                    .iter()
                    .any(|allowed| allowed == runner)
            })
        {
            return Err("inference scope changed before disclosure".into());
        }
        Ok(outcome)
    }
}

fn configured_codex(
    core: &Runtime,
    operator_uid: u32,
    deadline: Duration,
) -> Result<CodexClient, String> {
    let path = core.paths.inference_harnesses();
    let command = if path.exists() {
        trusted_harness_file(&path, operator_uid)?;
        let configured = load_harnesses(&path).map_err(|_| "invalid configured process runners")?;
        let mut codex = configured
            .iter()
            .filter(|spec| spec.kind == HarnessKind::Codex);
        let spec = codex
            .next()
            .ok_or("official Codex runner is not configured")?;
        if codex.next().is_some() || !spec.args.is_empty() || spec.endpoint.is_some() {
            return Err(
                "official Codex runner requires one command with no custom arguments".into(),
            );
        }
        let command = spec
            .command
            .as_deref()
            .filter(|command| !command.is_empty() && command.len() <= 4096)
            .ok_or("official Codex runner command is missing")?;
        command.to_owned()
    } else {
        "codex".to_owned()
    };
    let cwd = core.paths.runtime_dir.join("ai-client");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&cwd)
        .map_err(|_| "cannot create managed-client directory")?;
    let metadata =
        std::fs::symlink_metadata(&cwd).map_err(|_| "cannot inspect managed-client directory")?;
    if !metadata.is_dir() {
        return Err("managed-client directory must be a real directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("managed-client directory must be private".into());
        }
    }
    CodexClient::new(command, &cwd, deadline.min(Duration::from_secs(600)))
        .map_err(|error| error.to_string())
}

fn trusted_harness_file(path: &Path, operator_uid: u32) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "cannot inspect configured process runners")?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err("configured process runners must be a bounded regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if (metadata.uid() != operator_uid && metadata.uid() != 0)
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err(
                "configured process runners must be operator-owned and not writable by others"
                    .into(),
            );
        }
    }
    #[cfg(not(unix))]
    let _ = operator_uid;
    Ok(())
}
