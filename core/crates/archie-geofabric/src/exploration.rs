//! Pure, bounded geographic exploration planning. Calling [`plan`] performs no I/O.
//!
//! Radii describe ground distance on the mean-radius sphere, independent of a
//! renderer's camera or Web Mercator's latitude-dependent scale. Provider selection,
//! authorization, network scheduling and actual byte limits belong to the caller.
//! Application activity and focus are deliberately absent from this API.
//!
//! Spherical distance and Web Mercator math adapted from Aleph `src/geo.py`,
//! https://github.com/TheLoki2020/aleph at ad68fc4a3c60fd7af3969597ef0ba886c63b0a40.
//! Circular ROI selection, wrapped bounds and bounded planning are native additions.
//!
//! MIT License — Copyright (c) 2026 Belluxx
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

const EARTH_RADIUS_M: f64 = 6_371_008.8;
pub const MAX_MERCATOR_LATITUDE: f64 = 85.0511287798066;
pub const MAX_RADIUS_M: f64 = 100_000.0;
pub const MAX_ZOOM: u8 = 22;
pub const MAX_TILES: u32 = 4096;
pub const MAX_BYTES: u64 = 1_073_741_824;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorationLayer {
    Terrain,
    Buildings,
    Imagery,
    Signals,
    StreetView,
    Mesh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorationDomain {
    Personal,
    Work,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImagerySource {
    GoogleSatellite,
    BlueMarble,
    Sentinel2,
    Landsat,
    NaturalColor,
    Infrared,
    NightLights,
    Thermal,
}
impl std::str::FromStr for ImagerySource {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value.replace('-', "_").as_str() {
            "google_satellite" => Self::GoogleSatellite,
            "blue_marble" => Self::BlueMarble,
            "sentinel2" => Self::Sentinel2,
            "landsat" => Self::Landsat,
            "natural_color" => Self::NaturalColor,
            "infrared" => Self::Infrared,
            "night_lights" => Self::NightLights,
            "thermal" => Self::Thermal,
            _ => return Err("unknown imagery source".into()),
        })
    }
}
fn default_signal_updates() -> u32 {
    1
}
fn default_signal_interval() -> u32 {
    60
}

pub fn valid_imagery_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..].parse::<u32>() else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    (1900..=2100).contains(&year) && day > 0 && day <= days
}

/// An explicit geographic request. No caller activity can enlarge this region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplorationRequest {
    pub latitude: f64,
    pub longitude: f64,
    /// Positive ground radius, at most [`MAX_RADIUS_M`].
    pub radius_m: f64,
    /// Web Mercator tile zoom, from zero through [`MAX_ZOOM`].
    pub zoom: u8,
    /// Nonempty; duplicates do not multiply the request estimate.
    pub layers: Vec<ExplorationLayer>,
    pub domain: ExplorationDomain,
    /// Maximum unique spatial tiles, at most [`MAX_TILES`].
    pub max_tiles: u32,
    /// Transport's hard response-byte budget, at most [`MAX_BYTES`].
    /// Geometry cannot predict payload sizes; callers must enforce this during I/O.
    pub max_bytes: u64,
    /// Empty selects experimental volatile Google plus open Blue Marble.
    #[serde(default)]
    pub imagery_sources: Vec<ImagerySource>,
    #[serde(default)]
    pub imagery_date: Option<String>,
    #[serde(default = "default_signal_updates")]
    pub signal_updates: u32,
    #[serde(default = "default_signal_interval")]
    pub signal_interval_seconds: u32,
}

impl Default for ExplorationRequest {
    fn default() -> Self {
        Self {
            latitude: 0.0,
            longitude: 0.0,
            radius_m: 500.0,
            zoom: 14,
            layers: vec![ExplorationLayer::Terrain],
            domain: ExplorationDomain::Personal,
            max_tiles: 256,
            max_bytes: 64 * 1024 * 1024,
            imagery_sources: vec![],
            imagery_date: None,
            signal_updates: default_signal_updates(),
            signal_interval_seconds: default_signal_interval(),
        }
    }
}

/// Degrees on the mean-radius sphere. `west > east` explicitly represents a
/// region crossing the antimeridian: `[west, 180] ∪ [-180, east]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoBounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TileCoordinate {
    pub zoom: u8,
    pub x: u32,
    pub y: u32,
}

/// Suggested initial framing. Changing a renderer's camera does not change the ROI.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraTarget {
    pub latitude: f64,
    pub longitude: f64,
    pub radius_m: f64,
    /// Source elevation becomes known when terrain arrives; never a guessed height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplorationPlan {
    /// Retains the caller's explicit domain and budgets.
    pub request: ExplorationRequest,
    pub bounds: GeoBounds,
    /// Unique tiles, sorted by `(zoom, x, y)`, intersecting the ground circle.
    pub tiles: Vec<TileCoordinate>,
    /// Unique tiles times distinct layers. This is a planning estimate, not an
    /// observed transport count or provider-specific expansion guarantee.
    pub estimated_requests: u64,
    pub camera: CameraTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExplorationError {
    #[error("invalid exploration {field}: {reason}")]
    InvalidRequest {
        field: &'static str,
        reason: &'static str,
    },
    #[error("the entire exploration circle must remain within Web Mercator latitude coverage")]
    ProjectionLimit,
    #[error("exploration exceeds the unique tile budget of {limit}")]
    TileBudgetExceeded { limit: u32 },
}

