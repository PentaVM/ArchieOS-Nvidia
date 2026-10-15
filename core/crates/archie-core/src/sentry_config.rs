//! Trusted operator configuration; secret contents never serialize into this type.
/// Re-exported for `archie sentry cast-probe`: a handshake that only reports.
pub use archie_sentry::adapters::chromecast::probe_certificate as probe_cast_certificate;
use archie_sentry::{
    adapters::{
        adb::{Adb, AndroidTarget},
        chromecast::{CastDevice, Chromecast},
        home_assistant::HomeAssistant,
        ipp::{IppAdapter, IppClient},
        mqtt::{Mqtt, MqttBinding, MqttKind},
        onvif::OnvifProbe,
        process::ProcessRunner,
        roku::Roku,
        router::{RouterCapabilities, RouterConfig, SshConfig},
        wol::{Wol, WolTarget},
    },
    contracts::{validate_id, CommandKind},
    engine::AdapterError,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SentrySection {
    pub printers: Vec<PrinterBinding>,
    pub operator_uids: Vec<u32>,
    pub home_assistant: Option<HomeAssistantBinding>,
    /// Roku ECP televisions/actuators. Paired devices only; see the registry wiring.
    pub roku: Vec<RokuBinding>,
    /// One LAN MQTT broker and its actuable resources.
    pub mqtt: Option<MqttSection>,
    /// ONVIF cameras: observation-only telemetry, never actuable resources.
    pub onvif_cameras: Vec<OnvifCameraBinding>,
    /// Local IRIS context-feed defaults for `archie context-feed`.
    pub context_feed: Option<ContextFeedSection>,
    /// The one router Sentry administers, when configured.
    pub router: Option<RouterBinding>,
    /// Wake-on-LAN nodes behind one broadcast address.
    pub wol: Option<WolSection>,
    /// Android nodes reachable through adb (USB or wireless debugging).
    pub android: Option<AndroidSection>,
    /// Where the netwatch binary keeps `alerts.jsonl` / `told.json`, for
    /// `archie sentry-follow`.
    pub netwatch: Option<NetwatchSection>,
    /// The first, reversible response: which alerts become owner notices.
    pub response: Option<ResponseSection>,
    /// The chat-export drop folder Wanda ingests, and the sender-hash salt.
    pub chat_exports: Option<ChatExportSection>,
    /// Chromecasts over CAST v2 (TLS 8009), each with the certificate
    /// fingerprint the operator approved after `archie sentry cast-probe`.
    pub chromecasts: Vec<CastDevice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrinterBinding {
    pub resource_id: String,
    /// The printer URI (`ipp://`, `ipps://` or their HTTP forms).
    pub endpoint: String,
    pub expected_uuid: String,
    /// Absolute spool directory for reviewed PrintSubmit jobs. A printer
    /// without a spool directory stays query-only and never becomes actuable.
    #[serde(default)]
    pub spool_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeAssistantBinding {
    pub endpoint: String,
    pub token_file: String,
    pub resources: Vec<HomeAssistantResource>,
    /// Observation-only bindings. They never enter the actuator registry.
    #[serde(default)]
    pub telemetry_resources: Vec<HomeAssistantResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeAssistantResource {
    pub resource_id: String,
    pub entity: String,
}
impl HomeAssistantResource {
    pub fn capabilities(&self) -> Result<Vec<CommandKind>, AdapterError> {
        match self.entity.split_once('.').map(|(domain, _)| domain) {
            Some("switch") => Ok(vec![CommandKind::SwitchSet]),
            Some("light") => Ok(vec![CommandKind::LightSetBrightness]),
            Some("media_player") => Ok(vec![CommandKind::MediaPause, CommandKind::MediaPlay]),
            Some("climate") => Ok(vec![CommandKind::ClimateSetTemperature]),
            _ => Err(AdapterError::Unsupported),
        }
    }
}
impl HomeAssistantBinding {
    fn entities(&self) -> BTreeMap<String, String> {
        self.resources
            .iter()
            .chain(&self.telemetry_resources)
            .map(|r| (r.resource_id.clone(), r.entity.clone()))
            .collect()
    }
    pub fn validate(&self) -> Result<(), AdapterError> {
        let count = self.resources.len() + self.telemetry_resources.len();
        if count == 0
            || count > 64
            || self.entities().len() != count
            || self.token_file.len() > 512
            || !Path::new(&self.token_file).is_absolute()
        {
            return Err(AdapterError::Protocol);
        }
        for resource in &self.resources {
            resource.capabilities()?;
        }
        HomeAssistant::new(
            "local-core",
            "home-assistant",
            &self.endpoint,
            "validation-placeholder",
            self.entities(),
        )?;
        Ok(())
    }
    /// Opens a fresh token for each operation. No live token is kept in config/audit.
    pub fn adapter(&self) -> Result<HomeAssistant, AdapterError> {
        self.validate()?;
        let token = read_private_token(Path::new(&self.token_file))?;
        HomeAssistant::new(
            "local-core",
            "home-assistant",
            &self.endpoint,
            token.trim(),
            self.entities(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RokuBinding {
    pub resource_id: String,
    /// ECP base URL, e.g. `http://192.168.1.40` (port defaults to 8060).
    pub endpoint: String,
}
impl RokuBinding {
    /// Every configured Roku resource accepts exactly these commands.
    pub fn capabilities() -> Vec<CommandKind> {
        vec![CommandKind::RemoteKey, CommandKind::AppLaunch]
    }
}

/// One LAN MQTT broker with its actuable resources. Secrets are references,
/// never inline values: `password_ref` names `env:VARIABLE` or an absolute
/// private file (same rules as the Home Assistant token file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MqttSection {
    /// `"<ip>"` or `"<ip>:<port>"`; loopback or private IP literals only.
    pub broker: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password_ref: Option<String>,
    pub resources: BTreeMap<String, MqttBinding>,
}
impl MqttSection {
    fn credentials_shape(&self) -> Result<(), AdapterError> {
        // The adapter speaks CONNECT with a username/password pair or none;
        // a half-configured credential is an operator mistake, not a mode.
        match (&self.username, &self.password_ref) {
            (Some(_), Some(reference)) => validate_secret_ref(reference),
            (None, None) => Ok(()),
            _ => Err(AdapterError::Authentication),
        }
    }
    pub fn validate(&self) -> Result<(), AdapterError> {
        self.credentials_shape()?;
        let placeholder = self
            .username
            .as_ref()
            .map(|username| (username.clone(), "validation-placeholder".to_owned()));
        Mqtt::new(
            "local-core",
            "mqtt",
            &self.broker,
            placeholder,
            self.resources.clone(),
        )?;
        Ok(())
    }
    /// Resolves the referenced password for this one operation. No live
    /// credential is kept in config or audit.
    pub fn adapter(&self) -> Result<Mqtt, AdapterError> {
        self.validate()?;
        let credentials = match (&self.username, &self.password_ref) {
            (Some(username), Some(reference)) => {
                Some((username.clone(), read_secret_ref(reference)?))
            }
            _ => None,
        };
        Mqtt::new(
            "local-core",
            "mqtt",
            &self.broker,
            credentials,
            self.resources.clone(),
        )
    }
}
/// The actuable capability an MQTT binding kind carries.
pub fn mqtt_capabilities(binding: &MqttBinding) -> Vec<CommandKind> {
    match binding.kind {
        MqttKind::Switch => vec![CommandKind::SwitchSet],
        MqttKind::Light => vec![CommandKind::LightSetBrightness],
        MqttKind::Climate => vec![CommandKind::ClimateSetTemperature],
    }
}

/// An observation-only ONVIF camera. Cameras are telemetry resources and never
/// enter the actuable registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnvifCameraBinding {
    pub resource_id: String,
    /// Device service URL, e.g. `http://192.168.1.20/onvif/device_service`.
    pub endpoint: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password_ref: Option<String>,
}
impl OnvifCameraBinding {
    fn validate(&self) -> Result<(), AdapterError> {
        validate_id(&self.resource_id).map_err(|_| AdapterError::Protocol)?;
        match (&self.username, &self.password_ref) {
            (Some(_), Some(reference)) => validate_secret_ref(reference)?,
            (None, None) => {}
            _ => return Err(AdapterError::Authentication),
        }
        let placeholder = self
            .username
            .as_ref()
            .map(|username| (username.clone(), "validation-placeholder".to_owned()));
        OnvifProbe::new(&self.endpoint, placeholder)?;
        Ok(())
    }
}

fn default_poll_ms() -> u64 {
    5000
}
/// Local context-feed defaults. Both directories must be absolute; the poll
/// cadence is bounded so a typo cannot spin or stall the loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextFeedSection {
    pub watchdog_dir: String,
    pub out_dir: String,
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
}
impl ContextFeedSection {
    fn validate(&self) -> Result<(), AdapterError> {
        let dir =
            |value: &str| value.len() <= 512 && !value.is_empty() && Path::new(value).is_absolute();
        if !dir(&self.watchdog_dir) || !dir(&self.out_dir) || !(500..=60000).contains(&self.poll_ms)
        {
            return Err(AdapterError::Protocol);
        }
        Ok(())
    }
}

/// The router Sentry administers. `host` is its LAN address; `control_url` is the
/// WANIPConnection control URL when the firmware exposes UPnP IGD; `ssh` is the
/// admin channel when it exposes one. The on-site probe (`adapters::router::probe`)
/// decides what the channels actually answer; this section only names where they
/// are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouterBinding {
    pub resource_id: String,
    /// The router's LAN IPv4 address.
    pub host: String,
    /// The internal host that receives mapped ports (the internal port equals
    /// the external one; see `RouterConfig`).
    pub lan_client: String,
    /// WANIPConnection control URL, e.g. `http://192.168.1.1/.../control`.
    #[serde(default)]
    pub control_url: Option<String>,
    /// The SSH admin channel, when the firmware exposes one.
    #[serde(default)]
    pub ssh: Option<RouterSshBinding>,
    /// Hardware addresses a `DeviceBlock` may never cut off: this node, the
    /// router itself, anything the owner cannot afford to lose. Lowercase
    /// `aa:bb:cc:dd:ee:ff`.
    #[serde(default)]
    pub protected_macs: Vec<String>,
}

/// SSH access to the router: key-based only, matching `SshConfig` — no password
/// field exists here by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouterSshBinding {
    pub user: String,
    /// Absolute path of the private key file; absent means the agent/default key.
    #[serde(default)]
    pub identity_file: Option<String>,
    /// Absolute path of the ssh binary; defaults to `/usr/bin/ssh`.
    #[serde(default)]
    pub binary: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

impl RouterBinding {
    fn validate(&self) -> Result<(), AdapterError> {
        validate_id(&self.resource_id).map_err(|_| AdapterError::Protocol)?;
        if self.host.parse::<std::net::Ipv4Addr>().is_err()
            || self.lan_client.parse::<std::net::Ipv4Addr>().is_err()
        {
            return Err(AdapterError::Protocol);
        }
        if self.control_url.is_none() && self.ssh.is_none() {
            // A router with no channel can be observed by netwatch; it is not
            // something Sentry can administer, and config should say so.
            return Err(AdapterError::Protocol);
        }
        self.to_router_config().map(|_| ())
    }

    /// The adapter-facing configuration, plus the capabilities this section
    /// asserts. `igd`/`ssh` mean "configured"; whether they answer is the
    /// probe's question, and `radio_tools` stays empty until one runs — so a
    /// `WifiSet` intent against an unprobed radio is honestly `Unsupported`.
    pub fn to_router_config(&self) -> Result<(RouterConfig, RouterCapabilities), AdapterError> {
        use archie_sentry::adapters::router::IgdClient;
        let host = self
            .host
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| AdapterError::Protocol)?;
        let lan_client = self
            .lan_client
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| AdapterError::Protocol)?;
        if let Some(url) = &self.control_url {
            IgdClient::new(url)?;
        }
        let ssh = match &self.ssh {
            Some(binding) => {
                // The platform's stock ssh; an operator override names an
                // absolute path, and `SshConfig` re-checks absoluteness.
                let default_binary = if cfg!(windows) {
                    "C:/Windows/System32/OpenSSH/ssh.exe"
                } else {
                    "/usr/bin/ssh"
                };
                let binary = binding.binary.as_deref().unwrap_or(default_binary);
                let mut config =
                    SshConfig::new(binary, &binding.user, host, binding.identity_file.clone())?;
                if let Some(port) = binding.port {
                    config = config.with_port(port)?;
                }
                Some(config)
            }
            None => None,
        };
        let capabilities = RouterCapabilities {
            igd: self.control_url.is_some(),
            ssh: ssh.is_some(),
            radio_tools: Vec::new(),
            dhcp_leases_path: None,
            firmware_hint: None,
            // Asked on the router at the first block, read-only: configuration
            // cannot say which firewall the firmware ships.
            firewall_tools: Vec::new(),
        };
        if self
            .protected_macs
            .iter()
            .any(|mac| archie_sentry::contracts::validate_mac(mac).is_err())
        {
            return Err(AdapterError::Protocol);
        }
        Ok((
            RouterConfig {
                control_url: self.control_url.clone(),
                lan_client,
                ssh,
                protected_macs: self.protected_macs.clone(),
            },
            capabilities,
        ))
    }
}

/// Wake-on-LAN wiring: one subnet broadcast address, the neighbor-cache tool
/// used for the measured postcondition, and the nodes themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WolSection {
    /// `"<ipv4>"` or `"<ipv4>:<port>"` the magic packet is sent to, e.g. the
    /// subnet broadcast `192.168.1.255`.
    pub broadcast: String,
    /// Absolute path of `ip` (Linux) or `arp` (Windows): the reachability
    /// check after the packet is a neighbor-cache read, never a ping.
    pub arp_tool: String,
    pub nodes: Vec<WolNodeBinding>,
}

/// One WoL node: a name, its hardware address, and its IP for the
/// neighbor-cache postcondition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WolNodeBinding {
    pub resource_id: String,
    /// Lowercase hex with colons, e.g. `7c:10:c9:55:aa:01`.
    pub mac: String,
    pub ip: String,
}

