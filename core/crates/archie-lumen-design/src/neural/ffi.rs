//! C ABI of the neural sphere. The host owns a `NeuralSphere` through an opaque pointer; every buffer it passes
//! is its own and Rust never keeps it. Buffers are queried first (null output, capacity 0, returns the count)
//! and then filled (returns the count written, or 0 when the capacity is short). Strings are NUL-terminated
//! UTF-8. Nothing unwinds across the boundary: a panic returns 0, false or null.
//!
//! Listing a folder (`neural_scan`) is separate from the sphere, so the host can list off its GUI thread.

use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::{ptr, slice};

use super::model::{Processes, SphereNode};
use super::sphere::{decor, NeuralPoint, NeuralSpark, NeuralVertex, Sphere, LINE_LAYERS};

/// A listed folder, ready to become (or replace) a sphere's graph.
pub struct NeuralIndex(Vec<SphereNode>);

/// The scene state of one sphere.
pub struct NeuralSphere {
    sphere: Sphere,
    /// The last text written by the label and selection queries, kept so a query and its fill agree.
    scratch: String,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct NeuralStats {
    pub visible: u32,
    pub moving: u32,
    pub leaving: u32,
    pub born: u32,
    pub total: u32,
    pub sparks: u32,
    pub max_step: f32,
    pub dt: f32,
    pub time: f32,
    pub rebuilds: u64,
    pub revision: u64,
}

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

unsafe fn text<'a>(s: *const c_char) -> Option<&'a str> {
    if s.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(s) }.to_str().ok()
}

unsafe fn sphere<'a>(h: *const NeuralSphere) -> Option<&'a NeuralSphere> {
    unsafe { h.as_ref() }
}

unsafe fn sphere_mut<'a>(h: *mut NeuralSphere) -> Option<&'a mut NeuralSphere> {
    unsafe { h.as_mut() }
}

/// Copies `items` to `out` if it holds them; returns the count, or 0 when `out` is short. A null `out` with
/// capacity 0 is the query and returns the count.
unsafe fn fill<T: Copy>(items: &[T], out: *mut T, capacity: usize) -> usize {
    if out.is_null() {
        return if capacity == 0 { items.len() } else { 0 };
    }
    if capacity < items.len() || !out.is_aligned() {
        return 0;
    }
    unsafe { ptr::copy_nonoverlapping(items.as_ptr(), out, items.len()) };
    items.len()
}

/// Writes `s` and a NUL if it fits; returns its length in bytes (without the NUL) either way, so a call with
/// capacity 0 is the query.
unsafe fn write_text(s: &str, out: *mut c_char, capacity: usize) -> usize {
    if !out.is_null() && capacity > s.len() {
        unsafe {
            ptr::copy_nonoverlapping(s.as_ptr(), out.cast::<u8>(), s.len());
            out.add(s.len()).write(0);
        }
    }
    s.len()
}

/// Lists `root` (UTF-8, `root_len` bytes) read-only: at most `limit` nodes and `files_per_folder` files per
/// folder. Null when it is not a readable folder.
///
/// # Safety
/// `root` points to `root_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn neural_scan(
    root: *const u8,
    root_len: usize,
    limit: u32,
    files_per_folder: u32,
) -> *mut NeuralIndex {
    guard(ptr::null_mut(), || {
        if root.is_null() || root_len == 0 || root_len > 4096 {
            return ptr::null_mut();
        }
        let Ok(path) = std::str::from_utf8(unsafe { slice::from_raw_parts(root, root_len) }) else {
            return ptr::null_mut();
        };
        match super::scan(Path::new(path), limit as usize, files_per_folder as usize) {
            Some(nodes) => Box::into_raw(Box::new(NeuralIndex(nodes))),
            None => ptr::null_mut(),
        }
    })
}

/// Nodes in a listed folder (the root included).
///
/// # Safety
/// `index` is null or came from `neural_scan` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn neural_index_count(index: *const NeuralIndex) -> usize {
    guard(0, || unsafe { index.as_ref() }.map_or(0, |i| i.0.len()))
}