/// Build a deterministic spatial plan without network, clock or application inputs.
///
/// Polar regions are refused rather than silently clamped. Antimeridian regions
/// use wrapped bounds and two column spans. A guaranteed-inside rectangle rejects
/// large plans before enumeration; enumeration stores at most `max_tiles` entries.
pub fn plan(request: &ExplorationRequest) -> Result<ExplorationPlan, ExplorationError> {
    validate(request)?;
    let latitude = request.latitude.to_radians();
    let angle = request.radius_m / EARTH_RADIUS_M;
    let latitude_radius = angle.to_degrees();
    let south = request.latitude - latitude_radius;
    let north = request.latitude + latitude_radius;
    if south < -MAX_MERCATOR_LATITUDE || north > MAX_MERCATOR_LATITUDE {
        return Err(ExplorationError::ProjectionLimit);
    }
    // This is the spherical circle's extremal longitude, not the small-angle
    // approximation r / cos(latitude) used for a flat camera footprint.
    let longitude_radius = (angle.sin() / latitude.cos())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
    let bounds = wrap_bounds(
        south,
        request.longitude - longitude_radius,
        north,
        request.longitude + longitude_radius,
    );
    let size = 1_u32 << request.zoom;
    // A meridian + parallel path to any point in this sub-rectangle is at most
    // R * (|delta_lat| + |delta_lon|) <= radius. Every tile fully inside it
    // therefore meets the circle. This avoids allocating/scanning giant grids.
    let half = latitude_radius / 2.0;
    let inside = wrap_bounds(
        request.latitude - half,
        request.longitude - half,
        request.latitude + half,
        request.longitude + half,
    );
    if contained_tile_count(inside, size) > u64::from(request.max_tiles) {
        return Err(ExplorationError::TileBudgetExceeded {
            limit: request.max_tiles,
        });
    }

    let (y0, y1) = rows(bounds, size);
    let mut tiles = Vec::new();
    for (x0, x1) in columns(bounds, size) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                if intersects_circle(request, x, y, size) {
                    if tiles.len() == request.max_tiles as usize {
                        return Err(ExplorationError::TileBudgetExceeded {
                            limit: request.max_tiles,
                        });
                    }
                    tiles.push(TileCoordinate {
                        zoom: request.zoom,
                        x,
                        y,
                    });
                }
            }
        }
    }
    let distinct_layers = request
        .layers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    Ok(ExplorationPlan {
        request: request.clone(),
        bounds,
        estimated_requests: tiles.len() as u64 * distinct_layers as u64,
        tiles,
        camera: CameraTarget {
            latitude: request.latitude,
            longitude: request.longitude,
            radius_m: request.radius_m,
            elevation_m: None,
        },
    })
}

fn validate(request: &ExplorationRequest) -> Result<(), ExplorationError> {
    let invalid = |field, reason| ExplorationError::InvalidRequest { field, reason };
    if request.imagery_sources.len() > 8 {
        return Err(invalid("imagery_sources", "at most eight typed sources"));
    }
    if request
        .imagery_date
        .as_deref()
        .is_some_and(|date| !valid_imagery_date(date))
    {
        return Err(invalid(
            "imagery_date",
            "must be a valid YYYY-MM-DD civil date",
        ));
    }
    if !(1..=60).contains(&request.signal_updates) {
        return Err(invalid("signal_updates", "must be between 1 and 60"));
    }
    if !(60..=3600).contains(&request.signal_interval_seconds) {
        return Err(invalid(
            "signal_interval_seconds",
            "must be between 60 and 3600",
        ));
    }
    if !request.latitude.is_finite() || !(-90.0..=90.0).contains(&request.latitude) {
        return Err(invalid(
            "latitude",
            "must be finite and between -90 and 90 degrees",
        ));
    }
    if !request.longitude.is_finite() || !(-180.0..=180.0).contains(&request.longitude) {
        return Err(invalid(
            "longitude",
            "must be finite and between -180 and 180 degrees",
        ));
    }
    if !request.radius_m.is_finite() || request.radius_m <= 0.0 || request.radius_m > MAX_RADIUS_M {
        return Err(invalid(
            "radius_m",
            "must be positive, finite and at most 100000 meters",
        ));
    }
    if request.zoom > MAX_ZOOM {
        return Err(invalid("zoom", "must be between 0 and 22"));
    }
    if request.layers.is_empty() || request.layers.len() > 64 {
        return Err(invalid("layers", "must contain between 1 and 64 entries"));
    }
    if request.max_tiles == 0 || request.max_tiles > MAX_TILES {
        return Err(invalid("max_tiles", "must be between 1 and 4096"));
    }
    if request.max_bytes == 0 || request.max_bytes > MAX_BYTES {
        return Err(invalid("max_bytes", "must be between 1 and 1073741824"));
    }
    Ok(())
}

