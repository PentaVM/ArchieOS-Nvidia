//! SSH administration channel through the system `ssh` binary, driven by the
//! process infrastructure in [`crate::adapters::process`]: absolute configured
//! binary path, no shell on this side, BatchMode (never a password on argv or
//! a prompt), a bounded connect timeout, and `StrictHostKeyChecking`
//! configurable with `accept-new` as the default. Authentication is an
//! identity file whose path comes from configuration.
//!
//! Everything this channel can say to the router lives in two static tables.
//! [`READ_COMMANDS`] holds read-only probes (neighbours, leases, links, radio
//! state, firmware hints); [`RADIO_STRATEGIES`] holds the only write
//! sequences, keyed by the tool name a capability probe found. Any command
//! name outside the table is [`AdapterError::Unsupported`] before a process
//! exists. A non-zero exit status is [`AdapterError::Unavailable`]; stderr is
//! captured (bounded by the runner) and surfaced on the operator log only.
//!
//! Which firmware speaks which dialect is unknown here by design: the probe
//! in [`crate::adapters::router::probe`] runs the read-only candidates and
//! only what answered is ever offered.

use crate::{adapters::process::CommandRunner, engine::AdapterError};
use std::{net::Ipv4Addr, path::Path, sync::Arc, time::Duration};

/// How the host key is treated on first contact. `AcceptNew` is the default:
/// the first key is pinned into known_hosts and later changes are refused.
/// `Yes` exists only for operators who pin the key out of band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyChecking {
    AcceptNew,
    Yes,
}
impl HostKeyChecking {
    fn as_str(&self) -> &'static str {
        match self {
            Self::AcceptNew => "accept-new",
            Self::Yes => "yes",
        }
    }
}

/// SSH endpoint configuration. No password field exists: BatchMode makes
/// password prompts fail rather than hang, and keys are the only credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshConfig {
    /// Absolute path of the system ssh binary; existence is enforced by the
    /// runner (`ProcessRunner::tool`), not here, so configs stay testable.
    pub binary: String,
    pub user: String,
    pub host: Ipv4Addr,
    pub port: u16,
    pub identity_file: Option<String>,
    pub host_key: HostKeyChecking,
    pub connect_timeout_secs: u8,
}
impl SshConfig {
    pub fn new(
        binary: &str,
        user: &str,
        host: Ipv4Addr,
        identity_file: Option<String>,
    ) -> Result<Self, AdapterError> {
        if binary.is_empty() || binary.len() > 512 || !Path::new(binary).is_absolute() {
            return Err(AdapterError::Protocol);
        }
        if user.is_empty()
            || user.len() > 32
            || !user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(AdapterError::Protocol);
        }
        if !(host.is_private() || host.is_loopback()) {
            return Err(AdapterError::Protocol);
        }
        if let Some(key) = &identity_file {
            if key.is_empty() || key.len() > 512 || !Path::new(key).is_absolute() {
                return Err(AdapterError::Protocol);
            }
        }
        Ok(Self {
            binary: binary.into(),
            user: user.into(),
            host,
            port: 22,
            identity_file,
            host_key: HostKeyChecking::AcceptNew,
            connect_timeout_secs: 5,
        })
    }
    pub fn with_port(mut self, port: u16) -> Result<Self, AdapterError> {
        if port == 0 {
            return Err(AdapterError::Protocol);
        }
        self.port = port;
        Ok(self)
    }
    pub fn with_host_key_checking(mut self, host_key: HostKeyChecking) -> Self {
        self.host_key = host_key;
        self
    }
    pub fn with_connect_timeout(mut self, secs: u8) -> Result<Self, AdapterError> {
        if !(1..=30).contains(&secs) {
            return Err(AdapterError::Protocol);
        }
        self.connect_timeout_secs = secs;
        Ok(self)
    }
}

/// One allowlisted remote command. `argv` elements are passed to ssh as
/// separate arguments; there is no local shell, and the table is the only
/// source of remote commands, so nothing here is ever string-built.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub argv: &'static [&'static str],
}