impl WolSection {
    fn targets(&self) -> BTreeMap<String, WolTarget> {
        self.nodes
            .iter()
            .map(|n| {
                (
                    n.resource_id.clone(),
                    WolTarget {
                        mac: n.mac.clone(),
                        ip: n.ip.clone(),
                    },
                )
            })
            .collect()
    }
    /// Construction probe: bounds and address shapes live in the adapter.
    pub fn adapter(&self) -> Result<Wol, AdapterError> {
        Wol::new(
            "local-core",
            "wol",
            &self.broadcast,
            &self.arp_tool,
            self.targets(),
            Arc::new(ProcessRunner::default()),
        )
    }
}

/// Android nodes reached through adb. `adb` is the absolute binary path; the
/// runner re-checks existence before any process exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AndroidSection {
    pub adb: String,
    pub nodes: Vec<AndroidNodeBinding>,
}

/// One Android node: a name and its `adb devices` serial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AndroidNodeBinding {
    pub resource_id: String,
    /// USB serial, or `<ip>:<port>` with `network: true` for wireless debugging.
    pub serial: String,
    #[serde(default)]
    pub network: bool,
}

impl AndroidSection {
    fn targets(&self) -> BTreeMap<String, AndroidTarget> {
        self.nodes
            .iter()
            .map(|n| {
                (
                    n.resource_id.clone(),
                    AndroidTarget {
                        serial: n.serial.clone(),
                        network: n.network,
                    },
                )
            })
            .collect()
    }
    /// Construction probe: serial shapes and bounds live in the adapter.
    pub fn adapter(&self) -> Result<Adb, AdapterError> {
        Adb::new(
            "local-core",
            "adb",
            &self.adb,
            self.targets(),
            Arc::new(ProcessRunner::default()),
        )
    }
}