/// # Safety
/// `index` is null or came from `neural_scan` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn neural_index_free(index: *mut NeuralIndex) {
    if !index.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(index) })));
    }
}

/// A sphere of a listed folder. `still` freezes motion and the build (the original's deterministic frame);
/// `seed` drives the sparks (7 in the original).
///
/// # Safety
/// `index` is null or a live `NeuralIndex`.
#[no_mangle]
pub unsafe extern "C" fn neural_new(
    index: *const NeuralIndex,
    seed: u32,
    still: bool,
) -> *mut NeuralSphere {
    guard(ptr::null_mut(), || {
        let Some(index) = (unsafe { index.as_ref() }) else {
            return ptr::null_mut();
        };
        match Sphere::new(&index.0, Processes::Off, seed, None, still) {
            Some(sphere) => Box::into_raw(Box::new(NeuralSphere {
                sphere,
                scratch: String::new(),
            })),
            None => ptr::null_mut(),
        }
    })
}

/// # Safety
/// `h` is null or came from `neural_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn neural_free(h: *mut NeuralSphere) {
    if !h.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(h) })));
    }
}

/// Another graph without starting over (setNodes): it moves instead of jumping when `animate`.
///
/// # Safety
/// `h` and `index` are null or live.
#[no_mangle]
pub unsafe extern "C" fn neural_set_index(
    h: *mut NeuralSphere,
    index: *const NeuralIndex,
    animate: bool,
) -> bool {
    guard(false, || {
        match (unsafe { sphere_mut(h) }, unsafe { index.as_ref() }) {
            (Some(s), Some(index)) => s.sphere.set_nodes(&index.0, animate),
            _ => false,
        }
    })
}

/// Starts the live build: everything is born from the root over `seconds` (at most 12).
///
/// # Safety
/// `h` is null or live.
#[no_mangle]
pub unsafe extern "C" fn neural_grow(h: *mut NeuralSphere, seconds: f32) {
    guard((), || {
        if let Some(s) = unsafe { sphere_mut(h) } {
            s.sphere.begin_grow(f64::from(seconds));
        }
    })
}

/// One frame of `dt` seconds (clamped to 0..0.05 as the original).
///
/// # Safety
/// `h` is null or live.
#[no_mangle]
pub unsafe extern "C" fn neural_step(h: *mut NeuralSphere, dt: f32) {
    guard((), || {
        if let Some(s) = unsafe { sphere_mut(h) } {
            s.sphere.step(f64::from(dt));
        }
    })
}

/// The original's deterministic frame: 420 spark steps; returns the scene time to draw it at.
///
/// # Safety
/// `h` is null or live.
#[no_mangle]
pub unsafe extern "C" fn neural_prime_still(h: *mut NeuralSphere) -> f32 {
    guard(0.0, || {
        unsafe { sphere_mut(h) }.map_or(0.0, |s| s.sphere.prime_still() as f32)
    })
}

/// # Safety
/// `h` is null or live; `out` is null or writable and aligned.
#[no_mangle]
pub unsafe extern "C" fn neural_stats(h: *const NeuralSphere, out: *mut NeuralStats) -> bool {
    guard(false, || {
        let Some(s) = (unsafe { sphere(h) }) else {
            return false;
        };
        if out.is_null() || !out.is_aligned() {
            return false;
        }
        let st = s.sphere.stats();
        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        unsafe {
            out.write(NeuralStats {
                visible: n(st.visible),
                moving: n(st.moving),
                leaving: n(st.leaving),
                born: n(st.born),
                total: n(st.total),
                sparks: n(st.sparks),
                max_step: st.max_step as f32,
                dt: st.dt as f32,
                time: st.time as f32,
                rebuilds: st.rebuilds,
                revision: s.sphere.revision,
            })
        };
        true
    })
}

/// # Safety
/// `h` is null or live; `out` is null (query) or holds `capacity` points.
#[no_mangle]
pub unsafe extern "C" fn neural_points(
    h: *const NeuralSphere,
    out: *mut NeuralPoint,
    capacity: usize,
) -> usize {
    guard(0, || {
        unsafe { sphere(h) }.map_or(0, |s| unsafe { fill(&s.sphere.points, out, capacity) })
    })
}