/// Read-only commands the channel may run. `leases_*` entries are candidate
/// locations for the DHCP leases file; the probe records the first that
/// answers. `wl0_radio` / `nvram_wl0_radio` are also the radio state reads.
pub const READ_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "ip_neigh",
        argv: &["ip", "neigh"],
    },
    CommandSpec {
        name: "ip_link",
        argv: &["ip", "link"],
    },
    CommandSpec {
        name: "iwconfig",
        argv: &["iwconfig"],
    },
    CommandSpec {
        name: "leases_tmp_dhcp",
        argv: &["cat", "/tmp/dhcp.leases"],
    },
    CommandSpec {
        name: "leases_tmp_dnsmasq",
        argv: &["cat", "/tmp/dnsmasq.leases"],
    },
    CommandSpec {
        name: "leases_var_dnsmasq",
        argv: &["cat", "/var/lib/misc/dnsmasq.leases"],
    },
    CommandSpec {
        name: "nvram_productid",
        argv: &["nvram", "get", "productid"],
    },
    CommandSpec {
        name: "nvram_wl0_radio",
        argv: &["nvram", "get", "wl0_radio"],
    },
    CommandSpec {
        name: "wl0_radio",
        argv: &["wl", "-i", "wl0", "radio"],
    },
    CommandSpec {
        name: "iptables_forward",
        argv: &["iptables", "-S", "FORWARD"],
    },
    CommandSpec {
        name: "ip6tables_forward",
        argv: &["ip6tables", "-S", "FORWARD"],
    },
];

/// Firewalls a router may expose over SSH, each with the read-only command that
/// shows it answers. Blocking a device writes to every one that answers, so an
/// IPv4 block is never silently undone by IPv6.
pub const FIREWALL_TOOLS: &[(&str, &str)] = &[
    ("iptables", "iptables_forward"),
    ("ip6tables", "ip6tables_forward"),
];

/// The comment on every rule this system adds: it only ever checks, inserts or
/// deletes rules carrying it, never the router's own.
pub const BLOCK_COMMENT: &str = "archie-sentry-block";

/// The remote argv that checks (`-C`), inserts (`-I`) or deletes (`-D`) the drop rule
/// for one device in a firewall's FORWARD chain. `None` for anything but a known
/// tool, one of those three operations and a valid single-device hardware address,
/// so no other text ever reaches the router's shell.
pub fn block_rule(tool: &str, op: &str, mac: &str) -> Option<Vec<String>> {
    let known = FIREWALL_TOOLS.iter().any(|(name, _)| *name == tool);
    if !known || !matches!(op, "-C" | "-I" | "-D") || crate::contracts::validate_mac(mac).is_err() {
        return None;
    }
    Some(
        [
            tool,
            op,
            "FORWARD",
            "-m",
            "mac",
            "--mac-source",
            mac,
            "-m",
            "comment",
            "--comment",
            BLOCK_COMMENT,
            "-j",
            "DROP",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect(),
    )
}

/// The bounded write surface: how to switch the radio with a given tool.
/// `state_read` names a [`READ_COMMANDS`] entry whose output
/// [`parse_radio_state`] understands. Sequences run in order; any failure
/// aborts the sequence with the command's own error.
#[derive(Debug, Clone, Copy)]
pub struct RadioStrategy {
    pub tool: &'static str,
    pub state_read: &'static str,
    pub on: &'static [&'static [&'static str]],
    pub off: &'static [&'static [&'static str]],
}
pub const RADIO_STRATEGIES: &[RadioStrategy] = &[
    RadioStrategy {
        tool: "wl",
        state_read: "wl0_radio",
        on: &[&["wl", "-i", "wl0", "radio", "on"]],
        off: &[&["wl", "-i", "wl0", "radio", "off"]],
    },
    RadioStrategy {
        tool: "nvram",
        state_read: "nvram_wl0_radio",
        on: &[
            &["nvram", "set", "wl0_radio=1"],
            &["nvram", "commit"],
            &["service", "restart", "wireless"],
        ],
        off: &[
            &["nvram", "set", "wl0_radio=0"],
            &["nvram", "commit"],
            &["service", "restart", "wireless"],
        ],
    },
];

pub fn read_command(name: &str) -> Option<&'static CommandSpec> {
    READ_COMMANDS.iter().find(|spec| spec.name == name)
}
pub fn radio_strategy(tool: &str) -> Option<&'static RadioStrategy> {
    RADIO_STRATEGIES
        .iter()
        .find(|strategy| strategy.tool == tool)
}

/// The filesystem path a `leases_*` command reads, for probe reporting.
pub fn lease_path_of(spec: &CommandSpec) -> Option<&'static str> {
    if spec.name.starts_with("leases_") && spec.argv.len() == 2 && spec.argv[0] == "cat" {
        Some(spec.argv[1])
    } else {
        None
    }
}

