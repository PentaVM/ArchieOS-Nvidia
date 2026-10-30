//! Native Aleph-style capture execution, owned by worldd, independent of UI lifetime.
//! Upstream reference: TheLoki2020/aleph ad68fc4 (MIT, Belluxx), see third_party/aleph.
//! Provider limits are application admission controls, never OS packet shaping.
use crate::exploration::*;
use crate::geo_formats::{self, DecodeLimits, GeoAnchor, GeoBounds};
use archie_geofabric::exploration::{self, ExplorationDomain, ExplorationLayer, ExplorationPlan};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

const MAX_JOBS: usize = 32;
const MAX_ACTIVE: usize = 4;
const MAX_RESPONSE: u64 = 16 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 128 * 1024 * 1024;

pub fn now_ms() -> i64 {
    archie_types::Timestamp::now().as_millis()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Ready,
    WaitUntil(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLedger {
    requests_per_minute: u32,
    max_in_flight: u32,
    in_flight: u32,
    window_start_ms: Option<i64>,
    window_requests: u32,
    requests_sent: u64,
    bytes_received: u64,
    retry_at_ms: Option<i64>,
    remaining_requests: Option<u64>,
}
impl RateLedger {
    pub fn new(requests_per_minute: u32, max_in_flight: u32) -> Self {
        Self {
            requests_per_minute: requests_per_minute.max(1),
            max_in_flight: max_in_flight.max(1),
            in_flight: 0,
            window_start_ms: None,
            window_requests: 0,
            requests_sent: 0,
            bytes_received: 0,
            retry_at_ms: None,
            remaining_requests: None,
        }
    }
    pub fn recover_after_restart(&mut self) {
        self.in_flight = 0;
    }
    pub fn admit(&mut self, now: i64) -> Admission {
        if let Some(until) = self.retry_at_ms.filter(|t| *t > now) {
            return Admission::WaitUntil(until);
        }
        self.retry_at_ms = None;
        let start = *self.window_start_ms.get_or_insert(now);
        if now >= start.saturating_add(60_000) {
            self.window_start_ms = Some(now);
            self.window_requests = 0;
        }
        if self.window_requests >= self.requests_per_minute {
            return Admission::WaitUntil(self.window_start_ms.unwrap().saturating_add(60_000));
        }
        if self.in_flight >= self.max_in_flight {
            return Admission::WaitUntil(now.saturating_add(100));
        }
        self.window_requests += 1;
        self.requests_sent = self.requests_sent.saturating_add(1);
        self.in_flight += 1;
        Admission::Ready
    }
    pub fn finish(&mut self, bytes: u64, remaining: Option<u64>, retry: Option<i64>) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.bytes_received = self.bytes_received.saturating_add(bytes);
        if let Some(n) = remaining {
            self.remaining_requests = Some(n);
            if n == 0 && retry.is_none() {
                self.retry_at_ms = Some(
                    self.window_start_ms
                        .unwrap_or_else(now_ms)
                        .saturating_add(60_000)
                        .max(now_ms() + 1000),
                );
            }
        }
        if let Some(t) = retry {
            self.retry_at_ms = Some(self.retry_at_ms.map_or(t, |old| old.max(t)));
        }
    }
    pub fn snapshot(&self, id: &str) -> ProviderUsage {
        let local_wait = self
            .window_start_ms
            .map(|start| start.saturating_add(60_000))
            .filter(|until| self.window_requests >= self.requests_per_minute && *until > now_ms());
        let retry = self
            .retry_at_ms
            .filter(|until| *until > now_ms())
            .into_iter()
            .chain(local_wait)
            .max();
        ProviderUsage {
            id: id.into(),
            state: if retry.is_some() {
                "waiting_rate_limit"
            } else if self.in_flight > 0 {
                "fetching"
            } else {
                "ready"
            }
            .into(),
            requests_sent: self.requests_sent,
            bytes_received: self.bytes_received,
            requests_per_minute: self.requests_per_minute,
            max_in_flight: self.max_in_flight,
            in_flight: self.in_flight,
            remaining_requests: self.remaining_requests,
            retry_at_ms: retry,
            quota_source: if self.remaining_requests.is_some() {
                "provider"
            } else {
                "local"
            }
            .into(),
        }
    }
}

#[derive(Clone)]
pub struct ExplorationConfig {
    pub cache_dir: PathBuf,
    pub domain: ExplorationDomain,
}

#[derive(Clone)]
struct Endpoints {
    terrain: String,
    osm: String,
    geocoder: String,
}
impl Default for Endpoints {
    fn default() -> Self {
        Self {
            terrain: "https://elevation-tiles-prod.s3.amazonaws.com/geotiff".into(),
            osm: "https://overpass-api.de/api/interpreter".into(),
            geocoder: "https://photon.komoot.io/api/".into(),
        }
    }
}

struct State {
    jobs: Vec<ExplorationJob>,
    rates: BTreeMap<String, RateLedger>,
    workers: BTreeSet<String>,
}

// An execute timeout or explicit job cancellation must release concurrency, while
// retaining the already-recorded request and provider cooldown.
struct RequestPermit {
    state: Arc<Mutex<State>>,
    provider: String,
    armed: bool,
}
impl Drop for RequestPermit {
    fn drop(&mut self) {
        if self.armed {
            let state = self.state.clone();
            let provider = self.provider.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    if let Some(rate) = state.lock().await.rates.get_mut(&provider) {
                        rate.finish(0, None, None);
                    }
                });
            }
        }
    }
}
#[derive(Clone)]
pub struct ExplorationService {
    pub(crate) navigation: Arc<Mutex<crate::navigation_runtime::NavigationState>>,
    config: ExplorationConfig,
    client: reqwest::Client,
    state: Arc<Mutex<State>>,
    endpoints: Endpoints,
    cache_locks: Arc<Mutex<BTreeMap<String, std::sync::Weak<Mutex<()>>>>>,
    cache_writer: Arc<Mutex<()>>,
}

fn domain_name(domain: ExplorationDomain) -> &'static str {
    match domain {
        ExplorationDomain::Personal => "personal",
        ExplorationDomain::Work => "work",
    }
}
fn default_rates() -> BTreeMap<String, RateLedger> {
    [
        ("aws_terrain_tiles".into(), RateLedger::new(120, 4)),
        ("openstreetmap".into(), RateLedger::new(6, 1)),
        ("photon".into(), RateLedger::new(30, 1)),
        ("google_satellite".into(), RateLedger::new(180, 4)),
        ("google_earth".into(), RateLedger::new(180, 4)),
        ("nasa_gibs".into(), RateLedger::new(120, 4)),
        ("usgs_earthquakes".into(), RateLedger::new(12, 1)),
        ("nasa_eonet".into(), RateLedger::new(6, 1)),
        ("adsb_lol".into(), RateLedger::new(12, 1)),
    ]
    .into()
}

pub fn private_directory(path: &Path) -> Result<(), String> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("refusing symlink directory".into());
    }
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let temporary = path.with_extension(format!("tmp-{:016x}", rand::random::<u64>()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| e.to_string())
}