/// Line layer `layer`: 0 skeleton and axons, 2 to 6 the other link kinds, 7 the selection; segments.
///
/// # Safety
/// `h` is null or live; `out` is null (query) or holds `capacity` vertices.
#[no_mangle]
pub unsafe extern "C" fn neural_lines(
    h: *const NeuralSphere,
    layer: u32,
    out: *mut NeuralVertex,
    capacity: usize,
) -> usize {
    guard(0, || {
        let layer = layer as usize;
        if layer >= LINE_LAYERS || layer == 1 {
            return 0;
        }
        unsafe { sphere(h) }.map_or(0, |s| unsafe {
            fill(&s.sphere.lines[layer], out, capacity)
        })
    })
}

/// The 70 sparks' trails (350 points; a faded point has fade 0).
///
/// # Safety
/// `h` is null or live; `out` is null (query) or holds `capacity` sparks.
#[no_mangle]
pub unsafe extern "C" fn neural_sparks(
    h: *const NeuralSphere,
    out: *mut NeuralSpark,
    capacity: usize,
) -> usize {
    guard(0, || {
        unsafe { sphere(h) }.map_or(0, |s| unsafe { fill(&s.sphere.sparks(), out, capacity) })
    })
}

/// The decoration in field units (radius 100), as floats: 0 the surface's ghost points (xyz), 1 the globe,
/// 2 the projection base, 3 the orbit (segments, xyz per vertex), 4 the cone (triangles, xyz and v per vertex).
///
/// # Safety
/// `out` is null (query) or holds `capacity` floats.
#[no_mangle]
pub unsafe extern "C" fn neural_decor(part: u32, out: *mut f32, capacity: usize) -> usize {
    guard(0, || {
        let d = decor(100.0);
        let flat: Vec<f32> = match part {
            0 => d.ghosts.iter().flat_map(|v| v.map(|x| x as f32)).collect(),
            1 => d.globe.iter().flat_map(|v| v.map(|x| x as f32)).collect(),
            2 => d.base.iter().flat_map(|v| v.map(|x| x as f32)).collect(),
            3 => d.orbit.iter().flat_map(|v| v.map(|x| x as f32)).collect(),
            4 => d
                .cone
                .iter()
                .flat_map(|(v, t)| {
                    [
                        v[0] as f32,
                        (v[1] + d.cone_y) as f32,
                        v[2] as f32,
                        *t as f32,
                    ]
                })
                .collect(),
            _ => return 0,
        };
        unsafe { fill(&flat, out, capacity) }
    })
}

/// A numeric setting by its original name (`brightness`, `glow`, `motion.response`...). False, and nothing
/// changes, for an unknown name or a value that is not finite; limits are applied.
///
/// # Safety
/// `h` is null or live; `key` is null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn neural_set_number(
    h: *mut NeuralSphere,
    key: *const c_char,
    value: f64,
) -> bool {
    guard(false, || {
        match (unsafe { sphere_mut(h) }, unsafe { text(key) }) {
            (Some(s), Some(key)) => s.sphere.configure(|c| c.set_number(key, value)),
            _ => false,
        }
    })
}

/// `ghosts` or `base`.
///
/// # Safety
/// As `neural_set_number`.
#[no_mangle]
pub unsafe extern "C" fn neural_set_flag(
    h: *mut NeuralSphere,
    key: *const c_char,
    value: bool,
) -> bool {
    guard(false, || {
        match (unsafe { sphere_mut(h) }, unsafe { text(key) }) {
            (Some(s), Some(key)) => s.sphere.configure(|c| c.set_flag(key, value)),
            _ => false,
        }
    })
}

/// The background, `#rrggbb`.
///
/// # Safety
/// As `neural_set_number`.
#[no_mangle]
pub unsafe extern "C" fn neural_set_background(h: *mut NeuralSphere, hex: *const c_char) -> bool {
    guard(false, || {
        match (unsafe { sphere_mut(h) }, unsafe { text(hex) }) {
            (Some(s), Some(hex)) => s.sphere.configure(|c| c.set_background(hex)),
            _ => false,
        }
    })
}

