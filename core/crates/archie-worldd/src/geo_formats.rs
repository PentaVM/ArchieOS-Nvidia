//! Bounded native World artifact codecs. No network or execution authority.
//!
//! Aleph-derived terrain/mesh conventions: TheLoki2020/aleph, pinned at
//! ad68fc4a3c60fd7af3969597ef0ba886c63b0a40 (src/terrain.py, src/mesh.py).
//! Copyright (c) 2026 Belluxx, MIT License.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

#[derive(Debug, Clone)]
pub struct DecodeLimits {
    pub max_input_bytes: usize,
    pub max_raster_pixels: usize,
    pub max_preview_side: usize,
    pub max_features: usize,
    pub max_vertices_per_feature: usize,
    pub max_total_vertices: usize,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024 * 1024,
            max_raster_pixels: 4 * 1024 * 1024,
            max_preview_side: 128,
            max_features: 2048,
            max_vertices_per_feature: 2048,
            max_total_vertices: 100_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerrainPreview {
    pub width: usize,
    pub height: usize,
    /// North-to-south rows; None is source nodata, never a zero elevation.
    pub elevations_m: Vec<Option<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildingFeature {
    pub id: String,
    /// Closed WGS84 longitude/latitude exterior ring.
    pub ring: Vec<[f64; 2]>,
    /// Only the explicit source height tag; levels are not measured heights.
    pub height_m: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoAnchor {
    pub lon: f64,
    pub lat: f64,
}

/// Decode single-band AWS-compatible signed Int16 / Float32 TIFF. Geographic
/// bounds are supplied separately by the caller; this does not certify GeoTIFF
/// georeferencing or a vertical datum. Nearest-neighbor preview retains corners.
pub fn decode_terrain_tiff(bytes: &[u8], limits: &DecodeLimits) -> Result<TerrainPreview, String> {
    check_limits(limits)?;
    if bytes.len() > limits.max_input_bytes {
        return Err("TIFF input byte limit exceeded".into());
    }
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut codec_limits = tiff::decoder::Limits::default();
    codec_limits.decoding_buffer_size = limits.max_raster_pixels * 4;
    codec_limits.intermediate_buffer_size = limits.max_raster_pixels * 4;
    codec_limits.ifd_value_size = limits.max_input_bytes.min(1024 * 1024);
    decoder = decoder.with_limits(codec_limits);
    let (width, height) = decoder.dimensions().map_err(|e| e.to_string())?;
    let (width, height) = (width as usize, height as usize);
    let count = width
        .checked_mul(height)
        .ok_or("TIFF pixel limit overflow")?;
    if width == 0 || height == 0 || count > limits.max_raster_pixels {
        return Err("TIFF pixel limit exceeded".into());
    }
    if !matches!(
        decoder.colortype().map_err(|e| e.to_string())?,
        tiff::ColorType::Gray(16 | 32)
    ) {
        return Err("terrain requires single-band signed Int16 or Float32 TIFF".into());
    }
    if decoder
        .find_tag(tiff::tags::Tag::Orientation)
        .map_err(|e| e.to_string())?
        .map(|v| v.into_u16())
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(1)
        != 1
    {
        return Err("terrain requires north-to-south TIFF orientation".into());
    }
    let nodata = decoder
        .find_tag(tiff::tags::Tag::Unknown(42113))
        .map_err(|e| e.to_string())?
        .map(|v| v.into_string())
        .transpose()
        .map_err(|e| e.to_string())?
        .map(|s| {
            s.trim_matches('\0')
                .trim()
                .parse::<f32>()
                .map_err(|_| "invalid TIFF nodata value")
        })
        .transpose()?;
    let decoded = decoder.read_image().map_err(|e| e.to_string())?;
    // TIFF itself retains the sample kind, unlike the color type's bit depth.
    let sample: Box<dyn Fn(usize) -> f32> = match decoded {
        tiff::decoder::DecodingResult::I16(data) if data.len() == count => {
            Box::new(move |i| data[i] as f32)
        }
        tiff::decoder::DecodingResult::F32(data) if data.len() == count => {
            Box::new(move |i| data[i])
        }
        _ => return Err("terrain requires signed Int16 or Float32 samples".into()),
    };
    let out_width = width.min(limits.max_preview_side);
    let out_height = height.min(limits.max_preview_side);
    let mut elevations_m = Vec::with_capacity(out_width * out_height);
    for y in 0..out_height {
        let source_y = grid_index(y, out_height, height);
        for x in 0..out_width {
            let value = sample(source_y * width + grid_index(x, out_width, width));
            elevations_m.push((value.is_finite() && nodata != Some(value)).then_some(value));
        }
    }
    Ok(TerrainPreview {
        width: out_width,
        height: out_height,
        elevations_m,
    })
}

/// Parse closed building ways, with inline `out geom` geometry or returned nodes.
/// Building relations (holes/multipolygons) are rejected rather than flattened.
/// Unknown height stays None; an explicit tag is source-reported, not surveyed.
pub fn parse_overpass_buildings(
    bytes: &[u8],
    limits: &DecodeLimits,
) -> Result<Vec<BuildingFeature>, String> {
    check_limits(limits)?;
    if bytes.len() > limits.max_input_bytes {
        return Err("Overpass input byte limit exceeded".into());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let elements = value
        .get("elements")
        .and_then(Value::as_array)
        .ok_or("Overpass elements missing")?;
    let mut nodes = BTreeMap::new();
    for element in elements {
        if element["type"] == "node" {
            let id = element["id"].as_i64().ok_or("invalid node id")?;
            let point = parse_point(element)?;
            if nodes.insert(id, point).is_some() {
                return Err("duplicate Overpass node".into());
            }
            if nodes.len() > limits.max_total_vertices {
                return Err("Overpass node limit exceeded".into());
            }
        }
    }
    let mut buildings = Vec::new();
    let mut ids = BTreeSet::new();
    let mut total = 0usize;
    for element in elements {
        let tags = &element["tags"];
        let building = tags.get("building").and_then(Value::as_str);
        let part = tags.get("building:part").and_then(Value::as_str);
        if ![building, part]
            .iter()
            .any(|v| matches!(v, Some(s) if !s.is_empty() && *s != "no"))
        {
            continue;
        }
        if element["type"] != "way" {
            return Err("building relations/nodes are unsupported; require closed ways".into());
        }
        if buildings.len() >= limits.max_features {
            return Err("building feature limit exceeded".into());
        }
        let id = format!(
            "way/{}",
            element["id"].as_i64().ok_or("invalid building way id")?
        );
        if !ids.insert(id.clone()) {
            return Err("duplicate building way id".into());
        }
        let mut ring = Vec::new();
        if let Some(geometry) = element.get("geometry").and_then(Value::as_array) {
            if geometry.len() > limits.max_vertices_per_feature {
                return Err("building vertex limit exceeded".into());
            }
            for point in geometry {
                ring.push(parse_point(point)?);
            }
        } else {
            let refs = element["nodes"]
                .as_array()
                .ok_or("building way has no geometry or nodes")?;
            if refs.len() > limits.max_vertices_per_feature {
                return Err("building vertex limit exceeded".into());
            }
            for id in refs {
                ring.push(
                    *nodes
                        .get(&id.as_i64().ok_or("invalid node reference")?)
                        .ok_or("unresolved building node reference")?,
                );
            }
        }
        let height_m = tags.get("height").and_then(|v| {
            v.as_str()
                .and_then(parse_height)
                .or_else(|| v.as_f64().filter(|n| valid_height(*n)))
        });
        let feature = BuildingFeature { id, ring, height_m };
        validate_building(&feature, limits)?;
        total = total
            .checked_add(feature.ring.len())
            .ok_or("building vertex limit overflow")?;
        if total > limits.max_total_vertices {
            return Err("building total vertex limit exceeded".into());
        }
        buildings.push(feature);
    }
    buildings.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(buildings)
}

/// GeoJSON uses source longitude/latitude rings and points for valid terrain
/// samples. Nodata samples are absent; no inferred heights are exported.
pub fn export_geojson(
    bounds: GeoBounds,
    terrain: Option<&TerrainPreview>,
    buildings: &[BuildingFeature],
) -> Result<Vec<u8>, String> {
    validate_export(bounds, terrain, buildings)?;
    let mut features = Vec::new();
    for building in buildings {
        features.push(json!({"type":"Feature","id":building.id,
            "properties":{"kind":"building","height_m":building.height_m,"height_source":if building.height_m.is_some(){"source_tag"}else{"unknown"}},
            "geometry":{"type":"Polygon","coordinates":[building.ring]}}));
    }
    if let Some(terrain) = terrain {
        for (i, elevation) in terrain.elevations_m.iter().enumerate() {
            if let Some(elevation) = elevation {
                let point = terrain_point(bounds, terrain, i);
                features.push(json!({"type":"Feature","properties":{"kind":"terrain_sample","elevation_m":elevation},
                    "geometry":{"type":"Point","coordinates":[point[0],point[1],elevation]}}));
            }
        }
    }
    serde_json::to_vec(&json!({"type":"FeatureCollection","bbox":[bounds.west,bounds.south,bounds.east,bounds.north],
        "features":features,"archie":{"terrain_sampling":"nearest-neighbor; Web Mercator row spacing","vertical_datum":"source; unverified"}})).map_err(|e| e.to_string())
}

/// Deterministic glTF 2.0 binary in local east/up/south meters around a WGS84
/// anchor. Buildings rest on an explicitly labeled 0m preview base. Unknown
/// heights render flat unless the caller provides a labeled estimated height.
pub fn export_glb(
    anchor: GeoAnchor,
    bounds: GeoBounds,
    terrain: Option<&TerrainPreview>,
    buildings: &[BuildingFeature],
    estimated_height_m: Option<f64>,
) -> Result<Vec<u8>, String> {
    validate_export(bounds, terrain, buildings)?;
    validate_point([anchor.lon, anchor.lat])?;
    if estimated_height_m.is_some_and(|v| !valid_height(v)) {
        return Err("invalid estimated height".into());
    }
    let mut writer = GlbWriter::default();
    if let Some(terrain) = terrain {
        let mut positions = Vec::new();
        let mut remap = vec![None; terrain.elevations_m.len()];
        for (i, value) in terrain.elevations_m.iter().enumerate() {
            if let Some(height) = value {
                remap[i] = Some(positions.len() as u32);
                positions.push(local_position(
                    anchor,
                    terrain_point(bounds, terrain, i),
                    *height as f64,
                )?);
            }
        }
        let mut indices = Vec::new();
        for y in 0..terrain.height.saturating_sub(1) {
            for x in 0..terrain.width.saturating_sub(1) {
                let a = y * terrain.width + x;
                let b = a + 1;
                let c = a + terrain.width;
                let d = c + 1;
                // North-to-south rows: a,d,b and a,c,d face up.
                for triangle in [[a, d, b], [a, c, d]] {
                    if let [Some(a), Some(b), Some(c)] = triangle.map(|i| remap[i]) {
                        indices.extend([a, b, c]);
                    }
                }
            }
        }
        if !indices.is_empty() {
            writer.mesh(
                "terrain",
                positions,
                indices,
                json!({"kind":"terrain","nodata":"triangles omitted"}),
            )?;
        }
    }
    for building in buildings {
        let ring = &building.ring[..building.ring.len() - 1];
        let planar: Vec<f64> = ring.iter().flatten().copied().collect();
        let roof = earcutr::earcut(&planar, &[], 2).map_err(|e| e.to_string())?;
        if roof.len() != (ring.len() - 2) * 3 {
            return Err(format!("cannot triangulate building {}", building.id));
        }
        let height = building.height_m.or(estimated_height_m);
        let estimated = building.height_m.is_none() && estimated_height_m.is_some();
        let mut positions = Vec::with_capacity(ring.len() * 2);
        for point in ring {
            positions.push(local_position(anchor, *point, height.unwrap_or(0.))?);
        }
        let mut indices: Vec<u32> = roof.into_iter().map(|i| i as u32).collect();
        // Earcut roof order is corrected in local x/z, independent of source winding.
        for triangle in indices.chunks_exact_mut(3) {
            let [a, b, c] = [
                positions[triangle[0] as usize],
                positions[triangle[1] as usize],
                positions[triangle[2] as usize],
            ];
            if (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]) < 0. {
                triangle.swap(1, 2);
            }
        }
        if height.is_some() {
            let offset = positions.len() as u32;
            for point in ring {
                positions.push(local_position(anchor, *point, 0.)?);
            }
            for i in 0..offset {
                let j = (i + 1) % offset;
                indices.extend([i, j, offset + j, i, offset + j, offset + i]);
            }
        }
        writer.mesh(&building.id,positions,indices,json!({"kind":"building","height_m":height,
            "height_is_estimated":estimated,"height_source":if estimated{"preview_default"}else if height.is_some(){"source_tag"}else{"unknown"},
            "base_elevation_m":0,"base_elevation_source":"preview_zero; terrain placement not resolved"}))?;
    }
    writer.finish(anchor, bounds)
}

fn check_limits(limits: &DecodeLimits) -> Result<(), String> {
    // Public knobs may tighten these hard ceilings, never remove them.
    let hard = DecodeLimits::default();
    if limits.max_input_bytes == 0
        || limits.max_input_bytes > hard.max_input_bytes
        || limits.max_raster_pixels == 0
        || limits.max_raster_pixels > hard.max_raster_pixels
        || limits.max_preview_side == 0
        || limits.max_preview_side > hard.max_preview_side
        || limits.max_features > hard.max_features
        || limits.max_vertices_per_feature > hard.max_vertices_per_feature
        || limits.max_total_vertices > hard.max_total_vertices
    {
        return Err("invalid codec limit".into());
    }
    Ok(())
}

fn grid_index(i: usize, output: usize, input: usize) -> usize {
    if output == 1 {
        input / 2
    } else {
        i * (input - 1) / (output - 1)
    }
}

fn parse_point(value: &Value) -> Result<[f64; 2], String> {
    let point = [
        value["lon"].as_f64().ok_or("missing longitude")?,
        value["lat"].as_f64().ok_or("missing latitude")?,
    ];
    validate_point(point)?;
    Ok(point)
}

fn validate_point([lon, lat]: [f64; 2]) -> Result<(), String> {
    if !lon.is_finite()
        || !lat.is_finite()
        || !(-180. ..=180.).contains(&lon)
        || !(-90. ..=90.).contains(&lat)
    {
        return Err("invalid WGS84 longitude/latitude".into());
    }
    Ok(())
}

fn valid_height(height: f64) -> bool {
    height.is_finite() && height > 0. && height <= 10_000.
}

fn parse_height(text: &str) -> Option<f64> {
    let text = text.trim();
    let (number, factor) =
        if let Some(number) = text.strip_suffix(" ft").or_else(|| text.strip_suffix('\'')) {
            (number, 0.3048)
        } else {
            (text.strip_suffix('m').unwrap_or(text), 1.)
        };
    number
        .trim()
        .parse::<f64>()
        .ok()
        .map(|n| n * factor)
        .filter(|n| valid_height(*n))
}

fn validate_building(feature: &BuildingFeature, limits: &DecodeLimits) -> Result<(), String> {
    if feature.id.is_empty() || feature.id.len() > 256 {
        return Err("invalid building id".into());
    }
    let ring = &feature.ring;
    if ring.len() > limits.max_vertices_per_feature {
        return Err("building vertex limit exceeded".into());
    }
    if ring.len() < 4 || ring.first() != ring.last() {
        return Err("building ring must be closed with at least three vertices".into());
    }
    for point in ring {
        validate_point(*point)?;
    }
    let origin = ring[0];
    let area: f64 = ring
        .windows(2)
        .map(|p| {
            (p[0][0] - origin[0]) * (p[1][1] - origin[1])
                - (p[1][0] - origin[0]) * (p[0][1] - origin[1])
        })
        .sum();
    if area.abs() < 1e-16 || ring.windows(2).any(|p| p[0] == p[1]) {
        return Err("degenerate building ring".into());
    }
    if feature.height_m.is_some_and(|n| !valid_height(n)) {
        return Err("invalid building height".into());
    }
    Ok(())
}

fn validate_export(
    bounds: GeoBounds,
    terrain: Option<&TerrainPreview>,
    buildings: &[BuildingFeature],
) -> Result<(), String> {
    validate_point([bounds.west, bounds.south])?;
    validate_point([bounds.east, bounds.north])?;
    if bounds.west >= bounds.east
        || bounds.south >= bounds.north
        || bounds.south < -85.05112878
        || bounds.north > 85.05112878
    {
        return Err("require non-wrapping Web Mercator bounds".into());
    }
    let limits = DecodeLimits::default();
    if buildings.len() > limits.max_features {
        return Err("building feature limit exceeded".into());
    }
    let mut vertices = 0usize;
    for building in buildings {
        validate_building(building, &limits)?;
        vertices = vertices
            .checked_add(building.ring.len())
            .ok_or("vertex limit overflow")?;
        if vertices > limits.max_total_vertices {
            return Err("building total vertex limit exceeded".into());
        }
    }
    if let Some(terrain) = terrain {
        if terrain.width == 0
            || terrain.height == 0
            || terrain.width > limits.max_preview_side
            || terrain.height > limits.max_preview_side
            || terrain.width.checked_mul(terrain.height) != Some(terrain.elevations_m.len())
            || terrain
                .elevations_m
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
        {
            return Err("invalid bounded terrain preview".into());
        }
    }
    Ok(())
}

fn terrain_point(bounds: GeoBounds, terrain: &TerrainPreview, index: usize) -> [f64; 2] {
    let fraction = |i: usize, count: usize| {
        if count == 1 {
            0.5
        } else {
            i as f64 / (count - 1) as f64
        }
    };
    let x = fraction(index % terrain.width, terrain.width);
    let y = fraction(index / terrain.width, terrain.height);
    let mercator = |lat: f64| {
        (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.)
            .tan()
            .ln()
    };
    let north = mercator(bounds.north);
    let south = mercator(bounds.south);
    [
        bounds.west + x * (bounds.east - bounds.west),
        (2. * (north + y * (south - north)).exp().atan() - std::f64::consts::FRAC_PI_2)
            .to_degrees(),
    ]
}

fn local_position(anchor: GeoAnchor, point: [f64; 2], height: f64) -> Result<[f32; 3], String> {
    // WGS84 ellipsoid horizontal tangent projection. Y deliberately preserves
    // source elevation, without inventing an ellipsoid/geoid conversion.
    let ecef = |lon: f64, lat: f64| {
        let (sin_lat, cos_lat) = lat.to_radians().sin_cos();
        let (sin_lon, cos_lon) = lon.to_radians().sin_cos();
        let n = 6_378_137. / (1. - 6.69437999014e-3 * sin_lat * sin_lat).sqrt();
        [
            n * cos_lat * cos_lon,
            n * cos_lat * sin_lon,
            n * (1. - 6.69437999014e-3) * sin_lat,
        ]
    };
    let center = ecef(anchor.lon, anchor.lat);
    let position = ecef(point[0], point[1]);
    let d = [
        position[0] - center[0],
        position[1] - center[1],
        position[2] - center[2],
    ];
    let (sin_lat, cos_lat) = anchor.lat.to_radians().sin_cos();
    let (sin_lon, cos_lon) = anchor.lon.to_radians().sin_cos();
    let result = [
        (-sin_lon * d[0] + cos_lon * d[1]) as f32,
        height as f32,
        (sin_lat * cos_lon * d[0] + sin_lat * sin_lon * d[1] - cos_lat * d[2]) as f32,
    ];
    if result.iter().any(|v| !v.is_finite()) {
        return Err("non-finite mesh position".into());
    }
    Ok(result)
}

#[derive(Default)]
struct GlbWriter {
    binary: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
}

impl GlbWriter {
    fn view(&mut self, bytes: &[u8], target: u32) -> usize {
        let index = self.views.len();
        self.views.push(json!({"buffer":0,"byteOffset":self.binary.len(),"byteLength":bytes.len(),"target":target}));
        self.binary.extend_from_slice(bytes);
        while self.binary.len() % 4 != 0 {
            self.binary.push(0);
        }
        index
    }
    fn mesh(
        &mut self,
        name: &str,
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        extras: Value,
    ) -> Result<(), String> {
        if positions.is_empty() || indices.is_empty() {
            return Ok(());
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut bytes = Vec::with_capacity(positions.len() * 12);
        for position in &positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
                bytes.extend_from_slice(&position[axis].to_le_bytes());
            }
        }
        let position_view = self.view(&bytes, 34962);
        let index_bytes: Vec<u8> = indices.iter().flat_map(|n| n.to_le_bytes()).collect();
        let index_view = self.view(&index_bytes, 34963);
        let accessor = self.accessors.len();
        self.accessors.push(json!({"bufferView":position_view,"componentType":5126,"count":positions.len(),"type":"VEC3","min":min,"max":max}));
        self.accessors.push(json!({"bufferView":index_view,"componentType":5125,"count":indices.len(),"type":"SCALAR"}));
        self.meshes.push(json!({"name":name,"extras":extras,"primitives":[{"attributes":{"POSITION":accessor},"indices":accessor+1,"material":0,"mode":4}]}));
        if self.binary.len() > 32 * 1024 * 1024 {
            return Err("GLB preview byte limit exceeded".into());
        }
        Ok(())
    }
    fn finish(self, anchor: GeoAnchor, bounds: GeoBounds) -> Result<Vec<u8>, String> {
        if self.meshes.is_empty() {
            return Err("no triangulatable geometry for GLB".into());
        }
        let nodes: Vec<Value> = (0..self.meshes.len()).map(|i| json!({"mesh":i})).collect();
        let node_ids: Vec<usize> = (0..self.meshes.len()).collect();
        let value = json!({"asset":{"version":"2.0","generator":"ArchieOS World native geofabric; Aleph conventions (MIT Belluxx)",
            "extras":{"anchor":anchor,"bounds":bounds,"frame":"WGS84 local meters east/up/south","vertical_datum":"source elevations; unverified"}},
            "scene":0,"scenes":[{"nodes":node_ids}],"nodes":nodes,"meshes":self.meshes,
            "buffers":[{"byteLength":self.binary.len()}],"bufferViews":self.views,"accessors":self.accessors,
            "extensionsUsed":["KHR_materials_unlit"],"materials":[{"doubleSided":true,"extensions":{"KHR_materials_unlit":{}},"pbrMetallicRoughness":{"baseColorFactor":[0.45,0.65,0.55,1.0],"metallicFactor":0,"roughnessFactor":1}}]});
        let mut header = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        while header.len() % 4 != 0 {
            header.push(b' ');
        }
        let total = 28usize
            .checked_add(header.len())
            .and_then(|n| n.checked_add(self.binary.len()))
            .ok_or("GLB size overflow")?;
        if total > 64 * 1024 * 1024 {
            return Err("GLB artifact byte limit exceeded".into());
        }
        let mut output = Vec::with_capacity(total);
        for word in [
            0x46546c67u32,
            2,
            total as u32,
            header.len() as u32,
            0x4e4f534a,
        ] {
            output.extend_from_slice(&word.to_le_bytes());
        }
        output.extend_from_slice(&header);
        output.extend_from_slice(&(self.binary.len() as u32).to_le_bytes());
        output.extend_from_slice(&0x004e4942u32.to_le_bytes());
        output.extend_from_slice(&self.binary);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::io::Cursor;

    fn bounds() -> GeoBounds {
        GeoBounds {
            west: -70.0,
            south: -34.0,
            east: -69.99,
            north: -33.99,
        }
    }
    fn building(height_m: Option<f64>) -> BuildingFeature {
        BuildingFeature {
            id: "way/7".into(),
            ring: vec![
                [-70., -34.],
                [-69.999, -34.],
                [-69.999, -33.999],
                [-70., -34.],
            ],
            height_m,
        }
    }
    fn glb_json(bytes: &[u8]) -> Value {
        assert_eq!(&bytes[..4], b"glTF");
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
            bytes.len()
        );
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        serde_json::from_slice(&bytes[20..20 + len]).unwrap()
    }

    #[test]
    fn decodes_real_signed_tiff_and_preserves_nodata() {
        let mut data = Cursor::new(Vec::new());
        {
            let mut writer = tiff::encoder::TiffEncoder::new(&mut data).unwrap();
            let mut image = writer
                .new_image::<tiff::encoder::colortype::GrayI16>(3, 2)
                .unwrap();
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Unknown(42113), "-32768")
                .unwrap();
            image.write_data(&[10, 20, -32768, -5, 0, 80]).unwrap();
        }
        let result = decode_terrain_tiff(data.get_ref(), &DecodeLimits::default()).unwrap();
        assert_eq!((result.width, result.height), (3, 2));
        assert_eq!(
            result.elevations_m,
            vec![Some(10.), Some(20.), None, Some(-5.), Some(0.), Some(80.)]
        );
        let limits = DecodeLimits {
            max_raster_pixels: 4,
            ..DecodeLimits::default()
        };
        assert!(decode_terrain_tiff(data.get_ref(), &limits)
            .unwrap_err()
            .contains("limit"));
    }

    #[test]
    fn float_tiff_downsamples_without_filling_nan_holes() {
        let mut data = Cursor::new(Vec::new());
        {
            let mut writer = tiff::encoder::TiffEncoder::new(&mut data).unwrap();
            writer
                .write_image::<tiff::encoder::colortype::Gray32Float>(
                    3,
                    3,
                    &[1., 2., f32::NAN, 4., 5., 6., 7., 8., 9.],
                )
                .unwrap();
        }
        let limits = DecodeLimits {
            max_preview_side: 2,
            ..DecodeLimits::default()
        };
        let result = decode_terrain_tiff(data.get_ref(), &limits).unwrap();
        assert_eq!(
            result.elevations_m,
            vec![Some(1.), None, Some(7.), Some(9.)]
        );
    }

    #[test]
    fn overpass_uses_geometry_or_node_references_and_explicit_height_only() {
        let source = json!({"elements":[
            {"type":"node","id":1,"lon":-70.,"lat":-34.},
            {"type":"node","id":2,"lon":-69.999,"lat":-34.},
            {"type":"node","id":3,"lon":-69.999,"lat":-33.999},
            {"type":"way","id":7,"nodes":[1,2,3,1],"tags":{"building":"yes","height":"12.5 m"}},
            {"type":"way","id":8,"geometry":[{"lon":0.,"lat":0.},{"lon":0.01,"lat":0.},{"lon":0.,"lat":0.01},{"lon":0.,"lat":0.}],"tags":{"building":"yes","building:levels":"8"}},
            {"type":"way","id":9,"tags":{"building":"no"}}
        ]});
        let result = parse_overpass_buildings(
            &serde_json::to_vec(&source).unwrap(),
            &DecodeLimits::default(),
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], building(Some(12.5)));
        assert_eq!(result[1].height_m, None);
    }