/// Where netwatch keeps its state, for the live alert follower
/// (`archie sentry-follow`): `alerts.jsonl`, `told.json` and `state.json` all
/// live in this one directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetwatchSection {
    pub state_dir: String,
}
impl NetwatchSection {
    fn validate(&self) -> Result<(), AdapterError> {
        if self.state_dir.is_empty()
            || self.state_dir.len() > 512
            || !Path::new(&self.state_dir).is_absolute()
        {
            return Err(AdapterError::Protocol);
        }
        Ok(())
    }
}

/// Which netwatch alerts become owner notices (`archie sentry-notices`, the
/// Observatory, the voice). Below the floor an alert is still recorded by
/// Cerebra; it only does not interrupt anyone. Blocking or quarantine is never
/// configured here: it is an approved `Intent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseSection {
    /// `notice`, `warning` (the default) or `high`.
    pub min_severity: String,
}
impl ResponseSection {
    fn validate(&self) -> Result<(), AdapterError> {
        match self.min_severity.as_str() {
            "notice" | "warning" | "high" => Ok(()),
            _ => Err(AdapterError::Protocol),
        }
    }
}

/// The chat-export drop folder Wanda ingests (`archie wanda-watch`,
/// `archie wanda-ingest`). The salt keys the sender-hash pseudonyms; it lives
/// here in local config and never in a record — a pseudonym, not a defence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatExportSection {
    pub watch_dir: String,
    /// 1..=128 bytes, matching the bridge's salt bound.
    pub salt: String,
}
impl ChatExportSection {
    fn validate(&self) -> Result<(), AdapterError> {
        if self.watch_dir.is_empty()
            || self.watch_dir.len() > 512
            || !Path::new(&self.watch_dir).is_absolute()
            || self.salt.is_empty()
            || self.salt.len() > 128
        {
            return Err(AdapterError::Protocol);
        }
        Ok(())
    }
}

