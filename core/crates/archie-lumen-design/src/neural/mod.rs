//! Xavi's neural sphere (Lumenfield's «Esfera neuronal», API 2): a fine network of folders and files with a
//! hologram's look. Port of `web/src/neural-sphere` of Lumenfield (2026-10-15): live settings, seven link kinds,
//! another graph without starting over, and the live build. This is the model and the scene state; the host
//! draws the buffers (`ffi`) with the original's materials and post-processing.
//!
//! The one filesystem access is `scan`: a bounded, read-only listing of the folder the sphere draws (no file is
//! opened, nothing is written, links are not followed), as `content` does for the content sphere.

pub mod collate;
pub mod ffi;
pub mod links;
pub mod model;
pub mod motion;
pub mod settings;
pub mod sphere;
pub mod v3;

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub use model::{ProcessSpec, Processes, SphereNode};
pub use sphere::{NeuralPoint, NeuralSpark, NeuralVertex, Sphere};

/// Never read more than this many entries of one folder, whatever it holds.
const MAX_ENTRIES_PER_FOLDER: usize = 20_000;
/// Hidden entries and build output are not listed (as `content`).
const SKIPPED: [&str; 7] = [
    "target",
    "node_modules",
    "__pycache__",
    "build",
    "dist-newstyle",
    "venv",
    "site-packages",
];
pub const MAX_NODES: usize = 20_000;

/// Lists `root` breadth first into the sphere's index: at most `limit` nodes and `files_per_folder` files per
/// folder, folders first, names sorted. Reads directory entries and their metadata only; symbolic links are
/// listed as files and never followed. None if `root` is not a folder.
pub fn scan(root: &Path, limit: usize, files_per_folder: usize) -> Option<Vec<SphereNode>> {
    let limit = limit.clamp(1, MAX_NODES);
    if !fs::metadata(root).ok()?.is_dir() {
        return None;
    }
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let mut nodes = vec![SphereNode {
        id: ".".into(),
        p: None,
        k: true,
        d: 0,
        s: 0.0,
        m: 0.0,
        n: name,
    }];
    let mut queue: VecDeque<(usize, PathBuf)> = VecDeque::from([(0, root.to_path_buf())]);
    let millis = |meta: &fs::Metadata| {
        meta.modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0.0, |d| d.as_millis() as f64)
    };
    while let Some((at, path)) = queue.pop_front() {
        let Ok(read) = fs::read_dir(&path) else {
            continue;
        };
        let (mut dirs, mut files) = (Vec::new(), Vec::new());
        for entry in read.take(MAX_ENTRIES_PER_FOLDER).flatten() {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.') || SKIPPED.contains(&file_name.as_str()) {
                continue;
            }
            let (Ok(kind), Ok(meta)) = (entry.file_type(), entry.metadata()) else {
                continue;
            };
            if kind.is_dir() {
                dirs.push((file_name, entry.path(), millis(&meta)));
            } else {
                files.push((file_name, meta.len() as f64, millis(&meta)));
            }
        }
        dirs.sort_by(|a, b| a.0.cmp(&b.0));
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let (depth, prefix) = (
            nodes[at].d + 1,
            if at == 0 {
                String::new()
            } else {
                format!("{}/", nodes[at].id)
            },
        );
        let parent = nodes[at].id.clone();
        for (file_name, child, m) in dirs {
            if nodes.len() >= limit {
                break;
            }
            nodes.push(SphereNode {
                id: format!("{prefix}{file_name}"),
                p: Some(parent.clone()),
                k: true,
                d: depth,
                s: 0.0,
                m,
                n: file_name,
            });
            queue.push_back((nodes.len() - 1, child));
        }
        for (file_name, size, m) in files.into_iter().take(files_per_folder) {
            if nodes.len() >= limit {
                break;
            }
            nodes.push(SphereNode {
                id: format!("{prefix}{file_name}"),
                p: Some(parent.clone()),
                k: false,
                d: depth,
                s: size,
                m,
                n: file_name,
            });
        }
    }
    Some(nodes)
}