    #[test]
    fn overpass_rejects_open_or_unresolved_rings_and_resource_overflow() {
        let bytes =
            br#"{"elements":[{"type":"way","id":1,"nodes":[4,5,6,4],"tags":{"building":"yes"}}]}"#;
        assert!(parse_overpass_buildings(bytes, &DecodeLimits::default()).is_err());
        let source = json!({"elements":[{"type":"way","id":7,"geometry":[{"lon":0.,"lat":0.},{"lon":1.,"lat":0.},{"lon":0.,"lat":1.}],"tags":{"building":"yes"}}]});
        assert!(parse_overpass_buildings(
            &serde_json::to_vec(&source).unwrap(),
            &DecodeLimits::default()
        )
        .is_err());
        let limits = DecodeLimits {
            max_input_bytes: 8,
            ..DecodeLimits::default()
        };
        assert!(parse_overpass_buildings(bytes, &limits)
            .unwrap_err()
            .contains("limit"));
    }

    #[test]
    fn geojson_keeps_unknown_height_null_and_nodata_out_of_points() {
        let terrain = TerrainPreview {
            width: 2,
            height: 2,
            elevations_m: vec![Some(0.), None, Some(-3.), Some(9.)],
        };
        let result: Value = serde_json::from_slice(
            &export_geojson(bounds(), Some(&terrain), &[building(None)]).unwrap(),
        )
        .unwrap();
        assert_eq!(result["features"].as_array().unwrap().len(), 4);
        assert_eq!(result["features"][0]["properties"]["height_m"], Value::Null);
        assert_eq!(result["features"][1]["geometry"]["coordinates"][2], 0.);
    }

