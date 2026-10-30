//! The neural sphere's scene state without any graphics: where every node is now, what is leaving, the live
//! build, the tree's curves, the links, the selection, the sparks and the labels. Port of Lumenfield's
//! `neural-sphere/{scene,pulses,hud,index}.ts`. The host only uploads the buffers it fills and draws them with the
//! materials of the original (additive points and lines, bloom, hologram pass).

use std::collections::{HashMap, HashSet};

use super::links::{pick_budget, structural_links, IdLink, LinkPoint, KIND_WEIGHT};
use super::model::{hash, Processes, SphereModel, SphereNode, CATEGORIES, FOLDERS};
use super::motion::{
    birth_schedule, born_progress, smooth_factor, step_capped, MAX_SPEED, SETTLE_DISTANCE,
};
use super::settings::Settings;
use super::v3::{self, V3};

/// Segments of every tree curve (scene.ts `SEG`).
pub const SEG: usize = 10;
pub const PULSES: usize = 70;
pub const TRAIL: usize = 5;
/// Line layers: 0 skeleton and axons (one layer, as the original), 2 to 6 the other kinds, 7 the selection.
pub const LINE_LAYERS: usize = 8;
pub const HOT_LAYER: usize = 7;

/// sRGB hex to the linear colour three.js holds (`new Color('#rrggbb')` with colour management on).
pub fn linear_colour(hex: u32) -> [f64; 3] {
    let channel = |shift: u32| {
        let c = f64::from((hex >> shift) & 0xff) / 255.0;
        if c < 0.04045 {
            c * 0.077_399_380_8
        } else {
            (c * 0.947_867_298_6 + 0.052_132_701_4).powf(2.4)
        }
    };
    [channel(16), channel(8), channel(0)]
}

pub const CATEGORY_HEX: [u32; 7] = [
    0x72c8ff, 0xffc87d, 0x58e1bf, 0xb4dc89, 0xbd9cf4, 0xf48d82, 0xb4c7c9,
];
pub const FOLDER_HEX: u32 = 0xdfeaee;
pub const CREAM_HEX: u32 = 0xf3e3bf;
pub const HOT_HEX: u32 = 0xffb25a;

/// One drawn point: position, linear colour, kind (0 file, 1 folder, 2 process), the recent-file flicker seed,
/// the ring of a folded folder and how far it has been born (0 to 1).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NeuralPoint {
    pub position: [f32; 3],
    pub colour: [f32; 3],
    pub kind: f32,
    pub fire: f32,
    pub ring: f32,
    pub born: f32,
}

/// One line vertex: position and linear colour (segments, two vertices each).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NeuralVertex {
    pub position: [f32; 3],
    pub colour: [f32; 3],
}

/// One spark point: position, colour and fade.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NeuralSpark {
    pub position: [f32; 3],
    pub colour: [f32; 3],
    pub fade: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    pub visible: usize,
    pub moving: usize,
    pub leaving: usize,
    pub max_step: f64,
    pub dt: f64,
    pub born: usize,
    pub total: usize,
    pub time: f64,
    pub rebuilds: u64,
    pub sparks: usize,
}

/// What a node that is leaving still needs: it keeps being drawn while it folds back into its parent.
#[derive(Debug, Clone)]
struct Ghost {
    kind: u8,
    cat: usize,
    folded: bool,
    dir: Option<V3>,
    parent: String,
}

#[derive(Debug, Clone)]
enum Item {
    Node(usize),
    Process(usize),
    Leaving(String),
}

#[derive(Debug, Clone)]
struct Edge {
    id: String,
    pts: Vec<V3>,
}

#[derive(Debug, Clone, Default)]
struct Pulse {
    path: Option<Vec<V3>>,
    t: f64,
    speed: f64,
    colour: [f64; 3],
}

/// pulses.ts `makeRng` (the artifact's line 39).
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed)
    }
    pub fn float(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// A label for the HUD: top-level folders and running programs, where they are now.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub id: String,
    pub text: String,
    pub position: V3,
    pub process: bool,
}

pub struct Sphere {
    pub model: SphereModel,
    pub settings: Settings,
    cur: HashMap<String, V3>,
    leaving: Vec<(String, Ghost)>,
    born_at: Option<HashMap<String, f64>>,
    clock: f64,
    moving: bool,
    built_once: bool,
    still: bool,
    edges: Vec<Edge>,
    edge_index: HashMap<String, usize>,
    items: Vec<Item>,
    selected: String,
    tree_factor: [f64; 2],
    topology: Vec<IdLink>,
    extra: Vec<IdLink>,
    structural: Option<Vec<IdLink>>,
    counts: [usize; 2],
    recent: HashSet<String>,
    rng: Rng,
    pulses: Vec<Pulse>,
    active: usize,
    pulse_pos: Vec<f64>,
    pulse_fade: Vec<f64>,
    pulse_colour: Vec<f64>,
    last: Stats,
    rebuilds: u64,
    /// Bumped whenever the buffers change, so the host uploads only then.
    pub revision: u64,
    pub points: Vec<NeuralPoint>,
    pub lines: [Vec<NeuralVertex>; LINE_LAYERS],
}

