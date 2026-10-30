//! Read-only loopback client for the separately sandboxed world-data adapter.
//!
//! This module has no provider URL and no Internet client. `archie-displayd` can only
//! request the normalized contract from `archie-worldd` on a fixed loopback address.

use archie_types::Timestamp;
pub use archie_worldd::lan::{LanDeviceEntity, LanEntityOrigin, LanSource, LanSourceProvenance};
pub use archie_worldd::{
    EarthquakeSeverity, GeoPosition, ObservationDetails, ObservationProvenance, ProviderCondition,
    ProviderRetryState, WorldDataSource, WorldObservation, WorldProviderId, WorldProviderStatus,
    WorldSnapshot,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;

const DEFAULT_WORLD_PORT: u16 = 7667;
/// The snapshot also carries the LAN device graph (bounded at
/// `archie_worldd::lan::MAX_VIEW_BYTES` on the ingestion side), so the
/// transport bound mirrors it rather than the smaller observation-only one.
const MAX_NORMALIZED_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const LOCAL_REQUEST_TIMEOUT: Duration = Duration::from_millis(1_500);

#[derive(Clone)]
pub struct WorldService {
    endpoint: Option<SocketAddr>,
    state: Arc<RwLock<WorldSnapshot>>,
}

impl WorldService {
    /// Connect to the local adapter on the same owner-configured port as exploration.
    /// There is deliberately no provider URL or routable-address option. Invalid
    /// configuration stays unavailable instead of reading another service's data.
    pub fn live() -> Self {
        let port = match std::env::var("ARCHIE_WORLDD_PORT") {
            Ok(value) => value.parse::<u16>().ok().filter(|port| *port != 0),
            Err(std::env::VarError::NotPresent) => Some(DEFAULT_WORLD_PORT),
            Err(_) => None,
        };
        match port {
            Some(port) => Self::at_loopback(([127, 0, 0, 1], port).into()),
            None => Self {
                endpoint: None,
                state: Arc::new(RwLock::new(WorldSnapshot::service_unavailable(
                    WorldDataSource::LiveProvider,
                    Timestamp::now(),
                ))),
            },
        }
    }

    pub fn disabled(source: WorldDataSource) -> Self {
        debug_assert!(source != WorldDataSource::LiveProvider);
        Self {
            endpoint: None,
            state: Arc::new(RwLock::new(WorldSnapshot::service_unavailable(
                source,
                Timestamp::now(),
            ))),
        }
    }

    fn at_loopback(endpoint: SocketAddr) -> Self {
        assert!(
            endpoint.ip().is_loopback(),
            "world adapter must be loopback-only"
        );
        Self {
            endpoint: Some(endpoint),
            state: Arc::new(RwLock::new(WorldSnapshot::service_unavailable(
                WorldDataSource::LiveProvider,
                Timestamp::now(),
            ))),
        }
    }

    pub async fn refresh(&self) {
        let Some(endpoint) = self.endpoint else {
            return;
        };
        let result = tokio::time::timeout(LOCAL_REQUEST_TIMEOUT, fetch_normalized(endpoint)).await;
        let mut state = self.state.write().await;
        match result {
            Ok(Ok(snapshot)) => *state = snapshot,
            Ok(Err(_)) | Err(_) => {
                *state = state.clone().with_transport_failure(Timestamp::now());
            }
        }
    }

    pub async fn current(&self) -> WorldSnapshot {
        self.state.read().await.clone()
    }

    /// The LAN devices the world graph currently holds, from the graph — never
    /// from a file the display read itself.
    pub async fn lan_devices(&self) -> Vec<LanDeviceEntity> {
        self.current().await.lan_devices
    }
}

async fn fetch_normalized(endpoint: SocketAddr) -> Result<WorldSnapshot, &'static str> {
    if !endpoint.ip().is_loopback() {
        return Err("world endpoint is not loopback");
    }
    let mut stream = TcpStream::connect(endpoint)
        .await
        .map_err(|_| "world service unavailable")?;
    stream
        .write_all(b"GET /v1/world HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .map_err(|_| "world service unavailable")?;

    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8 * 1024];
    loop {
        let count = stream
            .read(&mut chunk)
            .await
            .map_err(|_| "world service unavailable")?;
        if count == 0 {
            break;
        }
        if bytes.len().saturating_add(count) > MAX_NORMALIZED_RESPONSE_BYTES {
            return Err("world response exceeded limit");
        }
        bytes.extend_from_slice(&chunk[..count]);
    }

    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("invalid world response")?;
    let head = std::str::from_utf8(&bytes[..split]).map_err(|_| "invalid world response")?;
    if !head
        .lines()
        .next()
        .is_some_and(|line| line.contains(" 200 "))
    {
        return Err("world service returned an error");
    }
    let snapshot: WorldSnapshot =
        serde_json::from_slice(&bytes[split + 4..]).map_err(|_| "invalid world contract")?;
    validate_contract(&snapshot)?;
    Ok(snapshot)
}