    #[test]
    fn glb_is_deterministic_anchored_and_labels_estimates() {
        let anchor = GeoAnchor {
            lon: -70.,
            lat: -34.,
        };
        let features = vec![building(None)];
        let bytes = export_glb(anchor, bounds(), None, &features, Some(6.)).unwrap();
        assert_eq!(
            bytes,
            export_glb(anchor, bounds(), None, &features, Some(6.)).unwrap()
        );
        let value = glb_json(&bytes);
        assert_eq!(
            value["asset"]["extras"]["frame"],
            "WGS84 local meters east/up/south"
        );
        assert_eq!(value["meshes"][0]["extras"]["height_is_estimated"], true);
        assert_eq!(value["meshes"][0]["extras"]["height_m"], 6.);
        assert!(value["accessors"][0]["min"][0].as_f64().unwrap().abs() < 0.01);
        assert!(value["accessors"][0]["max"][0].as_f64().unwrap() > 80.);
        assert!(value["accessors"][0]["min"][2].as_f64().unwrap() < -100.);
    }

    #[test]
    fn invalid_public_structs_do_not_panic_exporters() {
        let invalid = TerrainPreview {
            width: usize::MAX,
            height: 2,
            elevations_m: vec![],
        };
        assert!(export_geojson(bounds(), Some(&invalid), &[]).is_err());
        assert!(export_glb(GeoAnchor { lon: 0., lat: 91. }, bounds(), None, &[], None).is_err());
    }

