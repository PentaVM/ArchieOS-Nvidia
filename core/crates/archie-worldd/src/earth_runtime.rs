//! Volatile, display-only Earth Web streaming inside an admitted navigation.
use super::*;
use crate::earth::{self, Bounds, Node, Planet};
use base64::{engine::general_purpose::STANDARD, Engine};

const BASE: &str = "https://kh.google.com/rt/earth/";
const MAX_NODES: usize = 48;
const MAX_BULKS: usize = 64;
const MAX_CACHE: usize = 64 * 1024 * 1024;

struct Asset {
    raw: Vec<u8>,
    node: Node,
    used: u64,
    mesh_count: usize,
    cost: usize,
    texture_sizes: Vec<[u16; 2]>,
}

// Share immutable encoded buffers between publications; account for their wire
// cost once, rather than cloning/serializing every cached JPEG on each arrival.
#[derive(Clone)]
struct Packet {
    wire: Arc<Value>,
    wire_bytes: usize,
    payload_bytes: usize,
}
impl Packet {
    fn new(wire: Value) -> Self {
        let wire_bytes = wire.to_string().len();
        let payload_bytes = ["vertices_base64", "indices_base64", "texture_base64"]
            .iter()
            .map(|field| wire[*field].as_str().map_or(0, str::len))
            .sum();
        Self {
            wire: Arc::new(wire),
            wire_bytes,
            payload_bytes,
        }
    }
}

#[derive(Default)]
pub(super) struct EarthState {
    planet: Option<Planet>,
    bulks: BTreeMap<String, (Vec<Node>, usize)>,
    assets: BTreeMap<String, Asset>,
    packets: BTreeMap<String, Packet>,
    active: Vec<String>,
    known: BTreeSet<String>,
    generation: u64,
    running: bool,
    state: String,
    error: String,
    selected: usize,
    maximum_level: usize,
    packet_bytes: usize,
}

