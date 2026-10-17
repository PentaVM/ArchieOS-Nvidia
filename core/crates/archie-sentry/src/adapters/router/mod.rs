//! System administration adapter for a home router — written against the
//! owner's MitraStar GPT-2741GNAC (ISP ONT/router), but honest about the
//! fact that this firmware's specifics are unknowns: nothing here assumes
//! them. What the router offers is discovered by [`probe::probe_router`] and
//! only what the probe found is ever actuated.
//!
//! Two channels behind one [`RouterAdapter`]:
//!
//! * **UPnP IGD** ([`igd`]): credential-free SOAP 1.1 against
//!   `WANIPConnection:1`. Carries `Command::PortMapping`: Add/Delete, with
//!   the postcondition read back from GetSpecificPortMappingEntry — the
//!   router's own answer that the exact (port, protocol) mapping is enabled
//!   (or gone). SSDP discovery of the control URL is out of scope; the URL
//!   comes from configuration.
//! * **SSH** ([`ssh`]): the system ssh binary through
//!   [`crate::adapters::process::CommandRunner`], key auth only, BatchMode,
//!   bounded timeouts, and a static allowlist of remote commands. Carries
//!   `Command::WifiSet` through the radio strategies the probe found, and
//!   feeds read-only views (client list via [`clients::router_clients`]).
//!
//! Postcondition wiring: `apply(PortMapping)` records the (port, protocol)
//! it acted on; `observe` then asks the router about exactly that mapping,
//! so `ResourceState::Router::mapping_present` is `Some(true)`/`Some(false)`
//! from the router's mouth, or `None` when the adapter was never told what
//! to watch or the IGD channel was not probed. `wifi_enabled` is `Some`
//! only when a probed radio tool reports a parseable state. `None` never
//! verifies anything — that is the point.
//!
//! **Tested vs. untested.** Every code path here is exercised against a
//! mock SOAP IGD server on loopback and a fake `CommandRunner`
//! (`tests/adapters_router.rs`). Nothing has run against the real router:
//! the actual control URL, whether SSH is enabled at all, which radio tool
//! the firmware ships, and where it keeps its leases file are all live
//! unknowns that the probe must answer on site. Writes beyond IGD (the
//! radio) depend entirely on what that probe finds; if it finds no radio
//! tool, `WifiSet` is `Unsupported` and says so.

pub mod admin;
pub mod clients;
pub mod igd;
pub mod probe;
pub mod ssh;

pub use clients::{parse_dnsmasq_leases, parse_ip_neigh, router_clients, RouterClient};
pub use igd::{IgdClient, MappingEntry, SpecificEntry, StatusInfo};
pub use probe::{probe_router, RouterCapabilities};
pub use ssh::{
    radio_strategy, HostKeyChecking, RadioStrategy, SshChannel, SshConfig, RADIO_STRATEGIES,
    READ_COMMANDS,
};

use crate::{
    adapters::process::CommandRunner,
    contracts::{
        validate_id, Command, CommandKind, Observation, ObservationOrigin, PortProtocol,
        ResourceState,
    },
    engine::{AdapterAck, AdapterError, EdgeAdapter},
    registry::ResourceDescriptor,
};
use archie_types::Timestamp;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{net::Ipv4Addr, sync::Arc};

/// Static description stamped on mappings this system creates.
const MAPPING_DESCRIPTION: &str = "archie-sentry";

/// Router endpoint configuration. `control_url` is the WANIPConnection
/// control URL; `lan_client` is the internal host that receives mapped
/// ports (the command names only the external port and protocol, so the
/// internal target is configuration, and the internal port equals the
/// external one). `ssh` holds the admin channel when the router exposes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterConfig {
    pub control_url: Option<String>,
    pub lan_client: Ipv4Addr,
    pub ssh: Option<SshConfig>,
}

