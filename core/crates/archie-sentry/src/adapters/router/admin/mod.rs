//! Multi-firmware, owner-enrolled SSH SDK. Netmiko-inspired driver separation;
//! original Rust code. No credential discovery, root exploit, flash or shell API.
//! Existing Core/Policy/lease authority remains the only authority for writes.
mod profiles;
use super::{
    probe_router,
    ssh::{HostKeyChecking, SshChannel, SshConfig},
    RouterAdapter, RouterConfig,
};
use crate::{
    adapters::process::{off_runtime, CommandOutput, CommandRunner},
    engine::AdapterError,
};
pub use profiles::{Firmware, ReadKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    net::Ipv4Addr,
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const MAX_READ_BYTES: usize = 65_536;
#[derive(Clone)]
pub struct AdminConfig {
    pub ssh: SshConfig,
    pub known_hosts: String,
    pub firmware: Option<Firmware>,
    pub trust_first_use: bool,
    pub allow_sudo_read: bool,
    pub probe_budget_secs: u8,
}
impl AdminConfig {
    pub fn new(ssh: SshConfig, known_hosts: &str) -> Result<Self, AdapterError> {
        let value = Self {
            ssh,
            known_hosts: known_hosts.into(),
            firmware: None,
            trust_first_use: false,
            allow_sudo_read: false,
            probe_budget_secs: 30,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), AdapterError> {
        SshConfig::new(
            &self.ssh.binary,
            &self.ssh.user,
            self.ssh.host,
            self.ssh.identity_file.clone(),
        )?
        .with_port(self.ssh.port)?
        .with_connect_timeout(self.ssh.connect_timeout_secs)?;
        if self.ssh.identity_file.is_none()
            || self.ssh.user.starts_with('-')
            || !(1..=60).contains(&self.probe_budget_secs)
            || !Path::new(&self.known_hosts).is_absolute()
            || self.known_hosts.len() > 512
            || self
                .known_hosts
                .chars()
                .any(|c| c.is_control() || matches!(c, '"' | '%' | '~'))
        {
            return Err(AdapterError::Protocol);
        }
        if let Some(key) = self.ssh.identity_file.as_deref() {
            if Path::new(key) == Path::new(&self.known_hosts)
                || matches!((std::fs::canonicalize(key),std::fs::canonicalize(&self.known_hosts)),(Ok(a),Ok(b)) if a==b)
            {
                return Err(AdapterError::Protocol);
            }
        }
        Ok(())
    }
}
impl std::fmt::Debug for AdminConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminConfig")
            .field("host", &self.ssh.host)
            .field("user", &self.ssh.user)
            .field("firmware", &self.firmware)
            .field("trust_first_use", &self.trust_first_use)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Privilege {
    Unknown,
    User,
    Root,
    SudoRoot,
    NetworkAdmin,
}
#[derive(Debug, Clone, Serialize)]
pub struct AccessReport {
    pub firmware: Firmware,
    pub firmware_verified: bool,
    pub privilege: Privilege,
    pub privilege_scope: &'static str,
    pub policies: Vec<String>,
    pub host_trust: &'static str,
    pub root_is_posix_only: bool,
    pub profile_reads: Vec<ReadKind>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReadEvidence {
    pub host: Ipv4Addr,
    pub port: u16,
    pub user: String,
    pub kind: ReadKind,
    pub firmware: Firmware,
    pub output: String,
    pub sha256: String,
    pub observed_at_ms: u64,
    pub scope: &'static str,
}

#[derive(Clone)]
struct PinnedRunner {
    config: AdminConfig,
    inner: Arc<dyn CommandRunner>,
}
impl CommandRunner for PinnedRunner {
    fn run(&self, argv: &[String], timeout: Duration) -> Result<CommandOutput, AdapterError> {
        if argv.first() != Some(&self.config.ssh.binary) {
            return Err(AdapterError::Protocol);
        }
        self.config.validate()?;
        let mut hardened = vec![
            self.config.ssh.binary.clone(),
            "-F".into(),
            "none".into(),
            "-T".into(),
        ];
        let policy = if self.config.trust_first_use {
            "accept-new"
        } else {
            "yes"
        };
        let hosts = self.config.known_hosts.replace('\\', "/");
        for option in [
            format!("StrictHostKeyChecking={policy}"),
            format!("UserKnownHostsFile=\"{hosts}\""),
            "GlobalKnownHostsFile=none".into(),
            "IdentityAgent=none".into(),
            "IdentitiesOnly=yes".into(),
            "PreferredAuthentications=publickey".into(),
            "PasswordAuthentication=no".into(),
            "KbdInteractiveAuthentication=no".into(),
            "NumberOfPasswordPrompts=0".into(),
            "ClearAllForwardings=yes".into(),
            "PermitLocalCommand=no".into(),
            "ConnectionAttempts=1".into(),
        ] {
            hardened.extend(["-o".into(), option]);
        }
        hardened.extend_from_slice(&argv[1..]);
        let output = self.inner.run(&hardened, timeout)?;
        if output.stdout.len() > MAX_READ_BYTES || output.stderr.len() > MAX_READ_BYTES {
            return Err(AdapterError::Protocol);
        }
        Ok(output)
    }
}

#[derive(Clone)]
pub struct RouterAdmin {
    config: AdminConfig,
    runner: Arc<PinnedRunner>,
    report: AccessReport,
}
impl std::fmt::Debug for RouterAdmin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouterAdmin")
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}
impl RouterAdmin {
    /// Bounded blocking SSH work is offloaded from the async Core executor.
    pub async fn connect(
        config: AdminConfig,
        runner: Arc<dyn CommandRunner>,
    ) -> Result<Self, AdapterError> {
        config.validate()?;
        off_runtime(move || Self::probe(config, runner)).await
    }
    fn execute(
        config: &AdminConfig,
        runner: &PinnedRunner,
        command: &str,
        deadline: Instant,
    ) -> Result<Option<String>, AdapterError> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(AdapterError::Unavailable)?;
        let channel = SshChannel::new(config.ssh.clone(), Arc::new(runner.clone()));
        let timeout = remaining.min(Duration::from_secs(
            u64::from(config.ssh.connect_timeout_secs) + 5,
        ));
        let output = runner.run(&channel.argv(&[command]), timeout)?;
        if output.status == 255 {
            return Err(AdapterError::Unavailable);
        }
        if output.status != 0 {
            return Ok(None);
        }
        String::from_utf8(output.stdout)
            .map(Some)
            .map_err(|_| AdapterError::Protocol)
    }
    fn probe(config: AdminConfig, inner: Arc<dyn CommandRunner>) -> Result<Self, AdapterError> {
        let deadline = Instant::now() + Duration::from_secs(u64::from(config.probe_budget_secs));
        let runner = Arc::new(PinnedRunner {
            config: config.clone(),
            inner,
        });
        let mut verified = false;
        let firmware = if config.firmware == Some(Firmware::Routeros) {
            Firmware::Routeros
        } else {
            let text = Self::execute(&config, &runner, profiles::FINGERPRINT, deadline)?
                .unwrap_or_default();
            let detected = profiles::identify(&text)?;
            if let (Some(expected), Some(actual)) = (config.firmware, detected) {
                if expected != actual && actual != Firmware::Linux {
                    return Err(AdapterError::Protocol);
                }
            }
            verified =
                detected.is_some() && (config.firmware.is_none() || detected == config.firmware);
            if let Some(kind) = config.firmware.or(detected) {
                kind
            } else {
                Firmware::Routeros
            }
        };
        let mut policies = Vec::new();
        let privilege = if firmware == Firmware::Routeros {
            let resource = Self::execute(&config, &runner, "/system resource print", deadline)?
                .ok_or(AdapterError::Unsupported)?;
            if !(resource
                .lines()
                .any(|l| l.trim_start().starts_with("version:"))
                && resource
                    .lines()
                    .any(|l| l.trim_start().starts_with("board-name:")))
            {
                return Err(AdapterError::Protocol);
            }
            verified = true;
            let user = Self::execute(
                &config,
                &runner,
                &format!(
                    "/user print detail without-paging where name={}",
                    config.ssh.user
                ),
                deadline,
            )?
            .unwrap_or_default();
            if token(&user, "name").as_deref() == Some(config.ssh.user.as_str()) {
                if let Some(group) = token(&user, "group").filter(|s| identifier(s)) {
                    let text = Self::execute(
                        &config,
                        &runner,
                        &format!("/user group print detail without-paging where name={group}"),
                        deadline,
                    )?
                    .unwrap_or_default();
                    if token(&text, "name").as_deref() == Some(group.as_str()) {
                        if let Some(value) = token(&text, "policy") {
                            policies = value
                                .split(',')
                                .filter(|p| !p.is_empty())
                                .map(str::to_owned)
                                .collect();
                        }
                    }
                }
            }
            if ["ssh", "read", "write"].iter().all(|p| {
                policies.iter().any(|v| v == p) && !policies.iter().any(|v| v == &format!("!{p}"))
            }) {
                Privilege::NetworkAdmin
            } else {
                Privilege::Unknown
            }
        } else {
            let uid = Self::execute(&config, &runner, "id -u", deadline)?.unwrap_or_default();
            let uid = uid.trim();
            match uid
                .bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| uid.parse::<u32>().ok())
                .flatten()
            {
                Some(0) if uid == "0" => Privilege::Root,
                Some(_) if config.allow_sudo_read => {
                    let sudo = Self::execute(&config, &runner, "sudo -n -- id -u", deadline)?
                        .unwrap_or_default();
                    if sudo.trim() == "0" {
                        Privilege::SudoRoot
                    } else {
                        Privilege::User
                    }
                }
                Some(_) => Privilege::User,
                _ => Privilege::Unknown,
            }
        };
        let report = AccessReport {
            firmware,
            firmware_verified: verified,
            privilege,
            privilege_scope: match privilege {
                Privilege::Root => "uid 0 observed for id -u; Core authority still required",
                Privilege::SudoRoot => "sudo id -u only verified; each other read may be denied",
                Privilege::NetworkAdmin => {
                    "observed ssh/read/write group policies only; not unrestricted access"
                }
                Privilege::User => "nonzero POSIX UID observed; command permissions not inferred",
                Privilege::Unknown => "privilege not established",
            },
            policies,
            host_trust: if config.trust_first_use {
                "explicit-first-use"
            } else {
                "pinned"
            },
            root_is_posix_only: true,
            profile_reads: ReadKind::ALL
                .iter()
                .copied()
                .filter(|k| profiles::command(firmware, *k).is_some())
                .collect(),
        };
        Ok(Self {
            config,
            runner,
            report,
        })
    }
    pub fn report(&self) -> &AccessReport {
        &self.report
    }
    /// Exit-zero/read output is evidence of a command response, not a parsed
    /// claim that all interfaces/flows are present. Connections are snapshots.
    pub async fn read(&self, kind: ReadKind) -> Result<ReadEvidence, AdapterError> {
        let command =
            profiles::command(self.report.firmware, kind).ok_or(AdapterError::Unsupported)?;
        let this = self.clone();
        off_runtime(move || {
            let command = if this.report.privilege == Privilege::SudoRoot {
                format!("sudo -n -- {command}")
            } else {
                command.into()
            };
            let deadline = Instant::now()
                + Duration::from_secs(u64::from(this.config.ssh.connect_timeout_secs) + 5);
            let output = Self::execute(&this.config, &this.runner, &command, deadline)?
                .ok_or(AdapterError::Unavailable)?;
            let observed_at_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| AdapterError::Unavailable)?
                .as_millis()
                .try_into()
                .map_err(|_| AdapterError::Protocol)?;
            let sha256 = format!(
                "{:x}",
                Sha256::digest(
                    serde_json::to_vec(&(
                        this.config.ssh.host,
                        this.config.ssh.port,
                        &this.config.ssh.user,
                        this.report.firmware,
                        kind,
                        &output,
                        observed_at_ms
                    ))
                    .map_err(|_| AdapterError::Protocol)?
                )
            );
            Ok(ReadEvidence {
                host: this.config.ssh.host,
                port: this.config.ssh.port,
                user: this.config.ssh.user.clone(),
                kind,
                firmware: this.report.firmware,
                output,
                sha256,
                observed_at_ms,
                scope: "bounded response; not complete WAN capture",
            })
        })
        .await
    }
    /// Returns the existing adapter, for the Core Intent/Policy/lease pipeline.
    /// Enrollment trust becomes strict here; sudo-read never grants sudo writes.
    pub async fn core_adapter(
        &self,
        edge: &str,
        adapter: &str,
        resource: &str,
        lan_client: Ipv4Addr,
    ) -> Result<RouterAdapter, AdapterError> {
        let mut pinned = self.config.clone();
        pinned.trust_first_use = false;
        pinned.ssh.host_key = HostKeyChecking::Yes;
        let runner: Arc<dyn CommandRunner> = Arc::new(PinnedRunner {
            config: pinned.clone(),
            inner: Arc::clone(&self.runner.inner),
        });
        let channel = SshChannel::new(pinned.ssh.clone(), Arc::clone(&runner));
        let capabilities = probe_router(None, Some(&channel)).await;
        RouterAdapter::new(
            edge,
            adapter,
            resource,
            RouterConfig {
                control_url: None,
                lan_client,
                ssh: Some(pinned.ssh),
            },
            capabilities,
            runner,
        )
    }
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn token(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    let mut words = text.split_whitespace();
    while let Some(word) = words.next() {
        let Some(value) = word.strip_prefix(&prefix) else {
            continue;
        };
        let mut value = value.trim_matches('"').to_owned();
        if key == "policy" {
            while value.ends_with(',') {
                let next = words.next()?.trim_matches('"');
                if !next
                    .bytes()
                    .all(|b| b.is_ascii_alphabetic() || matches!(b, b',' | b'!' | b'-'))
                {
                    return None;
                }
                value.push_str(next);
            }
        }
        return Some(value);
    }
    None
}
