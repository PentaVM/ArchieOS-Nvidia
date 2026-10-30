//! Normalized, read-only observations about the physical world.
//!
//! Provider payloads terminate in this module. The public API contains only ArchieOS
//! types, so a renderer can never accidentally couple itself to a vendor's JSON shape
//! or render an untrusted provider field as executable markup.

use archie_types::Timestamp;
use serde::{Deserialize, Serialize};

#[cfg(feature = "provider")]
pub mod earth;
pub mod exploration;
pub mod exploration_projection;
#[cfg(feature = "provider")]
pub mod imagery;
pub mod lan;
pub mod layers;
pub mod navigation;
#[cfg(feature = "provider")]
pub mod navigation_runtime;
#[cfg(feature = "provider")]
pub mod world_signals;

#[cfg(feature = "provider")]
pub mod exploration_runtime;

#[cfg(feature = "provider")]
pub mod exploration_control;

#[cfg(feature = "provider")]
pub mod geo_formats;

#[cfg(feature = "provider")]
pub mod http;

#[cfg(feature = "provider")]
const USGS_ENDPOINT: &str =
    "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_hour.geojson";
#[cfg(feature = "provider")]
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
#[cfg(feature = "provider")]
const MAX_OBSERVATIONS: usize = 200;
#[cfg(feature = "provider")]
const STALE_AFTER_MS: i64 = 2 * 60 * 1_000;
#[cfg(feature = "provider")]
const BASE_BACKOFF_MS: i64 = 30 * 1_000;
#[cfg(feature = "provider")]
const MAX_BACKOFF_MS: i64 = 15 * 60 * 1_000;

/// Whether the world endpoint is backed by a provider, a recording, or no source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldDataSource {
    LiveProvider,
    Fixture,
    Disabled,
}

/// Stable provider identity; deliberately not a free-form string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldProviderId {
    UsgsEarthquakes,
}

/// The usefulness of the provider's current data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCondition {
    Disabled,
    Starting,
    Healthy,
    /// A last-known-good batch is visible, but it is no longer current.
    Stale,
    /// No usable batch has ever been obtained in this process.
    Degraded,
}

/// Whether another fetch is currently permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRetryState {
    Disabled,
    Ready,
    Backoff,
}

/// Observable provider state. No credentials, response bodies, or arbitrary JSON cross
/// this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldProviderStatus {
    pub provider: WorldProviderId,
    pub authority: String,
    pub dataset: String,
    pub authoritative: bool,
    pub condition: ProviderCondition,
    pub retry: ProviderRetryState,
    pub last_attempt_at: Option<Timestamp>,
    pub last_success_at: Option<Timestamp>,
    pub provider_generated_at: Option<Timestamp>,
    pub next_attempt_at: Option<Timestamp>,
    pub consecutive_failures: u32,
    pub data_age_ms: Option<i64>,
    pub error: Option<String>,
}

impl WorldProviderStatus {
    fn initial(source: WorldDataSource) -> Self {
        let disabled = source != WorldDataSource::LiveProvider;
        Self {
            provider: WorldProviderId::UsgsEarthquakes,
            authority: "U.S. Geological Survey".into(),
            dataset: "Earthquakes, past hour".into(),
            authoritative: true,
            condition: if disabled {
                ProviderCondition::Disabled
            } else {
                ProviderCondition::Starting
            },
            retry: if disabled {
                ProviderRetryState::Disabled
            } else {
                ProviderRetryState::Ready
            },
            last_attempt_at: None,
            last_success_at: None,
            provider_generated_at: None,
            next_attempt_at: None,
            consecutive_failures: 0,
            data_age_ms: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPosition {
    pub latitude: f64,
    pub longitude: f64,
    /// Positive distance below sea level, as reported by USGS.
    pub depth_km: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EarthquakeSeverity {
    Trace,
    Minor,
    Moderate,
    Strong,
    Major,
}

/// Type-specific observation values. New feeds extend this tagged enum instead of
/// adding ambiguous optional fields to every observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationDetails {
    Earthquake {
        magnitude: f64,
        severity: EarthquakeSeverity,
        felt_reports: Option<u64>,
        tsunami_warning: bool,
    },
}

/// Where one normalized record came from and when Archie observed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationProvenance {
    pub provider: WorldProviderId,
    pub authority: String,
    pub dataset: String,
    pub source_record_id: String,
    pub retrieved_at: Timestamp,
    pub provider_updated_at: Timestamp,
    pub authoritative: bool,
}

/// A provider-independent observation ready for geospatial rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldObservation {
    pub id: String,
    pub occurred_at: Timestamp,
    pub position: GeoPosition,
    pub summary: String,
    pub details: ObservationDetails,
    pub provenance: ObservationProvenance,
}

/// Atomic world-data view exposed at `GET /api/world`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub schema_version: u16,
    pub source: WorldDataSource,
    pub generated_at: Timestamp,
    pub observations: Vec<WorldObservation>,
    /// Devices on the owner's LAN, ingested from the `archie sentry-devices`
    /// unified view into the world graph (see [`lan`]). Additive to the v1
    /// contract: an older payload simply has none.
    #[serde(default)]
    pub lan_devices: Vec<lan::LanDeviceEntity>,
    pub providers: Vec<WorldProviderStatus>,
}