    #[test]
    fn terrain_glb_omits_triangles_touching_nodata() {
        let terrain = TerrainPreview {
            width: 2,
            height: 2,
            elevations_m: vec![Some(0.), Some(4.), None, Some(8.)],
        };
        let bytes = export_glb(
            GeoAnchor {
                lon: -70.,
                lat: -34.,
            },
            bounds(),
            Some(&terrain),
            &[],
            None,
        )
        .unwrap();
        let value = glb_json(&bytes);
        assert_eq!(value["accessors"][1]["count"], 3);
        assert_eq!(value["accessors"][0]["count"], 3);
    }

    #[test]
    fn concave_building_roofs_are_triangulated_and_unknown_heights_stay_flat() {
        let feature = BuildingFeature {
            id: "way/42".into(),
            height_m: None,
            ring: vec![
                [0., 0.],
                [0.002, 0.],
                [0.002, 0.001],
                [0.001, 0.001],
                [0.001, 0.002],
                [0., 0.002],
                [0., 0.],
            ],
        };
        let bytes = export_glb(
            GeoAnchor { lon: 0., lat: 0. },
            bounds(),
            None,
            &[feature],
            None,
        )
        .unwrap();
        let value = glb_json(&bytes);
        assert_eq!(value["accessors"][1]["count"], 12);
        assert_eq!(value["meshes"][0]["extras"]["height_m"], Value::Null);
        assert_eq!(value["accessors"][0]["max"][1], 0.);
    }