/// A secret reference is `env:VARIABLE` or an absolute path to a private file.
/// Inline secrets do not exist in this configuration surface by construction.
fn validate_secret_ref(reference: &str) -> Result<(), AdapterError> {
    if let Some(variable) = reference.strip_prefix("env:") {
        let valid = !variable.is_empty()
            && variable.len() <= 64
            && variable
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && variable
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_');
        return if valid {
            Ok(())
        } else {
            Err(AdapterError::Authentication)
        };
    }
    if reference.len() <= 512 && Path::new(reference).is_absolute() {
        Ok(())
    } else {
        Err(AdapterError::Authentication)
    }
}
fn read_secret_ref(reference: &str) -> Result<String, AdapterError> {
    validate_secret_ref(reference)?;
    if let Some(variable) = reference.strip_prefix("env:") {
        return std::env::var(variable).map_err(|_| AdapterError::Authentication);
    }
    read_private_token(Path::new(reference))
}

#[cfg(target_os = "linux")]
fn read_private_token(path: &Path) -> Result<String, AdapterError> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| AdapterError::Authentication)?;
    let metadata = file.metadata().map_err(|_| AdapterError::Authentication)?;
    if !metadata.is_file() || metadata.mode() & 0o077 != 0 || metadata.len() > 4096 {
        return Err(AdapterError::Authentication);
    }
    let mut token = String::new();
    file.take(4097)
        .read_to_string(&mut token)
        .map_err(|_| AdapterError::Authentication)?;
    if token.len() > 4096 {
        return Err(AdapterError::Authentication);
    }
    Ok(token)
}
#[cfg(not(target_os = "linux"))]
fn read_private_token(_: &Path) -> Result<String, AdapterError> {
    Err(AdapterError::Authentication)
}