impl ExplorationService {
    pub fn new(config: ExplorationConfig) -> Result<Self, String> {
        private_directory(&config.cache_dir)?;
        let root = config.cache_dir.join(domain_name(config.domain));
        private_directory(&root)?;
        private_directory(&root.join("cache"))?;
        let mut jobs = Vec::new();
        for item in std::fs::read_dir(&root)
            .map_err(|e| e.to_string())?
            .flatten()
            .take(MAX_JOBS + 16)
        {
            if item.file_name().to_string_lossy().starts_with("job-")
                && item.file_type().is_ok_and(|t| t.is_dir())
            {
                let p = item.path().join("manifest.json");
                if std::fs::metadata(&p).is_ok_and(|m| m.len() < 2 * 1024 * 1024) {
                    if let Ok(bytes) = std::fs::read(p) {
                        if let Ok(mut j) = serde_json::from_slice::<ExplorationJob>(&bytes) {
                            if j.domain != config.domain
                                || !valid_id(&j.id)
                                || exploration::plan(&j.request).is_err()
                            {
                                continue;
                            }
                            if matches!(
                                j.state,
                                JobState::Queued | JobState::Running | JobState::WaitingRateLimit
                            ) {
                                j.state = JobState::Paused;
                                j.error = Some("daemon restarted; explicit resume required".into());
                            }
                            jobs.push(j);
                            if jobs.len() >= MAX_JOBS {
                                break;
                            }
                        }
                    }
                }
            }
        }
        jobs.sort_by_key(|j| j.created_at_ms);
        let rates_path = root.join("usage.json");
        let mut rates = if std::fs::metadata(&rates_path).is_ok_and(|m| m.len() < 64 * 1024) {
            std::fs::read(rates_path)
                .ok()
                .and_then(|b| serde_json::from_slice::<BTreeMap<String, RateLedger>>(&b).ok())
                .unwrap_or_else(default_rates)
        } else {
            default_rates()
        };
        for rate in rates.values_mut() {
            rate.recover_after_restart();
        }
        for (id, rate) in default_rates() {
            rates.entry(id).or_insert(rate);
        }
        Ok(Self {
            navigation: Arc::new(Mutex::new(
                crate::navigation_runtime::NavigationState::default(),
            )),
            config,
            client: reqwest::Client::builder()
                .user_agent("ArchieOS/0.1 (+https://github.com/PentaVM/ArchieOS-Nvidia)")
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                .build()
                .map_err(|e| e.to_string())?,
            state: Arc::new(Mutex::new(State {
                jobs,
                rates,
                workers: BTreeSet::new(),
            })),
            endpoints: Endpoints::default(),
            cache_locks: Arc::new(Mutex::new(BTreeMap::new())),
            cache_writer: Arc::new(Mutex::new(())),
        })
    }
    fn root(&self) -> PathBuf {
        self.config.cache_dir.join(domain_name(self.config.domain))
    }
    fn save_job(&self, job: &ExplorationJob) -> Result<(), String> {
        let dir = self.root().join(format!("job-{}", job.id));
        private_directory(&dir)?;
        let mut persisted = serde_json::to_value(job).map_err(|e| e.to_string())?;
        crate::imagery::strip_online_pixels(&mut persisted);
        let bytes = serde_json::to_vec(&persisted).map_err(|e| e.to_string())?;
        if bytes.len() >= 2 * 1024 * 1024 {
            return Err("capture manifest exceeds its 2MiB bound".into());
        }
        atomic_write(&dir.join("manifest.json"), &bytes)
    }
    fn save_rates(&self, state: &State) -> Result<(), String> {
        atomic_write(
            &self.root().join("usage.json"),
            &serde_json::to_vec(&state.rates).map_err(|e| e.to_string())?,
        )
    }
    pub async fn snapshot(&self) -> ExplorationSnapshot {
        let state = self.state.lock().await;
        self.project_snapshot(&state, None)
    }
    /// Select an existing in-memory capture without scheduling or reading sources.
    pub async fn snapshot_for(&self, id: &str) -> Option<ExplorationSnapshot> {
        if !valid_id(id) {
            return None;
        }
        let state = self.state.lock().await;
        if !state.jobs.iter().any(|job| job.id == id) {
            return None;
        }
        Some(self.project_snapshot(&state, Some(id)))
    }
    fn project_snapshot(&self, state: &State, selected: Option<&str>) -> ExplorationSnapshot {
        let mut jobs = state.jobs.clone();
        let shown = selected.or_else(|| state.jobs.last().map(|job| job.id.as_str()));
        for job in &mut jobs {
            let keep_preview = shown == Some(job.id.as_str());
            job.projection_truncated = !job.plan.tiles.is_empty()
                || job.artifacts.len() > 16
                || !keep_preview && job.preview.is_some();
            job.plan.tiles.clear();
            job.artifacts.truncate(16);
            if !keep_preview {
                job.preview = None;
            }
        }
        ExplorationSnapshot {
            schema_version: 1,
            source: "live".into(),
            generated_at_ms: now_ms(),
            domain: self.config.domain,
            jobs,
            selected_job_id: selected.map(str::to_owned),
            providers: state.rates.iter().map(|(id, r)| r.snapshot(id)).collect(),
            error: None,
        }
    }
    fn check_domain(&self, domain: ExplorationDomain) -> Result<(), String> {
        if domain == self.config.domain {
            Ok(())
        } else {
            Err("domain differs from this world daemon".into())
        }
    }
    pub(crate) fn check_navigation_domain(&self, domain: ExplorationDomain) -> Result<(), String> {
        self.check_domain(domain)
    }
    pub(crate) async fn navigation_provider_usage(&self, provider: &str) -> Option<ProviderUsage> {
        self.state
            .lock()
            .await
            .rates
            .get(provider)
            .map(|rate| rate.snapshot(provider))
    }
    pub(crate) fn navigation_cached_time(&self, url: &str) -> Option<i64> {
        self.cached_retrieval_time(url)
    }
    fn checked_plan(
        &self,
        request: &exploration::ExplorationRequest,
    ) -> Result<ExplorationPlan, String> {
        self.check_domain(request.domain)?;
        let mut plan = exploration::plan(request).map_err(|e| e.to_string())?;
        if request.layers.contains(&ExplorationLayer::Terrain) && request.zoom > 14 {
            return Err("AWS terrain is available at zoom 0..14; choose a supported detail".into());
        }
        if plan.bounds.west > plan.bounds.east {
            return Err("split antimeridian captures into two areas before downloading".into());
        }
        for layer in &request.layers {
            if !matches!(
                layer,
                ExplorationLayer::Terrain
                    | ExplorationLayer::Buildings
                    | ExplorationLayer::Imagery
                    | ExplorationLayer::Signals
            ) {
                return Err(format!(
                    "{layer:?}: source capture/storage requires a compatible licensed adapter"
                ));
            }
            if matches!(layer, ExplorationLayer::Imagery | ExplorationLayer::Signals) {
                continue;
            }
            let source = match layer {
                ExplorationLayer::Terrain => archie_geofabric::GeoProviderId::AwsTerrainTiles,
                _ => archie_geofabric::GeoProviderId::OpenStreetMap,
            };
            let provenance = archie_geofabric::TileProvenance::from_provider(source, None);
            provenance.may_be_cached().map_err(|e| e.to_string())?;
            provenance
                .may_be_reasoned_about()
                .map_err(|e| e.to_string())?;
        }
        plan.estimated_requests = plan.tiles.len() as u64
            * u64::from(request.layers.contains(&ExplorationLayer::Terrain))
            + u64::from(request.layers.contains(&ExplorationLayer::Buildings));
        if request.layers.contains(&ExplorationLayer::Imagery) {
            let date = crate::imagery::effective_date(request.imagery_date.as_deref(), now_ms())?;
            for source in crate::imagery::selected_sources(request) {
                plan.estimated_requests +=
                    crate::imagery::source_tiles(request, source, &date)?.len() as u64;
            }
        }
        if request.layers.contains(&ExplorationLayer::Signals) {
            plan.estimated_requests += u64::from(request.signal_updates) * 3;
        }
        Ok(plan)
    }
    async fn command(&self, command: ExploreCommand) -> Result<Value, String> {
        match command {
            ExploreCommand::Plan { request } => Ok(json!({"plan":self.checked_plan(&request)?})),
            ExploreCommand::Status { domain } => {
                self.check_domain(domain)?;
                Ok(json!(self.snapshot().await))
            }
            ExploreCommand::Resolve { query, domain } => {
                self.check_domain(domain)?;
                self.resolve(&query).await
            }
            ExploreCommand::Explore { request } => {
                let plan = self.checked_plan(&request)?;
                let mut state = self.state.lock().await;
                if state.jobs.len() >= MAX_JOBS {
                    return Err("capture history limit reached; archive completed captures before starting more".into());
                }
                if state
                    .jobs
                    .iter()
                    .filter(|j| {
                        matches!(
                            j.state,
                            JobState::Queued | JobState::Running | JobState::WaitingRateLimit
                        )
                    })
                    .count()
                    >= MAX_ACTIVE
                {
                    return Err("four exploration jobs already active".into());
                }
                let total = plan.estimated_requests;
                let pinned_imagery_date = if request.layers.contains(&ExplorationLayer::Imagery) {
                    Some(crate::imagery::effective_date(
                        request.imagery_date.as_deref(),
                        now_ms(),
                    )?)
                } else {
                    None
                };
                let job = ExplorationJob {
                    id: format!("{:032x}", rand::random::<u128>()),
                    domain: request.domain,
                    state: JobState::Queued,
                    camera: plan.camera,
                    request,
                    tiles_total: total,
                    tiles_done: 0,
                    bytes_received: 0,
                    stored_bytes: 0,
                    signal_updates_done: 0,
                    pinned_imagery_date,
                    cache_hits: 0,
                    error: None,
                    artifacts: vec![],
                    preview: None,
                    created_at_ms: now_ms(),
                    updated_at_ms: now_ms(),
                    plan,
                    projection_truncated: false,
                };
                self.save_job(&job)?;
                let id = job.id.clone();
                state.jobs.push(job.clone());
                drop(state);
                self.spawn(id);
                Ok(json!({"job":job}))
            }
            ExploreCommand::Pause { id, domain }
            | ExploreCommand::Resume { id, domain }
            | ExploreCommand::Cancel { id, domain } => {
                // Kept below as separate action classification before borrowing the job.
                let _ = (id, domain);
                Err("job control must use execute".into())
            }
            ExploreCommand::Navigate { request } => self.start_navigation(request).await,
            ExploreCommand::NavigationStatus { id, domain } => {
                self.navigation_status(&id, domain, false).await
            }
            ExploreCommand::NavigationStop { id, domain } => {
                self.navigation_status(&id, domain, true).await
            }
        }
    }
    pub async fn execute(&self, command: ExploreCommand) -> Result<Value, String> {
        let (id, domain, desired) = match &command {
            ExploreCommand::Pause { id, domain } => (id.clone(), *domain, JobState::Paused),
            ExploreCommand::Resume { id, domain } => (id.clone(), *domain, JobState::Queued),
            ExploreCommand::Cancel { id, domain } => (id.clone(), *domain, JobState::Cancelled),
            _ => return self.command(command).await,
        };
        self.check_domain(domain)?;
        if !valid_id(&id) {
            return Err("invalid exploration id".into());
        }
        let mut state = self.state.lock().await;
        if desired == JobState::Queued && state.workers.contains(&id) {
            return Err("previous worker is stopping; retry resume shortly".into());
        }
        if desired == JobState::Queued
            && state
                .jobs
                .iter()
                .filter(|j| {
                    matches!(
                        j.state,
                        JobState::Queued | JobState::Running | JobState::WaitingRateLimit
                    )
                })
                .count()
                >= MAX_ACTIVE
        {
            return Err("four exploration jobs already active".into());
        }
        let job = state
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("unknown exploration id")?;
        if desired == JobState::Queued && !matches!(job.state, JobState::Paused | JobState::Failed)
        {
            return Err("only paused or failed captures can resume".into());
        }
        if desired == JobState::Paused
            && matches!(job.state, JobState::Complete | JobState::Cancelled)
        {
            return Err("capture already terminal".into());
        }
        job.state = desired;
        job.updated_at_ms = now_ms();
        job.error = None;
        self.save_job(job)?;
        let answer = json!({"job":job});
        drop(state);
        if desired == JobState::Queued {
            self.spawn(id);
        }
        Ok(answer)
    }
    fn spawn(&self, id: String) {
        let service = self.clone();
        tokio::spawn(async move {
            {
                let mut state = service.state.lock().await;
                if !state.workers.insert(id.clone()) {
                    return;
                }
            }
            if let Err(error) = service.run(&id).await {
                let mut state = service.state.lock().await;
                if let Some(job) = state.jobs.iter_mut().find(|j| j.id == id) {
                    if !matches!(job.state, JobState::Paused | JobState::Cancelled) {
                        job.state = JobState::Failed;
                        job.error = Some(error);
                        job.updated_at_ms = now_ms();
                        let _ = service.save_job(job);
                    }
                }
            }
            service.state.lock().await.workers.remove(&id);
        });
    }
    async fn active(&self, id: &str) -> Result<(), String> {
        let state = self.state.lock().await;
        match state.jobs.iter().find(|j| j.id == id).map(|j| j.state) {
            Some(JobState::Paused | JobState::Cancelled | JobState::Complete) | None => {
                Err("capture stopped by explicit command".into())
            }
            _ => Ok(()),
        }
    }
    async fn stopped(&self, id: Option<&str>) {
        let Some(id) = id else {
            std::future::pending::<()>().await;
            return;
        };
        loop {
            if self.active(id).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    async fn mark(&self, id: &str, next: JobState) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let j = state
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("unknown job")?;
        if matches!(j.state, JobState::Paused | JobState::Cancelled) {
            return Err("capture explicitly stopped".into());
        }
        if j.state == next {
            return Ok(());
        }
        j.state = next;
        j.updated_at_ms = now_ms();
        self.save_job(j)
    }
    async fn fetch(
        &self,
        id: Option<&str>,
        provider: &str,
        url: &str,
        budget: u64,
    ) -> Result<(Vec<u8>, bool), String> {
        self.fetch_policy(
            id,
            provider,
            url,
            budget,
            Some(Duration::from_secs(7 * 86400)),
        )
        .await
    }
    /// None forbids all disk reads/writes for this response, including cache replay.
    async fn fetch_policy(
        &self,
        id: Option<&str>,
        provider: &str,
        url: &str,
        budget: u64,
        cache_ttl: Option<Duration>,
    ) -> Result<(Vec<u8>, bool), String> {
        self.fetch_metered(id, provider, url, budget, cache_ttl, None)
            .await
    }
    pub(crate) async fn fetch_navigation(
        &self,
        provider: &str,
        url: &str,
        budget: u64,
        cache_ttl: Option<Duration>,
        meter: &crate::navigation_runtime::NavigationMeter,
    ) -> Result<(Vec<u8>, bool), String> {
        self.fetch_metered(None, provider, url, budget, cache_ttl, Some(meter))
            .await
    }
    async fn fetch_stopped(
        &self,
        id: Option<&str>,
        meter: Option<&crate::navigation_runtime::NavigationMeter>,
    ) {
        if let Some(meter) = meter {
            tokio::select! {_=self.stopped(id)=>{},_=meter.stopped()=>{}}
        } else {
            self.stopped(id).await;
        }
    }
    #[allow(clippy::too_many_arguments)]
    async fn fetch_metered(
        &self,
        id: Option<&str>,
        provider: &str,
        url: &str,
        budget: u64,
        cache_ttl: Option<Duration>,
        meter: Option<&crate::navigation_runtime::NavigationMeter>,
    ) -> Result<(Vec<u8>, bool), String> {
        if let Some(meter) = meter {
            meter.active()?;
        }
        // Provider policy is authoritative even if a caller requests a cache.
        let cache_ttl = if matches!(provider, "google_satellite" | "google_earth") {
            None
        } else {
            cache_ttl
        };
        let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
        let key_lock = {
            let mut locks = self.cache_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&hash).and_then(std::sync::Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(hash.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        let _same_resource = tokio::select! {guard=key_lock.lock_owned()=>guard,_=self.fetch_stopped(id,meter)=>return Err("world request explicitly stopped or expired".into())};
        let cache = self.root().join("cache").join(hash);
        if let Some(ttl) = cache_ttl {
            if let Ok(metadata) = std::fs::metadata(&cache) {
                if metadata.is_file()
                    && metadata.len() <= MAX_RESPONSE
                    && metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age < ttl)
                {
                    if let Ok(bytes) = std::fs::read(&cache) {
                        return Ok((bytes, true));
                    }
                    // Another resource's bounded-cache eviction may race metadata.
                    // An evicted object is a miss, not a failed capture.
                }
            }
        }
        if budget == 0 {
            return Err("capture network byte budget exhausted".into());
        }
        for attempt in 0..4 {
            loop {
                if let Some(meter) = meter {
                    meter.active()?;
                }
                if let Some(id) = id {
                    self.active(id).await?;
                }
                let decision = {
                    let mut state = self.state.lock().await;
                    let decision = state
                        .rates
                        .get_mut(provider)
                        .ok_or("unknown provider")?
                        .admit(now_ms());
                    if decision == Admission::Ready {
                        if let Some(meter) = meter {
                            if let Err(error) = meter.reserve_request() {
                                let rate = state.rates.get_mut(provider).unwrap();
                                rate.window_requests = rate.window_requests.saturating_sub(1);
                                rate.requests_sent = rate.requests_sent.saturating_sub(1);
                                rate.in_flight = rate.in_flight.saturating_sub(1);
                                return Err(error);
                            }
                        }
                        if let Err(error) = self.save_rates(&state) {
                            state.rates.get_mut(provider).unwrap().finish(0, None, None);
                            return Err(error);
                        }
                    }
                    decision
                };
                match decision {
                    Admission::Ready => break,
                    Admission::WaitUntil(until) => {
                        if let Some(id) = id {
                            self.mark(id, JobState::WaitingRateLimit).await?;
                        }
                        tokio::time::sleep(Duration::from_millis(
                            (until - now_ms()).clamp(1, 250) as u64
                        ))
                        .await;
                    }
                }
            }
            let mut permit = RequestPermit {
                state: self.state.clone(),
                provider: provider.into(),
                armed: true,
            };
            if let Some(id) = id {
                self.mark(id, JobState::Running).await?;
            }
            let outcome = async {
                let mut response=tokio::select! {
                    response=self.client.get(url).send()=>response.map_err(|_|"provider request failed".to_string())?,
                    _=self.fetch_stopped(id,meter)=>return Err("world request explicitly stopped or expired".into()),
                };
                let status = response.status();
                let remaining = ["ratelimit-remaining", "x-ratelimit-remaining"]
                    .iter()
                    .find_map(|h| {
                        response
                            .headers()
                            .get(*h)
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                    });
                let retry = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| retry_after(v, now_ms()));
                if status.as_u16() == 429 || status.is_server_error() {
                    return Ok((
                        Vec::new(),
                        remaining,
                        Some(retry.unwrap_or_else(|| now_ms() + 1000 * (1i64 << attempt))),
                        true,
                    ));
                }
                if !status.is_success() {
                    return Err(format!("provider HTTP {}", status.as_u16()));
                }
                let limit = budget.min(MAX_RESPONSE);
                if response.content_length().is_some_and(|n| n > limit) {
                    return Err("provider response exceeds remaining byte budget".into());
                }
                let mut bytes = Vec::new();
                loop {
                    let chunk=tokio::select!{
                        chunk=response.chunk()=>chunk.map_err(|_|"provider stream failed".to_string())?,
                        _=self.fetch_stopped(id,meter)=>return Err("world request explicitly stopped or expired".into()),
                    };
                    let Some(chunk)=chunk else {break;};
                    let navigation_budget=if let Some(meter)=meter{meter.charge_bytes(chunk.len() as u64)}else{Ok(())};
                    if let Some(id) = id {
                        self.active(id).await?;
                    }
                    {
                        let mut state = self.state.lock().await;
                        state.rates.get_mut(provider).unwrap().bytes_received = state.rates
                            [provider]
                            .bytes_received
                            .saturating_add(chunk.len() as u64);
                        if let Some(id) = id {
                            let job = state
                                .jobs
                                .iter_mut()
                                .find(|j| j.id == id)
                                .ok_or("unknown job")?;
                            job.bytes_received =
                                job.bytes_received.saturating_add(chunk.len() as u64);
                            self.save_job(job)?;
                        }
                        self.save_rates(&state)?;
                    }
                    navigation_budget?;
                    if bytes.len().saturating_add(chunk.len()) as u64 > limit {
                        return Err("capture byte budget exceeded".into());
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok::<_, String>((bytes, remaining, retry, false))
            }
            .await;
            {
                let mut state = self.state.lock().await;
                let (remaining, retry) = outcome
                    .as_ref()
                    .map(|(_, r, t, _)| (*r, *t))
                    .unwrap_or((None, None));
                state
                    .rates
                    .get_mut(provider)
                    .unwrap()
                    .finish(0, remaining, retry);
                permit.armed = false;
                self.save_rates(&state)?;
            }
            match outcome {
                Ok((_, _, _, true)) if attempt < 3 => continue,
                Ok((_, _, _, true)) => {
                    return Err("provider retries exhausted; cooldown retained".into())
                }
                Ok((bytes, _, _, false)) => {
                    validate_payload(provider, &bytes)?;
                    if cache_ttl.is_some() {
                        let _writer = self.cache_writer.lock().await;
                        trim_cache(cache.parent().unwrap(), bytes.len() as u64, MAX_CACHE_BYTES)?;
                        atomic_write(&cache, &bytes)?;
                    }
                    return Ok((bytes, false));
                }
                Err(e) => return Err(e),
            }
        }
        Err("provider retries exhausted".into())
    }
    async fn resolve(&self, query: &str) -> Result<Value, String> {
        if query.trim().is_empty() || query.chars().count() > 256 {
            return Err("place query must contain 1..256 characters".into());
        }
        let mut url = reqwest::Url::parse(&self.endpoints.geocoder).map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", "5");
        let (bytes, cached) = self
            .fetch(None, "photon", url.as_str(), 1024 * 1024)
            .await?;
        let raw: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid geocoder response")?;
        let places=raw["features"].as_array().ok_or("invalid geocoder features")?.iter().take(5).filter_map(|f|{
            let c=f["geometry"]["coordinates"].as_array()?;let lon=c.first()?.as_f64()?;let lat=c.get(1)?.as_f64()?;
            if !lat.is_finite()|| !lon.is_finite()||lat.abs()>90.||lon.abs()>180.{return None;}
            Some(json!({"latitude":lat,"longitude":lon,"name":f["properties"]["name"].as_str().unwrap_or("").chars().take(200).collect::<String>(),"source":"OpenStreetMap / Photon"}))
        }).collect::<Vec<_>>();
        Ok(json!({"places":places,"cache_hit":cached}))
    }
    async fn run(&self, id: &str) -> Result<(), String> {
        self.mark(id, JobState::Running).await?;
        let job = self
            .state
            .lock()
            .await
            .jobs
            .iter()
            .find(|job| job.id == id)
            .cloned()
            .ok_or("unknown job")?;
        let mut errors = Vec::new();
        if job.request.layers.iter().any(|layer| {
            matches!(
                layer,
                ExplorationLayer::Terrain | ExplorationLayer::Buildings
            )
        }) {
            if let Err(error) = self.run_geometry(id).await {
                self.active(id).await?;
                if fatal_budget(&error) {
                    return Err(error);
                }
                errors.push(format!("geometry: {error}"));
                self.merge_preview(id, "geometry_error", json!(error))
                    .await?;
            }
        }
        if job.request.layers.contains(&ExplorationLayer::Imagery) {
            errors.extend(self.capture_imagery(id, &job).await?);
        }
        if job.request.layers.contains(&ExplorationLayer::Signals) {
            errors.extend(self.capture_signals(id, &job).await?);
        }
        {
            let mut state = self.state.lock().await;
            let job = state
                .jobs
                .iter_mut()
                .find(|job| job.id == id)
                .ok_or("unknown job")?;
            job.error = if errors.is_empty() {
                None
            } else {
                Some(errors.join("; ").chars().take(2000).collect())
            };
            self.save_job(job)?;
        }
        self.mark(id, JobState::Complete).await
    }

    async fn merge_preview(&self, id: &str, key: &str, value: Value) -> Result<(), String> {
        self.active(id).await?;
        let mut state = self.state.lock().await;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or("unknown job")?;
        let preview = job.preview.get_or_insert_with(
            || json!({"bounds":job.plan.bounds,"row_projection":"web_mercator"}),
        );
        preview[key] = value;
        job.updated_at_ms = now_ms();
        self.save_job(job)
    }

    async fn progress_unit(&self, id: &str) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let job = state
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or("unknown job")?;
        job.tiles_done = job.tiles_done.saturating_add(1).min(job.tiles_total);
        self.save_job(job)
    }

    async fn capture_imagery(&self, id: &str, job: &ExplorationJob) -> Result<Vec<String>, String> {
        let date = job
            .pinned_imagery_date
            .clone()
            .unwrap_or(crate::imagery::effective_date(
                job.request.imagery_date.as_deref(),
                job.created_at_ms,
            )?);
        let sources = crate::imagery::selected_sources(&job.request);
        let mut remaining_inline = crate::imagery::INLINE_BUDGET;
        let mut remaining_tiles = sources.iter().try_fold(0usize, |count, source| {
            crate::imagery::source_tiles(&job.request, *source, &date)
                .map(|tiles| count + tiles.len())
        })?;
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        for source in sources {
            self.active(id).await?;
            let spec = crate::imagery::spec(source);
            let requests = crate::imagery::source_tiles(&job.request, source, &date)?;
            let mut entry = json!({"source_id":spec.id,"label":spec.label,"attribution":spec.attribution,
                "imagery_date":if spec.daily {json!(date)} else if spec.online_only {Value::Null} else {json!("static")},
                "requested_date":job.request.imagery_date,"date_selection":if job.request.imagery_date.is_some(){"requested"}else{"UTC_minus_three_days_default"},
                "date_applied":spec.daily,"retrieved_at_ms":Value::Null,
                "mode":if spec.online_only{"online_preview_only"}else{"open_data"},"state":"loading","error":Value::Null,"tiles":[]});
            let mut tiles = Vec::new();
            let mut source_errors = Vec::new();
            for request in requests {
                self.active(id).await?;
                let budget = self.remaining(id).await?;
                if budget == 0 {
                    return Err("capture network byte budget exhausted".into());
                }
                let filename = format!(
                    "imagery-{}-{}-{}-{}-{}.{}",
                    spec.id,
                    if spec.daily { date.as_str() } else { "static" },
                    request.tile.zoom,
                    request.tile.x,
                    request.tile.y,
                    spec.extension
                );
                let result = if spec.online_only {
                    self.fetch_policy(Some(id), "google_satellite", &request.url, budget, None)
                        .await
                } else {
                    self.source_bytes(id, &filename, "nasa_gibs", &request.url)
                        .await
                };
                match result {
                    Ok((bytes, cached)) => {
                        let retrieved = if cached {
                            self.cached_retrieval_time(&request.url)
                        } else {
                            Some(now_ms())
                        };
                        entry["retrieved_at_ms"] = json!(retrieved);
                        if spec.online_only {
                            self.progress_unit(id).await?;
                        } else {
                            self.artifact(
                                id,
                                "imagery",
                                if spec.extension == "png" {
                                    "png"
                                } else {
                                    "jpeg"
                                },
                                &filename,
                                &bytes,
                                spec.attribution,
                                cached,
                            )
                            .await?;
                        }
                        // Keep native encoding when space permits, reserving enough
                        // for small previews of tiles still needed for AOI coverage.
                        let reserved = remaining_tiles.saturating_sub(1) * 24 * 1024;
                        let available = remaining_inline.saturating_sub(reserved);
                        let tile_budget = if bytes.len().div_ceil(3) * 4 <= available {
                            available
                        } else {
                            remaining_inline / remaining_tiles.max(1)
                        };
                        match crate::imagery::encode_preview(&bytes, tile_budget) {
                            Ok(preview) => {
                                remaining_inline =
                                    remaining_inline.saturating_sub(preview.data_base64.len());
                                let span = (request.bounds.east - request.bounds.west)
                                    .abs()
                                    .to_radians()
                                    * 6_371_008.8
                                    * job.request.latitude.to_radians().cos();
                                tiles.push(json!({"bounds":request.bounds,"mime":preview.mime,"data_base64":preview.data_base64,
                                    "width":preview.width,"height":preview.height,"zoom":request.tile.zoom,
                                    "source_resolution_m":span/f64::from(preview.source_width),"preview_resolution_m":span/f64::from(preview.width)}));
                            }
                            Err(error) => source_errors.push(error),
                        }
                    }
                    Err(error) => {
                        self.active(id).await?;
                        if fatal_budget(&error) {
                            return Err(error);
                        }
                        source_errors.push(error);
                    }
                }
                remaining_tiles = remaining_tiles.saturating_sub(1);
                entry["tiles"] = json!(tiles);
                entries.retain(|old: &Value| old["source_id"] != spec.id);
                let mut partial = entry.clone();
                partial["state"] = json!("loading");
                entries.push(partial);
                self.merge_preview(id, "imagery", json!(entries)).await?;
            }
            let state = if source_errors.is_empty() {
                "ready"
            } else if !tiles.is_empty() {
                "partial"
            } else if source_errors
                .iter()
                .all(|error| error.contains("404") || error.contains("no-data"))
            {
                "no_data"
            } else if source_errors
                .iter()
                .all(|error| error.contains("inline byte cap"))
            {
                "preview_omitted"
            } else {
                "error"
            };
            entry["state"] = json!(state);
            entry["tiles"] = json!(tiles);
            if !source_errors.is_empty() {
                let error = source_errors.join("; ");
                entry["error"] = json!(error);
                errors.push(format!("{}: {}", spec.id, error));
            }
            entries.retain(|old: &Value| old["source_id"] != spec.id);
            entries.push(entry);
            self.merge_preview(id, "imagery", json!(entries)).await?;
        }
        Ok(errors)
    }

    fn cached_retrieval_time(&self, url: &str) -> Option<i64> {
        let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
        let modified = std::fs::metadata(self.root().join("cache").join(hash))
            .ok()?
            .modified()
            .ok()?;
        i64::try_from(
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis(),
        )
        .ok()
    }

    async fn capture_signals(
        &self,
        id: &str,
        initial: &ExplorationJob,
    ) -> Result<Vec<String>, String> {
        let mut errors = Vec::new();
        loop {
            self.active(id).await?;
            let job = self
                .state
                .lock()
                .await
                .jobs
                .iter()
                .find(|job| job.id == id)
                .cloned()
                .ok_or("unknown job")?;
            if job.signal_updates_done >= job.request.signal_updates {
                break;
            }
            let previous = job
                .preview
                .as_ref()
                .and_then(|preview| preview.get("signals"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            if let Some(until) = previous["next_update_at_ms"]
                .as_i64()
                .filter(|until| *until > now_ms())
            {
                tokio::select! {_=tokio::time::sleep(Duration::from_millis((until-now_ms()).max(1)as u64))=>{},_=self.stopped(Some(id))=>return Err("capture explicitly stopped".into())}
            }
            errors.clear();
            let requests = crate::world_signals::requests(
                initial.request.latitude,
                initial.request.longitude,
                initial.request.radius_m,
            );
            let mut sources = Vec::new();
            let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
            for request in &requests {
                let old = previous["points"]
                    .as_array()
                    .map(|points| {
                        points
                            .iter()
                            .filter(|point| point["source_id"] == request.provider)
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                groups.insert(request.provider.clone(), old);
            }
            for request in requests {
                self.active(id).await?;
                let budget = self.remaining(id).await?;
                if budget == 0 {
                    return Err("capture network byte budget exhausted".into());
                }
                let old = previous["sources"]
                    .as_array()
                    .and_then(|sources| {
                        sources
                            .iter()
                            .find(|source| source["id"] == request.provider)
                    })
                    .cloned();
                let mut source=old.clone().unwrap_or_else(||json!({"id":request.provider,"label":request.label,"retrieved_at_ms":Value::Null,"seen":0,"rejected":0,"truncated":false}));
                let had_success = old
                    .as_ref()
                    .is_some_and(|source| source["retrieved_at_ms"].as_i64().is_some());
                let result = self
                    .fetch_policy(
                        Some(id),
                        &request.provider,
                        &request.url,
                        budget,
                        Some(Duration::from_secs(request.ttl_seconds)),
                    )
                    .await;
                match result {
                    Ok((bytes, cached)) => match crate::world_signals::decode(
                        &request.provider,
                        &bytes,
                        initial.request.latitude,
                        initial.request.longitude,
                        initial.request.radius_m,
                    ) {
                        Ok(batch) => {
                            let filename = format!(
                                "signals-{}-{}.json",
                                job.signal_updates_done + 1,
                                request.provider
                            );
                            self.artifact(
                                id,
                                "signals",
                                "json",
                                &filename,
                                &bytes,
                                &request.label,
                                cached,
                            )
                            .await?;
                            groups.insert(
                                request.provider.clone(),
                                batch
                                    .points
                                    .iter()
                                    .filter_map(|point| serde_json::to_value(point).ok())
                                    .collect(),
                            );
                            source["state"] = json!(if cached {
                                "cached"
                            } else if batch.points.is_empty() {
                                "no_data"
                            } else {
                                "ready"
                            });
                            source["error"] = Value::Null;
                            source["retrieved_at_ms"] = json!(if cached {
                                self.cached_retrieval_time(&request.url)
                            } else {
                                Some(now_ms())
                            });
                            source["seen"] = json!(batch.seen);
                            source["rejected"] = json!(batch.rejected);
                            source["truncated"] = json!(batch.truncated);
                            source["matched_total"] = json!(batch.matched_total);
                            source["omitted_points"] = json!(batch.omitted_points);
                            source["total_is_lower_bound"] = json!(batch.total_is_lower_bound);
                        }
                        Err(error) => {
                            source["state"] = json!(if had_success { "stale" } else { "error" });
                            source["error"] = json!(error);
                            errors.push(format!("{}: {}", request.provider, error));
                        }
                    },
                    Err(error) => {
                        self.active(id).await?;
                        if fatal_budget(&error) {
                            return Err(error);
                        }
                        source["state"] = json!(if had_success { "stale" } else { "error" });
                        source["error"] = json!(error);
                        errors.push(format!("{}: {}", request.provider, error));
                    }
                }
                sources.push(source);
            }
            let mut points = Vec::new();
            let maximum = groups.values().map(Vec::len).max().unwrap_or(0);
            'points: for index in 0..maximum {
                for group in groups.values() {
                    if let Some(point) = group.get(index) {
                        points.push(point.clone());
                        if points.len() == 128 {
                            break 'points;
                        }
                    }
                }
            }
            let offered = groups.values().map(Vec::len).sum::<usize>();
            let mut truncated =
                offered > points.len() || sources.iter().any(|source| source["truncated"] == true);
            while serde_json::to_vec(&points)
                .map_err(|e| e.to_string())?
                .len()
                > 128 * 1024
            {
                points.pop();
                truncated = true;
            }
            let mut points_total = 0u64;
            let mut total_is_lower_bound = false;
            for source in &sources {
                match source["matched_total"].as_u64() {
                    Some(total) => points_total = points_total.saturating_add(total),
                    None => {
                        let retained = source["id"]
                            .as_str()
                            .and_then(|id| groups.get(id))
                            .map_or(0, Vec::len);
                        points_total = points_total.saturating_add(retained as u64);
                        total_is_lower_bound = true;
                    }
                }
                total_is_lower_bound |= source["total_is_lower_bound"].as_bool().unwrap_or(true);
            }
            let omitted = points_total.saturating_sub(points.len() as u64);
            truncated |= omitted > 0;
            let seen = sources
                .iter()
                .filter_map(|source| source["seen"].as_u64())
                .sum::<u64>();
            let rejected = sources
                .iter()
                .filter_map(|source| source["rejected"].as_u64())
                .sum::<u64>();
            let done = job.signal_updates_done + 1;
            let next = if done < job.request.signal_updates {
                Some(now_ms() + i64::from(job.request.signal_interval_seconds) * 1000)
            } else {
                None
            };
            {
                let mut state = self.state.lock().await;
                let job = state
                    .jobs
                    .iter_mut()
                    .find(|job| job.id == id)
                    .ok_or("unknown job")?;
                if matches!(
                    job.state,
                    JobState::Paused | JobState::Cancelled | JobState::Complete
                ) {
                    return Err("capture explicitly stopped".into());
                }
                // One checkpoint binds the last-good view to its completed cycle.
                // A restart must not replay a cycle whose view was already saved.
                let preview = job.preview.get_or_insert_with(
                    || json!({"bounds":job.plan.bounds,"row_projection":"web_mercator"}),
                );
                preview["signals"] = json!({"sources":sources,"points":points,"seen":seen,"rejected":rejected,"truncated":truncated,
                    "points_total":points_total,"omitted_points":omitted,"total_is_lower_bound":total_is_lower_bound,
                    "updates_done":done,"updates_total":job.request.signal_updates,"next_update_at_ms":next});
                job.signal_updates_done = done;
                job.updated_at_ms = now_ms();
                self.save_job(job)?;
            }
        }
        Ok(errors)
    }

    async fn run_geometry(&self, id: &str) -> Result<(), String> {
        self.mark(id, JobState::Running).await?;
        let job = self
            .state
            .lock()
            .await
            .jobs
            .iter()
            .find(|j| j.id == id)
            .cloned()
            .ok_or("unknown job")?;
        let bounds = GeoBounds {
            south: job.plan.bounds.south,
            west: job.plan.bounds.west,
            north: job.plan.bounds.north,
            east: job.plan.bounds.east,
        };
        let limits = DecodeLimits::default();
        let mut buildings = vec![];
        let mut terrain = geo_formats::TerrainPreview {
            width: 33,
            height: 33,
            elevations_m: vec![None; 33 * 33],
        };
        if job.request.layers.contains(&ExplorationLayer::Buildings) {
            if bounds.west > bounds.east {
                return Err(
                    "building query crossing antimeridian requires two separate captures".into(),
                );
            }
            let query = format!(
                "[out:json][timeout:25];way[building]({},{},{},{});out geom;",
                bounds.south, bounds.west, bounds.north, bounds.east
            );
            let mut url = reqwest::Url::parse(&self.endpoints.osm).map_err(|e| e.to_string())?;
            url.query_pairs_mut().append_pair("data", &query);
            let (bytes, cache) = self
                .source_bytes(id, "buildings.json", "openstreetmap", url.as_str())
                .await?;
            buildings = geo_formats::parse_overpass_buildings(&bytes, &limits)?;
            self.artifact(
                id,
                "buildings",
                "osm_json",
                "buildings.json",
                &bytes,
                "© OpenStreetMap contributors · ODbL",
                cache,
            )
            .await?;
            self.preview(id, &bounds, None, &buildings).await?;
        }
        if job.request.layers.contains(&ExplorationLayer::Terrain) {
            for tile in &job.plan.tiles {
                self.active(id).await?;
                let filename = format!("terrain-{}-{}-{}.tif", tile.zoom, tile.x, tile.y);
                let url = format!(
                    "{}/{}/{}/{}.tif",
                    self.endpoints.terrain, tile.zoom, tile.x, tile.y
                );
                let (bytes, cache) = self
                    .source_bytes(id, &filename, "aws_terrain_tiles", &url)
                    .await?;
                let preview = geo_formats::decode_terrain_tiff(&bytes, &limits)?;
                let b = tile_bounds(tile.zoom, tile.x, tile.y);
                self.artifact(
                    id,
                    "terrain",
                    "geotiff",
                    &filename,
                    &bytes,
                    "AWS Terrain Tiles · source attribution: tilezen/joerd",
                    cache,
                )
                .await?;
                merge_terrain(&mut terrain, &bounds, &preview, &b);
                self.preview(id, &bounds, Some(&terrain), &buildings)
                    .await?;
            }
        }
        let export_bounds = bounds;
        let dem = job
            .request
            .layers
            .contains(&ExplorationLayer::Terrain)
            .then_some(&terrain);
        let geojson = geo_formats::export_geojson(export_bounds, dem, &buildings)?;
        self.export(id, "capture.geojson", "geojson", &geojson)
            .await?;
        let glb = geo_formats::export_glb(
            GeoAnchor {
                lon: job.request.longitude,
                lat: job.request.latitude,
            },
            export_bounds,
            dem,
            &buildings,
            None,
        );
        match glb {
            Ok(bytes) => self.export(id, "capture.glb", "glb", &bytes).await?,
            Err(error) if error == "no triangulatable geometry for GLB" => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }
    async fn remaining(&self, id: &str) -> Result<u64, String> {
        let state = self.state.lock().await;
        let job = state
            .jobs
            .iter()
            .find(|j| j.id == id)
            .ok_or("unknown job")?;
        job.request
            .max_bytes
            .checked_sub(job.bytes_received)
            .ok_or("capture byte budget exhausted".into())
    }
    async fn source_bytes(
        &self,
        id: &str,
        name: &str,
        provider: &str,
        url: &str,
    ) -> Result<(Vec<u8>, bool), String> {
        let saved = {
            let state = self.state.lock().await;
            state
                .jobs
                .iter()
                .find(|j| j.id == id)
                .is_some_and(|j| j.artifacts.iter().any(|a| a.relative_path == name))
        };
        if saved {
            let path = self.root().join(format!("job-{id}")).join(name);
            if path
                .symlink_metadata()
                .is_ok_and(|m| m.is_file() && m.len() <= MAX_RESPONSE)
            {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                validate_payload(provider, &bytes)?;
                return Ok((bytes, true));
            }
        }
        self.fetch(Some(id), provider, url, self.remaining(id).await?)
            .await
    }
    async fn preview(
        &self,
        id: &str,
        bounds: &GeoBounds,
        terrain: Option<&geo_formats::TerrainPreview>,
        buildings: &[geo_formats::BuildingFeature],
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let job = state
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("unknown job")?;
        let mut vertices = 0usize;
        let shown = buildings
            .iter()
            .filter(|b| {
                if b.ring.len() > 512 || vertices + b.ring.len() > 4096 {
                    return false;
                }
                vertices += b.ring.len();
                true
            })
            .take(128)
            .collect::<Vec<_>>();
        let view = job.preview.get_or_insert_with(|| json!({}));
        view["bounds"] = json!(bounds);
        view["terrain"] = json!(terrain);
        view["buildings"] = json!(shown);
        view["omitted_buildings"] = json!(buildings.len() - shown.len());
        view["row_projection"] = json!("web_mercator");
        job.camera.elevation_m = terrain
            .and_then(|t| t.elevations_m.iter().flatten().copied().reduce(f32::max))
            .map(f64::from);
        self.save_job(job)
    }
    #[allow(clippy::too_many_arguments)]
    async fn artifact(
        &self,
        id: &str,
        layer: &str,
        kind: &str,
        name: &str,
        bytes: &[u8],
        source: &str,
        cached: bool,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let job = state
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("unknown job")?;
        self.store_artifact_bytes(job, name, bytes)?;
        if !job.artifacts.iter().any(|a| a.relative_path == name) {
            job.tiles_done += 1;
            job.artifacts.push(Artifact {
                layer: layer.into(),
                kind: kind.into(),
                relative_path: name.into(),
                source: source.into(),
                retrieved_at_ms: now_ms(),
            });
        }
        if cached {
            job.cache_hits += 1;
        }
        job.updated_at_ms = now_ms();
        self.save_job(job)
    }
    async fn export(&self, id: &str, name: &str, kind: &str, bytes: &[u8]) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let job = state
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("unknown job")?;
        self.store_artifact_bytes(job, name, bytes)?;
        job.artifacts.retain(|a| a.relative_path != name);
        job.artifacts.push(Artifact {
            layer: "export".into(),
            kind: kind.into(),
            relative_path: name.into(),
            source: "ArchieOS normalized capture".into(),
            retrieved_at_ms: now_ms(),
        });
        self.save_job(job)
    }
    fn store_artifact_bytes(
        &self,
        job: &mut ExplorationJob,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let path = self.root().join(format!("job-{}", job.id)).join(name);
        let old = std::fs::symlink_metadata(&path)
            .ok()
            .filter(|m| m.is_file())
            .map_or(0, |m| m.len());
        let stored = job
            .stored_bytes
            .saturating_sub(old)
            .saturating_add(bytes.len() as u64);
        if stored > job.request.max_bytes {
            return Err(
                "capture storage byte budget exceeded (includes cached files and exports)".into(),
            );
        }
        atomic_write(&path, bytes)?;
        job.stored_bytes = stored;
        Ok(())
    }
}
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn fatal_budget(error: &str) -> bool {
    error.contains("byte budget") || error.contains("explicitly stopped")
}
fn trim_cache(directory: &Path, incoming: u64, capacity: u64) -> Result<(), String> {
    if incoming > capacity {
        return Err("cache object exceeds cache capacity".into());
    }
    let mut files = std::fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if name.len() != 64 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let metadata = entry.path().symlink_metadata().ok()?;
            metadata
                .is_file()
                .then_some((metadata.modified().ok()?, entry.path(), metadata.len()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, path, _)| (*modified, path.clone()));
    let mut size = files.iter().map(|(_, _, size)| size).sum::<u64>();
    let mut count = files.len();
    for (_, path, bytes) in files {
        if size.saturating_add(incoming) <= capacity && count < 2048 {
            break;
        }
        // Only generated SHA256 filenames in this task-owned cache, no recursion.
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        size = size.saturating_sub(bytes);
        count -= 1;
    }
    Ok(())
}
pub fn retry_after(value: &str, now: i64) -> Option<i64> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(now.saturating_add(i64::try_from(seconds).ok()?.saturating_mul(1000)));
    }
    let date = httpdate::parse_http_date(value)
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(i64::try_from(date.as_millis()).ok()?.max(now))
}
fn tile_bounds(z: u8, x: u32, y: u32) -> GeoBounds {
    let n = 2f64.powi(i32::from(z));
    let lat = |y: f64| ((std::f64::consts::PI * (1. - 2. * y / n)).sinh().atan()).to_degrees();
    GeoBounds {
        west: f64::from(x) / n * 360. - 180.,
        east: (f64::from(x) + 1.) / n * 360. - 180.,
        north: lat(f64::from(y)),
        south: lat(f64::from(y) + 1.),
    }
}

fn merge_terrain(
    target: &mut geo_formats::TerrainPreview,
    bounds: &GeoBounds,
    source: &geo_formats::TerrainPreview,
    tile: &GeoBounds,
) {
    let span = if bounds.east >= bounds.west {
        bounds.east - bounds.west
    } else {
        bounds.east + 360. - bounds.west
    };
    let merc = |lat: f64| {
        (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.)
            .tan()
            .ln()
    };
    for y in 0..target.height {
        let normalized = merc(bounds.north)
            - (merc(bounds.north) - merc(bounds.south)) * y as f64 / (target.height - 1) as f64;
        let lat = (2. * normalized.exp().atan() - std::f64::consts::FRAC_PI_2)
            .to_degrees()
            .clamp(bounds.south, bounds.north);
        if lat < tile.south || lat > tile.north {
            continue;
        }
        let sy = ((merc(tile.north) - merc(lat)) / (merc(tile.north) - merc(tile.south))
            * (source.height - 1) as f64)
            .round()
            .clamp(0., (source.height - 1) as f64) as usize;
        for x in 0..target.width {
            let mut lon = (bounds.west + span * x as f64 / (target.width - 1) as f64 + 180.)
                .rem_euclid(360.)
                - 180.;
            if lon == -180. && tile.east == 180. {
                lon = 180.;
            }
            if lon < tile.west || lon > tile.east {
                continue;
            }
            let sx = ((lon - tile.west) / (tile.east - tile.west) * (source.width - 1) as f64)
                .round()
                .clamp(0., (source.width - 1) as f64) as usize;
            target.elevations_m[y * target.width + x] = source.elevations_m[sy * source.width + sx];
        }
    }
}

fn validate_payload(provider: &str, bytes: &[u8]) -> Result<(), String> {
    match provider {
        "google_earth" => {
            if bytes.is_empty()
                || bytes.len() > 8 * 1024 * 1024
                || matches!(bytes.first(), Some(b'<' | b'{'))
            {
                return Err("invalid bounded Earth protocol response".into());
            }
        }
        "aws_terrain_tiles" => {
            geo_formats::decode_terrain_tiff(
                bytes,
                &DecodeLimits {
                    max_preview_side: 33,
                    ..DecodeLimits::default()
                },
            )?;
        }
        "openstreetmap" => {
            geo_formats::parse_overpass_buildings(bytes, &DecodeLimits::default())?;
        }
        "photon" => {
            let value: Value =
                serde_json::from_slice(bytes).map_err(|_| "invalid geocoder JSON")?;
            if value["features"].as_array().is_none_or(|a| a.len() > 50) {
                return Err("invalid geocoder features".into());
            }
        }
        "google_satellite" | "nasa_gibs" => {
            crate::imagery::encode_preview(bytes, usize::MAX)?;
        }
        "usgs_earthquakes" | "nasa_eonet" | "adsb_lol" => {
            let value: Value = serde_json::from_slice(bytes).map_err(|_| "invalid signals JSON")?;
            if !value.is_object() {
                return Err("invalid signals response object".into());
            }
        }
        _ => return Err("unknown provider".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn service() -> ExplorationService {
        ExplorationService::new(ExplorationConfig {
            cache_dir: std::env::temp_dir()
                .join(format!("archie-geo-test-{:032x}", rand::random::<u128>())),
            domain: ExplorationDomain::Personal,
        })
        .unwrap()
    }
    async fn completed(service: &ExplorationService, id: &str) -> ExplorationJob {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let job = service
                    .snapshot()
                    .await
                    .jobs
                    .into_iter()
                    .find(|j| j.id == id)
                    .unwrap();
                if matches!(
                    job.state,
                    JobState::Complete | JobState::Failed | JobState::Cancelled
                ) {
                    return job;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    const BUILDING: &str = r#"{"elements":[{"type":"way","id":1,"tags":{"building":"yes","height":"12"},"geometry":[{"lon":0.0,"lat":0.0},{"lon":0.001,"lat":0.0},{"lon":0.001,"lat":0.001},{"lon":0.0,"lat":0.0}]}]}"#;
    #[tokio::test]
    async fn native_http_job_exports_real_geometry_while_views_are_read_independently() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut c, _) = listener.accept().await.unwrap();
            let mut b = [0; 4096];
            let _ = c.read(&mut b).await;
            c.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    BUILDING.len(),
                    BUILDING
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        });
        let mut service = service();
        service.endpoints.osm = format!("http://{address}/osm");
        let request = exploration::ExplorationRequest {
            layers: vec![ExplorationLayer::Buildings],
            ..Default::default()
        };
        let response = service
            .execute(ExploreCommand::Explore {
                request: request.clone(),
            })
            .await
            .unwrap();
        let id = response["job"]["id"].as_str().unwrap();
        for _ in 0..20 {
            let _ = service.snapshot().await;
            tokio::task::yield_now().await;
        }
        let job = completed(&service, id).await;
        assert_eq!(job.state, JobState::Complete, "{:?}", job.error);
        assert_eq!(job.tiles_done, 1);
        assert_eq!(
            job.preview.as_ref().unwrap()["buildings"][0]["height_m"],
            12.0
        );
        let glb =
            std::fs::read(service.root().join(format!("job-{id}")).join("capture.glb")).unwrap();
        assert_eq!(&glb[..4], b"glTF");
        server.await.unwrap();
        let again = service
            .execute(ExploreCommand::Explore { request })
            .await
            .unwrap();
        let second = completed(&service, again["job"]["id"].as_str().unwrap()).await;
        assert_eq!(second.state, JobState::Complete);
        assert_eq!(second.cache_hits, 1);
        assert_eq!(
            service
                .snapshot()
                .await
                .providers
                .iter()
                .find(|p| p.id == "openstreetmap")
                .unwrap()
                .requests_sent,
            1
        );
    }
    #[tokio::test]
    async fn only_explicit_cancel_stops_a_stalled_download_and_releases_admission() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (sent, ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut c, _) = listener.accept().await.unwrap();
            let mut b = [0; 4096];
            let _ = c.read(&mut b).await;
            c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 999\r\n\r\n")
                .await
                .unwrap();
            let _ = sent.send(());
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let mut service = service();
        service.endpoints.osm = format!("http://{addr}/osm");
        let request = exploration::ExplorationRequest {
            layers: vec![ExplorationLayer::Buildings],
            ..Default::default()
        };
        let accepted = service
            .execute(ExploreCommand::Explore { request })
            .await
            .unwrap();
        let id = accepted["job"]["id"].as_str().unwrap().to_string();
        ready.await.unwrap();
        service
            .execute(ExploreCommand::Cancel {
                id: id.clone(),
                domain: ExplorationDomain::Personal,
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if service.state.lock().await.workers.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(completed(&service, &id).await.state, JobState::Cancelled);
        assert_eq!(
            service
                .snapshot()
                .await
                .providers
                .iter()
                .find(|p| p.id == "openstreetmap")
                .unwrap()
                .in_flight,
            0
        );
        server.abort();
    }
    #[test]
    fn mosaic_covers_all_tiles_preserving_nodata() {
        let bounds = GeoBounds {
            west: 0.,
            south: 0.,
            east: 2.,
            north: 1.,
        };
        let mut target = geo_formats::TerrainPreview {
            width: 3,
            height: 2,
            elevations_m: vec![None; 6],
        };
        let source = geo_formats::TerrainPreview {
            width: 2,
            height: 2,
            elevations_m: vec![Some(1.); 4],
        };
        merge_terrain(
            &mut target,
            &bounds,
            &source,
            &GeoBounds {
                west: 0.,
                south: 0.,
                east: 1.,
                north: 1.,
            },
        );
        let source = geo_formats::TerrainPreview {
            width: 2,
            height: 2,
            elevations_m: vec![Some(2.), None, Some(2.), Some(2.)],
        };
        merge_terrain(
            &mut target,
            &bounds,
            &source,
            &GeoBounds {
                west: 1.,
                south: 0.,
                east: 2.,
                north: 1.,
            },
        );
        assert_eq!(
            target.elevations_m,
            vec![Some(1.), Some(2.), None, Some(1.), Some(2.), Some(2.)]
        );
    }
    #[test]
    fn http_retry_dates_and_disabled_capture_sources_are_explicit() {
        assert_eq!(retry_after("2", 1000), Some(3000));
        assert_eq!(retry_after("Thu, 01 Jan 1970 00:01:00 GMT", 0), Some(60000));
        assert!(retry_after("bad", 0).is_none());
        let service = service();
        let mut request = exploration::ExplorationRequest {
            layers: vec![ExplorationLayer::Mesh],
            ..Default::default()
        };
        assert!(service.checked_plan(&request).is_err());
        request.layers = vec![ExplorationLayer::Terrain];
        request.zoom = 15;
        assert!(service.checked_plan(&request).is_err());
    }

    fn seed(_service: &ExplorationService) -> ExplorationJob {
        let request = exploration::ExplorationRequest::default();
        let plan = exploration::plan(&request).unwrap();
        ExplorationJob {
            id: "1".repeat(32),
            domain: request.domain,
            state: JobState::Paused,
            request,
            camera: plan.camera,
            tiles_total: plan.tiles.len() as u64,
            tiles_done: 0,
            bytes_received: 0,
            stored_bytes: 0,
            signal_updates_done: 0,
            pinned_imagery_date: None,
            cache_hits: 0,
            error: None,
            artifacts: vec![],
            preview: None,
            created_at_ms: now_ms(),
            updated_at_ms: now_ms(),
            plan,
            projection_truncated: false,
        }
    }
    #[tokio::test]
    async fn selecting_an_older_job_only_projects_its_own_preview_without_fetching() {
        let service = service();
        let mut older = seed(&service);
        older.preview = Some(json!({"bounds":older.plan.bounds,"buildings":[],"capture":"older"}));
        let mut latest = older.clone();
        latest.id = "2".repeat(32);
        latest.preview.as_mut().unwrap()["capture"] = json!("latest");
        service.state.lock().await.jobs = vec![older.clone(), latest.clone()];
        let selected = service.snapshot_for(&older.id).await.unwrap();
        assert_eq!(selected.selected_job_id.as_deref(), Some(older.id.as_str()));
        assert_eq!(
            selected
                .jobs
                .iter()
                .map(|job| job.id.as_str())
                .collect::<Vec<_>>(),
            vec![older.id.as_str(), latest.id.as_str()]
        );
        assert_eq!(
            selected.jobs[0].preview.as_ref().unwrap()["capture"],
            "older"
        );
        assert!(selected.jobs[1].preview.is_none());
        assert!(selected
            .providers
            .iter()
            .all(|provider| provider.requests_sent == 0));
        assert!(service.state.lock().await.workers.is_empty());
        let normal = service.snapshot().await;
        assert!(normal.selected_job_id.is_none());
        assert!(normal.jobs[0].preview.is_none());
        assert_eq!(
            normal.jobs[1].preview.as_ref().unwrap()["capture"],
            "latest"
        );
        assert!(service.snapshot_for(&"3".repeat(32)).await.is_none());
        assert!(service.snapshot_for("../manifest.json").await.is_none());
        assert!(service
            .snapshot_for(&format!("{}/extra", older.id))
            .await
            .is_none());
    }

    #[test]
    fn imagery_and_signals_are_admitted_and_counted_without_licensing_geometry() {
        let service = service();
        let request = exploration::ExplorationRequest {
            layers: vec![ExplorationLayer::Imagery, ExplorationLayer::Signals],
            signal_updates: 2,
            ..Default::default()
        };
        let plan = service.checked_plan(&request).unwrap();
        assert!(plan.estimated_requests >= 8);
        assert!(service
            .checked_plan(&exploration::ExplorationRequest {
                layers: vec![ExplorationLayer::StreetView],
                ..request
            })
            .is_err());
    }

    #[tokio::test]
    async fn google_preview_is_visible_in_ram_but_never_manifest_pixels() {
        let service = service();
        let mut job = seed(&service);
        job.preview = Some(json!({"imagery":[
            {"source_id":"google_satellite","mode":"online_preview_only","tiles":[{"data_base64":"VOLATILE_SECRET"}]},
            {"source_id":"blue_marble","mode":"open_data","tiles":[{"data_base64":"OPEN_SOURCE"}]}
        ]}));
        service.save_job(&job).unwrap();
        service.state.lock().await.jobs.push(job.clone());
        assert!(serde_json::to_string(&service.snapshot().await)
            .unwrap()
            .contains("VOLATILE_SECRET"));
        let saved = std::fs::read_to_string(
            service
                .root()
                .join(format!("job-{}", job.id))
                .join("manifest.json"),
        )
        .unwrap();
        assert!(!saved.contains("VOLATILE_SECRET"));
        assert!(saved.contains("OPEN_SOURCE"));
    }

    #[tokio::test]
    async fn volatile_fetch_never_reads_or_writes_the_disk_cache() {
        let service = service();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/tile", listener.local_addr().unwrap());
        let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
        let cache = service.root().join("cache").join(hash);
        // A seeded file must be ignored by volatile policy.
        atomic_write(&cache, b"DO_NOT_REPLAY").unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            let bytes = br#"{"features":[]}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            socket.write_all(bytes).await.unwrap();
        });
        let (bytes, cached) = service
            .fetch_policy(None, "photon", &url, 1024, None)
            .await
            .unwrap();
        server.await.unwrap();
        assert!(!cached);
        assert_eq!(bytes, br#"{"features":[]}"#);
        assert_eq!(std::fs::read(cache).unwrap(), b"DO_NOT_REPLAY");
        assert_eq!(
            service
                .snapshot()
                .await
                .providers
                .iter()
                .find(|p| p.id == "photon")
                .unwrap()
                .requests_sent,
            1
        );
    }

    #[tokio::test]
    async fn google_provider_cannot_be_opted_into_persistent_cache() {
        let service = service();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/tile", listener.local_addr().unwrap());
        let cache = service
            .root()
            .join("cache")
            .join(format!("{:x}", Sha256::digest(url.as_bytes())));
        atomic_write(&cache, b"DO_NOT_REPLAY").unwrap();
        let mut tile = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut tile, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[10, 20, 30])
                .unwrap();
        }
        let expected = tile.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        tile.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            socket.write_all(&tile).await.unwrap();
        });
        let (bytes, cached) = service
            .fetch_policy(
                None,
                "google_satellite",
                &url,
                1024,
                Some(Duration::from_secs(86400)),
            )
            .await
            .unwrap();
        // Assert before awaiting the server: a wrong cache replay never sends a request.
        assert!(!cached);
        assert_eq!(bytes, expected);
        server.await.unwrap();
        assert_eq!(std::fs::read(cache).unwrap(), b"DO_NOT_REPLAY");
    }

    #[tokio::test]
    async fn a_signal_error_without_any_success_does_not_claim_last_good_data() {
        let service = service();
        let mut job = seed(&service);
        job.state = JobState::Running;
        job.request.layers = vec![ExplorationLayer::Signals];
        let requests = crate::world_signals::requests(
            job.request.latitude,
            job.request.longitude,
            job.request.radius_m,
        );
        assert_eq!(requests.len(), 3);
        let sources: Vec<_> = requests.iter().map(|request| {
            let hash = format!("{:x}",Sha256::digest(request.url.as_bytes()));
            atomic_write(&service.root().join("cache").join(hash),b"{}").unwrap();
            json!({"id":request.provider,"label":request.label,"state":"error","retrieved_at_ms":Value::Null})
        }).collect();
        job.preview = Some(json!({"signals":{"sources":sources,"points":[]}}));
        service.save_job(&job).unwrap();
        service.state.lock().await.jobs.push(job.clone());
        let errors = service.capture_signals(&job.id, &job).await.unwrap();
        assert_eq!(errors.len(), 3);
        let snapshot = service.snapshot().await;
        let signals = &snapshot.jobs[0].preview.as_ref().unwrap()["signals"];
        assert!(signals["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["state"] == "error"));
        assert_eq!(snapshot.jobs[0].signal_updates_done, 1);
    }

    #[tokio::test]
    async fn a_signal_failure_preserves_last_good_points_and_their_retrieval_time() {
        let service = service();
        let mut job = seed(&service);
        job.state = JobState::Running;
        job.request.layers = vec![ExplorationLayer::Signals];
        let requests = crate::world_signals::requests(
            job.request.latitude,
            job.request.longitude,
            job.request.radius_m,
        );
        for request in &requests {
            let data = match request.provider.as_str() {
                "usgs_earthquakes" => json!({"type":"FeatureCollection","features":(0..140).map(|n|
                    json!({"type":"Feature","id":format!("quake-{n}"),"properties":{"place":"Test observation","mag":2,"time":now_ms()-1000},
                        "geometry":{"type":"Point","coordinates":[0,0,1]}})).collect::<Vec<_>>()}),
                "nasa_eonet" => json!({"events":[]}),
                _ => json!({"now":now_ms(),"ac":[]}),
            };
            let hash = format!("{:x}", Sha256::digest(request.url.as_bytes()));
            atomic_write(
                &service.root().join("cache").join(hash),
                &serde_json::to_vec(&data).unwrap(),
            )
            .unwrap();
        }
        service.save_job(&job).unwrap();
        service.state.lock().await.jobs.push(job.clone());
        service.capture_signals(&job.id, &job).await.unwrap();
        let fresh = service.snapshot().await;
        let fresh_signals = &fresh.jobs[0].preview.as_ref().unwrap()["signals"];
        assert_eq!(fresh_signals["points"].as_array().unwrap().len(), 128);
        assert_eq!(fresh_signals["points_total"], 140);
        assert_eq!(fresh_signals["omitted_points"], 12);
        assert_eq!(fresh_signals["rejected"], 0);
        assert_eq!(fresh_signals["total_is_lower_bound"], false);
        let retrieved: Vec<_> = fresh_signals["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|source| source["retrieved_at_ms"].clone())
            .collect();
        for request in &requests {
            let hash = format!("{:x}", Sha256::digest(request.url.as_bytes()));
            atomic_write(&service.root().join("cache").join(hash), b"{}").unwrap();
        }
        {
            let mut state = service.state.lock().await;
            let current = &mut state.jobs[0];
            current.request.signal_updates = 2;
            current.preview.as_mut().unwrap()["signals"]["next_update_at_ms"] = Value::Null;
            job = current.clone();
        }
        service.capture_signals(&job.id, &job).await.unwrap();
        let snapshot = service.snapshot().await;
        let signals = &snapshot.jobs[0].preview.as_ref().unwrap()["signals"];
        assert_eq!(signals["points"], fresh_signals["points"]);
        assert_eq!(signals["points_total"], 140);
        assert_eq!(signals["omitted_points"], 12);
        assert_eq!(signals["total_is_lower_bound"], false);
        for (index, source) in signals["sources"].as_array().unwrap().iter().enumerate() {
            assert_eq!(source["state"], "stale");
            assert_eq!(source["retrieved_at_ms"], retrieved[index]);
        }
        let restarted = ExplorationService::new(service.config.clone()).unwrap();
        assert_eq!(restarted.snapshot().await.jobs[0].signal_updates_done, 2);
    }
    #[tokio::test]
    async fn a_waiting_signal_refresh_pauses_and_resumes_its_persisted_schedule() {
        let service = service();
        let mut job = seed(&service);
        job.request.layers = vec![ExplorationLayer::Signals];
        job.request.signal_updates = 2;
        job.signal_updates_done = 1;
        let next = now_ms() + 3_600_000;
        job.preview = Some(
            json!({"signals":{"sources":[],"points":[],"updates_done":1,"next_update_at_ms":next}}),
        );
        service.save_job(&job).unwrap();
        service.state.lock().await.jobs.push(job.clone());
        service
            .execute(ExploreCommand::Resume {
                id: job.id.clone(),
                domain: job.domain,
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if service.state.lock().await.workers.contains(&job.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        service
            .execute(ExploreCommand::Pause {
                id: job.id.clone(),
                domain: job.domain,
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if !service.state.lock().await.workers.contains(&job.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!service.state.lock().await.workers.contains(&job.id));
        let restarted = ExplorationService::new(service.config.clone()).unwrap();
        let view = restarted.snapshot().await;
        assert_eq!(view.jobs[0].state, JobState::Paused);
        assert_eq!(view.jobs[0].signal_updates_done, 1);
        assert_eq!(
            view.jobs[0].preview.as_ref().unwrap()["signals"]["next_update_at_ms"],
            next
        );
        restarted
            .execute(ExploreCommand::Resume {
                id: job.id.clone(),
                domain: job.domain,
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if restarted.state.lock().await.workers.contains(&job.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        restarted
            .execute(ExploreCommand::Cancel {
                id: job.id.clone(),
                domain: job.domain,
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if !restarted.state.lock().await.workers.contains(&job.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let view = restarted.snapshot().await;
        assert_eq!(view.jobs[0].state, JobState::Cancelled);
        assert_eq!(view.jobs[0].signal_updates_done, 1);
        assert!(view
            .providers
            .iter()
            .all(|provider| provider.requests_sent == 0));
    }
    #[tokio::test]
    async fn history_projection_and_manifest_remain_bounded_without_silently_hiding_current_counts()
    {
        let service = service();
        let mut job = seed(&service);
        service.save_job(&job).unwrap();
        service.state.lock().await.jobs.push(job.clone());
        let ring = (0..512)
            .map(|n| {
                let a = n as f64 / 511. * std::f64::consts::TAU;
                [a.cos() * 0.001, a.sin() * 0.001]
            })
            .collect::<Vec<_>>();
        let buildings = (0..128)
            .map(|n| geo_formats::BuildingFeature {
                id: n.to_string(),
                ring: ring.clone(),
                height_m: None,
            })
            .collect::<Vec<_>>();
        let bounds = GeoBounds {
            south: -0.01,
            west: -0.01,
            north: 0.01,
            east: 0.01,
        };
        service
            .preview(&job.id, &bounds, None, &buildings)
            .await
            .unwrap();
        job = service.state.lock().await.jobs[0].clone();
        assert!(
            job.preview.as_ref().unwrap()["omitted_buildings"]
                .as_u64()
                .unwrap()
                > 0
        );
        {
            let mut state = service.state.lock().await;
            for index in 2..=32 {
                let mut other = job.clone();
                other.id = format!("{index:032x}");
                state.jobs.push(other);
            }
        }
        let view = service.snapshot().await;
        assert_eq!(view.jobs.len(), 32);
        assert_eq!(view.jobs.iter().filter(|j| j.preview.is_some()).count(), 1);
        assert!(serde_json::to_vec(&view).unwrap().len() < 1024 * 1024);
        assert!(
            std::fs::metadata(
                service
                    .root()
                    .join(format!("job-{}", job.id))
                    .join("manifest.json")
            )
            .unwrap()
            .len()
                < 2 * 1024 * 1024
        );
    }
    #[tokio::test]
    async fn exhausted_network_budget_can_resume_from_its_preserved_source_artifact() {
        let service = service();
        let mut job = seed(&service);
        job.request.layers = vec![ExplorationLayer::Buildings];
        job.plan = exploration::plan(&job.request).unwrap();
        job.bytes_received = job.request.max_bytes;
        job.tiles_total = 1;
        service.save_job(&job).unwrap();
        service.state.lock().await.jobs.push(job.clone());
        service
            .artifact(
                &job.id,
                "buildings",
                "osm_json",
                "buildings.json",
                BUILDING.as_bytes(),
                "OSM",
                false,
            )
            .await
            .unwrap();
        let mut restarted = ExplorationService::new(service.config.clone()).unwrap();
        restarted.endpoints.osm = "http://127.0.0.1:9/should-not-fetch".into();
        restarted
            .execute(ExploreCommand::Resume {
                id: job.id.clone(),
                domain: ExplorationDomain::Personal,
            })
            .await
            .unwrap();
        let final_job = completed(&restarted, &job.id).await;
        assert_eq!(final_job.state, JobState::Complete, "{:?}", final_job.error);
        assert_eq!(
            restarted
                .snapshot()
                .await
                .providers
                .iter()
                .find(|p| p.id == "openstreetmap")
                .unwrap()
                .requests_sent,
            0
        );
    }
    #[tokio::test]
    async fn empty_building_coverage_completes_without_a_fabricated_glb() {
        let service = service();
        let mut job = seed(&service);
        job.request.layers = vec![ExplorationLayer::Buildings];
        job.plan = exploration::plan(&job.request).unwrap();
        job.tiles_total = 1;
        service.save_job(&job).unwrap();
        service.state.lock().await.jobs.push(job.clone());
        service
            .artifact(
                &job.id,
                "buildings",
                "osm_json",
                "buildings.json",
                br#"{"elements":[]}"#,
                "OSM",
                false,
            )
            .await
            .unwrap();
        service
            .execute(ExploreCommand::Resume {
                id: job.id.clone(),
                domain: ExplorationDomain::Personal,
            })
            .await
            .unwrap();
        let final_job = completed(&service, &job.id).await;
        assert_eq!(final_job.state, JobState::Complete, "{:?}", final_job.error);
        assert!(final_job.artifacts.iter().any(|a| a.kind == "geojson"));
        assert!(!final_job.artifacts.iter().any(|a| a.kind == "glb"));
    }
    #[tokio::test]
    async fn failed_quota_persistence_does_not_leave_an_in_flight_slot_reserved() {
        let service = service();
        std::fs::create_dir(service.root().join("usage.json")).unwrap();
        assert!(service
            .fetch(None, "photon", "http://127.0.0.1:9/never-sent", 1000)
            .await
            .is_err());
        assert_eq!(
            service
                .snapshot()
                .await
                .providers
                .iter()
                .find(|p| p.id == "photon")
                .unwrap()
                .in_flight,
            0
        );
    }
    #[test]
    fn high_latitude_mosaic_uses_the_same_mercator_rows_as_exports() {
        let bounds = GeoBounds {
            west: 0.,
            east: 1.,
            south: 70.,
            north: 85.,
        };
        let source = geo_formats::TerrainPreview {
            width: 2,
            height: 9,
            elevations_m: (0..9).flat_map(|v| [Some(v as f32); 2]).collect(),
        };
        let mut target = geo_formats::TerrainPreview {
            width: 2,
            height: 5,
            elevations_m: vec![None; 10],
        };
        merge_terrain(&mut target, &bounds, &source, &bounds);
        assert_eq!(
            target.elevations_m,
            (0..5)
                .flat_map(|v| [Some((v * 2) as f32); 2])
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn cache_capacity_evicts_only_its_generated_objects() {
        let service = service();
        let directory = service.root().join("cache");
        std::fs::write(directory.join("a".repeat(64)), [1u8; 600]).unwrap();
        std::fs::write(directory.join("b".repeat(64)), [2u8; 600]).unwrap();
        std::fs::write(directory.join("owner-note"), b"keep").unwrap();
        trim_cache(&directory, 500, 1000).unwrap();
        assert!(directory.join("owner-note").exists());
        assert!(!directory.join("a".repeat(64)).exists());
        assert!(!directory.join("b".repeat(64)).exists());
    }
}