impl WorldSnapshot {
    fn empty(source: WorldDataSource, now: Timestamp) -> Self {
        Self {
            schema_version: 1,
            source,
            generated_at: now,
            observations: Vec::new(),
            lan_devices: Vec::new(),
            providers: vec![WorldProviderStatus::initial(source)],
        }
    }

    #[cfg(feature = "provider")]
    fn aged(mut self, now: Timestamp) -> Self {
        self.generated_at = now;
        for status in &mut self.providers {
            status.data_age_ms = status
                .last_success_at
                .map(|success| success.elapsed_to(now).max(0));
            if status.condition == ProviderCondition::Healthy
                && status.data_age_ms.is_some_and(|age| age > STALE_AFTER_MS)
            {
                status.condition = ProviderCondition::Stale;
            }
            if status.retry == ProviderRetryState::Backoff
                && status.next_attempt_at.is_some_and(|next| next <= now)
            {
                status.retry = ProviderRetryState::Ready;
            }
        }
        self
    }

    /// Honest initial view for a local consumer that cannot yet reach this daemon.
    pub fn service_unavailable(source: WorldDataSource, now: Timestamp) -> Self {
        let mut snapshot = Self::empty(source, now);
        if source == WorldDataSource::LiveProvider {
            let status = &mut snapshot.providers[0];
            status.condition = ProviderCondition::Degraded;
            status.retry = ProviderRetryState::Backoff;
            status.error = Some("world service unavailable".into());
        }
        snapshot
    }

    /// Keep last-known-good normalized records, but never present them as fresh when
    /// the local adapter cannot be reached.
    pub fn with_transport_failure(mut self, now: Timestamp) -> Self {
        self.generated_at = now;
        for status in &mut self.providers {
            status.condition = if self.observations.is_empty() {
                ProviderCondition::Degraded
            } else {
                ProviderCondition::Stale
            };
            status.retry = ProviderRetryState::Backoff;
            status.error = Some("world service unavailable".into());
            status.data_age_ms = status
                .last_success_at
                .map(|success| success.elapsed_to(now).max(0));
        }
        self
    }
}

#[cfg(feature = "provider")]
pub use provider::WorldService;

#[cfg(feature = "provider")]
mod provider {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::RwLock;

    #[derive(Debug)]
    struct ProviderBatch {
        provider_generated_at: Timestamp,
        observations: Vec<WorldObservation>,
    }

