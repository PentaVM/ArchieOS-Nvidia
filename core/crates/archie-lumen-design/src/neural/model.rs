//! The neural sphere's model: the index, folded programs and where everything goes. Port of Lumenfield's
//! `neural-sphere/model.ts` (itself the artifact's lines 31 to 116). Folders sit in layers inside the sphere,
//! files on its surface. Doubles throughout, with three.js's vector operations in its order of operations.

use std::collections::{HashMap, HashSet};

use super::collate::locale_compare;
use super::v3::{self, V3};

/// One entry of the index: `k` folder (true) or file, `d` depth, `s` size, `m` modification time in ms, `n`
/// name. The root has id `.` and no parent.
#[derive(Debug, Clone, PartialEq)]
pub struct SphereNode {
    pub id: String,
    pub p: Option<String>,
    pub k: bool,
    pub d: u32,
    pub s: f64,
    pub m: f64,
    pub n: String,
}

/// The design's seven file types (palette.ts `CATEGORIES`) and, last, folders.
pub const CATEGORIES: [(&str, &str); 7] = [
    (
        "code",
        "c cc cpp cs css go h html java js jsx kt lua php py rb rs scss sh sql swift ts tsx vue",
    ),
    (
        "documents",
        "csv doc docx epub md odt pdf ppt pptx rtf txt xls xlsx",
    ),
    (
        "media",
        "avi bmp gif jpeg jpg m4a mkv mov mp3 mp4 ogg png svg wav webm webp",
    ),
    ("data", "db ini json toml xml yaml yml"),
    ("archive", "7z bz2 gz rar tar tgz zip"),
    ("executable", "bat cmd dll exe msi ps1 so app"),
    ("other", ""),
];
pub const FOLDERS: usize = 7;
const OTHER: usize = 6;

/// palette.ts `extOf`: after the last dot, unless the name starts or ends with it.
fn extension(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    match chars.iter().rposition(|&c| c == '.') {
        Some(i) if i > 0 && i < chars.len() - 1 => {
            chars[i + 1..].iter().collect::<String>().to_lowercase()
        }
        _ => String::new(),
    }
}

/// palette.ts `categoryOf`: an index into `CATEGORIES`, or `FOLDERS`.
pub fn category_of(folder: bool, name: &str) -> usize {
    if folder {
        return FOLDERS;
    }
    let ext = extension(name);
    if ext.is_empty() {
        return OTHER;
    }
    CATEGORIES
        .iter()
        .position(|(_, list)| list.split(' ').any(|e| e == ext))
        .unwrap_or(OTHER)
}

/// model.ts `hash`: FNV-1a over UTF-16 code units, in [0, 1).
pub fn hash(text: &str) -> f64 {
    let mut h: u32 = 2_166_136_261;
    for unit in text.encode_utf16() {
        h = (h ^ u32::from(unit)).wrapping_mul(16_777_619);
    }
    f64::from(h) / 4_294_967_296.0
}

const GOLDEN: f64 = 2.399_963_229_728_653;
const FILE_CAP: usize = 40;

fn unit_file(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "package.json" | "cargo.toml" | "pyproject.toml" | "setup.py" | "go.mod"
    )
}
fn unit_ext(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".exe") || lower.ends_with(".sln") || lower.ends_with(".csproj")
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub node: SphereNode,
    pub parent: Option<usize>,
    pub cat: usize,
    pub w: u32,
    pub bytes: f64,
    pub vw: u32,
    pub hidden: u32,
    pub folded: bool,
    /// Set the first time it folds as a program, and kept (model.ts never clears it).
    pub unit: bool,
    pub dir: Option<V3>,
}

/// A running program drawn in cream, hanging off a visible node.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSpec {
    pub id: String,
    pub name: String,
    pub host_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Processes {
    /// The artifact's sample process (`node.exe · vite (ejemplo)`), used only to compare with the original.
    Example,
    Specs(Vec<ProcessSpec>),
    Off,
}

#[derive(Debug, Clone)]
pub struct ProcessEntry {
    pub id: String,
    pub host: usize,
    pub name: String,
    pub dir: V3,
}