fn f32x3(v: V3) -> [f32; 3] {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

impl Sphere {
    /// None without a root. `selected` is the first selection (the root if absent); `still` freezes motion
    /// and the build, as the original's deterministic frame.
    pub fn new(
        nodes: &[SphereNode],
        processes: Processes,
        seed: u32,
        selected: Option<&str>,
        still: bool,
    ) -> Option<Self> {
        let model = SphereModel::new(nodes, true, processes, &[])?;
        let root = model.id(model.root).to_string();
        let mut sphere = Sphere {
            model,
            settings: Settings::default(),
            cur: HashMap::new(),
            leaving: Vec::new(),
            born_at: None,
            clock: 0.0,
            moving: false,
            built_once: false,
            still,
            edges: Vec::new(),
            edge_index: HashMap::new(),
            items: Vec::new(),
            selected: root,
            tree_factor: [1.0, 1.0],
            topology: Vec::new(),
            extra: Vec::new(),
            structural: None,
            counts: [0, 0],
            recent: HashSet::new(),
            rng: Rng::new(seed),
            pulses: vec![Pulse::default(); PULSES],
            active: PULSES,
            pulse_pos: vec![0.0; PULSES * TRAIL * 3],
            pulse_fade: vec![0.0; PULSES * TRAIL],
            pulse_colour: vec![0.0; PULSES * TRAIL * 3],
            last: Stats::default(),
            rebuilds: 0,
            revision: 0,
            points: Vec::new(),
            lines: Default::default(),
        };
        sphere.refresh_recent();
        sphere.apply_settings();
        sphere.relayout(false, None);
        if let Some(id) = selected.filter(|id| sphere.model.by_id.contains_key(*id)) {
            sphere.selected = id.to_string();
            sphere.build_selection();
        }
        Some(sphere)
    }

    // ---------- colours ----------
    fn colour_of_cat(cat: usize) -> [f64; 3] {
        linear_colour(if cat == FOLDERS {
            FOLDER_HEX
        } else {
            CATEGORY_HEX[cat]
        })
    }

    fn item_id(&self, item: &Item) -> String {
        match item {
            Item::Node(i) => self.model.id(*i).to_string(),
            Item::Process(p) => self.model.processes[*p].id.clone(),
            Item::Leaving(id) => id.clone(),
        }
    }

    /// (kind, colour, folded) of something drawn.
    fn item_look(&self, item: &Item) -> (u8, [f64; 3], bool) {
        match item {
            Item::Node(i) => {
                let n = &self.model.nodes[*i];
                (u8::from(n.node.k), Self::colour_of_cat(n.cat), n.folded)
            }
            Item::Process(_) => (2, linear_colour(CREAM_HEX), false),
            Item::Leaving(id) => {
                let g = &self.ghost(id).expect("leaving entry");
                let colour = if g.kind == 2 {
                    linear_colour(CREAM_HEX)
                } else {
                    Self::colour_of_cat(g.cat)
                };
                (g.kind, colour, g.folded)
            }
        }
    }

    fn ghost(&self, id: &str) -> Option<&Ghost> {
        self.leaving.iter().find(|(i, _)| i == id).map(|(_, g)| g)
    }

    fn snapshot(&self) -> Vec<(String, Ghost)> {
        self.items
            .iter()
            .map(|item| {
                let id = self.item_id(item);
                let ghost = match item {
                    Item::Node(i) => {
                        let n = &self.model.nodes[*i];
                        Ghost {
                            kind: u8::from(n.node.k),
                            cat: n.cat,
                            folded: n.folded,
                            dir: n.dir,
                            parent: n.node.p.clone().unwrap_or_else(|| ".".into()),
                        }
                    }
                    Item::Process(p) => {
                        let p = &self.model.processes[*p];
                        Ghost {
                            kind: 2,
                            cat: FOLDERS,
                            folded: false,
                            dir: Some(p.dir),
                            parent: self.model.id(p.host).to_string(),
                        }
                    }
                    Item::Leaving(id) => self.ghost(id).cloned().expect("leaving entry"),
                };
                (id, ghost)
            })
            .collect()
    }

    /// The 60 most recently modified files of the model flicker.
    fn refresh_recent(&mut self) {
        let mut files: Vec<&super::model::Entry> =
            self.model.nodes.iter().filter(|n| !n.node.k).collect();
        files.sort_by(|a, b| {
            b.node
                .m
                .partial_cmp(&a.node.m)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.recent = files
            .into_iter()
            .take(60)
            .map(|n| n.node.id.clone())
            .collect();
    }

    // ---------- live build ----------
    pub fn progress(&self, id: &str) -> f64 {
        match self.born_at.as_ref().and_then(|b| b.get(id)) {
            Some(&t) => born_progress(self.clock, t),
            None => 1.0,
        }
    }

    pub fn is_born(&self, id: &str) -> bool {
        match self.born_at.as_ref().and_then(|b| b.get(id)) {
            Some(&t) => self.clock > t,
            None => true,
        }
    }

    pub fn born_count(&self) -> (usize, usize) {
        let total = self.model.vis.len();
        if self.born_at.is_none() {
            return (total, total);
        }
        (
            self.model
                .vis
                .iter()
                .filter(|&&i| self.is_born(self.model.id(i)))
                .count(),
            total,
        )
    }

    /// scene.ts `beginGrow`: everything starts at the root and is born breadth first over `duration` seconds.
    pub fn begin_grow(&mut self, duration: f64) {
        if self.still {
            return;
        }
        let mut list: Vec<(String, Option<String>, u32)> = self
            .model
            .vis
            .iter()
            .map(|&i| {
                (
                    self.model.id(i).to_string(),
                    self.model.nodes[i].node.p.clone(),
                    self.model.nodes[i].node.d,
                )
            })
            .collect();
        for p in &self.model.processes {
            list.push((
                p.id.clone(),
                Some(self.model.id(p.host).to_string()),
                self.model.nodes[p.host].node.d + 1,
            ));
        }
        self.born_at = Some(birth_schedule(&list, duration));
        for c in self.cur.values_mut() {
            *c = [0.0; 3];
        }
        self.clock = 0.0;
        self.moving = true;
        self.rebuild();
        self.respawn_all();
    }

    pub fn building(&self) -> bool {
        let (born, total) = self.born_count();
        self.born_at.is_some() && (born < total || self.moving)
    }

    // ---------- geometry ----------
    /// scene.ts `curvePts`: the tree's cubic curve from a parent to a child.
    pub(crate) fn curve(p: V3, pdir: V3, c: V3, cdir: V3, is_root: bool) -> Vec<V3> {
        let len = v3::distance(c, p);
        let p1 = if is_root {
            v3::scale(c, 0.3)
        } else {
            v3::add_scaled(p, pdir, len * 0.45)
        };
        let p2 = v3::add_scaled(c, cdir, -len * 0.3);
        v3::bezier_points(p, p1, p2, c, SEG)
    }

    /// scene.ts `linkPoints`: a mesh link is a segment, the rest a curve that sinks towards the centre.
    pub fn link_points(p: V3, c: V3, kind: usize) -> Vec<V3> {
        if kind == 2 {
            return vec![p, c];
        }
        let chord = v3::distance(p, c);
        let mid = v3::scale(v3::add(p, c), 0.5);
        let off = if v3::length_sq(mid) > 1e-9 {
            v3::scale(v3::normalize(mid), -0.35 * chord)
        } else {
            [0.0; 3]
        };
        v3::bezier_points(
            p,
            v3::add(v3::lerp(p, c, 1.0 / 3.0), off),
            v3::add(v3::lerp(p, c, 2.0 / 3.0), off),
            c,
            SEG,
        )
    }

    fn dir_of(&self, id: &str) -> Option<V3> {
        if let Some(&i) = self.model.by_id.get(id) {
            return self.model.nodes[i].dir;
        }
        self.ghost(id).and_then(|g| g.dir)
    }

    #[allow(clippy::too_many_arguments)]
    fn add_tree(
        &mut self,
        lines: &mut Vec<NeuralVertex>,
        id: &str,
        parent: &str,
        colour: [f64; 3],
        k: f64,
        kind: usize,
    ) {
        let (Some(&p), Some(&c)) = (self.cur.get(parent), self.cur.get(id)) else {
            return;
        };
        let cdir = v3::normalize(c);
        let pdir = self.dir_of(parent).unwrap_or(cdir);
        let pts = Self::curve(p, pdir, c, cdir, parent == ".");
        let f = self.tree_factor[kind] * self.progress(id);
        if f > 0.0 {
            let col = [
                (colour[0] * k * f) as f32,
                (colour[1] * k * f) as f32,
                (colour[2] * k * f) as f32,
            ];
            for s in 0..SEG {
                lines.push(NeuralVertex {
                    position: f32x3(pts[s]),
                    colour: col,
                });
                lines.push(NeuralVertex {
                    position: f32x3(pts[s + 1]),
                    colour: col,
                });
            }
        }
        self.edge_index.insert(id.to_string(), self.edges.len());
        self.edges.push(Edge {
            id: id.to_string(),
            pts,
        });
        self.counts[kind] += 1;
    }

    /// scene.ts `rebuild`: every buffer from the current positions.
    fn rebuild(&mut self) {
        self.rebuilds += 1;
        self.revision += 1;
        self.edges.clear();
        self.edge_index.clear();
        self.counts = [0, 0];
        let mut tree = Vec::new();
        let root_vw = f64::from(self.model.nodes[self.model.root].vw);
        let vis = self.model.vis.clone();
        for &i in &vis {
            if i == self.model.root {
                continue;
            }
            let n = &self.model.nodes[i];
            let (id, parent) = (
                n.node.id.clone(),
                n.node.p.clone().unwrap_or_else(|| ".".into()),
            );
            let k = if parent == "." && !n.node.k {
                0.08
            } else if n.node.k {
                0.14 + 0.3 * f64::from(n.vw).ln_1p() / root_vw.ln_1p()
            } else {
                0.15
            };
            let (colour, kind) = (Self::colour_of_cat(n.cat), usize::from(!n.node.k));
            self.add_tree(&mut tree, &id, &parent, colour, k, kind);
        }
        let processes: Vec<(String, String)> = self
            .model
            .processes
            .iter()
            .map(|p| (p.id.clone(), self.model.id(p.host).to_string()))
            .collect();
        for (id, host) in processes {
            self.add_tree(&mut tree, &id, &host, linear_colour(CREAM_HEX), 0.5, 1);
        }
        let leaving = self.leaving.clone();
        for (id, g) in &leaving {
            let colour = if g.kind == 2 {
                linear_colour(CREAM_HEX)
            } else {
                Self::colour_of_cat(g.cat)
            };
            let k = if g.kind == 2 {
                0.5
            } else if g.kind == 1 {
                0.14
            } else {
                0.15
            };
            self.add_tree(
                &mut tree,
                id,
                &g.parent,
                colour,
                k,
                if g.kind == 1 { 0 } else { 1 },
            );
        }
        self.lines[0] = tree;
        self.draw_topology();
        self.built_once = true;
        self.items = vis.iter().map(|&i| Item::Node(i)).collect();
        self.items
            .extend((0..self.model.processes.len()).map(Item::Process));
        self.items
            .extend(self.leaving.iter().map(|(id, _)| Item::Leaving(id.clone())));
        let mut points = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let id = self.item_id(item);
            let (kind, colour, folded) = self.item_look(item);
            let at = self.cur.get(&id).copied().unwrap_or([0.0; 3]);
            points.push(NeuralPoint {
                position: f32x3(at),
                colour: f32x3(colour),
                kind: f32::from(kind),
                fire: if self.recent.contains(&id) {
                    (0.1 + hash(&id)) as f32
                } else {
                    0.0
                },
                ring: if folded && kind == 1 { 1.0 } else { 0.0 },
                born: self.progress(&id) as f32,
            });
        }
        self.points = points;
        self.build_selection();
    }

    fn draw_topology(&mut self) {
        let mut layers: [Vec<NeuralVertex>; 5] = Default::default();
        for l in &self.topology {
            let (Some(&p), Some(&c)) = (self.cur.get(&l.a), self.cur.get(&l.b)) else {
                continue;
            };
            let born = self.progress(&l.a).min(self.progress(&l.b));
            if born <= 0.0 {
                continue;
            }
            let colour = match self.model.by_id.get(&l.a) {
                Some(&i) => Self::colour_of_cat(self.model.nodes[i].cat),
                None => linear_colour(CREAM_HEX),
            };
            let col = [
                (colour[0] * l.weight * born) as f32,
                (colour[1] * l.weight * born) as f32,
                (colour[2] * l.weight * born) as f32,
            ];
            let pts = Self::link_points(p, c, l.kind);
            for s in 0..pts.len() - 1 {
                layers[l.kind - 2].push(NeuralVertex {
                    position: f32x3(pts[s]),
                    colour: col,
                });
                layers[l.kind - 2].push(NeuralVertex {
                    position: f32x3(pts[s + 1]),
                    colour: col,
                });
            }
        }
        for (k, layer) in layers.into_iter().enumerate() {
            self.lines[k + 2] = layer;
        }
    }

    /// scene.ts `buildSelection`: the path to the root, and, for a folder, everything under it.
    fn build_selection(&mut self) {
        let hot = linear_colour(HOT_HEX);
        let mut out = Vec::new();
        let mut push = |pts: &[V3], k: f64| {
            let col = [
                (hot[0] * k) as f32,
                (hot[1] * k) as f32,
                (hot[2] * k) as f32,
            ];
            for s in 0..SEG {
                out.push(NeuralVertex {
                    position: f32x3(pts[s]),
                    colour: col,
                });
                out.push(NeuralVertex {
                    position: f32x3(pts[s + 1]),
                    colour: col,
                });
            }
        };
        let mut id = Some(self.selected.clone());
        let mut hops = 0;
        while let Some(a) = id.filter(|a| a != "." && hops < 64) {
            if let Some(&e) = self.edge_index.get(&a) {
                push(&self.edges[e].pts, 0.9);
            }
            id = self
                .model
                .by_id
                .get(&a)
                .and_then(|&i| self.model.nodes[i].node.p.clone());
            hops += 1;
        }
        let folder = self
            .model
            .by_id
            .get(&self.selected)
            .is_some_and(|&i| self.model.nodes[i].node.k);
        if folder {
            let prefix = if self.selected == "." {
                String::new()
            } else {
                format!("{}/", self.selected)
            };
            for e in &self.edges {
                if e.id.starts_with(&prefix) && e.id != self.selected {
                    push(&e.pts, 0.25);
                }
            }
        }
        self.lines[HOT_LAYER] = out;
        self.revision += 1;
    }

    // ---------- layout changes ----------
    fn spawn_point(&self, id: &str) -> V3 {
        let mut a = self.model.parent_of(id);
        while let Some(x) = a.clone() {
            if self.cur.contains_key(&x) {
                break;
            }
            a = self
                .model
                .by_id
                .get(&x)
                .and_then(|&i| self.model.nodes[i].node.p.clone());
        }
        self.cur
            .get(a.as_deref().unwrap_or("."))
            .copied()
            .unwrap_or([0.0; 3])
    }

    fn parent_cur(&self, id: &str) -> V3 {
        self.model
            .parent_of(id)
            .and_then(|p| self.cur.get(&p).copied())
            .unwrap_or([0.0; 3])
    }

    /// scene.ts `relayout`: the model has a new layout (already applied). `before` is what was drawn before it.
    fn relayout(&mut self, animate: bool, before: Option<Vec<(String, Ghost)>>) {
        if !animate || self.still {
            self.cur.clear();
            self.leaving.clear();
            self.born_at = None;
            for id in &self.model.target_order {
                self.cur.insert(id.clone(), self.model.target[id]);
            }
            self.moving = false;
            self.structural = None;
            self.refresh_topology();
            self.rebuild();
            self.respawn_all();
            return;
        }
        let order = self.model.target_order.clone();
        self.leaving
            .retain(|(id, _)| !self.model.target.contains_key(id));
        for (id, ghost) in before.unwrap_or_default() {
            if !self.model.target.contains_key(&id) && id != "." && self.ghost(&id).is_none() {
                self.leaving.push((id, ghost));
            }
        }
        for id in order {
            if !self.cur.contains_key(&id) {
                let at = self.spawn_point(&id);
                self.cur.insert(id.clone(), at);
                let clock = self.clock;
                if let Some(b) = self.born_at.as_mut() {
                    b.insert(id, clock);
                }
            }
        }
        self.moving = true;
        self.structural = None;
        self.refresh_topology();
    }

    /// scene.ts `stepMotion`: every node closes in on its target, smoothed and capped; nothing jumps.
    fn step_motion(&mut self, dt: f64) {
        self.last.dt = dt;
        self.clock += dt;
        if !self.moving {
            self.last.max_step = 0.0;
            self.last.moving = 0;
            return;
        }
        let r = self.model.radius;
        let (k, cap, settle) = (
            smooth_factor(dt, self.settings.motion_response),
            MAX_SPEED * r * dt,
            SETTLE_DISTANCE * r,
        );
        let (mut max, mut moving) = (0.0_f64, 0);
        for id in self.model.target_order.clone() {
            let to = self.model.target[&id];
            let mut c = match self.cur.get(&id) {
                Some(&c) => c,
                None => self.spawn_point(&id),
            };
            if !self.is_born(&id) {
                let pc = self.parent_cur(&id);
                max = max.max(v3::distance(c, pc));
                self.cur.insert(id, pc);
                moving += 1;
                continue;
            }
            let d = step_capped(&mut c, to, k, cap, settle);
            max = max.max(d);
            if c != to || self.progress(&id) < 1.0 {
                moving += 1;
            }
            self.cur.insert(id, c);
        }
        let waited: HashSet<String> = self.leaving.iter().map(|(_, g)| g.parent.clone()).collect();
        let mut done = Vec::new();
        for (id, g) in self.leaving.clone() {
            let Some(mut c) = self.cur.get(&id).copied() else {
                done.push(id);
                continue;
            };
            let mut p = Some(g.parent.clone());
            while let Some(x) = p.clone() {
                if self.cur.contains_key(&x) {
                    break;
                }
                p = self
                    .model
                    .by_id
                    .get(&x)
                    .and_then(|&i| self.model.nodes[i].node.p.clone())
                    .or_else(|| self.ghost(&x).map(|g| g.parent.clone()));
            }
            let to = p
                .and_then(|x| self.cur.get(&x).copied())
                .unwrap_or([0.0; 3]);
            let d = step_capped(&mut c, to, k, cap, settle);
            max = max.max(d);
            self.cur.insert(id.clone(), c);
            if c == to && !waited.contains(&id) {
                done.push(id);
            }
        }
        for id in done {
            self.leaving.retain(|(i, _)| *i != id);
            self.cur.remove(&id);
        }
        self.last.max_step = max / r;
        self.last.moving = moving;
        self.rebuild();
        if moving == 0 && self.leaving.is_empty() {
            self.moving = false;
            self.respawn_all();
        }
    }

    // ---------- links ----------
    fn structural_all(&mut self) -> &[IdLink] {
        if self.structural.is_none() {
            let points: Vec<LinkPoint> = self
                .model
                .vis
                .iter()
                .map(|&i| {
                    let n = &self.model.nodes[i];
                    LinkPoint {
                        id: n.node.id.clone(),
                        parent: n.node.p.clone().unwrap_or_default(),
                        dir: n.node.k,
                        p: self.model.target[&n.node.id],
                        category: n.cat,
                    }
                })
                .collect();
            self.structural = Some(structural_links(&points));
        }
        self.structural.as_deref().unwrap_or(&[])
    }

    /// index.ts `refreshTopology`: mesh, bridge and host links that are on, cut to the budget.
    fn refresh_topology(&mut self) {
        let on = self.settings.links;
        let mut list: Vec<IdLink> = if on[2].on || on[3].on {
            self.structural_all()
                .iter()
                .filter(|l| on[l.kind].on)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        list.extend(self.extra.iter().filter(|l| on[l.kind].on).map(|l| IdLink {
            weight: if l.weight > 0.0 {
                l.weight
            } else {
                KIND_WEIGHT[l.kind]
            },
            ..l.clone()
        }));
        self.topology = pick_budget(&list, self.settings.link_budget);
        if self.built_once {
            self.rebuild();
        }
    }

    /// Replaces the host links (kinds 4 to 6); other kinds are ignored.
    pub fn set_links(&mut self, list: Vec<IdLink>) {
        self.extra = list
            .into_iter()
            .filter(|l| (4..=6).contains(&l.kind))
            .collect();
        self.refresh_topology();
    }

    /// index.ts `linkCounts`: pairs available per kind, drawn or not.
    pub fn link_counts(&mut self) -> [usize; 7] {
        let mut out = [self.counts[0], self.counts[1], 0, 0, 0, 0, 0];
        let structural = self.structural_all().to_vec();
        for l in structural {
            out[l.kind] += 1;
        }
        for l in &self.extra {
            if self.cur.contains_key(&l.a) && self.cur.contains_key(&l.b) {
                out[l.kind] += 1;
            }
        }
        out
    }

    // ---------- settings ----------
    /// After any settings change: what needs geometry (tree factor, topology) is rebuilt, the rest is uniforms.
    pub fn apply_settings(&mut self) {
        self.active = (self.settings.pulses.round().max(0.0) as usize).min(PULSES);
        let tree = self.settings.tree_factor();
        if tree != self.tree_factor {
            self.tree_factor = tree;
            if self.built_once {
                self.rebuild();
            }
        }
    }

    /// Applies a settings change made through `f`, rebuilding only what it needs.
    pub fn configure<F: FnOnce(&mut Settings) -> bool>(&mut self, f: F) -> bool {
        let before = self.settings.topology_key();
        let changed = f(&mut self.settings);
        self.apply_settings();
        if self.settings.topology_key() != before {
            self.refresh_topology();
        }
        changed
    }

    // ---------- sparks ----------
    fn chain_to(&self, id: &str) -> Option<Vec<V3>> {
        let mut ids = Vec::new();
        let mut a = Some(id.to_string());
        while let Some(x) = a.filter(|x| x != ".") {
            a = self.model.parent_of(&x);
            ids.push(x);
        }
        ids.reverse();
        let mut out: Vec<V3> = Vec::new();
        for a in ids {
            if let Some(&e) = self.edge_index.get(&a) {
                let pts = &self.edges[e].pts;
                out.extend_from_slice(if out.is_empty() { pts } else { &pts[1..] });
            }
        }
        (out.len() > 1).then_some(out)
    }

    fn spawn(&mut self, i: usize, offset: f64) {
        let candidates: Vec<(String, u8, [f64; 3])> = self
            .items
            .iter()
            .map(|item| (self.item_id(item), item))
            .filter(|(id, _)| id != "." && self.is_born(id))
            .map(|(id, item)| {
                let (kind, colour, _) = self.item_look(item);
                (id, kind, colour)
            })
            .collect();
        if candidates.is_empty() {
            self.pulses[i].path = None;
            return;
        }
        for _ in 0..8 {
            let pick = (self.rng.float() * candidates.len() as f64).floor() as usize;
            let (id, _, colour) = &candidates[pick.min(candidates.len() - 1)];
            let Some(path) = self.chain_to(id) else {
                continue;
            };
            let speed = (0.35 + self.rng.float() * 0.4) / (path.len() as f64 / SEG as f64).max(1.0);
            self.pulses[i] = Pulse {
                path: Some(path),
                t: offset,
                speed,
                colour: *colour,
            };
            return;
        }
        self.pulses[i].path = None;
    }

    fn respawn_all(&mut self) {
        for i in 0..self.active {
            let offset = self.rng.float();
            self.spawn(i, offset);
        }
    }

    fn step_pulses(&mut self, dt: f64) {
        let speed_mul = self.settings.pulse_speed;
        for i in 0..PULSES {
            if i >= self.active {
                for k in 0..TRAIL {
                    self.pulse_fade[i * TRAIL + k] = 0.0;
                }
                continue;
            }
            if self.pulses[i].path.is_none() {
                self.spawn(i, 0.0);
                if self.pulses[i].path.is_none() {
                    continue;
                }
            }
            self.pulses[i].t += self.pulses[i].speed * dt * speed_mul;
            if self.pulses[i].t >= 1.15 {
                self.spawn(i, 0.0);
                if self.pulses[i].path.is_none() {
                    continue;
                }
            }
            let p = &self.pulses[i];
            let path = p.path.as_ref().expect("path");
            for k in 0..TRAIL {
                let t = (p.t - k as f64 * 0.012).clamp(0.0, 1.0);
                let f = t * (path.len() - 1) as f64;
                let j = f.floor() as usize;
                let (q, r) = (path[j], path[(j + 1).min(path.len() - 1)]);
                let idx = (i * TRAIL + k) * 3;
                for a in 0..3 {
                    self.pulse_pos[idx + a] = q[a] + (r[a] - q[a]) * (f - j as f64);
                    self.pulse_colour[idx + a] = p.colour[a];
                }
                let tail = if p.t > 1.0 {
                    (1.0 - (p.t - 1.0) / 0.15).max(0.0)
                } else {
                    1.0
                };
                self.pulse_fade[i * TRAIL + k] = tail * (1.0 - k as f64 / TRAIL as f64);
            }
        }
    }

    pub fn sparks(&self) -> Vec<NeuralSpark> {
        (0..PULSES * TRAIL)
            .map(|i| NeuralSpark {
                position: [
                    self.pulse_pos[i * 3] as f32,
                    self.pulse_pos[i * 3 + 1] as f32,
                    self.pulse_pos[i * 3 + 2] as f32,
                ],
                colour: [
                    self.pulse_colour[i * 3] as f32,
                    self.pulse_colour[i * 3 + 1] as f32,
                    self.pulse_colour[i * 3 + 2] as f32,
                ],
                fade: self.pulse_fade[i] as f32,
            })
            .collect()
    }

    pub fn live_sparks(&self) -> usize {
        self.pulses
            .iter()
            .take(self.active)
            .filter(|p| p.path.is_some())
            .count()
    }

    // ---------- frame ----------
    /// One frame: motion (and the live build), then the sparks. `dt` in seconds, at most 0.05 as the original.
    pub fn step(&mut self, dt: f64) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        if !self.still {
            self.step_motion(dt);
        }
        self.step_pulses(dt);
    }

    /// The original's deterministic frame (`?still`): 420 spark steps of 1/60 s; returns the scene time.
    pub fn prime_still(&mut self) -> f64 {
        let mut t = 0.0;
        for _ in 0..420 {
            t += 1.0 / 60.0;
            self.step_pulses(1.0 / 60.0);
        }
        t
    }

    pub fn stats(&self) -> Stats {
        let (born, total) = self.born_count();
        Stats {
            visible: self.model.vis.len(),
            moving: self.last.moving,
            leaving: self.leaving.len(),
            max_step: self.last.max_step,
            dt: self.last.dt,
            born,
            total,
            time: self.clock,
            rebuilds: self.rebuilds,
            sparks: self.live_sparks(),
        }
    }

    // ---------- interaction ----------
    pub fn selected(&self) -> &str {
        &self.selected
    }

    /// Selects a node of the index; false (and nothing changes) for an unknown id.
    pub fn select(&mut self, id: &str) -> bool {
        if !self.model.by_id.contains_key(id) {
            return false;
        }
        self.selected = id.to_string();
        self.build_selection();
        true
    }

    fn before(&self) -> Option<Vec<(String, Ghost)>> {
        Some(self.snapshot())
    }

    pub fn toggle_fold(&mut self, id: &str) -> bool {
        if !self.model.by_id.contains_key(id) {
            return false;
        }
        let before = self.before();
        self.model.toggle_fold(id);
        self.relayout(true, before);
        true
    }

    pub fn set_fold(&mut self, on: bool) {
        let before = self.before();
        self.model.set_fold(on);
        self.relayout(true, before);
    }

    pub fn set_processes(&mut self, mode: Processes) {
        let before = self.before();
        self.model.set_processes(mode);
        self.relayout(true, before);
    }

    /// index.ts `setNodes`: another graph without starting over. Keeps selection (the root if it is gone),
    /// unfolded programs and settings; new nodes are born from their ancestor and gone ones fold back.
    /// False without a root: nothing changes.
    pub fn set_nodes(&mut self, nodes: &[SphereNode], animate: bool) -> bool {
        let expanded: Vec<String> = self.model.expanded.iter().cloned().collect();
        let Some(next) = SphereModel::new(
            nodes,
            self.model.fold_on,
            self.model.processes_mode.clone(),
            &expanded,
        ) else {
            return false;
        };
        let before = self.before();
        let selected = self.selected.clone();
        self.model = next;
        self.structural = None;
        self.refresh_recent();
        self.relayout(animate, before);
        if !self.select(&selected) {
            self.selected = ".".into();
            self.build_selection();
        }
        true
    }

    /// three.js `Raycaster` on the points (threshold in field units): the nearest node the ray passes within
    /// `threshold` of, along the ray. Processes are not picked.
    pub fn pick(&self, origin: V3, direction: V3, threshold: f64) -> Option<String> {
        let dir = v3::normalize(direction);
        let mut best: Option<(f64, &Item)> = None;
        for (item, point) in self.items.iter().zip(&self.points) {
            let p = [
                f64::from(point.position[0]),
                f64::from(point.position[1]),
                f64::from(point.position[2]),
            ];
            let rel = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
            let t = (rel[0] * dir[0] + rel[1] * dir[1] + rel[2] * dir[2]).max(0.0);
            let closest = v3::add_scaled(origin, dir, t);
            if v3::distance(closest, p) >= threshold {
                continue;
            }
            if best.is_none_or(|(d, _)| t < d) {
                best = Some((t, item));
            }
        }
        match best? {
            (_, Item::Node(i)) => Some(self.model.id(*i).to_string()),
            _ => None,
        }
    }

    /// A click on a node: a folded program unfolds, and it is selected (index.ts `pointerup`).
    pub fn activate(&mut self, id: &str) -> bool {
        let Some(&i) = self.model.by_id.get(id) else {
            return false;
        };
        if self.model.nodes[i].node.k && self.model.nodes[i].folded {
            self.toggle_fold(id);
        }
        self.select(id)
    }

    pub fn position_of(&self, id: &str) -> Option<V3> {
        self.cur.get(id).copied()
    }

    // ---------- HUD ----------
    /// hud.ts: labels of the top-level folders and the running programs, except the selection.
    pub fn labels(&self) -> Vec<Label> {
        let mut out = Vec::new();
        for &i in &self.model.vis {
            let n = &self.model.nodes[i];
            if !n.node.k || n.node.d != 1 || n.node.id == self.selected {
                continue;
            }
            let Some(&at) = self.cur.get(&n.node.id) else {
                continue;
            };
            let extra = if n.folded {
                format!("  +{}", format_count(u64::from(n.hidden)))
            } else {
                String::new()
            };
            out.push(Label {
                id: n.node.id.clone(),
                text: format!("{}{}", n.node.n.to_uppercase(), extra),
                position: at,
                process: false,
            });
        }
        for p in &self.model.processes {
            if p.id == self.selected {
                continue;
            }
            let Some(&at) = self.cur.get(&p.id) else {
                continue;
            };
            out.push(Label {
                id: p.id.clone(),
                text: p.name.to_uppercase(),
                position: at,
                process: true,
            });
        }
        out
    }

    /// hud.ts: the selection's title, its counts and, when folded, how to unfold it.
    pub fn selection_text(&self, root_label: Option<&str>) -> [String; 3] {
        let Some(&i) = self.model.by_id.get(&self.selected) else {
            return Default::default();
        };
        let n = &self.model.nodes[i];
        let title = if self.selected == "." {
            root_label.unwrap_or(&n.node.n).to_uppercase()
        } else {
            n.node.n.to_uppercase()
        };
        let counts = if n.node.k {
            format!(
                "{} ELEMENTOS · {}",
                format_count(u64::from(n.w - 1)),
                format_size(n.bytes)
            )
        } else {
            format_size(n.node.s)
        };
        let folded = if n.folded {
            format!(
                "PLEGADO · {} OCULTOS · TOCA PARA DESPLEGAR",
                format_count(u64::from(n.hidden))
            )
        } else {
            String::new()
        };
        [title, counts, folded]
    }
}

/// `n.toLocaleString('es-ES')` for whole numbers: groups of three with a dot, from five digits on.
pub fn format_count(n: u64) -> String {
    let digits = n.to_string();
    if digits.len() < 5 {
        return digits;
    }
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        let left = digits.len() - i;
        if i > 0 && left % 3 < 1 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// hud.ts `fmtSize`: binary units, at most one decimal, with a decimal comma.
pub fn format_size(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut b, mut i) = (bytes, 0);
    while b >= 1024.0 && i < UNITS.len() - 1 {
        b /= 1024.0;
        i += 1;
    }
    let tenths = (b * 10.0).round() as u64;
    let (whole, tenth) = (format_count(tenths / 10), tenths % 10);
    if tenth > 0 {
        format!("{whole},{tenth} {}", UNITS[i])
    } else {
        format!("{whole} {}", UNITS[i])
    }
}

/// The scene's decoration in field units (radius 100): the ghost particles on the surface, and the line sets of
/// the globe, the projection base and the tilted orbit (as segment pairs). scene.ts constructor.
pub struct Decor {
    pub ghosts: Vec<V3>,
    pub globe: Vec<V3>,
    pub base: Vec<V3>,
    pub orbit: Vec<V3>,
    /// The projection cone: an open cylinder (top radius 0.85 R, bottom 0.3 R, height 0.75 R, 64 sides), as
    /// (position, v) with v = 0 at the top as three.js's `uv.y`; triangles.
    pub cone: Vec<(V3, f64)>,
    /// Where the base sits and where the cone's centre is.
    pub base_y: f64,
    pub cone_y: f64,
}

fn strip(points: Vec<V3>) -> Vec<V3> {
    points.windows(2).flat_map(|w| [w[0], w[1]]).collect()
}

pub fn decor(r: f64) -> Decor {
    let ghosts = (0..1600)
        .map(|i| {
            let z = 1.0 - 2.0 * (f64::from(i) + 0.5) / 1600.0;
            let rad = (1.0 - z * z).sqrt();
            let a = f64::from(i) * 2.399_963_229_728_653;
            let rr = r * (1.0 + (hash(&format!("g{i}")) - 0.5) * 0.04);
            [a.cos() * rad * rr, z * rr, a.sin() * rad * rr]
        })
        .collect();
    let circle = |radius: f64, y: f64, seg: usize, flat: bool| -> Vec<V3> {
        (0..=seg)
            .map(|j| {
                let t = j as f64 / seg as f64 * std::f64::consts::PI * 2.0;
                if flat {
                    [t.cos() * radius, y, t.sin() * radius]
                } else {
                    [t.cos() * radius, t.sin() * radius, 0.0]
                }
            })
            .collect()
    };
    let rot_y = |p: V3, a: f64| {
        [
            p[0] * a.cos() + p[2] * a.sin(),
            p[1],
            -p[0] * a.sin() + p[2] * a.cos(),
        ]
    };
    let mut globe = Vec::new();
    for i in 0..3 {
        globe.extend(
            strip(circle(r, 0.0, 160, false))
                .into_iter()
                .map(|p| rot_y(p, f64::from(i) * std::f64::consts::PI / 3.0)),
        );
    }
    globe.extend(strip(circle(r, 0.0, 160, true)));
    let ticks = |radius: f64, y: f64, count: usize, len: f64| -> Vec<V3> {
        (0..count)
            .flat_map(|i| {
                let t = i as f64 / count as f64 * std::f64::consts::PI * 2.0;
                let l = if i % 6 != 0 { len } else { len * 2.0 };
                [
                    [t.cos() * radius, y, t.sin() * radius],
                    [t.cos() * (radius + l), y, t.sin() * (radius + l)],
                ]
            })
            .collect()
    };
    let base_y = -r * 1.3;
    let mut base = Vec::new();
    for f in [0.3, 0.62, 0.95] {
        base.extend(strip(circle(r * f, base_y, 180, true)));
    }
    base.extend(ticks(r * 0.95, base_y, 120, r * 0.02));
    let mut orbit = strip(circle(r * 1.12, 0.0, 240, true));
    orbit.extend(ticks(r * 1.12, 0.0, 180, r * 0.012));
    let (top, bottom, height, sides) = (r * 0.85, r * 0.3, r * 0.75, 64);
    let ring = |y: f64, radius: f64, s: usize| {
        let t = s as f64 / sides as f64 * std::f64::consts::PI * 2.0;
        [radius * t.sin(), y, radius * t.cos()]
    };
    let mut cone = Vec::new();
    for s in 0..sides {
        let (a, b) = (ring(height / 2.0, top, s), ring(height / 2.0, top, s + 1));
        let (c, d) = (
            ring(-height / 2.0, bottom, s),
            ring(-height / 2.0, bottom, s + 1),
        );
        cone.extend([(a, 1.0), (c, 0.0), (b, 1.0), (c, 0.0), (d, 0.0), (b, 1.0)]);
    }
    Decor {
        ghosts,
        globe,
        base,
        orbit,
        cone,
        base_y,
        cone_y: base_y + r * 0.375,
    }
}

pub fn category_name(cat: usize) -> &'static str {
    if cat == FOLDERS {
        "folders"
    } else {
        CATEGORIES[cat].0
    }
}