    #[derive(Debug, Clone, Copy)]
    struct FetchError(&'static str);

    trait WorldProvider: Send + Sync {
        fn fetch(
            &self,
            retrieved_at: Timestamp,
        ) -> Pin<Box<dyn Future<Output = Result<ProviderBatch, FetchError>> + Send + '_>>;
    }

    struct UsgsEarthquakeProvider {
        client: reqwest::Client,
    }

    impl UsgsEarthquakeProvider {
        fn build() -> Result<Self, FetchError> {
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(4))
                .user_agent("ArchieOS-Nvidia/0.1 (+https://github.com/PentaVM/ArchieOS-Nvidia)")
                .build()
                .map_err(|_| FetchError("world provider client could not be initialized"))?;
            Ok(Self { client })
        }

        async fn fetch_inner(&self, retrieved_at: Timestamp) -> Result<ProviderBatch, FetchError> {
            let mut response = self
                .client
                .get(USGS_ENDPOINT)
                .send()
                .await
                .map_err(classify_network_error)?;
            if !response.status().is_success() {
                return Err(FetchError("world provider returned a non-success status"));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
            {
                return Err(FetchError(
                    "world provider response exceeded the size limit",
                ));
            }

            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(classify_network_error)? {
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err(FetchError(
                        "world provider response exceeded the size limit",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            parse_usgs(&bytes, retrieved_at)
        }
    }

    impl WorldProvider for UsgsEarthquakeProvider {
        fn fetch(
            &self,
            retrieved_at: Timestamp,
        ) -> Pin<Box<dyn Future<Output = Result<ProviderBatch, FetchError>> + Send + '_>> {
            Box::pin(self.fetch_inner(retrieved_at))
        }
    }

    fn classify_network_error(error: reqwest::Error) -> FetchError {
        if error.is_timeout() {
            FetchError("world provider request timed out")
        } else {
            // Do not propagate URLs, response fragments or transport internals to the renderer.
            FetchError("world provider request failed")
        }
    }

    #[derive(Debug, Deserialize)]
    struct UsgsFeed {
        metadata: UsgsMetadata,
        features: Vec<UsgsFeature>,
    }

    #[derive(Debug, Deserialize)]
    struct UsgsMetadata {
        generated: i64,
    }

    #[derive(Debug, Deserialize)]
    struct UsgsFeature {
        id: String,
        properties: UsgsProperties,
        geometry: UsgsGeometry,
    }

    #[derive(Debug, Deserialize)]
    struct UsgsProperties {
        mag: Option<f64>,
        place: Option<String>,
        time: Option<i64>,
        updated: Option<i64>,
        felt: Option<u64>,
        tsunami: Option<u8>,
    }

    #[derive(Debug, Deserialize)]
    struct UsgsGeometry {
        coordinates: Vec<f64>,
    }

    fn parse_usgs(bytes: &[u8], retrieved_at: Timestamp) -> Result<ProviderBatch, FetchError> {
        let feed: UsgsFeed = serde_json::from_slice(bytes)
            .map_err(|_| FetchError("world provider payload was invalid"))?;
        let mut observations: Vec<_> = feed
            .features
            .into_iter()
            .filter_map(|feature| normalize_usgs(feature, retrieved_at))
            .collect();
        observations.sort_by(|a, b| {
            b.occurred_at
                .cmp(&a.occurred_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        let mut seen = std::collections::BTreeSet::new();
        observations.retain(|observation| seen.insert(observation.id.clone()));
        observations.truncate(MAX_OBSERVATIONS);
        Ok(ProviderBatch {
            provider_generated_at: Timestamp::from_millis(feed.metadata.generated),
            observations,
        })
    }

    fn normalize_usgs(feature: UsgsFeature, retrieved_at: Timestamp) -> Option<WorldObservation> {
        let source_id = safe_identifier(&feature.id)?;
        let magnitude = feature.properties.mag?;
        let occurred_at = Timestamp::from_millis(feature.properties.time?);
        let updated_at = Timestamp::from_millis(feature.properties.updated?);
        let coordinates = feature.geometry.coordinates;
        let (longitude, latitude, depth_km) = (
            *coordinates.first()?,
            *coordinates.get(1)?,
            *coordinates.get(2)?,
        );

        if !magnitude.is_finite()
            || !(-2.0..=12.0).contains(&magnitude)
            || !longitude.is_finite()
            || !(-180.0..=180.0).contains(&longitude)
            || !latitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !depth_km.is_finite()
            || !(-20.0..=1_000.0).contains(&depth_km)
        {
            return None;
        }

        let authority = "U.S. Geological Survey".to_string();
        let dataset = "Earthquakes, past hour".to_string();
        Some(WorldObservation {
            id: format!("usgs:{source_id}"),
            occurred_at,
            position: GeoPosition {
                latitude,
                longitude,
                depth_km,
            },
            summary: safe_text(
                feature
                    .properties
                    .place
                    .as_deref()
                    .unwrap_or("Unknown location"),
                160,
            ),
            details: ObservationDetails::Earthquake {
                magnitude,
                severity: severity_for(magnitude),
                felt_reports: feature.properties.felt,
                tsunami_warning: feature.properties.tsunami == Some(1),
            },
            provenance: ObservationProvenance {
                provider: WorldProviderId::UsgsEarthquakes,
                authority,
                dataset,
                source_record_id: source_id,
                retrieved_at,
                provider_updated_at: updated_at,
                authoritative: true,
            },
        })
    }

    fn safe_identifier(value: &str) -> Option<String> {
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return None;
        }
        Some(value.to_string())
    }

    fn safe_text(value: &str, max_chars: usize) -> String {
        value
            .chars()
            .filter(|ch| !ch.is_control())
            .take(max_chars)
            .collect()
    }

    fn severity_for(magnitude: f64) -> EarthquakeSeverity {
        if magnitude < 2.0 {
            EarthquakeSeverity::Trace
        } else if magnitude < 4.0 {
            EarthquakeSeverity::Minor
        } else if magnitude < 6.0 {
            EarthquakeSeverity::Moderate
        } else if magnitude < 8.0 {
            EarthquakeSeverity::Strong
        } else {
            EarthquakeSeverity::Major
        }
    }

    /// Owns the bounded provider client, last-known-good cache and retry state.
    #[derive(Clone)]
    pub struct WorldService {
        provider: Option<Arc<dyn WorldProvider>>,
        state: Arc<RwLock<WorldSnapshot>>,
    }

    impl WorldService {
        pub fn live_usgs() -> Self {
            match UsgsEarthquakeProvider::build() {
                Ok(provider) => Self::with_provider(Arc::new(provider)),
                Err(error) => {
                    let now = Timestamp::now();
                    let mut snapshot = WorldSnapshot::empty(WorldDataSource::LiveProvider, now);
                    let status = &mut snapshot.providers[0];
                    status.condition = ProviderCondition::Degraded;
                    status.error = Some(error.0.into());
                    Self {
                        provider: None,
                        state: Arc::new(RwLock::new(snapshot)),
                    }
                }
            }
        }

        pub fn disabled(source: WorldDataSource) -> Self {
            debug_assert!(source != WorldDataSource::LiveProvider);
            Self {
                provider: None,
                state: Arc::new(RwLock::new(WorldSnapshot::empty(source, Timestamp::now()))),
            }
        }

        fn with_provider(provider: Arc<dyn WorldProvider>) -> Self {
            Self {
                provider: Some(provider),
                state: Arc::new(RwLock::new(WorldSnapshot::empty(
                    WorldDataSource::LiveProvider,
                    Timestamp::now(),
                ))),
            }
        }

        /// Attempt one refresh. A failure never clears the last-known-good observations.
        pub async fn refresh(&self) {
            self.refresh_at(Timestamp::now()).await;
        }

        async fn refresh_at(&self, now: Timestamp) {
            let Some(provider) = &self.provider else {
                return;
            };
            {
                let snapshot = self.state.read().await;
                if snapshot.providers[0]
                    .next_attempt_at
                    .is_some_and(|next| next > now)
                {
                    return;
                }
            }

            let result = provider.fetch(now).await;
            let mut snapshot = self.state.write().await;
            match result {
                Ok(batch) => {
                    snapshot.observations = batch.observations;
                    let status = &mut snapshot.providers[0];
                    status.last_attempt_at = Some(now);
                    status.condition = ProviderCondition::Healthy;
                    status.retry = ProviderRetryState::Ready;
                    status.last_success_at = Some(now);
                    status.provider_generated_at = Some(batch.provider_generated_at);
                    status.next_attempt_at = Some(now.plus_millis(60_000));
                    status.consecutive_failures = 0;
                    status.data_age_ms = Some(0);
                    status.error = None;
                }
                Err(error) => {
                    let status = &mut snapshot.providers[0];
                    status.last_attempt_at = Some(now);
                    status.consecutive_failures = status.consecutive_failures.saturating_add(1);
                    let shift = status.consecutive_failures.saturating_sub(1).min(20);
                    let backoff = BASE_BACKOFF_MS
                        .saturating_mul(1_i64 << shift)
                        .min(MAX_BACKOFF_MS);
                    status.next_attempt_at = Some(now.plus_millis(backoff));
                    status.retry = ProviderRetryState::Backoff;
                    status.error = Some(error.0.into());
                    status.condition = if status.last_success_at.is_some() {
                        ProviderCondition::Stale
                    } else {
                        ProviderCondition::Degraded
                    };
                    status.data_age_ms = status
                        .last_success_at
                        .map(|success| success.elapsed_to(now).max(0));
                }
            }
            snapshot.generated_at = now;
        }

        /// Ingest one `archie-sentry-devices-v1` document (as written by
        /// `archie sentry-devices --json`) into the world graph. Atomic: a
        /// rejected view changes nothing, and a provider refresh never touches
        /// the device entities — the two flows meet only in the snapshot.
        pub async fn ingest_lan_view(
            &self,
            view: &[u8],
        ) -> Result<lan::LanIngestReport, lan::LanIngestError> {
            self.ingest_lan_view_at(view, Timestamp::now()).await
        }

        async fn ingest_lan_view_at(
            &self,
            view: &[u8],
            now: Timestamp,
        ) -> Result<lan::LanIngestReport, lan::LanIngestError> {
            let mut graph = lan::LanGraph::new();
            let report = graph.ingest(view, now)?;
            let mut snapshot = self.state.write().await;
            snapshot.lan_devices = graph.entities().cloned().collect();
            snapshot.generated_at = now;
            Ok(report)
        }

        pub async fn current(&self) -> WorldSnapshot {
            self.current_at(Timestamp::now()).await
        }

        async fn current_at(&self, now: Timestamp) -> WorldSnapshot {
            self.state.read().await.clone().aged(now)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::collections::VecDeque;
        use std::sync::Mutex;

        const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/usgs_all_hour.geojson");

        struct ScriptedProvider {
            results: Mutex<VecDeque<Result<ProviderBatch, FetchError>>>,
        }

        impl WorldProvider for ScriptedProvider {
            fn fetch(
                &self,
                _retrieved_at: Timestamp,
            ) -> Pin<Box<dyn Future<Output = Result<ProviderBatch, FetchError>> + Send + '_>>
            {
                Box::pin(async move {
                    self.results
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or(Err(FetchError("script exhausted")))
                })
            }
        }

        fn batch(retrieved_at: Timestamp) -> ProviderBatch {
            parse_usgs(FIXTURE, retrieved_at).unwrap()
        }

        #[test]
        fn fixture_is_normalized_and_sorted_without_vendor_json() {
            let retrieved = Timestamp::from_millis(1_725_000_010_000);
            let batch = batch(retrieved);
            assert_eq!(batch.observations.len(), 2);
            assert_eq!(batch.observations[0].id, "usgs:us7000test2");
            assert_eq!(batch.observations[1].id, "usgs:ci407test1");
            assert_eq!(batch.observations[0].position.longitude, -71.25);
            assert_eq!(batch.observations[0].provenance.retrieved_at, retrieved);
            assert!(batch.observations[0].provenance.authoritative);

            let public = serde_json::to_string(&batch.observations).unwrap();
            assert!(!public.contains("\"url\""));
            assert!(!public.contains("geojson"));
            assert!(!public.contains("example.invalid"));
        }

        #[test]
        fn malformed_coordinates_and_identifiers_are_dropped() {
            let invalid = br#"{
          "metadata":{"generated":1725000000000},
          "features":[
            {"id":"bad/id","properties":{"mag":4.0,"place":"x","time":1,"updated":1,"felt":null,"tsunami":0},"geometry":{"coordinates":[0,0,2]}},
            {"id":"valid","properties":{"mag":4.0,"place":"x","time":1,"updated":1,"felt":null,"tsunami":0},"geometry":{"coordinates":[500,0,2]}}
          ]
        }"#;
            assert!(parse_usgs(invalid, Timestamp::from_millis(2))
                .unwrap()
                .observations
                .is_empty());
        }

        #[tokio::test]
        async fn failure_keeps_cache_marks_it_stale_and_applies_backoff() {
            let t0 = Timestamp::from_millis(1_000_000);
            let provider = Arc::new(ScriptedProvider {
                results: Mutex::new(VecDeque::from([
                    Ok(batch(t0)),
                    Err(FetchError("controlled failure")),
                    Ok(batch(t0.plus_millis(90_000))),
                ])),
            });
            let service = WorldService::with_provider(provider);

            service.refresh_at(t0).await;
            let fresh = service.current_at(t0).await;
            assert_eq!(fresh.providers[0].condition, ProviderCondition::Healthy);
            assert_eq!(fresh.observations.len(), 2);

            // The successful poll cadence prevents an early request.
            service.refresh_at(t0.plus_millis(59_999)).await;
            assert_eq!(
                service.current_at(t0).await.providers[0].last_attempt_at,
                Some(t0)
            );

            service.refresh_at(t0.plus_millis(60_000)).await;
            let stale = service.current_at(t0.plus_millis(60_000)).await;
            assert_eq!(stale.providers[0].condition, ProviderCondition::Stale);
            assert_eq!(stale.providers[0].retry, ProviderRetryState::Backoff);
            assert_eq!(stale.providers[0].consecutive_failures, 1);
            assert_eq!(stale.observations.len(), 2, "last good data must survive");

            // First failure backs off for exactly 30 seconds.
            service.refresh_at(t0.plus_millis(89_999)).await;
            assert_eq!(
                service.current_at(t0).await.providers[0].consecutive_failures,
                1
            );
            service.refresh_at(t0.plus_millis(90_000)).await;
            let recovered = service.current_at(t0.plus_millis(90_000)).await;
            assert_eq!(recovered.providers[0].condition, ProviderCondition::Healthy);
            assert_eq!(recovered.providers[0].consecutive_failures, 0);
            assert!(recovered.providers[0].error.is_none());
        }

        #[tokio::test]
        async fn first_failure_is_degraded_and_offline_safe() {
            let provider = Arc::new(ScriptedProvider {
                results: Mutex::new(VecDeque::from([Err(FetchError("offline"))])),
            });
            let service = WorldService::with_provider(provider);
            service.refresh_at(Timestamp::from_millis(1_000)).await;
            let current = service.current_at(Timestamp::from_millis(1_001)).await;
            assert_eq!(current.providers[0].condition, ProviderCondition::Degraded);
            assert_eq!(current.providers[0].retry, ProviderRetryState::Backoff);
            assert!(current.observations.is_empty());
        }

        #[tokio::test]
        async fn old_success_becomes_stale_even_without_an_explicit_error() {
            let t0 = Timestamp::from_millis(1_000);
            let provider = Arc::new(ScriptedProvider {
                results: Mutex::new(VecDeque::from([Ok(batch(t0))])),
            });
            let service = WorldService::with_provider(provider);
            service.refresh_at(t0).await;
            let aged = service.current_at(t0.plus_millis(STALE_AFTER_MS + 1)).await;
            assert_eq!(aged.providers[0].condition, ProviderCondition::Stale);
            assert_eq!(aged.providers[0].data_age_ms, Some(STALE_AFTER_MS + 1));
        }

        #[tokio::test]
        async fn a_lan_view_lands_in_the_served_snapshot_and_survives_refresh() {
            const VIEW: &[u8] = include_bytes!("../tests/fixtures/unified_view_small_lan.json");
            let t0 = Timestamp::from_millis(1_000_000);
            let provider = Arc::new(ScriptedProvider {
                results: Mutex::new(VecDeque::from([Ok(batch(t0))])),
            });
            let service = WorldService::with_provider(provider);

            let report = service.ingest_lan_view_at(VIEW, t0).await.unwrap();
            assert_eq!(report.devices, 3);
            let current = service.current_at(t0).await;
            assert_eq!(current.lan_devices.len(), 3);
            assert_eq!(current.lan_devices[0].identity, "54:4e:f0:07:3d:e8");
            assert_eq!(
                current.lan_devices[0]
                    .sources
                    .keys()
                    .copied()
                    .collect::<Vec<_>>(),
                [
                    crate::lan::LanSource::LanScan,
                    crate::lan::LanSource::Netwatch,
                    crate::lan::LanSource::PassiveImport
                ]
            );

            // Provider refreshes and LAN ingestions are independent flows.
            service.refresh_at(t0).await;
            let after_refresh = service.current_at(t0).await;
            assert_eq!(
                after_refresh.lan_devices.len(),
                3,
                "refresh does not touch the graph"
            );
            assert_eq!(
                after_refresh.observations.len(),
                2,
                "ingestion does not touch observations"
            );

            // A rejected view leaves the served devices untouched.
            assert!(service.ingest_lan_view_at(b"junk", t0).await.is_err());
            assert_eq!(service.current_at(t0).await.lan_devices.len(), 3);
        }
    }
}