    #[test]
    fn rejects_color_unsigned_and_truncated_tiff_instead_of_treating_it_as_elevation() {
        let mut data = Cursor::new(Vec::new());
        {
            let mut writer = tiff::encoder::TiffEncoder::new(&mut data).unwrap();
            writer
                .write_image::<tiff::encoder::colortype::RGB8>(1, 1, &[1, 2, 3])
                .unwrap();
        }
        assert!(decode_terrain_tiff(data.get_ref(), &DecodeLimits::default()).is_err());
        assert!(decode_terrain_tiff(&data.get_ref()[..12], &DecodeLimits::default()).is_err());
        let mut unsigned = Cursor::new(Vec::new());
        {
            let mut writer = tiff::encoder::TiffEncoder::new(&mut unsigned).unwrap();
            writer
                .write_image::<tiff::encoder::colortype::Gray16>(1, 1, &[32768])
                .unwrap();
        }
        assert!(decode_terrain_tiff(unsigned.get_ref(), &DecodeLimits::default()).is_err());
    }

    #[test]
    fn overpass_feature_vertex_and_total_limits_are_enforced() {
        let source = json!({"elements":[{"type":"way","id":7,"geometry":[
            {"lon":0.,"lat":0.},{"lon":0.01,"lat":0.},{"lon":0.,"lat":0.01},{"lon":0.,"lat":0.}],
            "tags":{"building":"yes","height":"10 ft"}}]});
        let bytes = serde_json::to_vec(&source).unwrap();
        for limits in [
            DecodeLimits {
                max_features: 0,
                ..DecodeLimits::default()
            },
            DecodeLimits {
                max_vertices_per_feature: 3,
                ..DecodeLimits::default()
            },
            DecodeLimits {
                max_total_vertices: 3,
                ..DecodeLimits::default()
            },
        ] {
            assert!(parse_overpass_buildings(&bytes, &limits)
                .unwrap_err()
                .contains("limit"));
        }
        let result = parse_overpass_buildings(&bytes, &DecodeLimits::default()).unwrap();
        assert!((result[0].height_m.unwrap() - 3.048).abs() < 1e-12);
    }

    #[test]
    fn rejects_building_relations_instead_of_silently_filling_courtyards() {
        let bytes = br#"{"elements":[{"type":"relation","id":9,"tags":{"building":"yes","type":"multipolygon"},"members":[] }]}"#;
        assert!(parse_overpass_buildings(bytes, &DecodeLimits::default())
            .unwrap_err()
            .contains("unsupported"));
    }
}