pub struct SphereModel {
    pub radius: f64,
    pub nodes: Vec<Entry>,
    pub by_id: HashMap<String, usize>,
    pub kids: Vec<Vec<usize>>,
    pub root: usize,
    pub expanded: HashSet<String>,
    /// Where each visible node (and process) goes, by id, and the order they were placed in (a JS Map's order).
    pub target: HashMap<String, V3>,
    pub target_order: Vec<String>,
    pub vis: Vec<usize>,
    pub max_vd: u32,
    pub processes: Vec<ProcessEntry>,
    pub fold_on: bool,
    pub processes_mode: Processes,
}

impl SphereModel {
    /// None without a root (id `.`), as model.ts throws.
    pub fn new(
        input: &[SphereNode],
        fold: bool,
        processes: Processes,
        expanded: &[String],
    ) -> Option<Self> {
        let nodes: Vec<Entry> = input
            .iter()
            .map(|n| Entry {
                cat: category_of(n.k, &n.n),
                node: n.clone(),
                parent: None,
                w: 0,
                bytes: 0.0,
                vw: 0,
                hidden: 0,
                folded: false,
                unit: false,
                dir: None,
            })
            .collect();
        let mut by_id = HashMap::with_capacity(nodes.len());
        for (i, n) in nodes.iter().enumerate() {
            by_id.insert(n.node.id.clone(), i);
        }
        let root = *by_id.get(".")?;
        let mut model = SphereModel {
            radius: 100.0,
            kids: vec![Vec::new(); nodes.len()],
            nodes,
            by_id,
            root,
            expanded: expanded.iter().cloned().collect(),
            target: HashMap::new(),
            target_order: Vec::new(),
            vis: Vec::new(),
            max_vd: 1,
            processes: Vec::new(),
            fold_on: fold,
            processes_mode: processes,
        };
        for i in 0..model.nodes.len() {
            let parent = model.nodes[i]
                .node
                .p
                .as_ref()
                .and_then(|p| model.by_id.get(p).copied());
            model.nodes[i].parent = parent;
            if let Some(p) = parent {
                model.kids[p].push(i);
            }
        }
        let mut order: Vec<usize> = (0..model.nodes.len()).collect();
        order.sort_by(|&a, &b| model.nodes[b].node.d.cmp(&model.nodes[a].node.d));
        for i in order {
            let w = 1 + model.kids[i].iter().map(|&k| model.nodes[k].w).sum::<u32>();
            let bytes = if model.nodes[i].node.k {
                model.kids[i].iter().map(|&k| model.nodes[k].bytes).sum()
            } else {
                model.nodes[i].node.s
            };
            model.nodes[i].w = w;
            model.nodes[i].bytes = bytes;
        }
        model.layout();
        Some(model)
    }

    pub fn id(&self, i: usize) -> &str {
        &self.nodes[i].node.id
    }

    fn is_unit(&self, i: usize) -> bool {
        let n = &self.nodes[i];
        n.node.k
            && n.node.id != "."
            && self.kids[i].iter().any(|&c| {
                let c = &self.nodes[c].node;
                unit_file(&c.n) || (!c.k && unit_ext(&c.n)) || c.n == ".git"
            })
    }