/// One link kind (0 to 6): `on` 0 or 1 (any other value keeps it) and its opacity (NaN keeps it).
///
/// # Safety
/// `h` is null or live.
#[no_mangle]
pub unsafe extern "C" fn neural_set_link(
    h: *mut NeuralSphere,
    kind: u32,
    on: i32,
    opacity: f64,
) -> bool {
    guard(false, || {
        let Some(s) = (unsafe { sphere_mut(h) }) else {
            return false;
        };
        let on = match on {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        s.sphere
            .configure(|c| c.set_link(kind as usize, on, Some(opacity)))
    })
}

/// The effective settings as JSON, the original's names (`links` by kind, `motion.response`).
///
/// # Safety
/// `h` is null or live; `out` is null or holds `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn neural_settings_json(
    h: *const NeuralSphere,
    out: *mut c_char,
    capacity: usize,
) -> usize {
    guard(0, || {
        let Some(s) = (unsafe { sphere(h) }) else {
            return 0;
        };
        let c = &s.sphere.settings;
        let links: Vec<String> = c
            .links
            .iter()
            .enumerate()
            .map(|(k, l)| format!("\"{k}\":{{\"on\":{},\"opacity\":{}}}", l.on, l.opacity))
            .collect();
        let json = format!(
            "{{\"brightness\":{},\"nodeSize\":{},\"glow\":{},\"holo\":{},\"rotationSpeed\":{},\"pulses\":{},\"pulseSpeed\":{},\"ghosts\":{},\"ghostOpacity\":{},\"base\":{},\"background\":\"{}\",\"linkOpacity\":{},\"linkBudget\":{},\"links\":{{{}}},\"motion\":{{\"response\":{}}}}}",
            c.brightness, c.node_size, c.glow, c.holo, c.rotation_speed, c.pulses, c.pulse_speed, c.ghosts, c.ghost_opacity, c.base, c.background, c.link_opacity, c.link_budget, links.join(","), c.motion_response
        );
        unsafe { write_text(&json, out, capacity) }
    })
}

/// Selects a node by id; false for an unknown id.
///
/// # Safety
/// `h` is null or live; `id` is null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn neural_select(h: *mut NeuralSphere, id: *const c_char) -> bool {
    guard(false, || {
        match (unsafe { sphere_mut(h) }, unsafe { text(id) }) {
            (Some(s), Some(id)) => s.sphere.select(id),
            _ => false,
        }
    })
}

/// A click on a node: a folded program unfolds and the node is selected.
///
/// # Safety
/// As `neural_select`.
#[no_mangle]
pub unsafe extern "C" fn neural_activate(h: *mut NeuralSphere, id: *const c_char) -> bool {
    guard(false, || {
        match (unsafe { sphere_mut(h) }, unsafe { text(id) }) {
            (Some(s), Some(id)) => s.sphere.activate(id),
            _ => false,
        }
    })
}

/// Folds or unfolds a program or folder.
///
/// # Safety
/// As `neural_select`.
#[no_mangle]
pub unsafe extern "C" fn neural_toggle_fold(h: *mut NeuralSphere, id: *const c_char) -> bool {
    guard(false, || {
        match (unsafe { sphere_mut(h) }, unsafe { text(id) }) {
            (Some(s), Some(id)) => s.sphere.toggle_fold(id),
            _ => false,
        }
    })
}

/// The node a ray meets first (field coordinates; `threshold` in field units, 2.2 in the original), written as
/// a NUL-terminated id. Returns its length, 0 when the ray meets nothing.
///
/// # Safety
/// `h` is null or live; `origin` and `direction` are null or hold three floats; `out` holds `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn neural_pick(
    h: *const NeuralSphere,
    origin: *const f32,
    direction: *const f32,
    threshold: f32,
    out: *mut c_char,
    capacity: usize,
) -> usize {
    guard(0, || {
        let Some(s) = (unsafe { sphere(h) }) else {
            return 0;
        };
        if origin.is_null() || direction.is_null() {
            return 0;
        }
        let (o, d) = unsafe {
            (
                slice::from_raw_parts(origin, 3),
                slice::from_raw_parts(direction, 3),
            )
        };
        let three = |v: &[f32]| [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])];
        match s.sphere.pick(three(o), three(d), f64::from(threshold)) {
            Some(id) => unsafe { write_text(&id, out, capacity) },
            _ => 0,
        }
    })
}

