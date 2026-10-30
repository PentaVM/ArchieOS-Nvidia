//! Mesh (2) and bridge (3) links of the neural sphere and the budget cut: links.ts, by ids.

use std::collections::{HashMap, HashSet};

pub const KIND_WEIGHT: [f64; 7] = [0.3, 0.2, 0.24, 0.36, 0.3, 0.34, 0.28];

#[derive(Debug, Clone, PartialEq)]
pub struct LinkPoint {
    pub id: String,
    pub parent: String,
    pub dir: bool,
    pub p: [f64; 3],
    pub category: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IdLink {
    pub a: String,
    pub b: String,
    pub kind: usize,
    pub weight: f64,
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// links.ts `nearestPairs`: each point's `k` nearest, with a uniform grid past 48 points.
pub fn nearest_pairs(points: &[[f64; 3]], k: usize) -> Vec<(usize, usize)> {
    let n = points.len();
    let mut pairs = Vec::new();
    if n < 2 || k < 1 {
        return pairs;
    }
    let by_distance = |found: &mut Vec<(usize, f64)>| {
        found.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    };
    if n <= 48 {
        for i in 0..n {
            let mut order: Vec<(usize, f64)> = (0..n)
                .map(|j| {
                    (
                        j,
                        if j == i {
                            f64::INFINITY
                        } else {
                            dist2(points[j], points[i])
                        },
                    )
                })
                .collect();
            by_distance(&mut order);
            pairs.extend(order.iter().take(k.min(n - 1)).map(|o| (i, o.0)));
        }
        return pairs;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in points {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let mut spans = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    spans.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let mut area = spans[0] * spans[1];
    if area == 0.0 || area.is_nan() {
        area = spans[0] * spans[0];
    }
    let area = area.max(1e-9);
    let cell = (area * 4.0 / n as f64).sqrt().max(1e-6);
    let cell_of: Vec<[i64; 3]> = points
        .iter()
        .map(|p| {
            [
                ((p[0] - lo[0]) / cell).floor() as i64,
                ((p[1] - lo[1]) / cell).floor() as i64,
                ((p[2] - lo[2]) / cell).floor() as i64,
            ]
        })
        .collect();
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, c) in cell_of.iter().enumerate() {
        grid.entry(*c).or_default().push(i);
    }
    for i in 0..n {
        let [cx, cy, cz] = cell_of[i];
        let mut found: Vec<(usize, f64)> = Vec::new();
        for ring in 1..=6_i64 {
            found.clear();
            for x in cx - ring..=cx + ring {
                for y in cy - ring..=cy + ring {
                    for z in cz - ring..=cz + ring {
                        if let Some(list) = grid.get(&[x, y, z]) {
                            for &j in list {
                                if j != i {
                                    found.push((j, dist2(points[j], points[i])));
                                }
                            }
                        }
                    }
                }
            }
            if found.len() >= k + 2 {
                break;
            }
        }
        by_distance(&mut found);
        pairs.extend(found.iter().take(k).map(|o| (i, o.0)));
    }
    pairs
}

/// links.ts `structuralLinks`: mesh links between neighbouring files of a folder and bridges between
/// neighbouring clusters of the same type. Deterministic.
pub fn structural_links(points: &[LinkPoint]) -> Vec<IdLink> {
    let count = points.len();
    let neighbours = if count > 45_000 { 1 } else { 2 };
    let bridges_per_cluster = 2;
    let index: HashMap<&str, usize> = points
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |links: &mut Vec<IdLink>, a: usize, b: usize, kind: usize| {
        if a == b {
            return;
        }
        let key = (a.min(b), a.max(b));
        if !seen.insert(key) {
            return;
        }
        links.push(IdLink {
            a: points[a].id.clone(),
            b: points[b].id.clone(),
            kind,
            weight: KIND_WEIGHT[kind],
        });
    };
    // Files of each folder, folders in order of first appearance (a JS Map keeps insertion order).
    let mut order: Vec<usize> = Vec::new();
    let mut files: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, p) in points.iter().enumerate() {
        let parent = if p.id == "." {
            None
        } else {
            index.get(p.parent.as_str()).copied()
        };
        let Some(parent) = parent else { continue };
        if p.dir {
            continue;
        }
        files.entry(parent).or_insert_with(|| {
            order.push(parent);
            Vec::new()
        });
        files.get_mut(&parent).unwrap().push(i);
    }
    struct Cluster {
        members: Vec<usize>,
        centre: [f64; 3],
        category: usize,
    }
    let direction = |p: [f64; 3]| {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        let l = if l == 0.0 { 1.0 } else { l };
        [p[0] / l, p[1] / l, p[2] / l]
    };
    let mut clusters = Vec::new();
    for parent in order {
        let members = &files[&parent];
        let coords: Vec<[f64; 3]> = members.iter().map(|&i| points[i].p).collect();
        for (a, b) in nearest_pairs(&coords, neighbours) {
            add(&mut links, members[a], members[b], 2);
        }
        let mut centre = [0.0; 3];
        // The most frequent type; ties go to the one seen first, as a stable sort of a Map's entries does.
        let mut tally: Vec<(usize, usize)> = Vec::new();
        for &i in members {
            let f = direction(points[i].p);
            centre = [centre[0] + f[0], centre[1] + f[1], centre[2] + f[2]];
            match tally.iter_mut().find(|t| t.0 == points[i].category) {
                Some(t) => t.1 += 1,
                None => tally.push((points[i].category, 1)),
            }
        }
        let length = (centre[0] * centre[0] + centre[1] * centre[1] + centre[2] * centre[2]).sqrt();
        let length = if length == 0.0 { 1.0 } else { length };
        tally.sort_by_key(|t| std::cmp::Reverse(t.1));
        clusters.push(Cluster {
            members: members.clone(),
            centre: [centre[0] / length, centre[1] / length, centre[2] / length],
            category: tally[0].0,
        });
    }
    if bridges_per_cluster > 0 && clusters.len() > 1 {
        let sample = |list: &[usize]| -> Vec<usize> {
            if list.len() <= 40 {
                list.to_vec()
            } else {
                (0..40).map(|k| list[k * list.len() / 40]).collect()
            }
        };
        for c in 0..clusters.len() {
            let me = &clusters[c];
            let mut near: Vec<(usize, f64)> = Vec::new();
            for (o, other) in clusters.iter().enumerate() {
                if o == c || other.category != me.category {
                    continue;
                }
                near.push((
                    o,
                    me.centre[0] * other.centre[0]
                        + me.centre[1] * other.centre[1]
                        + me.centre[2] * other.centre[2],
                ));
            }
            near.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for m in 0..bridges_per_cluster.min(near.len()) {
                if near[m].1 < 0.57 {
                    break;
                }
                let other = &clusters[near[m].0];
                let mut best = None;
                let mut best_distance = f64::INFINITY;
                for a in sample(&me.members) {
                    for b in sample(&other.members) {
                        let d = dist2(points[a].p, points[b].p);
                        if d < best_distance {
                            best_distance = d;
                            best = Some((a, b));
                        }
                    }
                }
                if let Some((a, b)) = best {
                    add(&mut links, a, b, 3);
                }
            }
        }
    }
    links
}

/// links.ts `pickBudget`: the fraction of links with the most weight, ties broken by ids.
pub fn pick_budget(links: &[IdLink], fraction: f64) -> Vec<IdLink> {
    if fraction >= 1.0 {
        return links.to_vec();
    }
    let keep = (links.len() as f64 * fraction.max(0.0)).ceil() as usize;
    let mut sorted = links.to_vec();
    sorted.sort_by(|x, y| {
        y.weight
            .partial_cmp(&x.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                // The original compares the joined ids with < and >, which is code-unit order.
                let (kx, ky) = (format!("{}|{}", x.a, x.b), format!("{}|{}", y.a, y.b));
                kx.encode_utf16().cmp(ky.encode_utf16())
            })
    });
    sorted.truncate(keep);
    sorted
}