/// One paired router, administered through the channels the probe found.
/// Construct with the [`RouterCapabilities`] from [`probe_router`]; the
/// adapter refuses, as `Unsupported`, any command whose channel the probe
/// did not confirm.
pub struct RouterAdapter {
    edge: String,
    adapter: String,
    resource: String,
    lan_client: Ipv4Addr,
    igd: Option<IgdClient>,
    ssh: Option<SshChannel>,
    capabilities: RouterCapabilities,
    /// The (external port, protocol) the last PortMapping apply acted on;
    /// observe reports on exactly this mapping, or nothing.
    watch: Option<(u16, PortProtocol)>,
    observations: u64,
}
impl std::fmt::Debug for RouterAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouterAdapter")
            .field("edge", &self.edge)
            .field("adapter", &self.adapter)
            .field("resource", &self.resource)
            .field("capabilities", &self.capabilities)
            .field("watch", &self.watch)
            .finish_non_exhaustive()
    }
}
impl RouterAdapter {
    pub fn new(
        edge: &str,
        adapter: &str,
        resource: &str,
        config: RouterConfig,
        capabilities: RouterCapabilities,
        runner: Arc<dyn CommandRunner>,
    ) -> Result<Self, AdapterError> {
        validate_id(edge).map_err(|_| AdapterError::Protocol)?;
        validate_id(adapter).map_err(|_| AdapterError::Protocol)?;
        validate_id(resource).map_err(|_| AdapterError::Protocol)?;
        if !(config.lan_client.is_private() || config.lan_client.is_loopback()) {
            return Err(AdapterError::Protocol);
        }
        let igd = match &config.control_url {
            Some(url) => Some(IgdClient::new(url)?),
            None => None,
        };
        let ssh = config.ssh.map(|ssh| SshChannel::new(ssh, runner));
        Ok(Self {
            edge: edge.into(),
            adapter: adapter.into(),
            resource: resource.into(),
            lan_client: config.lan_client,
            igd,
            ssh,
            capabilities,
            watch: None,
            observations: 0,
        })
    }
    /// What the probe found; for inspection and honest CLI reporting.
    pub fn capabilities(&self) -> &RouterCapabilities {
        &self.capabilities
    }
    /// A radio tool the probe found that has a known strategy, if any.
    fn radio_tool(&self) -> Option<&str> {
        self.capabilities
            .radio_tools
            .iter()
            .map(String::as_str)
            .find(|tool| ssh::radio_strategy(tool).is_some())
    }
    fn resource_check(&self, resource: &ResourceDescriptor) -> Result<(), AdapterError> {
        if !resource.paired
            || resource.adapter_id != self.adapter
            || resource.edge_id != self.edge
            || resource.resource_id != self.resource
        {
            return Err(AdapterError::Unsupported);
        }
        Ok(())
    }
}
#[async_trait]
impl EdgeAdapter for RouterAdapter {
    fn edge_id(&self) -> &str {
        &self.edge
    }
    fn adapter_id(&self) -> &str {
        &self.adapter
    }
    /// The router's own answers: the watched mapping's state via IGD, and
    /// the radio state via the probed tool. Either side is `None` when its
    /// channel was not probed or nothing has named what to watch — never a
    /// guess.
    async fn observe(
        &mut self,
        resource: &ResourceDescriptor,
    ) -> Result<Observation, AdapterError> {
        self.resource_check(resource)?;
        let mapping_present = match (self.watch, self.igd.as_ref()) {
            (Some((port, protocol)), Some(igd)) if self.capabilities.igd => {
                match igd.get_specific_port_mapping_entry(port, protocol).await? {
                    Some(entry) => Some(entry.enabled),
                    None => Some(false),
                }
            }
            _ => None,
        };
        let wifi_enabled = match (self.ssh.as_ref(), self.radio_tool()) {
            (Some(ssh), Some(tool)) => {
                // A radio read failure degrades to "unknown" so the mapping
                // evidence is not lost with it.
                ssh.radio_state(tool).await.ok().flatten()
            }
            _ => None,
        };
        let state = ResourceState::Router {
            mapping_present,
            wifi_enabled,
        };
        let now = Timestamp::now();
        self.observations = self.observations.saturating_add(1);
        // The counter keeps evidence identity honest when nothing changed.
        let report = serde_json::to_vec(&(&state, now.0, self.observations))
            .map_err(|_| AdapterError::Protocol)?;
        Ok(Observation {
            resource_id: resource.resource_id.clone(),
            adapter_id: self.adapter.clone(),
            evidence_id: format!("router-{:x}", Sha256::digest(report)),
            observed_at: now,
            origin: ObservationOrigin::Measurement,
            state,
        })
    }
    async fn apply(
        &mut self,
        resource: &ResourceDescriptor,
        command: &Command,
    ) -> Result<AdapterAck, AdapterError> {
        command.validate().map_err(|_| AdapterError::Protocol)?;
        self.resource_check(resource)?;
        match command {
            Command::PortMapping {
                present,
                external_port,
                protocol,
            } => {
                if !resource.capabilities.contains(&CommandKind::PortMapping) {
                    return Err(AdapterError::Unsupported);
                }
                let igd = match (&self.igd, self.capabilities.igd) {
                    (Some(igd), true) => igd,
                    // The probe never confirmed IGD: say so, don't guess.
                    _ => return Err(AdapterError::Unsupported),
                };
                if *present {
                    igd.add_port_mapping(
                        *external_port,
                        *protocol,
                        *external_port,
                        self.lan_client,
                        MAPPING_DESCRIPTION,
                        0,
                    )
                    .await?;
                } else {
                    igd.delete_port_mapping(*external_port, *protocol).await?;
                }
                self.watch = Some((*external_port, *protocol));
                Ok(AdapterAck { accepted: true })
            }
            Command::WifiSet { on } => {
                if !resource.capabilities.contains(&CommandKind::WifiSet) {
                    return Err(AdapterError::Unsupported);
                }
                let (ssh, tool) = match (self.ssh.as_ref(), self.radio_tool()) {
                    (Some(ssh), Some(tool)) => (ssh, tool),
                    // No probed radio tool: the probe result is the answer.
                    _ => return Err(AdapterError::Unsupported),
                };
                ssh.set_radio(tool, *on).await?;
                Ok(AdapterAck { accepted: true })
            }
            _ => Err(AdapterError::Unsupported),
        }
    }
}