impl SentrySection {
    pub fn validate(&self) -> Result<(), crate::config::ConfigError> {
        let invalid = || crate::config::ConfigError::Invalid("invalid Sentry configuration".into());
        if self.printers.len() > 64
            || self.operator_uids.len() > 32
            || self.operator_uids.iter().collect::<BTreeSet<_>>().len() != self.operator_uids.len()
            || self.roku.len() > 64
            || self.onvif_cameras.len() > 64
            || self.chromecasts.len() > 64
        {
            return Err(invalid());
        }
        let mut resources = BTreeSet::new();
        for printer in &self.printers {
            validate_id(&printer.resource_id).map_err(|_| invalid())?;
            if !resources.insert(&printer.resource_id) {
                return Err(invalid());
            }
            IppClient::new(&printer.endpoint).map_err(|_| invalid())?;
            let raw = printer
                .expected_uuid
                .strip_prefix("urn:uuid:")
                .ok_or_else(invalid)?;
            if raw.len() != 36
                || !raw.bytes().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
                    }
                })
            {
                return Err(invalid());
            }
            // A spool directory makes the printer actuable; probe the exact
            // adapter construction the execution path will use.
            if let Some(spool) = &printer.spool_dir {
                if spool.len() > 512 {
                    return Err(invalid());
                }
                IppAdapter::new(
                    "local-core",
                    "ipp",
                    &printer.endpoint,
                    &printer.resource_id,
                    PathBuf::from(spool),
                )
                .map_err(|_| invalid())?;
            }
        }
        if let Some(ha) = &self.home_assistant {
            ha.validate().map_err(|_| invalid())?;
            for resource in ha.resources.iter().chain(&ha.telemetry_resources) {
                if !resources.insert(&resource.resource_id) {
                    return Err(invalid());
                }
            }
        }
        if !self.roku.is_empty() {
            let mut endpoints = BTreeMap::new();
            for roku in &self.roku {
                validate_id(&roku.resource_id).map_err(|_| invalid())?;
                if roku.endpoint.len() > 512 || !resources.insert(&roku.resource_id) {
                    return Err(invalid());
                }
                endpoints.insert(roku.resource_id.clone(), roku.endpoint.clone());
            }
            // Construction probe: endpoint policy and bounds live in the adapter.
            Roku::new("local-core", "roku", endpoints).map_err(|_| invalid())?;
        }
        if let Some(mqtt) = &self.mqtt {
            mqtt.validate().map_err(|_| invalid())?;
            for resource_id in mqtt.resources.keys() {
                if !resources.insert(resource_id) {
                    return Err(invalid());
                }
            }
        }
        for camera in &self.onvif_cameras {
            camera.validate().map_err(|_| invalid())?;
            if !resources.insert(&camera.resource_id) {
                return Err(invalid());
            }
        }
        if let Some(feed) = &self.context_feed {
            feed.validate().map_err(|_| invalid())?;
        }
        if let Some(router) = &self.router {
            router.validate().map_err(|_| invalid())?;
            if !resources.insert(&router.resource_id) {
                return Err(invalid());
            }
        }
        if let Some(wol) = &self.wol {
            if wol.nodes.len() > 256 {
                return Err(invalid());
            }
            // Construction probe: bounds and address shapes live in the adapter.
            wol.adapter().map_err(|_| invalid())?;
            for node in &wol.nodes {
                if !resources.insert(&node.resource_id) {
                    return Err(invalid());
                }
            }
        }
        if let Some(android) = &self.android {
            if android.nodes.len() > 64 {
                return Err(invalid());
            }
            android.adapter().map_err(|_| invalid())?;
            for node in &android.nodes {
                if !resources.insert(&node.resource_id) {
                    return Err(invalid());
                }
            }
        }
        for cast in &self.chromecasts {
            // Deserialization skips the constructor; rebuild through it.
            CastDevice::new(
                &cast.resource_id,
                &cast.name,
                &cast.model,
                cast.address,
                cast.port,
                &cast.certificate_sha256,
            )
            .map_err(|_| invalid())?;
            if !resources.insert(&cast.resource_id) {
                return Err(invalid());
            }
        }
        if let Some(response) = &self.response {
            response.validate().map_err(|_| invalid())?;
        }
        if let Some(netwatch) = &self.netwatch {
            netwatch.validate().map_err(|_| invalid())?;
        }
        if let Some(chat) = &self.chat_exports {
            chat.validate().map_err(|_| invalid())?;
        }
        Ok(())
    }

    /// The actuable route of a configured resource: (adapter_id, capabilities).
    /// Telemetry-only resources (HA telemetry, ONVIF cameras) are absent here
    /// by construction, and a printer without a spool directory is query-only.
    pub fn actuable_route(&self, resource_id: &str) -> Option<(&'static str, Vec<CommandKind>)> {
        if let Some(ha) = &self.home_assistant {
            if let Some(resource) = ha.resources.iter().find(|r| r.resource_id == resource_id) {
                return resource.capabilities().ok().map(|c| ("home-assistant", c));
            }
        }
        if self.roku.iter().any(|r| r.resource_id == resource_id) {
            return Some(("roku", RokuBinding::capabilities()));
        }
        if self
            .chromecasts
            .iter()
            .any(|c| c.resource_id == resource_id)
        {
            return Some(("cast", Chromecast::CAPABILITIES.to_vec()));
        }
        if self
            .printers
            .iter()
            .any(|p| p.resource_id == resource_id && p.spool_dir.is_some())
        {
            return Some(("ipp", vec![CommandKind::PrintSubmit]));
        }
        if let Some(mqtt) = &self.mqtt {
            if let Some(binding) = mqtt.resources.get(resource_id) {
                return Some(("mqtt", mqtt_capabilities(binding)));
            }
        }
        if let Some(wol) = &self.wol {
            if wol.nodes.iter().any(|n| n.resource_id == resource_id) {
                return Some(("wol", vec![CommandKind::WakeHost]));
            }
        }
        if let Some(android) = &self.android {
            if android.nodes.iter().any(|n| n.resource_id == resource_id) {
                return Some((
                    "adb",
                    vec![
                        CommandKind::AppLaunch,
                        CommandKind::SessionLock,
                        CommandKind::VolumeSet,
                    ],
                ));
            }
        }
        if let Some(router) = &self.router {
            if router.resource_id == resource_id {
                let mut capabilities = Vec::new();
                if router.control_url.is_some() {
                    capabilities.push(CommandKind::PortMapping);
                }
                if router.ssh.is_some() {
                    // The adapter declares Unsupported for the radio until an
                    // on-site probe names a tool; configuration alone does not.
                    capabilities.push(CommandKind::WifiSet);
                    // Blocking asks the router's firewall itself, read-only,
                    // before writing; a router without one says Unsupported.
                    capabilities.push(CommandKind::DeviceBlock);
                }
                if !capabilities.is_empty() {
                    return Some(("router", capabilities));
                }
            }
        }
        None
    }
}

impl SentrySection {
    /// The Chromecast adapter over every configured device.
    pub fn cast_adapter(&self) -> Result<Chromecast, AdapterError> {
        Chromecast::new("local-core", "cast", self.chromecasts.clone())
    }
}