fn validate_contract(snapshot: &WorldSnapshot) -> Result<(), &'static str> {
    use archie_worldd::lan::{MAX_CLASSES, MAX_FIELD_CHARS, MAX_IPS, MAX_LAN_DEVICES, MAX_NAMES};

    if snapshot.schema_version != 1
        || snapshot.source != WorldDataSource::LiveProvider
        || snapshot.providers.len() > 8
        || snapshot.observations.len() > 200
        || snapshot.lan_devices.len() > MAX_LAN_DEVICES
    {
        return Err("unsupported world contract");
    }
    for observation in &snapshot.observations {
        let p = observation.position;
        if observation.id.len() > 160
            || observation.summary.chars().count() > 160
            || !p.latitude.is_finite()
            || !(-90.0..=90.0).contains(&p.latitude)
            || !p.longitude.is_finite()
            || !(-180.0..=180.0).contains(&p.longitude)
            || !p.depth_km.is_finite()
        {
            return Err("invalid normalized observation");
        }
    }
    for device in &snapshot.lan_devices {
        if device.identity.len() > 64
            || device.kind.chars().count() > MAX_FIELD_CHARS
            || device.mac.as_deref().is_some_and(|mac| mac.len() != 17)
            || device.names.len() > MAX_NAMES
            || device.classes.len() > MAX_CLASSES
            || device.ips.len() > MAX_IPS
            || device
                .names
                .iter()
                .chain(device.classes.iter())
                .any(|field| field.chars().count() > MAX_FIELD_CHARS)
        {
            return Err("invalid normalized lan device");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn configured_world_port() {
        // Each configuration runs in its own process; other async tests never see
        // a changed process environment.
        const EXPECTED: &str = "ARCHIE_TEST_WORLD_ENDPOINT";
        if let Ok(expected) = std::env::var(EXPECTED) {
            let actual = WorldService::live().endpoint.map(|value| value.to_string());
            assert_eq!(
                actual.as_deref(),
                (expected != "none").then_some(expected.as_str())
            );
            return;
        }
        for (port, expected) in [
            (None, "127.0.0.1:7667"),
            (Some("17667"), "127.0.0.1:17667"),
            (Some("0"), "none"),
            (Some("65536"), "none"),
            (Some(""), "none"),
            (Some("192.0.2.1:7667"), "none"),
            (Some("invalid"), "none"),
        ] {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "world::tests::configured_world_port",
                    "--nocapture",
                ])
                .env(EXPECTED, expected);
            match port {
                Some(port) => {
                    child.env("ARCHIE_WORLDD_PORT", port);
                }
                None => {
                    child.env_remove("ARCHIE_WORLDD_PORT");
                }
            }
            let result = child.output().unwrap();
            assert!(
                result.status.success(),
                "port {port:?}: {}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    async fn serve_once(body: Vec<u8>) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 512];
            let _ = stream.read(&mut request).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        });
        address
    }

    #[tokio::test]
    async fn only_accepts_typed_normalized_loopback_data() {
        let snapshot = WorldSnapshot::service_unavailable(
            WorldDataSource::LiveProvider,
            Timestamp::from_millis(1),
        );
        let address = serve_once(serde_json::to_vec(&snapshot).unwrap()).await;
        let service = WorldService::at_loopback(address);
        service.refresh().await;
        assert_eq!(service.current().await.schema_version, 1);
    }

    #[tokio::test]
    async fn disconnect_keeps_cache_but_marks_it_unavailable() {
        let snapshot = WorldSnapshot::service_unavailable(
            WorldDataSource::LiveProvider,
            Timestamp::from_millis(1),
        );
        let address = serve_once(serde_json::to_vec(&snapshot).unwrap()).await;
        let service = WorldService::at_loopback(address);
        service.refresh().await;
        service.refresh().await;
        let current = service.current().await;
        assert_eq!(current.providers[0].condition, ProviderCondition::Degraded);
        assert_eq!(
            current.providers[0].error.as_deref(),
            Some("world service unavailable")
        );
    }

    #[test]
    #[should_panic(expected = "loopback-only")]
    fn refuses_a_routable_adapter() {
        let _ = WorldService::at_loopback("192.0.2.1:7667".parse().unwrap());
    }

    fn snapshot_with_lan_devices() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::service_unavailable(
            WorldDataSource::LiveProvider,
            Timestamp::from_millis(1),
        );
        // The same entities `archie-worldd`'s graph produces for the fixture view.
        let mut graph = archie_worldd::lan::LanGraph::new();
        graph
            .ingest(
                include_bytes!("../../archie-worldd/tests/fixtures/unified_view_small_lan.json"),
                Timestamp::from_millis(1_790_000_200_000),
            )
            .unwrap();
        snapshot.lan_devices = graph.entities().cloned().collect();
        snapshot
    }

    #[tokio::test]
    async fn lan_devices_reach_the_display_from_the_graph() {
        let expected = snapshot_with_lan_devices();
        let address = serve_once(serde_json::to_vec(&expected).unwrap()).await;
        let service = WorldService::at_loopback(address);
        service.refresh().await;

        let devices = service.lan_devices().await;
        assert_eq!(devices.len(), 3);
        let tv = &devices[0];
        assert_eq!(tv.identity, "54:4e:f0:07:3d:e8");
        assert_eq!(tv.kind, "mdns-roku");
        assert_eq!(tv.origin, LanEntityOrigin::Measurement);
        assert_eq!(
            tv.sources.keys().copied().collect::<Vec<_>>(),
            [
                LanSource::LanScan,
                LanSource::Netwatch,
                LanSource::PassiveImport
            ]
        );
        assert_eq!(tv.ingested_at, Timestamp::from_millis(1_790_000_200_000));
        // The weak identity is visibly weak on the display side too.
        assert_eq!(devices[2].identity, "ip:192.168.1.77");
        assert_eq!(devices[2].mac, None);
    }

    #[tokio::test]
    async fn an_oversized_lan_section_fails_the_whole_contract() {
        let mut snapshot = snapshot_with_lan_devices();
        let device = snapshot.lan_devices[0].clone();
        while snapshot.lan_devices.len() <= archie_worldd::lan::MAX_LAN_DEVICES {
            snapshot.lan_devices.push(device.clone());
        }
        let address = serve_once(serde_json::to_vec(&snapshot).unwrap()).await;
        let service = WorldService::at_loopback(address);
        service.refresh().await;

        let current = service.current().await;
        assert!(current.lan_devices.is_empty(), "never a partial graph");
        assert_eq!(current.providers[0].condition, ProviderCondition::Degraded);
    }
}
