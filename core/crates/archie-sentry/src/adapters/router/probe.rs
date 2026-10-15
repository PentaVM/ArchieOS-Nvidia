//! Capability probe: discovers what this router actually offers before the
//! adapter offers anything. The IGD channel is probed with a real
//! GetStatusInfo call; the SSH channel with read-only commands only (link
//! state, lease-file candidates, radio state reads, a firmware hint). A
//! capability that did not answer is reported as absent — nothing here
//! hard-codes MitraStar GPT-2741GNAC firmware specifics.

use super::{
    igd::IgdClient,
    ssh::{
        lease_path_of, parse_radio_state, SshChannel, FIREWALL_TOOLS, RADIO_STRATEGIES,
        READ_COMMANDS,
    },
};
use serde::{Deserialize, Serialize};

/// What the probe found. Persistable, so a sentry config can record the
/// discovered shape and re-probe on schedule.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouterCapabilities {
    /// WANIPConnection answered GetStatusInfo.
    pub igd: bool,
    /// SSH connected and ran a read-only command.
    pub ssh: bool,
    /// Radio tools whose state read answered with a recognisable state;
    /// entries are [`super::ssh::RADIO_STRATEGIES`] tool names.
    pub radio_tools: Vec<String>,
    /// The leases file that answered, when one did.
    pub dhcp_leases_path: Option<String>,
    /// `nvram get productid` when available; bounded, printable.
    pub firmware_hint: Option<String>,
    /// Firewalls whose FORWARD chain answered a read-only listing; entries are
    /// [`super::ssh::FIREWALL_TOOLS`] names. Absent in probes recorded before it.
    #[serde(default)]
    pub firewall_tools: Vec<String>,
}

/// Probe both channels. Either side may be absent from configuration
/// (`None`) or simply not answer; both cases read the same in the result.
/// All SSH probing is read-only — the probe never actuates.
pub async fn probe_router(igd: Option<&IgdClient>, ssh: Option<&SshChannel>) -> RouterCapabilities {
    let mut capabilities = RouterCapabilities::default();
    if let Some(igd) = igd {
        capabilities.igd = igd.get_status_info().await.is_ok();
    }
    if let Some(ssh) = ssh {
        if ssh.run_named_async("ip_link").await.is_ok() {
            capabilities.ssh = true;
            for spec in READ_COMMANDS {
                let Some(path) = lease_path_of(spec) else {
                    continue;
                };
                if ssh.run_spec_async(spec).await.is_ok() {
                    capabilities.dhcp_leases_path = Some(path.into());
                    break;
                }
            }
            for strategy in RADIO_STRATEGIES {
                // The state read doubles as the probe: a tool whose answer
                // parses is a tool we can later write through.
                if let Ok(output) = ssh.run_named_async(strategy.state_read).await {
                    if parse_radio_state(strategy.tool, &output).is_some() {
                        capabilities.radio_tools.push(strategy.tool.into());
                    }
                }
            }
            for (tool, _) in FIREWALL_TOOLS {
                if ssh.firewall_answers(tool).await {
                    capabilities.firewall_tools.push((*tool).into());
                }
            }
            if let Ok(output) = ssh.run_named_async("nvram_productid").await {
                let hint = output.trim();
                if !hint.is_empty()
                    && hint.len() <= 64
                    && hint.bytes().all(|b| (0x20..0x7f).contains(&b))
                {
                    capabilities.firmware_hint = Some(hint.to_owned());
                }
            }
        }
    }
    capabilities
}