fn wrap_bounds(south: f64, mut west: f64, north: f64, mut east: f64) -> GeoBounds {
    // Preserve opposite signed seam endpoints even for a tiny positive radius
    // whose subtraction at 180 degrees rounds to the endpoint itself.
    if west <= -180.0 {
        west += 360.0;
    }
    if east >= 180.0 {
        east -= 360.0;
    }
    GeoBounds {
        south,
        west,
        north,
        east,
    }
}

fn tile_index(position: f64, size: u32) -> u32 {
    position.floor().clamp(0.0, f64::from(size - 1)) as u32
}

fn contained_tile_count(bounds: GeoBounds, size: u32) -> u64 {
    let count = |low: f64, high: f64| {
        let first = (low + 1e-9).ceil().clamp(0.0, f64::from(size)) as u64;
        let after = (high - 1e-9).floor().clamp(0.0, f64::from(size)) as u64;
        after.saturating_sub(first)
    };
    let longitude = |lon: f64| (lon + 180.0) / 360.0 * f64::from(size);
    let latitude = |lat: f64| (1.0 - lat.to_radians().tan().asinh() / PI) * 0.5 * f64::from(size);
    let columns = if bounds.west <= bounds.east {
        count(longitude(bounds.west), longitude(bounds.east))
    } else {
        count(0.0, longitude(bounds.east)) + count(longitude(bounds.west), f64::from(size))
    };
    columns * count(latitude(bounds.north), latitude(bounds.south))
}

fn rows(bounds: GeoBounds, size: u32) -> (u32, u32) {
    let projected =
        |latitude: f64| (1.0 - latitude.to_radians().tan().asinh() / PI) * 0.5 * f64::from(size);
    // Include touching boundaries despite projection roundoff.
    (
        tile_index(projected(bounds.north) - 1e-9, size),
        tile_index(projected(bounds.south) + 1e-9, size),
    )
}

fn columns(bounds: GeoBounds, size: u32) -> Vec<(u32, u32)> {
    let projected = |longitude: f64| (longitude + 180.0) / 360.0 * f64::from(size);
    if bounds.west <= bounds.east {
        vec![(
            tile_index(projected(bounds.west) - 1e-9, size),
            tile_index(projected(bounds.east) + 1e-9, size),
        )]
    } else {
        let first_end = tile_index(projected(bounds.east) + 1e-9, size);
        let second_start = tile_index(projected(bounds.west) - 1e-9, size);
        // Coarse zoom may place both dateline spans in the same tile. Merge
        // overlapping/adjacent ranges before counting or emitting anything.
        if first_end + 1 >= second_start {
            vec![(0, size - 1)]
        } else {
            vec![(0, first_end), (second_start, size - 1)]
        }
    }
}

fn tile_latitude(y: u32, size: u32) -> f64 {
    (PI * (1.0 - 2.0 * f64::from(y) / f64::from(size)))
        .sinh()
        .atan()
}

/// Exact nearest point on a spherical longitude/latitude rectangle, followed by
/// haversine distance. Choosing the nearest longitude first maximizes cos(delta)
/// at every permitted latitude. Its optimal latitude lies on that meridian or
/// a latitude boundary. The seam is handled by testing equivalent longitudes.
fn intersects_circle(request: &ExplorationRequest, x: u32, y: u32, size: u32) -> bool {
    let west = f64::from(x) / f64::from(size) * 360.0 - 180.0;
    let east = f64::from(x + 1) / f64::from(size) * 360.0 - 180.0;
    let mut delta_longitude = f64::INFINITY;
    for longitude in [
        request.longitude - 360.0,
        request.longitude,
        request.longitude + 360.0,
    ] {
        delta_longitude = delta_longitude.min((longitude - longitude.clamp(west, east)).abs());
    }
    let delta = delta_longitude.to_radians();
    let latitude = request.latitude.to_radians();
    let nearest_latitude = latitude
        .sin()
        .atan2(latitude.cos() * delta.cos())
        .clamp(tile_latitude(y + 1, size), tile_latitude(y, size));
    let h = ((nearest_latitude - latitude) / 2.0).sin().powi(2)
        + latitude.cos() * nearest_latitude.cos() * (delta / 2.0).sin().powi(2);
    let distance = 2.0 * EARTH_RADIUS_M * h.clamp(0.0, 1.0).sqrt().asin();
    distance <= request.radius_m + 1e-6
}