    fn main_files(&self, i: usize) -> Vec<usize> {
        let files: Vec<usize> = self.kids[i]
            .iter()
            .copied()
            .filter(|&c| !self.nodes[c].node.k)
            .collect();
        let exe: Vec<usize> = files
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].node.n.to_lowercase().ends_with(".exe"))
            .collect();
        if !exe.is_empty() {
            return exe.into_iter().take(3).collect();
        }
        files
            .into_iter()
            .find(|&c| unit_file(&self.nodes[c].node.n) || unit_ext(&self.nodes[c].node.n))
            .into_iter()
            .collect()
    }

    fn visit(&mut self, i: usize, vis: &mut Vec<usize>) {
        vis.push(i);
        self.nodes[i].hidden = 0;
        self.nodes[i].folded = false;
        if self.fold_on && self.is_unit(i) && !self.expanded.contains(&self.nodes[i].node.id) {
            let main = self.main_files(i);
            vis.extend(&main);
            let n = &mut self.nodes[i];
            n.hidden = n.w - 1 - main.len() as u32;
            n.folded = true;
            n.unit = true;
            return;
        }
        let mut files: Vec<usize> = self.kids[i]
            .iter()
            .copied()
            .filter(|&c| !self.nodes[c].node.k)
            .collect();
        files.sort_by(|&a, &b| {
            let (a, b) = (&self.nodes[a].node, &self.nodes[b].node);
            b.s.partial_cmp(&a.s)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| locale_compare(&a.id, &b.id))
        });
        let all = !self.fold_on || self.expanded.contains(&self.nodes[i].node.id);
        let shown = if all {
            files.len()
        } else {
            files.len().min(FILE_CAP)
        };
        vis.extend(&files[..shown]);
        if files.len() > shown {
            self.nodes[i].hidden = (files.len() - shown) as u32;
            self.nodes[i].folded = true;
        }
        let folders: Vec<usize> = self.kids[i]
            .iter()
            .copied()
            .filter(|&c| self.nodes[c].node.k)
            .collect();
        for k in folders {
            self.visit(k, vis);
        }
    }

    /// model.ts `layout`: what is visible and the target of every visible node and process.
    pub fn layout(&mut self) {
        let r = self.radius;
        let mut vis = Vec::new();
        self.visit(self.root, &mut vis);
        let mut vk: HashMap<usize, Vec<usize>> = HashMap::new();
        for &i in &vis {
            if i == self.root {
                continue;
            }
            if let Some(p) = self.nodes[i].parent {
                vk.entry(p).or_default().push(i);
            }
        }
        let mut by_depth = vis.clone();
        by_depth.sort_by(|&a, &b| self.nodes[b].node.d.cmp(&self.nodes[a].node.d));
        for i in by_depth {
            let vw = 1 + vk
                .get(&i)
                .map_or(0, |k| k.iter().map(|&c| self.nodes[c].vw).sum::<u32>());
            self.nodes[i].vw = vw;
        }
        self.max_vd = vis
            .iter()
            .filter(|&&i| self.nodes[i].node.k)
            .map(|&i| self.nodes[i].node.d)
            .max()
            .unwrap_or(0)
            .max(1);
        self.target.clear();
        self.target_order.clear();
        self.nodes[self.root].dir = Some([0.0, 1.0, 0.0]);
        self.set_target(".".into(), [0.0, 0.0, 0.0]);
        self.place(self.root, std::f64::consts::PI, &vk, r);
        self.vis = vis;
        self.place_processes(r);
    }

    fn place(&mut self, node: usize, half: f64, vk: &HashMap<usize, Vec<usize>>, r: f64) {
        let Some(list) = vk.get(&node) else { return };
        let mut list = list.clone();
        list.sort_by(|&a, &b| {
            self.nodes[b]
                .vw
                .cmp(&self.nodes[a].vw)
                .then_with(|| locale_compare(&self.nodes[a].node.id, &self.nodes[b].node.id))
        });
        if list.is_empty() {
            return;
        }
        let total = f64::from(list.iter().map(|&k| self.nodes[k].vw).sum::<u32>());
        let dir = self.nodes[node].dir.unwrap_or([0.0, 1.0, 0.0]);
        let (u, v) = v3::basis(dir);
        let spin = hash(&self.nodes[node].node.id) * std::f64::consts::PI * 2.0;
        let mut cum = 0.0;
        for (i, &k) in list.iter().enumerate() {
            let vw = f64::from(self.nodes[k].vw);
            let frac = (cum + vw * 0.5) / total;
            cum += vw;
            let id = self.nodes[k].node.id.clone();
            let jit = hash(&format!("{id}~"));
            let theta = (1.0 - (1.0 - half.cos()) * (frac * (0.9 + 0.2 * jit)).min(1.0)).acos();
            let phi = spin + i as f64 * GOLDEN + (jit - 0.5) * 0.9;
            let d = v3::normalize(v3::add_scaled(
                v3::add_scaled(v3::scale(dir, theta.cos()), u, phi.cos() * theta.sin()),
                v,
                phi.sin() * theta.sin(),
            ));
            self.nodes[k].dir = Some(d);
            if self.nodes[k].node.k {
                let depth = f64::from(self.nodes[k].node.d) / f64::from(self.max_vd);
                self.set_target(id, v3::scale(d, r * (0.14 + 0.56 * depth.powf(0.8))));
                let child = (half * 0.95)
                    .min(half * (vw / total).sqrt() * 1.3)
                    .max(0.07);
                self.place(k, child, vk, r);
            } else {
                let lift = 0.86 + 0.13 * hash(&format!("{id}^"));
                self.set_target(id, v3::scale(d, r * lift));
            }
        }
    }

    fn place_processes(&mut self, r: f64) {
        self.processes.clear();
        let visible: HashSet<&str> = self
            .vis
            .iter()
            .map(|&i| self.nodes[i].node.id.as_str())
            .collect();
        let mut out = Vec::new();
        match &self.processes_mode {
            Processes::Off => {}
            Processes::Specs(specs) => {
                for spec in specs {
                    let mut host = Some(spec.host_id.clone());
                    while let Some(h) = host.clone() {
                        if visible.contains(h.as_str()) {
                            break;
                        }
                        host = self
                            .by_id
                            .get(&h)
                            .and_then(|&i| self.nodes[i].node.p.clone());
                    }
                    if let Some(&h) = host.as_ref().and_then(|h| self.by_id.get(h)) {
                        out.push(ProcessEntry {
                            id: spec.id.clone(),
                            host: h,
                            name: spec.name.clone(),
                            dir: self.nodes[h].dir.unwrap_or_default(),
                        });
                    }
                }
            }
            Processes::Example => {
                let host = self
                    .vis
                    .iter()
                    .copied()
                    .find(|&i| self.nodes[i].node.id == "node_modules/vite")
                    .or_else(|| self.vis.iter().copied().find(|&i| self.nodes[i].unit));
                if let Some(h) = host {
                    out.push(ProcessEntry {
                        id: format!("proc:{}", self.nodes[h].node.id),
                        host: h,
                        name: "node.exe · vite (ejemplo)".into(),
                        dir: self.nodes[h].dir.unwrap_or_default(),
                    });
                }
            }
        }
        for p in &out {
            self.set_target(p.id.clone(), v3::scale(p.dir, r * 1.07));
        }
        self.processes = out;
    }

    fn set_target(&mut self, id: String, at: V3) {
        if self.target.insert(id.clone(), at).is_none() {
            self.target_order.push(id);
        }
    }

    /// The parent of a node, or the host of a process, by id.
    pub fn parent_of(&self, id: &str) -> Option<String> {
        if let Some(&i) = self.by_id.get(id) {
            return self.nodes[i].node.p.clone();
        }
        self.processes
            .iter()
            .find(|p| p.id == id)
            .map(|p| self.nodes[p.host].node.id.clone())
    }

    pub fn toggle_fold(&mut self, id: &str) {
        let Some(&i) = self.by_id.get(id) else { return };
        if self.nodes[i].folded {
            self.expanded.insert(id.to_string());
        } else {
            self.expanded.remove(id);
        }
        self.layout();
    }

    pub fn set_fold(&mut self, on: bool) {
        self.fold_on = on;
        self.layout();
    }

    pub fn set_processes(&mut self, mode: Processes) {
        self.processes_mode = mode;
        self.layout();
    }

    /// (visible, hidden by folding, folded nodes, total without the root).
    pub fn counts(&self) -> (usize, u32, usize, usize) {
        let folded: Vec<usize> = self
            .vis
            .iter()
            .copied()
            .filter(|&i| self.nodes[i].folded)
            .collect();
        (
            self.vis.len(),
            folded.iter().map(|&i| self.nodes[i].hidden).sum(),
            folded.len(),
            self.nodes.len() - 1,
        )
    }
}