/// Parse a radio state read for a known tool. Returns None when the output
/// is not a recognisable state — an honest "the router could not say",
/// never a guess. `wl` prints a number (often hex like `0x0001`); nvram
/// prints `1`/`0`.
pub fn parse_radio_state(tool: &str, output: &str) -> Option<bool> {
    let token = output.split_whitespace().next()?;
    let value: u64 = if let Some(hex) = token.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()?
    } else {
        token.parse().ok()?
    };
    match (tool, value) {
        ("wl" | "nvram", 0) => Some(false),
        ("wl" | "nvram", 1) => Some(true),
        _ => None,
    }
}

/// A configured SSH channel over an injected runner.
pub struct SshChannel {
    config: SshConfig,
    runner: Arc<dyn CommandRunner>,
}
impl std::fmt::Debug for SshChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshChannel")
            .field("user", &self.config.user)
            .field("host", &self.config.host)
            .field("port", &self.config.port)
            .finish_non_exhaustive()
    }
}
impl SshChannel {
    pub fn new(config: SshConfig, runner: Arc<dyn CommandRunner>) -> Self {
        Self { config, runner }
    }
    pub fn config(&self) -> &SshConfig {
        &self.config
    }
    /// The exact local argv for a remote command: binary, BatchMode, connect
    /// timeout, host-key policy, port, identity file, `user@host`, then the
    /// remote argv as separate arguments.
    pub fn argv(&self, remote: &[&str]) -> Vec<String> {
        let mut argv = vec![
            self.config.binary.clone(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            format!("ConnectTimeout={}", self.config.connect_timeout_secs),
            "-o".into(),
            format!("StrictHostKeyChecking={}", self.config.host_key.as_str()),
            "-p".into(),
            self.config.port.to_string(),
        ];
        if let Some(key) = &self.config.identity_file {
            argv.push("-i".into());
            argv.push(key.clone());
        }
        argv.push(format!("{}@{}", self.config.user, self.config.host));
        argv.extend(remote.iter().map(|arg| (*arg).to_string()));
        argv
    }
    fn timeout(&self) -> Duration {
        // Connect timeout plus margin for the remote command itself.
        Duration::from_secs(u64::from(self.config.connect_timeout_secs) + 10)
    }
    fn finish(output: crate::adapters::process::CommandOutput) -> Result<String, AdapterError> {
        if output.status != 0 {
            // stderr is captured, bounded by the runner, and logged locally
            // for the operator; it never enters receipts or observations.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            if !stderr.is_empty() {
                eprintln!(
                    "archie-sentry router ssh: exit {}: {}",
                    output.status,
                    // By characters: a byte cut could split one, and that panics.
                    stderr.chars().take(200).collect::<String>()
                );
            }
            return Err(AdapterError::Unavailable);
        }
        String::from_utf8(output.stdout).map_err(|_| AdapterError::Protocol)
    }
    /// Run an allowlisted read command by table name. Unknown names are
    /// Unsupported and never reach the runner.
    pub fn run_named(&self, name: &str) -> Result<String, AdapterError> {
        let spec = read_command(name).ok_or(AdapterError::Unsupported)?;
        self.run_spec(spec)
    }
    pub(crate) fn run_spec(&self, spec: &CommandSpec) -> Result<String, AdapterError> {
        self.run_argv(spec.argv)
    }
    pub(crate) fn run_argv(&self, remote: &[&str]) -> Result<String, AdapterError> {
        if remote.is_empty()
            || remote
                .iter()
                .any(|arg| arg.is_empty() || arg.len() > 256 || arg.bytes().any(|b| b < 0x20))
        {
            return Err(AdapterError::Protocol);
        }
        let argv = self.argv(remote);
        Self::finish(self.runner.run(&argv, self.timeout())?)
    }
    /// Run a remote command and return its exit status instead of failing on a
    /// non-zero one, for commands whose status is the answer (`iptables -C`).
    /// Status 255 is ssh's own failure (no connection, key refused) and is
    /// `Unavailable`, never an answer.
    async fn run_status_async(&self, remote: Vec<String>) -> Result<i32, AdapterError> {
        if remote.is_empty()
            || remote
                .iter()
                .any(|arg| arg.is_empty() || arg.len() > 256 || arg.bytes().any(|b| b < 0x20))
        {
            return Err(AdapterError::Protocol);
        }
        let borrowed: Vec<&str> = remote.iter().map(String::as_str).collect();
        let argv = self.argv(&borrowed);
        let runner = Arc::clone(&self.runner);
        let timeout = self.timeout();
        let output = tokio::task::spawn_blocking(move || runner.run(&argv, timeout))
            .await
            .map_err(|_| AdapterError::Unavailable)??;
        if output.status == 255 {
            return Err(AdapterError::Unavailable);
        }
        Ok(output.status)
    }
    /// Whether the firewall `tool` answers on this router at all.
    pub async fn firewall_answers(&self, tool: &str) -> bool {
        match FIREWALL_TOOLS.iter().find(|(name, _)| *name == tool) {
            Some((_, read)) => self.run_named_async(read).await.is_ok(),
            None => false,
        }
    }
    /// Whether this system's drop rule for `mac` is in `tool`'s FORWARD chain, as
    /// `-C` answers: exit 0 present, 1 absent. Any other status (the mac match
    /// module missing, the tool refusing) is `Unavailable`: the router could not say.
    pub async fn block_present(&self, tool: &str, mac: &str) -> Result<bool, AdapterError> {
        let check = block_rule(tool, "-C", mac).ok_or(AdapterError::Protocol)?;
        match self.run_status_async(check).await? {
            0 => Ok(true),
            1 => Ok(false),
            _ => Err(AdapterError::Unavailable),
        }
    }
    /// Make `tool`'s FORWARD chain hold exactly one drop rule for `mac`
    /// (`blocked`) or none of this system's (`!blocked`). Idempotent: the rule is
    /// inserted only when absent, and removed until `-C` says it is gone.
    pub async fn set_blocked(
        &self,
        tool: &str,
        mac: &str,
        blocked: bool,
    ) -> Result<(), AdapterError> {
        let present = self.block_present(tool, mac).await?;
        if blocked && !present {
            let insert = block_rule(tool, "-I", mac).ok_or(AdapterError::Protocol)?;
            if self.run_status_async(insert).await? != 0 {
                return Err(AdapterError::Unavailable);
            }
        }
        if !blocked && present {
            // A rule added twice by hand would survive one delete; a few passes
            // remove every copy this system's comment marks, and no more.
            for _ in 0..8 {
                let delete = block_rule(tool, "-D", mac).ok_or(AdapterError::Protocol)?;
                if self.run_status_async(delete).await? != 0
                    || !self.block_present(tool, mac).await?
                {
                    break;
                }
            }
        }
        Ok(())
    }
    async fn run_argv_async(&self, remote: &[&str]) -> Result<String, AdapterError> {
        if remote.is_empty()
            || remote
                .iter()
                .any(|arg| arg.is_empty() || arg.len() > 256 || arg.bytes().any(|b| b < 0x20))
        {
            return Err(AdapterError::Protocol);
        }
        let argv = self.argv(remote);
        let runner = Arc::clone(&self.runner);
        let timeout = self.timeout();
        tokio::task::spawn_blocking(move || Self::finish(runner.run(&argv, timeout)?))
            .await
            .map_err(|_| AdapterError::Unavailable)?
    }
    pub(crate) async fn run_spec_async(&self, spec: &CommandSpec) -> Result<String, AdapterError> {
        self.run_argv_async(spec.argv).await
    }
    pub(crate) async fn run_named_async(&self, name: &str) -> Result<String, AdapterError> {
        let spec = read_command(name).ok_or(AdapterError::Unsupported)?;
        self.run_argv_async(spec.argv).await
    }
    /// Switch the radio with a strategy the probe found. Unknown tools are
    /// Unsupported before anything spawns.
    pub async fn set_radio(&self, tool: &str, on: bool) -> Result<(), AdapterError> {
        let strategy = radio_strategy(tool).ok_or(AdapterError::Unsupported)?;
        let sequence = if on { strategy.on } else { strategy.off };
        for command in sequence {
            self.run_argv_async(command).await?;
        }
        Ok(())
    }
    /// The router's own radio state via a strategy's read command, or None
    /// when the output is not a recognisable state.
    pub async fn radio_state(&self, tool: &str) -> Result<Option<bool>, AdapterError> {
        let strategy = radio_strategy(tool).ok_or(AdapterError::Unsupported)?;
        let output = self.run_named_async(strategy.state_read).await?;
        Ok(parse_radio_state(tool, &output))
    }
}