impl EarthState {
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
    pub(super) fn inventory(&mut self, keys: &[String]) {
        self.known = keys.iter().cloned().collect();
    }
    pub(super) fn frame(&self, budget: usize) -> (Value, Vec<Value>, bool) {
        let mut bytes = 0;
        let mut more = false;
        let mut packets = vec![];
        for key in &self.active {
            if self.known.contains(key) {
                continue;
            }
            if let Some(packet) = self.packets.get(key) {
                let size = packet.payload_bytes;
                if size + bytes > budget {
                    more = true;
                    continue;
                }
                bytes += size;
                packets.push(packet.wire.as_ref().clone());
            }
        }
        (
            json!({"state":if self.state.is_empty(){"orbit"}else{&self.state},
            "source_id":"google_earth","mode":"online_preview_only",
            "active_keys":self.active,"selected_nodes":self.selected,
            "loaded_nodes":self.assets.len(),"active_meshes":self.active.len(),
            "maximum_level":self.maximum_level,"loading":self.running,
            "more_meshes":more,"error":self.error,
            "attribution":"© Google · Earth Web experimental online 3D",
                "texture_dimensions":self.assets.values().flat_map(|a|a.texture_sizes.iter().copied()).collect::<BTreeSet<_>>(),
                "cache_bytes":self.assets.values().map(|a|a.raw.len()).sum::<usize>()+
                self.packet_bytes}),
            packets,
            more,
        )
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a).max(1e-20);
    [a[0] / n, a[1] / n, a[2] / n]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn native(center: [f64; 3], radius: f64) -> [f64; 3] {
    let s = globe::EARTH_RADIUS_M / radius;
    [center[1] * s, center[2] * s, center[0] * s]
}
fn visible(view: &GlobeViewport, bounds: &Bounds, radius: f64) -> bool {
    let center = native(bounds.center, radius);
    let r = norm(bounds.half) * globe::EARTH_RADIUS_M / radius;
    if dot(center, view.eye) + r * norm(view.eye)
        < globe::EARTH_RADIUS_M * (globe::EARTH_RADIUS_M - 12000.)
    {
        return false;
    }
    let forward = unit(sub(view.target, view.eye));
    let right = unit(cross(
        forward,
        if forward[1].abs() > 0.99 {
            [0., 0., 1.]
        } else {
            [0., 1., 0.]
        },
    ));
    let up = cross(right, forward);
    let delta = sub(center, view.eye);
    let depth = dot(delta, forward);
    let tangent = (view.fov_degrees.to_radians() / 2.).tan();
    depth + r > 0.
        && dot(delta, right).abs()
            <= (depth + r) * tangent * f64::from(view.width) / f64::from(view.height) + r
        && dot(delta, up).abs() <= (depth + r) * tangent + r
}
fn refine(view: &GlobeViewport, node: &Node, radius: f64) -> bool {
    let Some(bounds) = &node.bounds else {
        return false;
    };
    let distance = norm(sub(native(bounds.center, radius), view.eye)).max(1.);
    let pixels = norm(bounds.half) * 2. * f64::from(view.height)
        / (2. * (view.fov_degrees.to_radians() / 2.).tan() * distance);
    node.path.len() < 22 && node.flags & 4 == 0 && pixels > 512.
}
fn node_url(node: &Node) -> Result<String, String> {
    if node.path.is_empty()
        || node.path.len() > 22
        || !node.path.bytes().all(|c| (b'0'..=b'7').contains(&c))
    {
        return Err("invalid Earth node path".into());
    }
    Ok(format!(
        "{BASE}NodeData/pb=!1m2!1s{}!2u{}!2e1{}!4b0",
        node.path,
        node.epoch,
        node.imagery_epoch
            .filter(|_| node.flags & 16 != 0)
            .map(|v| format!("!3u{v}"))
            .unwrap_or_default()
    ))
}

fn prepare_asset(data: Vec<u8>, node: Node, generation: u64, radius: f64) -> Result<Asset, String> {
    let decoded = earth::decode(&data, 0, radius)?;
    let mut cost = 0;
    let mut texture_sizes = vec![];
    for mesh in &decoded {
        let bytes =
            (mesh.vertices.len() + mesh.indices.len() + mesh.jpeg.len()).div_ceil(3) * 4 + 2048;
        if bytes > 3 * 1024 * 1024 || mesh.vertex_count > 131072 || mesh.triangle_count > 262144 {
            return Err("3D mesh exceeds renderer bounds".into());
        }
        let mut jpeg = jpeg_decoder::Decoder::new(std::io::Cursor::new(&mesh.jpeg));
        jpeg.read_info().map_err(|_| "invalid Earth JPEG header")?;
        let info = jpeg.info().ok_or("Earth JPEG dimensions missing")?;
        if info.width > 4096 || info.height > 4096 {
            return Err("Earth JPEG exceeds pixel bounds".into());
        }
        texture_sizes.push([info.width, info.height]);
        cost += bytes.max(
            mesh.vertices.len()
                + mesh.indices.len()
                + usize::from(info.width) * usize::from(info.height) * 4,
        );
    }
    Ok(Asset {
        raw: data,
        node,
        used: generation,
        mesh_count: decoded.len(),
        cost,
        texture_sizes,
    })
}

impl ExplorationService {
    pub(super) async fn pump_earth(&self) {
        let mut state = self.navigation.lock().await;
        let Some(s) = state.session.as_mut() else {
            return;
        };
        let enabled = s.request.photogrammetry && s.source == ImagerySource::GoogleSatellite;
        let near = s
            .viewport
            .as_ref()
            .is_some_and(|v| norm(v.eye) - globe::EARTH_RADIUS_M < 250000.);
        if !enabled || !near {
            s.earth.active.clear();
            s.earth.state = if enabled { "orbit" } else { "disabled" }.into();
            return;
        }
        if s.earth.running || s.earth.generation == s.generation || s.meter.active().is_err() {
            return;
        }
        s.earth.running = true;
        s.earth.state = "loading".into();
        s.earth.error.clear();
        let worker_id = s.id.clone();
        let service = self.clone();
        drop(state);
        tokio::spawn(async move {
            loop {
                let snapshot = {
                    let state = service.navigation.lock().await;
                    state
                        .session
                        .as_ref()
                        .filter(|s| {
                            s.id == worker_id
                                && s.request.photogrammetry
                                && s.source == ImagerySource::GoogleSatellite
                                && s.meter.active().is_ok()
                                && s.viewport
                                    .as_ref()
                                    .is_some_and(|v| norm(v.eye) - globe::EARTH_RADIUS_M < 250000.)
                        })
                        .and_then(|s| {
                            s.viewport
                                .clone()
                                .map(|v| (s.id.clone(), s.generation, v, s.meter.clone()))
                        })
                };
                let Some((id, generation, view, meter)) = snapshot else {
                    break;
                };
                let result = service
                    .earth_generation(&id, generation, &view, &meter)
                    .await;
                let mut state = service.navigation.lock().await;
                let Some(s) = state.session.as_mut().filter(|s| {
                    s.id == id
                        && s.generation == generation
                        && s.meter.active().is_ok()
                        && s.source == ImagerySource::GoogleSatellite
                }) else {
                    break;
                };
                if s.generation != generation && s.meter.active().is_ok() {
                    continue;
                }
                s.earth.running = false;
                s.earth.generation = generation;
                match result {
                    Ok(()) => {
                        s.earth.state = if s.earth.active.is_empty() {
                            "no_coverage"
                        } else {
                            "ready"
                        }
                        .into()
                    }
                    Err(error) => {
                        s.earth.error = error;
                        s.earth.state = "partial".into();
                    }
                }
                break;
            }
            if let Some(s) = service
                .navigation
                .lock()
                .await
                .session
                .as_mut()
                .filter(|s| s.id == worker_id)
            {
                s.earth.running = false;
            }
        });
    }
    async fn earth_current(&self, id: &str, generation: u64) -> bool {
        self.navigation
            .lock()
            .await
            .session
            .as_ref()
            .is_some_and(|s| {
                s.id == id
                    && s.generation == generation
                    && s.source == ImagerySource::GoogleSatellite
                    && s.meter.active().is_ok()
                    && s.viewport
                        .as_ref()
                        .is_some_and(|v| norm(v.eye) - globe::EARTH_RADIUS_M < 250000.)
            })
    }
    async fn earth_obsolete(&self, id: &str, generation: u64) {
        while self.earth_current(id, generation).await {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    async fn earth_fetch(
        &self,
        id: &str,
        generation: u64,
        url: &str,
        meter: &NavigationMeter,
    ) -> Result<Vec<u8>, String> {
        if !self.earth_current(id, generation).await {
            return Err("obsolete 3D camera".into());
        }
        tokio::select! {data=self.fetch_navigation("google_earth",url,meter.remaining().min(8*1024*1024),None,meter)=>data.map(|(b,_)|b),
        _=self.earth_obsolete(id,generation)=>Err("obsolete 3D camera".into())}
    }
    async fn earth_generation(
        &self,
        id: &str,
        generation: u64,
        view: &GlobeViewport,
        meter: &Arc<NavigationMeter>,
    ) -> Result<(), String> {
        let cached = self
            .navigation
            .lock()
            .await
            .session
            .as_ref()
            .and_then(|s| s.earth.planet.clone());
        let planet = if let Some(p) = cached {
            p
        } else {
            let data = self
                .earth_fetch(id, generation, &format!("{BASE}PlanetoidMetadata"), meter)
                .await?;
            let p = earth::planet(&data)?;
            if let Some(s) = self.navigation.lock().await.session.as_mut().filter(|s| {
                s.id == id
                    && s.generation == generation
                    && s.meter.active().is_ok()
                    && s.source == ImagerySource::GoogleSatellite
            }) {
                s.earth.planet = Some(p.clone());
            }
            p
        };
        let mut pending = vec![(String::new(), planet.epoch)];
        let mut priority = BTreeMap::<String, f64>::new();
        let mut candidates = BTreeMap::<String, Node>::new();
        let mut visits = 0;
        while let Some((head, epoch)) = pending.pop() {
            if !self.earth_current(id, generation).await {
                return Err("obsolete 3D camera".into());
            }
            visits += 1;
            if visits > MAX_BULKS {
                break;
            }
            let cache_key = format!("{head}:{epoch}");
            let cached = self
                .navigation
                .lock()
                .await
                .session
                .as_ref()
                .and_then(|s| s.earth.bulks.get(&cache_key).map(|(b, _)| b.clone()));
            let nodes = if let Some(nodes) = cached {
                nodes
            } else {
                let data = self
                    .earth_fetch(
                        id,
                        generation,
                        &format!("{BASE}BulkMetadata/pb=!1m2!1s{head}!2u{epoch}"),
                        meter,
                    )
                    .await?;
                let nodes = earth::bulk(&data, &head, epoch)?;
                if let Some(s) = self.navigation.lock().await.session.as_mut().filter(|s| {
                    s.id == id
                        && s.generation == generation
                        && s.meter.active().is_ok()
                        && s.source == ImagerySource::GoogleSatellite
                }) {
                    if s.earth.bulks.len() >= MAX_BULKS
                        || s.earth.bulks.values().map(|(_, b)| b).sum::<usize>() + data.len()
                            > 16 * 1024 * 1024
                    {
                        s.earth.bulks.clear();
                    }
                    s.earth.bulks.insert(cache_key, (nodes.clone(), data.len()));
                }
                nodes
            };
            let mut visible_paths = BTreeSet::new();
            let mut nodes = nodes;
            nodes.sort_by_key(|n| n.path.len());
            for node in nodes {
                if node.path.len() > 22 {
                    continue;
                }
                if node.path.len() > head.len() + 1
                    && !visible_paths.contains(&node.path[..node.path.len() - 1])
                {
                    continue;
                }
                let Some(bounds) = &node.bounds else {
                    continue;
                };
                if !visible(view, bounds, planet.radius) {
                    continue;
                }
                let split = refine(view, &node, planet.radius);
                if split {
                    visible_paths.insert(node.path.clone());
                }
                if node.flags & 8 == 0 && node.path.len() >= 12 {
                    candidates.insert(node.path.clone(), node.clone());
                }
                if node.path.len() == head.len() + 4 && split {
                    priority.insert(
                        node.path.clone(),
                        norm(sub(native(bounds.center, planet.radius), view.target)),
                    );
                    pending.push((node.path.clone(), node.bulk_epoch));
                }
            }
            // Visit nearest metadata blocks first under a bounded traversal budget.
            pending.sort_by(|(a, _), (b, _)| {
                priority
                    .get(b)
                    .unwrap_or(&f64::MAX)
                    .total_cmp(priority.get(a).unwrap_or(&f64::MAX))
            });
        }
        let mut leaves = candidates
            .values()
            .filter(|n| !(0..8).any(|k| candidates.contains_key(&format!("{}{k}", n.path))))
            .cloned()
            .collect::<Vec<_>>();
        leaves.sort_by(|a, b| {
            let distance = |n: &Node| {
                n.bounds.as_ref().map_or(f64::MAX, |v| {
                    norm(sub(native(v.center, planet.radius), view.target))
                })
            };
            distance(a).total_cmp(&distance(b))
        });
        let mut selected = BTreeSet::new();
        for leaf in leaves {
            let bundle = (12..=leaf.path.len())
                .filter_map(|depth| candidates.get(&leaf.path[..depth]).map(|n| n.path.clone()))
                .collect::<Vec<_>>();
            if selected.len() + bundle.iter().filter(|p| !selected.contains(*p)).count()
                <= MAX_NODES
            {
                selected.extend(bundle);
            }
        }
        let mut nodes = selected
            .into_iter()
            .filter_map(|p| candidates.remove(&p))
            .collect::<Vec<_>>();
        nodes.sort_by_key(|n| n.path.len());
        if let Some(s) = self.navigation.lock().await.session.as_mut().filter(|s| {
            s.id == id
                && s.generation == generation
                && s.meter.active().is_ok()
                && s.source == ImagerySource::GoogleSatellite
        }) {
            s.earth.selected = nodes.len();
            s.earth.maximum_level = nodes.iter().map(|n| n.path.len()).max().unwrap_or(0);
        }
        let paths = nodes
            .iter()
            .map(|n| n.path.clone())
            .collect::<BTreeSet<_>>();
        // Reuse the complete cached plan once. The previous path re-published
        // and re-serialized all cached geometry for every individual node.
        self.publish_earth(id, generation, &paths, planet.radius)
            .await?;
        let mut pending = std::collections::VecDeque::from(nodes);
        let mut work = tokio::task::JoinSet::new();
        let mut changed = 0usize;
        let mut last_publication = std::time::Instant::now();
        let mut failure = None;
        loop {
            if !self.earth_current(id, generation).await {
                work.abort_all();
                return Err("obsolete 3D camera".into());
            }
            while work.len() < 4 {
                let Some(node) = pending.pop_front() else {
                    break;
                };
                let cached = self
                    .navigation
                    .lock()
                    .await
                    .session
                    .as_ref()
                    .is_some_and(|s| s.id == id && s.earth.assets.contains_key(&node.path));
                if cached {
                    continue;
                }
                let service = self.clone();
                let id = id.to_string();
                let meter = meter.clone();
                let radius = planet.radius;
                work.spawn(async move {
                    let data = service
                        .earth_fetch(&id, generation, &node_url(&node)?, &meter)
                        .await?;
                    tokio::task::spawn_blocking(move || {
                        prepare_asset(data, node, generation, radius)
                    })
                    .await
                    .map_err(|_| "3D decoder task failed".to_string())?
                });
            }
            let Some(result) = work.join_next().await else {
                break;
            };
            let asset = match result {
                Ok(Ok(asset)) => asset,
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                    continue;
                }
                Err(_) => {
                    failure.get_or_insert("3D download task failed".into());
                    continue;
                }
            };
            let mut state = self.navigation.lock().await;
            let Some(s) = state.session.as_mut().filter(|s| {
                s.id == id
                    && s.generation == generation
                    && s.meter.active().is_ok()
                    && s.source == ImagerySource::GoogleSatellite
            }) else {
                work.abort_all();
                return Err("obsolete 3D asset".into());
            };
            let path = asset.node.path.clone();
            s.earth.assets.insert(path.clone(), asset);
            while s.earth.assets.len() > MAX_NODES
                || s.earth.assets.values().map(|a| a.raw.len()).sum::<usize>() > 32 * 1024 * 1024
            {
                let victim = s
                    .earth
                    .assets
                    .iter()
                    .filter(|(key, _)| *key != &path)
                    .min_by_key(|(key, a)| (paths.contains(*key), a.used, key.len()))
                    .map(|(k, _)| k.clone());
                if let Some(victim) = victim {
                    s.earth.assets.remove(&victim);
                } else {
                    break;
                }
            }
            let first = s.earth.active.is_empty();
            drop(state);
            changed += 1;
            if first || changed >= 4 || last_publication.elapsed() >= Duration::from_millis(120) {
                self.publish_earth(id, generation, &paths, planet.radius)
                    .await?;
                changed = 0;
                last_publication = std::time::Instant::now();
            }
        }
        if changed > 0 {
            self.publish_earth(id, generation, &paths, planet.radius)
                .await?;
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(())
    }
    async fn publish_earth(
        &self,
        id: &str,
        generation: u64,
        paths: &BTreeSet<String>,
        radius: f64,
    ) -> Result<(), String> {
        let mut state = self.navigation.lock().await;
        let Some(s) = state.session.as_mut().filter(|s| {
            s.id == id
                && s.generation == generation
                && s.meter.active().is_ok()
                && s.source == ImagerySource::GoogleSatellite
        }) else {
            return Err("obsolete 3D publication".into());
        };
        let mut packets = BTreeMap::new();
        let mut active = vec![];
        let mut size = 0;
        let mut ready = BTreeSet::new();
        let mut meshes = 0;
        let mut reserved = 0;
        let mut ordered = s
            .earth
            .assets
            .iter()
            .filter(|(path, _)| paths.contains(*path))
            .collect::<Vec<_>>();
        ordered.sort_by_key(|(path, _)| path.len());
        for (path, asset) in ordered {
            if asset.mesh_count > 0
                && meshes + asset.mesh_count <= 64
                && reserved + asset.cost <= MAX_CACHE
            {
                meshes += asset.mesh_count;
                reserved += asset.cost;
                ready.insert(path.clone());
            }
        }
        for (path, asset) in &s.earth.assets {
            if !ready.contains(path) {
                continue;
            }
            let mask = (0..8).fold(0u8, |mask, child| {
                if ready.contains(&format!("{path}{child}")) {
                    mask | (1 << child)
                } else {
                    mask
                }
            });
            if mask == 255 {
                continue;
            }
            let prefix = format!("earth:{path}:{}:{mask}:", asset.node.epoch);
            let mut decoded = s
                .earth
                .packets
                .range(prefix.clone()..)
                .take_while(|(key, _)| key.starts_with(&prefix))
                .map(|(_, p)| p.clone())
                .collect::<Vec<_>>();
            if decoded.is_empty() {
                for (index, mesh) in earth::decode(&asset.raw, mask, radius)?
                    .into_iter()
                    .enumerate()
                {
                    let key = format!("{prefix}{index}");
                    let mut jpeg = jpeg_decoder::Decoder::new(std::io::Cursor::new(&mesh.jpeg));
                    jpeg.read_info().map_err(|_| "invalid 3D JPEG header")?;
                    let info = jpeg.info().ok_or("missing 3D JPEG dimensions")?;
                    if info.width > 4096 || info.height > 4096 {
                        return Err("3D JPEG dimensions exceed renderer bounds".into());
                    }
                    decoded.push(Packet::new(json!({"key":key,"node_path":path,"origin":mesh.origin,"vertices_base64":STANDARD.encode(&mesh.vertices),"indices_base64":STANDARD.encode(&mesh.indices),"texture_base64":STANDARD.encode(&mesh.jpeg),"vertex_count":mesh.vertex_count,"triangle_count":mesh.triangle_count,"replace_mask":mask,"attribution":"© Google"})));
                }
            }
            for packet in decoded {
                let key = packet.wire["key"]
                    .as_str()
                    .ok_or("invalid cached3D key")?
                    .to_string();
                let bytes = packet.wire_bytes;
                if bytes > 3 * 1024 * 1024 {
                    return Err("3D node exceeds live-frame mesh bound".into());
                }
                if active.len() >= 64 || size + bytes > MAX_CACHE {
                    continue;
                }
                size += bytes;
                active.push(key.clone());
                packets.insert(key, packet);
            }
        }
        s.earth.packets = packets;
        s.earth.packet_bytes = size;
        s.earth.active = active;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_packets_share_buffers_and_keep_delta_and_clear_bounds() {
        let value = json!({"key":"earth:test","vertices_base64":"abcd","indices_base64":"efgh","texture_base64":"ijkl"});
        let serialized = value.to_string().len();
        let packet = Packet::new(value);
        let wire = packet.wire.clone();
        let reused = packet.clone();
        assert!(Arc::ptr_eq(&wire, &reused.wire));
        assert_eq!(packet.wire_bytes, serialized);
        assert_eq!(packet.payload_bytes, 12);
        drop(reused);
        let mut state = EarthState::default();
        state.active.push("earth:test".into());
        state.packets.insert("earth:test".into(), packet);
        let (_, small, more) = state.frame(11);
        assert!(small.is_empty() && more);
        let (_, full, more) = state.frame(12);
        assert_eq!(full.len(), 1);
        assert!(!more);
        state.inventory(&["earth:test".into()]);
        let (_, known, more) = state.frame(0);
        assert!(known.is_empty() && !more);
        assert_eq!(Arc::strong_count(&wire), 2);
        state.clear();
        assert_eq!(Arc::strong_count(&wire), 1);
    }
    #[tokio::test]
    async fn stop_blocks_late_mesh_publication_and_old_worker_cannot_adopt_new_session() {
        let root = std::env::temp_dir().join(format!(
            "archie-earth-revoke-{:032x}",
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
            .configure_navigation(runtime, "127.0.0.1:12345".into())
            .await
            .unwrap();
        let request = NavigationRequest {
            signals: false,
            ..Default::default()
        };
        let old = service.start_navigation(request.clone()).await.unwrap();
        let old_id = old["navigation"]["id"].as_str().unwrap();
        let view = GlobeViewport {
            eye: [0., 0., globe::EARTH_RADIUS_M + 1000.],
            target: [0., 0., globe::EARTH_RADIUS_M],
            width: 800,
            height: 600,
            fov_degrees: 45.,
        };
        {
            let mut state = service.navigation.lock().await;
            let s = state.session.as_mut().unwrap();
            s.viewport = Some(view.clone());
            s.generation = 1;
        }
        service.pump_earth().await; // Queue worker without yielding the current-thread executor.
        service
            .navigation_status(old_id, ExplorationDomain::Personal, true)
            .await
            .unwrap();
        let late = service
            .publish_earth(old_id, 1, &BTreeSet::new(), globe::EARTH_RADIUS_M)
            .await;
        assert!(
            late.is_err(),
            "revoked session must reject even a queued empty publication"
        );
        service.start_navigation(request).await.unwrap();
        {
            let mut state = service.navigation.lock().await;
            let s = state.session.as_mut().unwrap();
            s.viewport = Some(view);
            s.generation = 1;
            Arc::get_mut(&mut s.meter).unwrap().max_requests = 0;
        }
        // A broken old worker would adopt the replacement, fail its zero-request
        // test meter without any HTTP, and mutate that session's 3D generation.
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(
            service
                .navigation
                .lock()
                .await
                .session
                .as_ref()
                .unwrap()
                .earth
                .generation,
            0
        );
        service.shutdown_navigation().await;
        assert_eq!(
            service
                .navigation
                .lock()
                .await
                .session
                .as_ref()
                .unwrap()
                .earth
                .frame(0)
                .0["active_meshes"],
            0
        );
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn fixed_earth_addresses_cannot_become_arbitrary_urls() {
        let node = Node {
            path: "03127".into(),
            flags: 0,
            epoch: 1016,
            bulk_epoch: 1016,
            imagery_epoch: None,
            bounds: None,
        };
        assert_eq!(
            node_url(&node).unwrap(),
            "https://kh.google.com/rt/earth/NodeData/pb=!1m2!1s03127!2u1016!2e1!4b0"
        );
        assert!(node_url(&Node {
            path: "../../evil".into(),
            ..node
        })
        .is_err());
    }
    #[test]
    fn near_camera_rejects_far_side_and_offscreen_3d_nodes() {
        let r = globe::EARTH_RADIUS_M;
        let view = GlobeViewport {
            eye: [0., 0., r + 1000.],
            target: [0., 0., r],
            width: 1200,
            height: 800,
            fov_degrees: 45.,
        };
        let mut bounds = Bounds {
            center: [r, 0., 0.],
            half: [20.; 3],
            axes: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        };
        assert!(visible(&view, &bounds, r));
        bounds.center = [-r, 0., 0.];
        assert!(!visible(&view, &bounds, r));
        bounds.center = [r, 100000., 0.];
        assert!(!visible(&view, &bounds, r));
    }
}
