//! Camera-driven, bounded Web Mercator quadtree selection. No I/O or authority.
use crate::exploration::{GeoBounds, TileCoordinate};
use serde::{Deserialize, Serialize};

pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Earth-centered meters: +Y north, +Z Greenwich, +X 90 degrees east.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobeViewport {
    pub eye: [f64; 3],
    pub target: [f64; 3],
    pub width: u32,
    pub height: u32,
    pub fov_degrees: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobeSelection {
    pub tiles: Vec<TileCoordinate>,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub visited_nodes: usize,
}

/// Select disjoint visible leaves, refining the greatest projected error first.
/// `max_zoom` is the source's available detail (0..=19); `max_tiles` is 1..=96.
/// A tight budget keeps a coarser parent instead of leaving holes. The 256-pixel
/// projected tile-radius threshold is a geometric LOD heuristic, not terrain
/// accuracy. Only the spherical surface is considered; polar caps beyond the
/// Mercator limit need the renderer's base globe. Ordering is deterministic.
pub fn select(
    view: &GlobeViewport,
    max_zoom: u8,
    max_tiles: usize,
) -> Result<GlobeSelection, String> {
    let camera = Camera::new(view)?;
    if max_zoom > 19 || !(1..=96).contains(&max_tiles) {
        return Err("globe requires max_zoom 0..=19 and max_tiles 1..=96".into());
    }
    let root = TileCoordinate {
        zoom: 0,
        x: 0,
        y: 0,
    };
    let mut visited_nodes = 1;
    let mut leaves: Vec<Node> = camera.node(root).into_iter().collect();
    loop {
        let candidate = leaves
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.blocked && n.tile.zoom < max_zoom && n.error > 256.)
            .max_by(|(ia, a), (ib, b)| a.error.total_cmp(&b.error).then_with(|| ib.cmp(ia)))
            .map(|(i, _)| i);
        let Some(index) = candidate else {
            break;
        };
        let tile = leaves[index].tile;
        let mut children = Vec::with_capacity(4);
        for dy in 0..2 {
            for dx in 0..2 {
                visited_nodes += 1;
                if let Some(child) = camera.node(TileCoordinate {
                    zoom: tile.zoom + 1,
                    x: tile.x * 2 + dx,
                    y: tile.y * 2 + dy,
                }) {
                    children.push(child);
                }
            }
        }
        if leaves.len() - 1 + children.len() <= max_tiles {
            leaves.remove(index);
            leaves.extend(children);
        } else {
            leaves[index].blocked = true;
        }
    }
    let mut tiles: Vec<_> = leaves.into_iter().map(|n| n.tile).collect();
    tiles.sort();
    Ok(GlobeSelection {
        min_zoom: tiles.iter().map(|t| t.zoom).min().unwrap_or(0),
        max_zoom: tiles.iter().map(|t| t.zoom).max().unwrap_or(0),
        tiles,
        visited_nodes,
    })
}

/// Geographic extent of a valid Web Mercator tile; root retains both seam ends.
pub fn tile_bounds(tile: TileCoordinate) -> GeoBounds {
    let size = 2_f64.powi(i32::from(tile.zoom));
    let lat = |row: f64| {
        (std::f64::consts::PI * (1. - 2. * row / size))
            .sinh()
            .atan()
            .to_degrees()
    };
    GeoBounds {
        south: lat(f64::from(tile.y) + 1.),
        north: lat(f64::from(tile.y)),
        west: f64::from(tile.x) / size * 360. - 180.,
        east: (f64::from(tile.x) + 1.) / size * 360. - 180.,
    }
}

/// Unique fallback ancestors, coarse first, excluding the requested leaves.
/// These are a separate progressive loading list, not extra selected leaves.
pub fn ancestors(tiles: &[TileCoordinate]) -> Vec<TileCoordinate> {
    use std::collections::BTreeSet;
    let selected: BTreeSet<_> = tiles.iter().copied().collect();
    let mut result = BTreeSet::new();
    for tile in tiles.iter().filter(|t| t.zoom <= 19) {
        let mut tile = *tile;
        while tile.zoom > 0 {
            tile = TileCoordinate {
                zoom: tile.zoom - 1,
                x: tile.x / 2,
                y: tile.y / 2,
            };
            if !selected.contains(&tile) {
                result.insert(tile);
            }
        }
    }
    result.into_iter().collect()
}

