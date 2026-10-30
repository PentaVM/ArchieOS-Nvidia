//! Camera-driven streaming under a previously admitted, bounded owner session.
//! No model calls, foreground/input hooks, arbitrary URLs or persisted Google pixels.
use crate::{
    exploration::ProviderUsage,
    exploration_runtime::{now_ms, ExplorationService},
    navigation::NavigationRequest,
};
use archie_geofabric::{
    exploration::{ExplorationDomain, ImagerySource, TileCoordinate},
    globe::{self, GlobeViewport},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const CACHE_TILES: usize = 192;
const FRAME_BYTES: usize = 4 * 1024 * 1024;
#[path = "earth_runtime.rs"]
mod earth_stream;

pub(crate) struct NavigationMeter {
    pub requests: AtomicU64,
    pub bytes: AtomicU64,
    pub cache_hits: AtomicU64,
    cancelled: AtomicBool,
    max_requests: u64,
    max_bytes: u64,
    expires: i64,
}
impl NavigationMeter {
    fn new(request: &NavigationRequest) -> Self {
        Self {
            requests: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
            max_requests: request.max_requests.into(),
            max_bytes: request.max_bytes,
            expires: now_ms() + i64::from(request.duration_seconds) * 1000,
        }
    }
    pub(crate) fn active(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("navigation explicitly stopped".into());
        }
        if now_ms() >= self.expires {
            return Err("navigation session expired".into());
        }
        if self.bytes.load(Ordering::Relaxed) >= self.max_bytes {
            return Err("navigation byte budget exhausted".into());
        }
        Ok(())
    }
    pub(crate) fn reserve_request(&self) -> Result<(), String> {
        self.active()?;
        let mut used = self.requests.load(Ordering::Relaxed);
        loop {
            if used >= self.max_requests {
                return Err("navigation request budget exhausted".into());
            }
            match self.requests.compare_exchange_weak(
                used,
                used + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(current) => used = current,
            }
        }
    }
    pub(crate) fn charge_bytes(&self, bytes: u64) -> Result<(), String> {
        let before = self.bytes.fetch_add(bytes, Ordering::Relaxed);
        if before.saturating_add(bytes) > self.max_bytes {
            return Err("navigation byte budget exhausted".into());
        }
        Ok(())
    }
    pub(crate) async fn stopped(&self) {
        loop {
            if self.active().is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    fn remaining(&self) -> u64 {
        self.max_bytes
            .saturating_sub(self.bytes.load(Ordering::Relaxed))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigationEnvelope {
    token: String,
    session_id: String,
    command: NavigationCommand,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum NavigationCommand {
    Viewport {
        generation: u64,
        source: ImagerySource,
        viewport: GlobeViewport,
        #[serde(default)]
        known_keys: Vec<String>,
        #[serde(default)]
        known_mesh_keys: Vec<String>,
    },
    Status {
        #[serde(default)]
        known_keys: Vec<String>,
        #[serde(default)]
        known_mesh_keys: Vec<String>,
    },
}
struct CachedTile {
    value: Value,
    bytes: usize,
    used: i64,
}
struct Session {
    id: String,
    token: String,
    request: NavigationRequest,
    meter: Arc<NavigationMeter>,
    created: i64,
    date: String,
    generation: u64,
    source: ImagerySource,
    viewport: Option<GlobeViewport>,
    desired: Vec<TileCoordinate>,
    candidates: Vec<TileCoordinate>,
    needed: BTreeSet<String>,
    client_known: BTreeSet<String>,
    attempted: BTreeSet<String>,
    cache: BTreeMap<String, CachedTile>,
    errors: BTreeMap<String, String>,
    in_flight: BTreeSet<String>,
    points: Vec<Value>,
    signal_sources: Vec<Value>,
    signals_key: String,
    signals_at: i64,
    signals_in_flight: bool,
    min_zoom: u8,
    max_zoom: u8,
    earth: earth_stream::EarthState,
}
#[derive(Default)]
pub(crate) struct NavigationState {
    endpoint: Option<(PathBuf, String)>,
    session: Option<Session>,
    pumping: bool,
}

fn key(source: ImagerySource, date: &str, tile: TileCoordinate) -> String {
    format!(
        "{}:{date}:{}/{}/{}",
        crate::imagery::spec(source).id,
        tile.zoom,
        tile.x,
        tile.y
    )
}
fn equal_token(a: &str, b: &str) -> bool {
    a.len() == 64
        && b.len() == 64
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |v, (a, b)| v | std::hint::black_box(a ^ b))
            == 0
}
fn session_state(session: &Session) -> &'static str {
    if session.meter.cancelled.load(Ordering::Relaxed) {
        "stopped"
    } else if now_ms() >= session.meter.expires {
        "expired"
    } else if session.meter.remaining() == 0
        || session.meter.requests.load(Ordering::Relaxed) >= session.meter.max_requests
    {
        "budget_exhausted"
    } else if session.candidates.iter().any(|t| {
        let k = key(session.source, &session.date, *t);
        !session.attempted.contains(&k)
            && !session.cache.contains_key(&k)
            && !session.client_known.contains(&k)
    }) || !session.in_flight.is_empty()
    {
        "loading"
    } else if !session.errors.is_empty() {
        "partial"
    } else {
        "ready"
    }
}
fn metadata(session: &Session) -> Value {
    json!({"id":session.id,"domain":session.request.domain,"state":session_state(session),"request":session.request,"created_at_ms":session.created,"updated_at_ms":now_ms(),"expires_at_ms":session.meter.expires,"requests_sent":session.meter.requests.load(Ordering::Relaxed),"bytes_received":session.meter.bytes.load(Ordering::Relaxed),"cache_hits":session.meter.cache_hits.load(Ordering::Relaxed),"generation":session.generation,"desired_tiles":session.desired.len(),"cached_tiles":session.cache.len()})
}

impl ExplorationService {
    pub(crate) async fn configure_navigation(
        &self,
        runtime: PathBuf,
        address: String,
    ) -> Result<(), String> {
        let mut state = self.navigation.lock().await;
        let path = runtime.join("navigation.json");
        if path.exists() {
            crate::exploration_control::secure_owner_path(&path, false)?;
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        state.endpoint = Some((runtime, address));
        Ok(())
    }
    pub(crate) async fn start_navigation(
        &self,
        request: NavigationRequest,
    ) -> Result<Value, String> {
        request.validate()?;
        self.check_navigation_domain(request.domain)?;
        let mut state = self.navigation.lock().await;
        if state.session.as_ref().is_some_and(|s| {
            !matches!(session_state(s), "expired" | "stopped" | "budget_exhausted")
        }) {
            return Err(
                "an admitted navigation session is already active; stop it before replacing it"
                    .into(),
            );
        }
        let (runtime, address) = state
            .endpoint
            .clone()
            .ok_or("private navigation transport is not ready")?;
        if let Some(old) = state.session.take() {
            old.meter.cancelled.store(true, Ordering::Relaxed);
        }
        let meter = Arc::new(NavigationMeter::new(&request));
        let session = Session {
            id: format!("{:032x}", rand::random::<u128>()),
            token: format!(
                "{:032x}{:032x}",
                rand::random::<u128>(),
                rand::random::<u128>()
            ),
            source: request.sources[0],
            date: crate::imagery::effective_date(request.imagery_date.as_deref(), now_ms())?,
            request,
            meter,
            created: now_ms(),
            generation: 0,
            viewport: None,
            desired: vec![],
            candidates: vec![],
            needed: BTreeSet::new(),
            client_known: BTreeSet::new(),
            attempted: BTreeSet::new(),
            cache: BTreeMap::new(),
            errors: BTreeMap::new(),
            in_flight: BTreeSet::new(),
            points: vec![],
            signal_sources: vec![],
            signals_key: String::new(),
            signals_at: 0,
            signals_in_flight: false,
            min_zoom: 0,
            max_zoom: 0,
            earth: earth_stream::EarthState::default(),
        };
        let path = runtime.join("navigation.json");
        if path.exists() {
            crate::exploration_control::secure_owner_path(&path, false)?;
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        crate::exploration_runtime::atomic_write(&path,&serde_json::to_vec(&json!({"schema_version":1,"address":address,"token":session.token,"session_id":session.id,"domain":session.request.domain,"expires_at_ms":session.meter.expires})).map_err(|e|e.to_string())?)?;
        crate::exploration_control::secure_owner_path(&path, false)?;
        let result = json!({"navigation":metadata(&session)});
        state.session = Some(session);
        Ok(result)
    }
    pub(crate) async fn navigation_status(
        &self,
        id: &str,
        domain: ExplorationDomain,
        stop: bool,
    ) -> Result<Value, String> {
        self.check_navigation_domain(domain)?;
        let mut state = self.navigation.lock().await;
        let session = state
            .session
            .as_mut()
            .filter(|s| s.id == id && s.request.domain == domain)
            .ok_or("navigation session unavailable in this domain")?;
        if stop {
            session.meter.cancelled.store(true, Ordering::Relaxed);
            session.cache.clear();
            session.points.clear();
            session.earth.clear();
        }
        let result = json!({"navigation":metadata(session)});
        if stop {
            if let Some((runtime, _)) = &state.endpoint {
                let path = runtime.join("navigation.json");
                if path.exists() {
                    crate::exploration_control::secure_owner_path(&path, false)?;
                    std::fs::remove_file(path).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(result)
    }
    pub(crate) async fn navigation_message(&self, bytes: &[u8]) -> Result<Value, String> {
        let envelope: NavigationEnvelope =
            serde_json::from_slice(bytes).map_err(|_| "invalid navigation intent")?;
        let inventory = match &envelope.command {
            NavigationCommand::Viewport { known_keys, .. }
            | NavigationCommand::Status { known_keys, .. } => known_keys,
        };
        if inventory.len() > 256
            || inventory
                .iter()
                .any(|v| v.len() > 96 || !v.is_ascii() || v.chars().any(char::is_control))
        {
            return Err("navigation cache inventory exceeds its bound".into());
        }
        let mesh_inventory = match &envelope.command {
            NavigationCommand::Viewport {
                known_mesh_keys, ..
            }
            | NavigationCommand::Status {
                known_mesh_keys, ..
            } => known_mesh_keys,
        };
        if mesh_inventory.len() > 64
            || mesh_inventory
                .iter()
                .any(|k| k.len() > 96 || !k.is_ascii() || k.chars().any(char::is_control))
        {
            return Err("3D mesh inventory exceeds its bound".into());
        }
        let known = {
            let mut state = self.navigation.lock().await;
            let session = state
                .session
                .as_mut()
                .filter(|s| s.id == envelope.session_id && equal_token(&s.token, &envelope.token))
                .ok_or("navigation capability refused")?;
            if matches!(session_state(session), "expired" | "stopped") {
                return Err("navigation capability expired or stopped".into());
            }
            session.earth.inventory(mesh_inventory);
            let known = match envelope.command {
                NavigationCommand::Viewport {
                    generation,
                    source,
                    viewport,
                    known_keys,
                    ..
                } => {
                    if generation == 0
                        || generation > 9_007_199_254_740_991
                        || !session.request.sources.contains(&source)
                    {
                        return Err("navigation viewport source or generation refused".into());
                    }
                    let selected = globe::select(
                        &viewport,
                        session
                            .request
                            .max_zoom
                            .min(crate::imagery::spec(source).max_zoom),
                        session.request.max_visible_tiles as usize,
                    )?;
                    if generation > session.generation {
                        let same = session.source == source
                            && session.viewport.as_ref() == Some(&viewport);
                        session.generation = generation;
                        if !same {
                            session.source = source;
                            session.viewport = Some(viewport);
                            session.min_zoom = selected.min_zoom;
                            session.max_zoom = selected.max_zoom;
                            session.desired = selected.tiles;
                            session.candidates = globe::ancestors(&session.desired);
                            session.candidates.extend_from_slice(&session.desired);
                            session.needed = session
                                .candidates
                                .iter()
                                .map(|tile| key(session.source, &session.date, *tile))
                                .collect();
                            session.attempted.clear();
                            session.errors.clear();
                        }
                    }
                    known_keys
                }
                NavigationCommand::Status { known_keys, .. } => known_keys,
            };
            if known.len() > 256
                || known
                    .iter()
                    .any(|v| v.len() > 96 || !v.is_ascii() || v.chars().any(char::is_control))
            {
                return Err("navigation cache inventory exceeds its bound".into());
            }
            let known = known.into_iter().collect::<BTreeSet<_>>();
            session.client_known = known.clone();
            known
        };
        self.pump_navigation().await;
        self.pump_earth().await;
        self.navigation_frame(&known, &envelope.session_id, &envelope.token)
            .await
    }
    async fn navigation_frame(
        &self,
        known: &BTreeSet<String>,
        id: &str,
        token: &str,
    ) -> Result<Value, String> {
        let (mut frame, provider) = {
            let mut state = self.navigation.lock().await;
            let s = state
                .session
                .as_mut()
                .filter(|s| {
                    s.id == id
                        && equal_token(&s.token, token)
                        && !matches!(session_state(s), "expired" | "stopped")
                })
                .ok_or("navigation capability changed before disclosure")?;
            let source = crate::imagery::spec(s.source);
            let provider = if source.online_only {
                "google_satellite"
            } else {
                "nasa_gibs"
            };
            let mut tiles = vec![];
            let mut bytes = 0usize;
            let mut more_tiles = false;
            let retrieved = s
                .cache
                .values()
                .filter(|tile| {
                    tile.value["key"]
                        .as_str()
                        .is_some_and(|v| v.starts_with(source.id))
                })
                .filter_map(|tile| tile.value["retrieved_at_ms"].as_i64())
                .max();
            let finest = s
                .candidates
                .iter()
                .filter_map(|t| {
                    s.cache
                        .get(&key(s.source, &s.date, *t))
                        .and_then(|tile| tile.value["width"].as_u64().map(|width| (t.zoom, width)))
                })
                .max_by_key(|(zoom, _)| *zoom);
            let resolution = s
                .viewport
                .as_ref()
                .zip(finest)
                .map(|(view, (zoom, pixels))| {
                    let (lat, _, _, _) = signal_area(view);
                    std::f64::consts::TAU * globe::EARTH_RADIUS_M * lat.to_radians().cos()
                        / (2f64.powi(i32::from(zoom)) * pixels as f64)
                });
            // Ancestors first, then leaves. Only previously fetched bytes are read.
            for tile in &s.candidates {
                let key = key(s.source, &s.date, *tile);
                if known.contains(&key) {
                    continue;
                }
                if let Some(cached) = s.cache.get_mut(&key) {
                    let size = cached.value["data_base64"].as_str().map_or(0, str::len);
                    if bytes + size > FRAME_BYTES {
                        more_tiles = true;
                        continue;
                    }
                    bytes += size;
                    cached.used = now_ms();
                    tiles.push(cached.value.clone());
                }
            }
            let pending = s
                .candidates
                .iter()
                .filter(|t| {
                    let k = key(s.source, &s.date, **t);
                    !s.attempted.contains(&k)
                        && !s.cache.contains_key(&k)
                        && !s.client_known.contains(&k)
                })
                .count()
                + s.in_flight.len();
            let (earth, meshes, more_meshes) = s.earth.frame(FRAME_BYTES.saturating_sub(bytes));
            (
                json!({"earth":earth,"meshes":meshes,"session_id":s.id,"generation":s.generation,"source_id":source.id,"state":session_state(s),"desired_tiles":s.desired,"tiles":tiles,"pending_tiles":pending,"min_zoom":s.min_zoom,"max_zoom":s.max_zoom,
                "source":{"source_id":source.id,"label":source.label,"attribution":source.attribution,"mode":if source.online_only{"online_preview_only"}else{"open_data"},"imagery_date":if source.online_only{Value::Null}else if source.daily{json!(s.date)}else{json!("static")},"retrieved_at_ms":retrieved,"state":session_state(s),"source_resolution_m":resolution,"preview_resolution_m":resolution,"native_max_zoom":source.max_zoom},
                "usage":{"requests_sent":s.meter.requests.load(Ordering::Relaxed),"bytes_received":s.meter.bytes.load(Ordering::Relaxed),"max_requests":s.request.max_requests,"max_bytes":s.request.max_bytes,"expires_at_ms":s.meter.expires,"cache_tiles":s.cache.len(),"cache_bytes":s.cache.values().map(|v|v.bytes).sum::<usize>()},
                "errors":s.errors.values().take(8).cloned().collect::<Vec<_>>(),"points":s.points,"signal_sources":s.signal_sources,"signals":signal_counts(&s.points,&s.signal_sources),"more_tiles":more_tiles||more_meshes}),
                provider,
            )
        };
        let usage: Option<ProviderUsage> = self.navigation_provider_usage(provider).await;
        frame["provider"] = json!(usage);
        frame["earth"]["provider"] = json!(self.navigation_provider_usage("google_earth").await);
        if frame["state"] == "loading" && frame["provider"]["state"] == "waiting_rate_limit" {
            frame["state"] = json!("rate_limited");
        }
        Ok(frame)
    }
    async fn pump_navigation(&self) {
        let mut state = self.navigation.lock().await;
        if state.pumping {
            return;
        }
        state.pumping = true;
        drop(state);
        let service = self.clone();
        tokio::spawn(async move {
            let signals = service.clone();
            tokio::spawn(async move {
                signals.refresh_navigation_signals().await;
            });
            let mut work = tokio::task::JoinSet::new();
            loop {
                while work.len() < 4 {
                    let next = {
                        let mut state = service.navigation.lock().await;
                        state.session.as_mut().and_then(|s| {
                            if s.meter.active().is_err()
                                || s.meter.requests.load(Ordering::Relaxed) >= s.meter.max_requests
                            {
                                return None;
                            }
                            let tile = s.candidates.iter().copied().find(|t| {
                                let key = key(s.source, &s.date, *t);
                                !s.attempted.contains(&key)
                                    && !s.in_flight.contains(&key)
                                    && !s.cache.contains_key(&key)
                                    && !s.client_known.contains(&key)
                            })?;
                            let key = key(s.source, &s.date, tile);
                            s.attempted.insert(key.clone());
                            s.in_flight.insert(key.clone());
                            Some((
                                s.id.clone(),
                                s.source,
                                s.date.clone(),
                                tile,
                                key,
                                s.meter.clone(),
                            ))
                        })
                    };
                    let Some((id, source, date, tile, key, meter)) = next else {
                        break;
                    };
                    let fetcher = service.clone();
                    work.spawn(async move {
                        let result = tokio::select!{result=fetcher.navigation_tile(source,&date,tile,&meter)=>result,_=fetcher.obsolete_navigation_tile(&id,&key)=>Err("obsolete viewport tile".into())};
                        let mut state = fetcher.navigation.lock().await;
                        if let Some(s) = state.session.as_mut().filter(|s| s.id == id) {
                            s.in_flight.remove(&key);
                            match result {
                                Ok((value, bytes)) => {
                                    s.cache.insert(
                                        key,
                                        CachedTile {
                                            value,
                                            bytes,
                                            used: now_ms(),
                                        },
                                    );
                                    trim(s);
                                }
                                Err(error) => {
                                    if s.needed.contains(&key)&&error!="obsolete viewport tile"{s.errors.insert(key,error);}
                                }
                            }
                        }
                    });
                }
                if work.is_empty() {
                    let mut state = service.navigation.lock().await;
                    // Lock handoff prevents losing a viewport that arrives as the pump idles.
                    let more = state.session.as_ref().is_some_and(|s| {
                        s.meter.active().is_ok()
                            && s.meter.requests.load(Ordering::Relaxed) < s.meter.max_requests
                            && s.candidates.iter().any(|t| {
                                let k = key(s.source, &s.date, *t);
                                !s.attempted.contains(&k)
                                    && !s.cache.contains_key(&k)
                                    && !s.client_known.contains(&k)
                            })
                    });
                    if more {
                        drop(state);
                        continue;
                    }
                    state.pumping = false;
                    break;
                }
                let _ = work.join_next().await;
            }
            service.refresh_navigation_signals().await;
        });
    }
    async fn navigation_tile(
        &self,
        source: ImagerySource,
        date: &str,
        tile: TileCoordinate,
        meter: &Arc<NavigationMeter>,
    ) -> Result<(Value, usize), String> {
        let source_spec = crate::imagery::spec(source);
        let url = tile_url(source, date, tile)?;
        let (bytes, cached) = self
            .fetch_navigation(
                if source_spec.online_only {
                    "google_satellite"
                } else {
                    "nasa_gibs"
                },
                &url,
                meter.remaining().min(1024 * 1024),
                Some(Duration::from_secs(7 * 86400)),
                meter,
            )
            .await?;
        if cached {
            meter.cache_hits.fetch_add(1, Ordering::Relaxed);
        }
        let encoded = crate::imagery::encode_preview(&bytes, 2 * 1024 * 1024)?;
        let mut image = json!({"mime":encoded.mime,"data_base64":encoded.data_base64,"width":encoded.width,"height":encoded.height});
        image["retrieved_at_ms"] = json!(if cached {
            self.navigation_cached_time(&url)
        } else {
            Some(now_ms())
        });
        // Streaming transmits original tile dimensions. A view is not an AOI thumbnail.
        image["key"] = json!(key(source, date, tile));
        image["zoom"] = json!(tile.zoom);
        image["x"] = json!(tile.x);
        image["y"] = json!(tile.y);
        image["bounds"] = json!(globe::tile_bounds(tile));
        Ok((image, bytes.len()))
    }
    async fn obsolete_navigation_tile(&self, id: &str, key: &str) {
        loop {
            let current = {
                let state = self.navigation.lock().await;
                state
                    .session
                    .as_ref()
                    .is_some_and(|s| s.id == id && s.needed.contains(key))
            };
            if !current {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    pub(crate) async fn shutdown_navigation(&self) {
        let mut state = self.navigation.lock().await;
        if let Some(s) = state.session.as_mut() {
            s.meter.cancelled.store(true, Ordering::Relaxed);
            s.cache.clear();
            s.earth.clear();
        }
        if let Some((directory, _)) = &state.endpoint {
            let path = directory.join("navigation.json");
            if path.exists() && crate::exploration_control::secure_owner_path(&path, false).is_ok()
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    async fn refresh_navigation_signals(&self) {
        let request = {
            let mut state = self.navigation.lock().await;
            state.session.as_mut().and_then(|s| {
                if !s.request.signals || s.signals_in_flight || s.meter.active().is_err() {
                    return None;
                }
                let viewport = s.viewport.as_ref()?;
                let (lat, lon, radius, local) = signal_area(viewport);
                let area = if local {
                    format!(
                        "local:{:.0}:{:.0}:{:.0}",
                        lat * 20.,
                        lon * 20.,
                        radius / 10000.
                    )
                } else {
                    "global".into()
                };
                if s.signals_key == area && now_ms() - s.signals_at < 60_000 {
                    return None;
                }
                s.signals_in_flight = true;
                s.signals_key = area;
                s.signals_at = now_ms();
                Some((s.id.clone(), lat, lon, radius, local, s.meter.clone()))
            })
        };
        let Some((id, lat, lon, radius, local, meter)) = request else {
            return;
        };
        let requests = if local {
            crate::world_signals::requests(lat, lon, radius)
        } else {
            crate::world_signals::global_requests()
        };
        let mut groups: Vec<Vec<Value>> = vec![];
        let mut sources = vec![];
        for request in requests {
            let result = self
                .fetch_navigation(
                    &request.provider,
                    &request.url,
                    meter.remaining().min(2 * 1024 * 1024),
                    Some(Duration::from_secs(request.ttl_seconds)),
                    &meter,
                )
                .await;
            match result {
                Ok((bytes,cached))=>{
                    let decoded=if local{crate::world_signals::decode(&request.provider,&bytes,lat,lon,radius)}else{crate::world_signals::decode_global(&request.provider,&bytes)};
                    if cached{meter.cache_hits.fetch_add(1,Ordering::Relaxed);}
                    match decoded {Ok(batch)=>{groups.push(batch.points.iter().filter_map(|v|serde_json::to_value(v).ok()).collect());sources.push(json!({"id":request.provider,"label":request.label,"state":if cached{"cached"}else{"ready"},"retrieved_at_ms":if cached{self.navigation_cached_time(&request.url)}else{Some(now_ms())},"matched_total":batch.matched_total,"omitted_points":batch.omitted_points,"total_is_lower_bound":batch.total_is_lower_bound,"truncated":batch.truncated}));},Err(error)=>sources.push(json!({"id":request.provider,"label":request.label,"state":"error","error":error}))}
                },Err(error)=>sources.push(json!({"id":request.provider,"label":request.label,"state":"error","error":error})),
            }
        }
        let points = balanced_points(&groups, 128);
        for source in &mut sources {
            if let Some(total) = source["matched_total"].as_u64() {
                let returned = points
                    .iter()
                    .filter(|point| point["source_id"] == source["id"])
                    .count() as u64;
                source["points_returned"] = json!(returned);
                source["omitted_points"] = json!(total.saturating_sub(returned));
                if total > returned {
                    source["truncated"] = json!(true);
                }
            }
        }
        let mut state = self.navigation.lock().await;
        if let Some(s) = state.session.as_mut().filter(|s| s.id == id) {
            s.points = points;
            s.signal_sources = sources;
            s.signals_in_flight = false;
        }
    }
}
fn balanced_points(groups: &[Vec<Value>], limit: usize) -> Vec<Value> {
    let mut points = vec![];
    for index in 0..groups.iter().map(Vec::len).max().unwrap_or(0) {
        for group in groups {
            if let Some(point) = group.get(index) {
                points.push(point.clone());
                if points.len() == limit {
                    return points;
                }
            }
        }
    }
    points
}
fn signal_counts(points: &[Value], sources: &[Value]) -> Value {
    let total = sources
        .iter()
        .filter_map(|s| s["matched_total"].as_u64())
        .sum::<u64>();
    let incomplete = sources.is_empty()
        || sources
            .iter()
            .any(|s| s["total_is_lower_bound"].as_bool().unwrap_or(true));
    let omitted = total.saturating_sub(points.len() as u64);
    json!({"state":"Instantánea","points_total":total,"points_returned":points.len(),"omitted_points":omitted,"total_is_lower_bound":incomplete,"truncated":incomplete||omitted>0})
}
fn trim(session: &mut Session) {
    while session.cache.len() > CACHE_TILES
        || session.cache.values().map(|v| v.bytes).sum::<usize>() > CACHE_BYTES
    {
        let victim = session
            .cache
            .iter()
            .filter(|(_, v)| v.value["zoom"] != 0)
            .min_by_key(|(_, v)| v.used)
            .map(|(k, _)| k.clone());
        let Some(victim) = victim else {
            break;
        };
        session.cache.remove(&victim);
    }
}
fn signal_area(view: &GlobeViewport) -> (f64, f64, f64, bool) {
    let radius = globe::EARTH_RADIUS_M;
    let norm = |v: [f64; 3]| v.iter().map(|v| v * v).sum::<f64>().sqrt();
    let target = if norm(view.target) > radius * 0.9 {
        view.target
    } else {
        view.eye
    };
    let n = norm(target);
    let lat = (target[1] / n).clamp(-1., 1.).asin().to_degrees();
    let lon = target[0].atan2(target[2]).to_degrees();
    let altitude = norm(view.eye) - radius;
    (
        lat,
        lon,
        (altitude * (view.fov_degrees.to_radians() / 2.).tan() * 2.).clamp(1000., 100000.),
        altitude < 100000.,
    )
}
pub(crate) fn tile_url(
    source: ImagerySource,
    date: &str,
    tile: TileCoordinate,
) -> Result<String, String> {
    if !archie_geofabric::exploration::valid_imagery_date(date)
        || tile.zoom > crate::imagery::spec(source).max_zoom
        || tile.zoom > 19
        || tile.x >= 1 << tile.zoom
        || tile.y >= 1 << tile.zoom
    {
        return Err("invalid navigation tile".into());
    }
    let s = crate::imagery::spec(source);
    Ok(if s.online_only {
        format!(
            "https://mt1.google.com/vt/lyrs=s&x={}&y={}&z={}",
            tile.x, tile.y, tile.zoom
        )
    } else {
        let time = if s.daily {
            format!("{date}/")
        } else {
            String::new()
        };
        format!("https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/{}/default/{}GoogleMapsCompatible_Level{}/{}/{}/{}.{}",s.layer,time,s.max_zoom,tile.zoom,tile.y,tile.x,s.extension)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_observations_keep_feed_diversity_and_true_omissions() {
        let a = (0..128).map(|i| json!({"source_id":"a","id":i})).collect();
        let b = (0..47).map(|i| json!({"source_id":"b","id":i})).collect();
        let points = balanced_points(&[a, b], 128);
        assert_eq!(points.len(), 128);
        assert_eq!(points.iter().filter(|p| p["source_id"] == "b").count(), 47);
        let counts = signal_counts(
            &points,
            &[
                json!({"matched_total":174,"total_is_lower_bound":false}),
                json!({"matched_total":47,"total_is_lower_bound":false}),
            ],
        );
        assert_eq!(counts["points_total"], 221);
        assert_eq!(counts["omitted_points"], 93);
        assert_eq!(counts["total_is_lower_bound"], false);
    }
    #[tokio::test]
    async fn navigation_capability_is_separate_scoped_revocable_and_monotonic() {
        let root = std::env::temp_dir().join(format!(
            "archie-nav-capability-{:032x}",
            rand::random::<u128>()
        ));
        let runtime = root.join("runtime");
        crate::exploration_runtime::private_directory(&runtime).unwrap();
        let service = ExplorationService::new(crate::exploration_runtime::ExplorationConfig {
            cache_dir: root.join("cache"),
            domain: ExplorationDomain::Personal,
        })
        .unwrap();
        service
            .configure_navigation(runtime.clone(), "127.0.0.1:12345".into())
            .await
            .unwrap();
        let accepted = service
            .start_navigation(NavigationRequest {
                sources: vec![ImagerySource::BlueMarble],
                signals: false,
                max_visible_tiles: 1,
                ..Default::default()
            })
            .await
            .unwrap();
        let id = accepted["navigation"]["id"].as_str().unwrap();
        let cap: Value =
            serde_json::from_slice(&std::fs::read(runtime.join("navigation.json")).unwrap())
                .unwrap();
        assert_eq!(cap["session_id"], id);
        assert!(!accepted
            .to_string()
            .contains(cap["token"].as_str().unwrap()));
        assert!(service
            .navigation_status(id, ExplorationDomain::Work, false)
            .await
            .is_err());
        let envelope =
            |command: Value| json!({"token":cap["token"],"session_id":id,"command":command});
        let query = envelope(json!({"action":"status","known_keys":[]}));
        assert!(service
            .navigation_message(&serde_json::to_vec(&query).unwrap())
            .await
            .is_ok());
        let mut wrong = query.clone();
        wrong["token"] = json!("0".repeat(64));
        assert!(service
            .navigation_message(&serde_json::to_vec(&wrong).unwrap())
            .await
            .is_err());
        let injection = envelope(json!({"action":"explore","request":{}}));
        assert!(service
            .navigation_message(&serde_json::to_vec(&injection).unwrap())
            .await
            .is_err());
        // Looking away from the globe selects nothing and performs no network I/O.
        let view = json!({"eye":[0.,0.,globe::EARTH_RADIUS_M+100000.],"target":[0.,0.,globe::EARTH_RADIUS_M*3.],"width":800,"height":600,"fov_degrees":45.});
        let update = envelope(
            json!({"action":"viewport","generation":3,"source":"blue_marble","viewport":view,"known_keys":[]}),
        );
        let updated = service
            .navigation_message(&serde_json::to_vec(&update).unwrap())
            .await
            .unwrap();
        assert_eq!(updated["generation"], 3);
        assert_eq!(updated["usage"]["requests_sent"], 0);
        let mut stale = update.clone();
        stale["command"]["generation"] = json!(2);
        assert_eq!(
            service
                .navigation_message(&serde_json::to_vec(&stale).unwrap())
                .await
                .unwrap()["generation"],
            3
        );
        let mut invalid = update.clone();
        invalid["command"]["generation"] = json!(4);
        invalid["command"]["known_keys"] = json!(vec!["k"; 257]);
        assert!(service
            .navigation_message(&serde_json::to_vec(&invalid).unwrap())
            .await
            .is_err());
        assert_eq!(
            service
                .navigation_message(&serde_json::to_vec(&query).unwrap())
                .await
                .unwrap()["generation"],
            3
        );
        let date = service
            .navigation
            .lock()
            .await
            .session
            .as_ref()
            .unwrap()
            .date
            .clone();
        let root_key = key(
            ImagerySource::BlueMarble,
            &date,
            TileCoordinate {
                zoom: 0,
                x: 0,
                y: 0,
            },
        );
        let cached_view = envelope(
            json!({"action":"viewport","generation":5,"source":"blue_marble","viewport":{"eye":[0.,0.,globe::EARTH_RADIUS_M*3.],"target":[0.,0.,0.],"width":800,"height":600,"fov_degrees":45.},"known_keys":[root_key]}),
        );
        let known = service
            .navigation_message(&serde_json::to_vec(&cached_view).unwrap())
            .await
            .unwrap();
        assert_eq!(
            known["usage"]["requests_sent"], 0,
            "resident client imagery must not trigger another download"
        );
        assert_eq!(known["pending_tiles"], 0);
        service
            .navigation_status(id, ExplorationDomain::Personal, true)
            .await
            .unwrap();
        assert!(!runtime.join("navigation.json").exists());
        assert!(service
            .navigation_message(&serde_json::to_vec(&query).unwrap())
            .await
            .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn local_rate_wait_is_published_without_inventing_external_quota() {
        let mut rate = crate::exploration_runtime::RateLedger::new(1, 1);
        assert_eq!(
            rate.admit(now_ms()),
            crate::exploration_runtime::Admission::Ready
        );
        rate.finish(0, None, None);
        let status = rate.snapshot("test");
        assert_eq!(status.state, "waiting_rate_limit");
        assert!(status.retry_at_ms.unwrap() > now_ms());
        assert_eq!(status.remaining_requests, None);
        assert_eq!(status.quota_source, "local");
    }
    #[test]
    fn navigation_meter_counts_attempts_bytes_stop_and_expiry() {
        let request = NavigationRequest {
            max_requests: 2,
            max_bytes: 1024,
            ..Default::default()
        };
        let meter = NavigationMeter::new(&request);
        assert!(meter.reserve_request().is_ok());
        assert!(meter.reserve_request().is_ok());
        assert!(meter.reserve_request().is_err());
        assert!(meter.charge_bytes(1000).is_ok());
        assert_eq!(meter.remaining(), 24);
        assert!(meter.charge_bytes(25).is_err());
        let meter = NavigationMeter::new(&request);
        meter.cancelled.store(true, Ordering::Relaxed);
        assert!(meter.reserve_request().is_err());
        let expired = NavigationMeter {
            expires: now_ms() - 1,
            ..NavigationMeter::new(&request)
        };
        assert!(expired.reserve_request().is_err());
    }
    #[test]
    fn only_fixed_typed_tiles_can_construct_urls() {
        let t = TileCoordinate {
            zoom: 0,
            x: 0,
            y: 0,
        };
        assert_eq!(
            tile_url(ImagerySource::GoogleSatellite, "2026-10-20", t).unwrap(),
            "https://mt1.google.com/vt/lyrs=s&x=0&y=0&z=0"
        );
        assert!(tile_url(ImagerySource::BlueMarble, "../bad", t).is_err());
        assert!(tile_url(
            ImagerySource::BlueMarble,
            "2026-10-20",
            TileCoordinate { zoom: 19, ..t }
        )
        .is_err());
        assert!(tile_url(
            ImagerySource::GoogleSatellite,
            "2026-10-20",
            TileCoordinate { x: 1, ..t }
        )
        .is_err());
    }
}