/// Where a node is now, in field coordinates.
///
/// # Safety
/// `h` is null or live; `id` is null or a NUL-terminated string; `out` is null or holds three floats.
#[no_mangle]
pub unsafe extern "C" fn neural_position(
    h: *const NeuralSphere,
    id: *const c_char,
    out: *mut f32,
) -> bool {
    guard(false, || {
        let (Some(s), Some(id)) = (unsafe { sphere(h) }, unsafe { text(id) }) else {
            return false;
        };
        let (Some(at), false) = (s.sphere.position_of(id), out.is_null()) else {
            return false;
        };
        unsafe {
            for (a, v) in at.iter().enumerate() {
                out.add(a).write(*v as f32);
            }
        }
        true
    })
}

/// The HUD's labels, one per line: `kind\tx\ty\tz\tid\ttext` (kind `p` for a running program, `f` for a
/// folder). Returns the length; a call with capacity 0 is the query, and the fill writes the same text.
///
/// # Safety
/// `h` is null or live; `out` is null or holds `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn neural_labels(
    h: *mut NeuralSphere,
    out: *mut c_char,
    capacity: usize,
) -> usize {
    guard(0, || {
        let Some(s) = (unsafe { sphere_mut(h) }) else {
            return 0;
        };
        if out.is_null() || capacity == 0 {
            s.scratch = s
                .sphere
                .labels()
                .iter()
                .map(|l| {
                    format!(
                        "{}\t{}\t{}\t{}\t{}\t{}\n",
                        if l.process { 'p' } else { 'f' },
                        l.position[0],
                        l.position[1],
                        l.position[2],
                        l.id,
                        l.text
                    )
                })
                .collect();
        }
        unsafe { write_text(&s.scratch, out, capacity) }
    })
}

/// The selection, one line each: id, title, counts, and the folded note (or an empty line). `root_label` is
/// null or names the root. Query (capacity 0) then fill, as `neural_labels`.
///
/// # Safety
/// `h` is null or live; `root_label` is null or a NUL-terminated string; `out` holds `capacity` bytes.
#[no_mangle]
pub unsafe extern "C" fn neural_selection(
    h: *mut NeuralSphere,
    root_label: *const c_char,
    out: *mut c_char,
    capacity: usize,
) -> usize {
    guard(0, || {
        let Some(s) = (unsafe { sphere_mut(h) }) else {
            return 0;
        };
        if out.is_null() || capacity == 0 {
            let [title, counts, folded] = s.sphere.selection_text(unsafe { text(root_label) });
            s.scratch = format!("{}\n{title}\n{counts}\n{folded}\n", s.sphere.selected());
        }
        unsafe { write_text(&s.scratch, out, capacity) }
    })
}

/// Pairs available per link kind (0 to 6), drawn or not.
///
/// # Safety
/// `h` is null or live; `out` is null or holds seven u32.
#[no_mangle]
pub unsafe extern "C" fn neural_link_counts(h: *mut NeuralSphere, out: *mut u32) -> bool {
    guard(false, || {
        let Some(s) = (unsafe { sphere_mut(h) }) else {
            return false;
        };
        if out.is_null() {
            return false;
        }
        for (k, n) in s.sphere.link_counts().iter().enumerate() {
            unsafe { out.add(k).write(u32::try_from(*n).unwrap_or(u32::MAX)) };
        }
        true
    })
}

/// Whether the live build is still running.
///
/// # Safety
/// `h` is null or live.
#[no_mangle]
pub unsafe extern "C" fn neural_building(h: *const NeuralSphere) -> bool {
    guard(false, || {
        unsafe { sphere(h) }.is_some_and(|s| s.sphere.building())
    })
}