type V3 = [f64; 3];
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V3) -> V3 {
    let d = norm(a);
    a.map(|v| v / d)
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn surface(lat: f64, lon: f64) -> V3 {
    [
        EARTH_RADIUS_M * lat.cos() * lon.sin(),
        EARTH_RADIUS_M * lat.sin(),
        EARTH_RADIUS_M * lat.cos() * lon.cos(),
    ]
}

struct Node {
    tile: TileCoordinate,
    error: f64,
    blocked: bool,
}
struct Camera {
    eye: V3,
    forward: V3,
    right: V3,
    up: V3,
    tan_h: f64,
    tan_v: f64,
    focal: f64,
}
impl Camera {
    fn new(v: &GlobeViewport) -> Result<Self, String> {
        if v.eye.iter().chain(&v.target).any(|n| !n.is_finite())
            || !(16..=8192).contains(&v.width)
            || !(16..=8192).contains(&v.height)
            || !v.fov_degrees.is_finite()
            || !(10.0..=120.0).contains(&v.fov_degrees)
            || !(EARTH_RADIUS_M + 1.0..=EARTH_RADIUS_M * 64.).contains(&norm(v.eye))
            || !norm(sub(v.target, v.eye)).is_finite()
            || norm(sub(v.target, v.eye)) < 1e-6
        {
            return Err("invalid globe camera: finite eye outside Earth, distinct target, dimensions 16..8192 and FOV 10..120 required".into());
        }
        let forward = unit(sub(v.target, v.eye));
        let reference = if forward[1].abs() > 0.999 {
            [0., 0., 1.]
        } else {
            [0., 1., 0.]
        };
        let right = unit(cross(forward, reference));
        let up = cross(right, forward);
        let tan_v = (v.fov_degrees.to_radians() * 0.5).tan();
        Ok(Self {
            eye: v.eye,
            forward,
            right,
            up,
            tan_v,
            tan_h: tan_v * f64::from(v.width) / f64::from(v.height),
            focal: f64::from(v.height) / (2. * tan_v),
        })
    }

    fn node(&self, tile: TileCoordinate) -> Option<Node> {
        let b = tile_bounds(tile);
        let (south, north, west, east) = (
            b.south.to_radians(),
            b.north.to_radians(),
            b.west.to_radians(),
            b.east.to_radians(),
        );
        // Exact maximum eye dot surface point on this spherical rectangle.
        // If the entire rectangle lies beyond the tangent horizon it is hidden.
        let lon = self.eye[0].atan2(self.eye[2]);
        let mut delta = f64::INFINITY;
        for lon in [
            lon - std::f64::consts::TAU,
            lon,
            lon + std::f64::consts::TAU,
        ] {
            delta = delta.min((lon - lon.clamp(west, east)).abs());
        }
        let horizontal = self.eye[0].hypot(self.eye[2]) * delta.cos();
        let best_lat = self.eye[1].atan2(horizontal).clamp(south, north);
        let max_dot = [south, north, best_lat]
            .into_iter()
            .map(|lat| EARTH_RADIUS_M * (self.eye[1] * lat.sin() + horizontal * lat.cos()))
            .fold(f64::NEG_INFINITY, f64::max);
        if max_dot < EARTH_RADIUS_M * EARTH_RADIUS_M - 1. {
            return None;
        }

        // Cartesian extrema occur at lat/lon endpoints or the equator and
        // cardinal meridians. Their AABB encloses the curved patch, unlike a
        // corner-only bound which can cull the center of a large world tile.
        let mut lats = vec![south, north];
        if south <= 0. && north >= 0. {
            lats.push(0.);
        }
        let mut lons = vec![west, east];
        for lon in [
            -std::f64::consts::PI,
            -std::f64::consts::FRAC_PI_2,
            0.,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
        ] {
            if lon >= west && lon <= east {
                lons.push(lon);
            }
        }
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for lat in lats {
            for &lon in &lons {
                let p = surface(lat, lon);
                for i in 0..3 {
                    low[i] = low[i].min(p[i]);
                    high[i] = high[i].max(p[i]);
                }
            }
        }
        let center = if tile.zoom == 0 {
            [0.; 3]
        } else {
            [
                (low[0] + high[0]) * 0.5,
                (low[1] + high[1]) * 0.5,
                (low[2] + high[2]) * 0.5,
            ]
        };
        // The planet itself bounds the root more tightly than its Cartesian
        // box, including when a one-tile budget prevents further refinement.
        let radius = if tile.zoom == 0 {
            EARTH_RADIUS_M
        } else {
            norm(sub(high, low)) * 0.5
        };
        let relative = sub(center, self.eye);
        let depth = dot(relative, self.forward);
        if depth + radius < 0.
            || dot(relative, self.right).abs() - self.tan_h * depth
                > radius * (1. + self.tan_h * self.tan_h).sqrt()
            || dot(relative, self.up).abs() - self.tan_v * depth
                > radius * (1. + self.tan_v * self.tan_v).sqrt()
        {
            return None;
        }
        let nearest = (norm(relative) - radius).max(1.);
        Some(Node {
            tile,
            error: radius * self.focal / nearest,
            blocked: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn nadir(latitude: f64, longitude: f64, altitude: f64) -> GlobeViewport {
        let (lat, lon) = (latitude.to_radians(), longitude.to_radians());
        let unit = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
        GlobeViewport {
            eye: unit.map(|v| v * (EARTH_RADIUS_M + altitude)),
            target: [0.; 3],
            width: 1280,
            height: 800,
            fov_degrees: 50.,
        }
    }

    #[test]
    fn approach_refines_and_distant_view_retains_world_root() {
        let far = select(&nadir(0., 0., EARTH_RADIUS_M * 30.), 19, 96).unwrap();
        assert_eq!(
            far.tiles,
            vec![TileCoordinate {
                zoom: 0,
                x: 0,
                y: 0
            }]
        );
        let orbit = select(&nadir(0., 0., EARTH_RADIUS_M * 2.), 19, 96).unwrap();
        let close = select(&nadir(0., 0., 1000.), 19, 96).unwrap();
        assert!(close.max_zoom >= 13, "{close:?}");
        assert!(close.max_zoom > orbit.max_zoom);
    }

    #[test]
    fn pan_changes_selected_geographic_area() {
        let west = select(&nadir(-33., -71., 10000.), 19, 64).unwrap();
        let east = select(&nadir(35., 139., 10000.), 19, 64).unwrap();
        assert!(!west.tiles.is_empty());
        assert_ne!(west.tiles, east.tiles);
        assert!(west
            .tiles
            .iter()
            .any(|t| t.zoom > 8 && t.x < (1 << t.zoom) / 2 && t.y > (1 << t.zoom) / 2));
        assert!(east
            .tiles
            .iter()
            .any(|t| t.zoom > 8 && t.x > (1 << t.zoom) / 2 && t.y < (1 << t.zoom) / 2));
    }

    #[test]
    fn seam_selects_both_edge_columns() {
        let seam = select(&nadir(0., 180., 10000.), 19, 96).unwrap();
        assert!(
            seam.tiles.iter().any(|t| t.zoom >= 8 && t.x == 0),
            "{seam:?}"
        );
        assert!(seam
            .tiles
            .iter()
            .any(|t| t.zoom >= 8 && t.x + 1 == 1 << t.zoom));
    }

    #[test]
    fn bounded_poles_and_zoom_and_leaves() {
        for lat in [-90., -85., 0., 85., 90.] {
            for cap in [1, 4, 16, 64, 96] {
                let result = select(&nadir(lat, 179.9, 1000.), 19, cap).unwrap();
                assert!(result.tiles.len() <= cap);
                assert!(result.visited_nodes <= 1 + 4 * 96 * 20);
                for tile in &result.tiles {
                    assert!(tile.zoom <= 19 && tile.x < 1 << tile.zoom && tile.y < 1 << tile.zoom);
                }
                for a in &result.tiles {
                    for b in &result.tiles {
                        if a.zoom < b.zoom {
                            assert!(
                                a.x != b.x >> (b.zoom - a.zoom) || a.y != b.y >> (b.zoom - a.zoom)
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            select(&nadir(0., 0., 1000.), 0, 96).unwrap().tiles,
            vec![TileCoordinate {
                zoom: 0,
                x: 0,
                y: 0
            }]
        );
    }

    #[test]
    fn rejects_invalid_camera_and_budget() {
        let valid = nadir(0., 0., 1000.);
        let mut cases = vec![];
        let mut v = valid.clone();
        v.eye = [0.; 3];
        cases.push(v);
        let mut v = valid.clone();
        v.eye = [0., 0., EARTH_RADIUS_M];
        cases.push(v);
        let mut v = valid.clone();
        v.eye = [f64::NAN, 0., 0.];
        cases.push(v);
        let mut v = valid.clone();
        v.target = v.eye;
        cases.push(v);
        let mut v = valid.clone();
        v.target = [f64::INFINITY, 0., 0.];
        cases.push(v);
        let mut v = valid.clone();
        v.width = 0;
        cases.push(v);
        let mut v = valid.clone();
        v.height = 8193;
        cases.push(v);
        let mut v = valid.clone();
        v.fov_degrees = 9.;
        cases.push(v);
        let mut v = valid.clone();
        v.fov_degrees = f64::NAN;
        cases.push(v);
        let mut v = valid.clone();
        v.eye = [0., 0., EARTH_RADIUS_M * 65.];
        cases.push(v);
        for v in cases {
            assert!(select(&v, 19, 64).is_err(), "{v:?}");
        }
        assert!(select(&valid, 20, 64).is_err());
        assert!(select(&valid, 19, 0).is_err());
        assert!(select(&valid, 19, 97).is_err());
        assert!(serde_json::from_str::<GlobeViewport>(r#"{"eye":[0,0,7000000],"target":[0,0,0],"width":800,"height":600,"fov_degrees":45,"activity":"secret"}"#).is_err());
    }

    #[test]
    fn rejects_back_hemisphere_and_outward_view() {
        let front = select(&nadir(0., 0., 100000.), 19, 96).unwrap();
        assert!(!front.tiles.is_empty());
        for tile in front.tiles {
            let b = tile_bounds(tile);
            assert!(b.east >= -90. && b.west <= 90., "{tile:?}");
        }
        let mut out = nadir(0., 0., 1000.);
        out.target = [0., 0., EARTH_RADIUS_M * 3.];
        assert!(select(&out, 19, 96).unwrap().tiles.is_empty());
        assert!(select(&out, 19, 1).unwrap().tiles.is_empty());
    }

    #[test]
    fn selected_leaves_cover_viewport_surface_rays() {
        // Independent ray/sphere intersections for a Greenwich-facing camera.
        // A missing visible quadtree child must leave a sampled ray uncovered.
        for altitude in [1000., 100000., EARTH_RADIUS_M, EARTH_RADIUS_M * 8.] {
            let view = nadir(0., 0., altitude);
            let result = select(&view, 19, 96).unwrap();
            let r = EARTH_RADIUS_M;
            let z = r + altitude;
            let tan_v = (25_f64.to_radians()).tan();
            for sy in -8..=8 {
                for sx in -12..=12 {
                    let direction = unit([
                        f64::from(sx) / 12. * tan_v * 1.6,
                        f64::from(sy) / 8. * tan_v,
                        -1.,
                    ]);
                    let projection = z * direction[2];
                    let discriminant = projection * projection - (z * z - r * r);
                    if discriminant < 0. {
                        continue;
                    }
                    let t = -projection - discriminant.sqrt();
                    if t <= 0. {
                        continue;
                    }
                    let p = [direction[0] * t, direction[1] * t, z + direction[2] * t];
                    let lat = (p[1] / r).asin();
                    if lat.abs() > 85.0511287798066_f64.to_radians() {
                        continue;
                    }
                    let lon = p[0].atan2(p[2]);
                    assert!(
                        result.tiles.iter().any(|tile| {
                            let n = f64::from(1_u32 << tile.zoom);
                            let x = ((lon + std::f64::consts::PI) / std::f64::consts::TAU * n)
                                .floor()
                                .clamp(0., n - 1.) as u32;
                            let y = ((1. - lat.tan().asinh() / std::f64::consts::PI) * 0.5 * n)
                                .floor()
                                .clamp(0., n - 1.) as u32;
                            tile.x == x && tile.y == y
                        }),
                        "uncovered ray ({sx},{sy}), altitude {altitude}"
                    );
                }
            }
        }
    }

    #[test]
    fn bounds_and_parent_fallbacks_preserve_world_and_order() {
        let b = tile_bounds(TileCoordinate {
            zoom: 0,
            x: 0,
            y: 0,
        });
        assert_eq!((b.west, b.east), (-180., 180.));
        assert!((b.north - 85.0511287798066).abs() < 1e-9);
        assert!((b.south + 85.0511287798066).abs() < 1e-9);
        assert_eq!(
            ancestors(&[
                TileCoordinate {
                    zoom: 2,
                    x: 3,
                    y: 1
                },
                TileCoordinate {
                    zoom: 2,
                    x: 2,
                    y: 1
                }
            ]),
            vec![
                TileCoordinate {
                    zoom: 0,
                    x: 0,
                    y: 0
                },
                TileCoordinate {
                    zoom: 1,
                    x: 1,
                    y: 0
                }
            ]
        );
    }
}
