//! Bounded, in-memory Earth Web decoder. No networking, files or model input.
//!
//! Protocol/mesh algorithms ported from TheLoki2020/aleph src/mesh.py and
//! src/pbf.py, revision ad68fc4a3c60fd7af3969597ef0ba886c63b0a40.
//! https://github.com/TheLoki2020/aleph
//!
//! MIT License — Copyright (c) 2026 Belluxx
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

const MAX_INPUT: usize = 32 * 1024 * 1024;
const MAX_OUTPUT: usize = 64 * 1024 * 1024;
const MAX_FIELDS: usize = 65_536;
const MAX_NODES: usize = 4680; // Four complete octree levels.
const MAX_MESHES: usize = 64;
const MAX_VERTICES: usize = 262_144;
const MAX_STRIP: usize = 1_048_576;
const MAX_TOTAL_VERTICES: usize = 1_048_576;
const MAX_TOTAL_STRIP: usize = 2_097_152;
const MAX_JPEG: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Planet {
    pub radius: f64,
    pub epoch: u32,
}
/// Metadata bounds retain source ECEF meters, before native-axis conversion.
#[derive(Debug, Clone)]
pub struct Bounds {
    pub center: [f64; 3],
    pub half: [f64; 3],
    pub axes: [[f64; 3]; 3],
}
#[derive(Debug, Clone)]
pub struct Node {
    pub path: String,
    pub flags: u32,
    pub epoch: u32,
    pub bulk_epoch: u32,
    pub imagery_epoch: Option<u32>,
    pub bounds: Option<Bounds>,
}
#[derive(Debug)]
pub struct DecodedMesh {
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub origin: [f64; 3],
    pub jpeg: Vec<u8>,
    pub vertex_count: u32,
    pub triangle_count: u32,
}
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy)]
enum Value<'a> {
    Integer(u64),
    Bytes(&'a [u8]),
}
impl<'a> Value<'a> {
    fn bytes(self) -> Result<&'a [u8]> {
        match self {
            Self::Bytes(b) => Ok(b),
            _ => Err("expected protobuf bytes".into()),
        }
    }
    fn uint(self) -> Result<u32> {
        match self {
            Self::Integer(n) => u32::try_from(n).map_err(|_| "integer exceeds u32".into()),
            _ => Err("expected protobuf integer".into()),
        }
    }
}

struct Wire<'a> {
    data: &'a [u8],
    at: usize,
    fields: usize,
}
impl<'a> Wire<'a> {
    fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() > MAX_INPUT {
            return Err("Earth response exceeds input limit".into());
        }
        Ok(Self {
            data,
            at: 0,
            fields: 0,
        })
    }
    fn varint(&mut self) -> Result<u64> {
        let mut n = 0;
        for i in 0..10 {
            let b = *self.data.get(self.at).ok_or("truncated varint")?;
            self.at += 1;
            if i == 9 && b > 1 {
                return Err("varint overflow".into());
            }
            n |= u64::from(b & 127) << (7 * i);
            if b & 128 == 0 {
                return Ok(n);
            }
        }
        Err("varint overflow".into())
    }
    fn next(&mut self) -> Result<Option<(u32, Value<'a>)>> {
        if self.at == self.data.len() {
            return Ok(None);
        }
        self.fields += 1;
        if self.fields > MAX_FIELDS {
            return Err("too many protobuf fields".into());
        }
        let tag = self.varint()?;
        let number = tag >> 3;
        if number == 0 || number > 0x1fff_ffff {
            return Err("invalid protobuf field".into());
        }
        let value = match tag & 7 {
            0 => Value::Integer(self.varint()?),
            wire @ (1 | 2 | 5) => {
                let len = match wire {
                    1 => 8,
                    5 => 4,
                    _ => usize::try_from(self.varint()?).map_err(|_| "field length overflow")?,
                };
                let end = self.at.checked_add(len).ok_or("field length overflow")?;
                let b = self
                    .data
                    .get(self.at..end)
                    .ok_or("truncated protobuf field")?;
                self.at = end;
                Value::Bytes(b)
            }
            _ => return Err("unsupported protobuf wire type".into()),
        };
        Ok(Some((number as u32, value)))
    }
}

fn allocate<T>(count: usize) -> Result<Vec<T>> {
    let mut v = Vec::new();
    v.try_reserve_exact(count)
        .map_err(|_| "Earth allocation limit")?;
    Ok(v)
}
fn f32_value(b: &[u8]) -> Result<f64> {
    let b: [u8; 4] = b.try_into().map_err(|_| "expected fixed32")?;
    let n = f32::from_le_bytes(b) as f64;
    if !n.is_finite() {
        return Err("nonfinite float".into());
    }
    Ok(n)
}
fn f64_value(b: &[u8]) -> Result<f64> {
    let b: [u8; 8] = b.try_into().map_err(|_| "expected fixed64")?;
    let n = f64::from_le_bytes(b);
    if !n.is_finite() {
        return Err("nonfinite double".into());
    }
    Ok(n)
}
fn radius_ok(radius: f64) -> Result<()> {
    if !radius.is_finite() || !(1.0..=1e9).contains(&radius) {
        return Err("invalid planet radius".into());
    }
    Ok(())
}

pub fn planet(data: &[u8]) -> Result<Planet> {
    let mut wire = Wire::new(data)?;
    let (mut root, mut radius) = (None, None);
    while let Some((n, v)) = wire.next()? {
        match n {
            1 => root = Some(v.bytes()?),
            2 => radius = Some(f32_value(v.bytes()?)?),
            _ => {}
        }
    }
    let mut wire = Wire::new(root.ok_or("missing planet root")?)?;
    let (mut epoch, mut bulk_epoch) = (None, None);
    while let Some((n, v)) = wire.next()? {
        match n {
            2 => epoch = Some(v.uint()?),
            5 => bulk_epoch = Some(v.uint()?),
            _ => {}
        }
    }
    let radius = radius.ok_or("missing planet radius")?;
    radius_ok(radius)?;
    Ok(Planet {
        radius,
        epoch: bulk_epoch.or(epoch).ok_or("missing planet epoch")?,
    })
}

pub fn bulk(data: &[u8], head: &str, mut epoch: u32) -> Result<Vec<Node>> {
    if head.len() > 28 || !head.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err("invalid octree head".into());
    }
    let mut wire = Wire::new(data)?;
    let mut raw = allocate(8)?;
    let (mut center, mut texels, mut imagery) = (None, None, None);
    while let Some((n, v)) = wire.next()? {
        match n {
            1 => {
                if raw.len() >= MAX_NODES {
                    return Err("too many octree nodes".into());
                }
                raw.try_reserve(1).map_err(|_| "metadata allocation")?;
                raw.push(v.bytes()?);
            }
            2 => {
                let mut h = Wire::new(v.bytes()?)?;
                while let Some((n, v)) = h.next()? {
                    if n == 2 {
                        epoch = v.uint()?;
                    }
                }
            }
            3 => {
                let b = v.bytes()?;
                if b.len() != 24 {
                    return Err("invalid bulk center".into());
                }
                let c = [
                    f64_value(&b[0..8])?,
                    f64_value(&b[8..16])?,
                    f64_value(&b[16..24])?,
                ];
                if c.iter().any(|x| x.abs() > 1e9) {
                    return Err("bulk center out of range".into());
                }
                center = Some(c);
            }
            4 => {
                let b = v.bytes()?;
                if b.len() > 16 || b.len() % 4 != 0 {
                    return Err("invalid bulk texels".into());
                }
                texels = Some(b);
            }
            5 => imagery = Some(v.uint()?),
            _ => {}
        }
    }
    let mut nodes = allocate(raw.len())?;
    for raw in raw {
        let mut w = Wire::new(raw)?;
        let (mut packed, mut node_epoch, mut bulk_epoch, mut node_imagery, mut bounds, mut texel) =
            (None, epoch, epoch, imagery, None, None);
        while let Some((n, v)) = w.next()? {
            match n {
                1 => packed = Some(v.uint()?),
                2 => node_epoch = v.uint()?,
                3 => bounds = Some(v.bytes()?),
                4 => texel = Some(f32_value(v.bytes()?)?),
                5 => bulk_epoch = v.uint()?,
                7 => node_imagery = Some(v.uint()?),
                _ => {}
            }
        }
        let mut packed = packed.ok_or("node missing path")?;
        let depth = 1 + (packed & 3) as usize;
        packed >>= 2;
        let mut path = String::with_capacity(head.len() + depth);
        path.push_str(head);
        for _ in 0..depth {
            path.push(char::from(b'0' + (packed & 7) as u8));
            packed >>= 3;
        }
        let bounds = if let Some(b) = bounds {
            let texel = if let Some(t) = texel {
                t
            } else {
                let ts = texels.ok_or("missing bulk texels")?;
                f32_value(
                    ts.get((depth - 1) * 4..depth * 4)
                        .ok_or("missing depth texel")?,
                )?
            };
            if texel <= 0. || texel > 1e8 {
                return Err("invalid node texel".into());
            }
            Some(node_box(b, center.ok_or("missing bulk center")?, texel)?)
        } else {
            None
        };
        nodes.push(Node {
            path,
            flags: packed,
            epoch: node_epoch,
            bulk_epoch,
            imagery_epoch: node_imagery,
            bounds,
        });
    }
    Ok(nodes)
}

fn node_box(b: &[u8], head: [f64; 3], texel: f64) -> Result<Bounds> {
    if b.len() != 15 {
        return Err("invalid packed node bounds".into());
    }
    let mut center = head;
    let mut half = [0.; 3];
    let mut angle = [0.; 3];
    for i in 0..3 {
        center[i] += i16::from_le_bytes([b[i * 2], b[i * 2 + 1]]) as f64 * texel;
        half[i] = b[6 + i] as f64 * texel;
        angle[i] = u16::from_le_bytes([b[9 + i * 2], b[10 + i * 2]]) as f64 * std::f64::consts::PI
            / if i == 1 { 65536. } else { 32768. };
    }
    if center.iter().any(|x| !x.is_finite() || x.abs() > 1e10)
        || half.iter().any(|x| !x.is_finite() || *x > 1e10)
    {
        return Err("node bounds out of range".into());
    }
    let (s0, c0) = angle[0].sin_cos();
    let (s1, c1) = angle[1].sin_cos();
    let (s2, c2) = angle[2].sin_cos();
    Ok(Bounds {
        center,
        half,
        axes: [
            [c0 * c2 - c1 * s0 * s2, c1 * c0 * s2 + c2 * s0, s2 * s1],
            [-c0 * s2 - c2 * c1 * s0, c0 * c1 * c2 - s0 * s2, c2 * s1],
            [s1 * s0, -c0 * s1, c1],
        ],
    })
}

fn counted(data: &[u8], limit: usize) -> Result<Vec<u64>> {
    let mut w = Wire::new(data)?;
    let count = usize::try_from(w.varint()?).map_err(|_| "packed count overflow")?;
    if count > limit || count > data.len().saturating_sub(w.at) {
        return Err("packed count exceeds bounds".into());
    }
    let mut values = allocate(count)?;
    for _ in 0..count {
        values.push(w.varint()?);
    }
    if w.at != data.len() {
        return Err("packed count mismatch".into());
    }
    Ok(values)
}

#[derive(Default)]
struct Budget {
    vertices: usize,
    strip: usize,
    output: usize,
}

/// Decode native local xyz + normal xyz + uv (eight LE f32) and LE u32 indices.
/// `mask` excludes parent octants only after their replacement children are ready.
/// JPEGs are original, transient payloads; callers must also bound image dimensions
/// before decompression and must never persist or send these buffers to a model.
pub fn decode(data: &[u8], mask: u8, radius: f64) -> Result<Vec<DecodedMesh>> {
    radius_ok(radius)?;
    let mut wire = Wire::new(data)?;
    let mut matrix = None;
    let mut raws = allocate(8)?;
    while let Some((n, v)) = wire.next()? {
        match n {
            1 => {
                let b = v.bytes()?;
                if b.len() != 128 {
                    return Err("invalid node matrix".into());
                }
                let mut m = [0.; 16];
                for (i, c) in b.chunks_exact(8).enumerate() {
                    m[i] = f64_value(c)?;
                    if m[i].abs() > radius * 4. {
                        return Err("node matrix out of bounds".into());
                    }
                }
                matrix = Some(m);
            }
            2 => {
                if raws.len() >= MAX_MESHES {
                    return Err("too many node meshes".into());
                }
                raws.try_reserve(1).map_err(|_| "mesh allocation")?;
                raws.push(v.bytes()?);
            }
            _ => {}
        }
    }
    let m = matrix.ok_or("missing node matrix")?;
    if m[3].abs() > 1e-12 || m[7].abs() > 1e-12 || m[11].abs() > 1e-12 || (m[15] - 1.).abs() > 1e-12
    {
        return Err("non-affine node matrix".into());
    }
    let scale = 20. / radius;
    let origin = [m[13] * scale, m[14] * scale, m[12] * scale];
    let mut output = allocate(raws.len())?;
    let mut budget = Budget::default();
    for raw in raws {
        if let Some(mesh) = decode_mesh(raw, &m, origin, scale, mask, &mut budget)? {
            output.push(mesh);
        }
    }
    Ok(output)
}

fn decode_mesh(
    raw: &[u8],
    m: &[f64; 16],
    origin: [f64; 3],
    scale: f64,
    mask: u8,
    budget: &mut Budget,
) -> Result<Option<DecodedMesh>> {
    // Mesh fields follow Aleph's first-value semantics; offsets may be repeated.
    let mut w = Wire::new(raw)?;
    let mut fields = [None; 14];
    let mut offsets = [0u8; 16];
    let mut offset_len = 0;
    while let Some((n, v)) = w.next()? {
        if n == 10 {
            let b = v.bytes()?;
            let end = offset_len + b.len();
            if end > 16 {
                return Err("too many UV offsets".into());
            }
            offsets[offset_len..end].copy_from_slice(b);
            offset_len = end;
        } else if (n as usize) < fields.len() && fields[n as usize].is_none() {
            fields[n as usize] = Some(v);
        }
    }
    let get = |n: usize| -> Result<&[u8]> {
        fields[n]
            .ok_or_else(|| format!("missing mesh field {n}"))?
            .bytes()
    };
    let vertices = get(1)?;
    let uv = get(7)?;
    let count = vertices.len() / 3;
    if count == 0
        || count > MAX_VERTICES
        || vertices.len() != count * 3
        || uv.len() != 4 + 4 * count
    {
        return Err("invalid mesh vertex planes".into());
    }
    if count > MAX_TOTAL_VERTICES - budget.vertices {
        return Err("node vertex count exceeds limit".into());
    }
    budget.vertices += count;
    let mut texture = Wire::new(get(6)?)?;
    let (mut jpeg, mut format) = (None, 1);
    while let Some((n, v)) = texture.next()? {
        match n {
            1 if jpeg.is_none() => jpeg = Some(v.bytes()?),
            2 => format = v.uint()?,
            _ => {}
        }
    }
    let jpeg = jpeg.ok_or("missing JPEG texture")?;
    if format != 1
        || jpeg.len() < 4
        || jpeg.len() > MAX_JPEG
        || !jpeg.starts_with(&[255, 216])
        || !jpeg.ends_with(&[255, 217])
    {
        return Err("invalid JPEG texture".into());
    }
    let um = 1 + u16::from_le_bytes([uv[0], uv[1]]) as u64;
    let vm = 1 + u16::from_le_bytes([uv[2], uv[3]]) as u64;
    let params = if offset_len == 0 {
        [0.5, 0.5, 1. / um as f64, 1. / vm as f64]
    } else if offset_len == 16 {
        [
            f32_value(&offsets[0..4])?,
            f32_value(&offsets[4..8])?,
            f32_value(&offsets[8..12])?,
            f32_value(&offsets[12..16])?,
        ]
    } else {
        return Err("invalid UV offsets".into());
    };
    let mut positions = allocate(count)?;
    let mut texcoords = allocate(count)?;
    let (mut xyz, mut u, mut v) = ([0u8; 3], 0u64, 0u64);
    for i in 0..count {
        for k in 0..3 {
            xyz[k] = xyz[k].wrapping_add(vertices[k * count + i]);
        }
        let mut ecef = [0.; 3];
        for k in 0..3 {
            ecef[k] = (m[k] * xyz[0] as f64 + m[4 + k] * xyz[1] as f64 + m[8 + k] * xyz[2] as f64)
                * scale;
        }
        let p = [ecef[1], ecef[2], ecef[0]];
        if p.iter().any(|x| !x.is_finite() || x.abs() > 80.) {
            return Err("local vertex out of bounds".into());
        }
        positions.push(p);
        u = (u + uv[4 + i] as u64 + ((uv[4 + 2 * count + i] as u64) << 8)) % um;
        v = (v + uv[4 + count + i] as u64 + ((uv[4 + 3 * count + i] as u64) << 8)) % vm;
        let t = [
            (u as f64 + params[0]) * params[2],
            (v as f64 + params[1]) * params[3],
        ];
        if t.iter().any(|x| !x.is_finite() || x.abs() > 16.) {
            return Err("texture coordinate out of bounds".into());
        }
        texcoords.push(t);
    }
    let codes = counted(get(3)?, MAX_STRIP.min(MAX_TOTAL_STRIP - budget.strip))?;
    budget.strip += codes.len();
    let mut strip = allocate(codes.len())?;
    let mut high = 0u64;
    for code in codes {
        let index = high.checked_sub(code).ok_or("negative high-water index")?;
        if code == 0 {
            high += 1;
        }
        if index >= count as u64 {
            return Err("mesh index exceeds vertex count".into());
        }
        strip.push(index as u32);
    }
    let runs = counted(get(8)?, 256)?;
    let mut keep = allocate(strip.len())?;
    keep.resize(strip.len(), false);
    let mut end: usize = 0;
    for (layer, run) in runs.into_iter().enumerate() {
        let run = usize::try_from(run).map_err(|_| "octant run overflow")?;
        let next = end.checked_add(run).ok_or("octant run overflow")?;
        if next > strip.len() {
            return Err("octant runs exceed strip".into());
        }
        keep[end..next].fill(layer < 24 && (mask & (1 << (layer & 7))) == 0);
        end = next;
    }
    let skirts = if let Some(v) = fields[13] {
        v.bytes()?
    } else {
        &[]
    };
    if skirts.len() > strip.len().div_ceil(8) {
        return Err("skirt bits exceed strip".into());
    }
    let mut triangles = allocate(strip.len().saturating_sub(2))?;
    let mut normals = allocate(count)?;
    normals.resize(count, [0f64; 3]);
    for (i, win) in strip.windows(3).enumerate() {
        if !keep[i + 2] || skirts.get(i / 8).is_some_and(|b| b & (1 << (i & 7)) != 0) {
            continue;
        }
        let [a, b, c] = if i & 1 == 0 {
            [win[0], win[1], win[2]]
        } else {
            [win[0], win[2], win[1]]
        };
        if a == b || b == c || c == a {
            continue;
        }
        let (pa, pb, pc) = (
            positions[a as usize],
            positions[b as usize],
            positions[c as usize],
        );
        let ab = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let ac = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        if cross.iter().all(|x| *x == 0.) {
            continue;
        }
        for j in [a, b, c] {
            for (k, n) in cross.iter().enumerate() {
                normals[j as usize][k] += n;
            }
        }
        triangles.push([a, b, c]);
    }
    if triangles.is_empty() {
        return Ok(None);
    }
    // Compact in first-use order, so the output size tracks drawn vertices.
    let mut remap = allocate(count)?;
    remap.resize(count, u32::MAX);
    let mut used = allocate(count)?;
    for triangle in &triangles {
        for &index in triangle {
            let mapped = &mut remap[index as usize];
            if *mapped == u32::MAX {
                *mapped = used.len() as u32;
                used.push(index as usize);
            }
        }
    }
    let output_size = used.len() * 32 + triangles.len() * 12 + jpeg.len();
    if output_size > MAX_OUTPUT - budget.output {
        return Err("decoded node exceeds output limit".into());
    }
    budget.output += output_size;
    let mut indices = allocate(triangles.len() * 12)?;
    for triangle in &triangles {
        for &index in triangle {
            indices.extend_from_slice(&remap[index as usize].to_le_bytes());
        }
    }
    let mut bytes = allocate(used.len() * 32)?;
    for &i in &used {
        let n = normals[i];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let normal = if len > 0. {
            n.map(|x| x / len)
        } else {
            let len = origin.iter().map(|x| x * x).sum::<f64>().sqrt();
            if len == 0. {
                [0., 1., 0.]
            } else {
                origin.map(|x| x / len)
            }
        };
        for x in positions[i].into_iter().chain(normal).chain(texcoords[i]) {
            let x = x as f32;
            if !x.is_finite() {
                return Err("nonfinite output vertex".into());
            }
            bytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    let mut texture = allocate(jpeg.len())?;
    texture.extend_from_slice(jpeg);
    Ok(Some(DecodedMesh {
        vertices: bytes,
        indices,
        origin,
        jpeg: texture,
        vertex_count: used.len() as u32,
        triangle_count: triangles.len() as u32,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vi(mut v: u64) -> Vec<u8> {
        let mut r = vec![];
        while v >= 128 {
            r.push((v as u8 & 127) | 128);
            v >>= 7;
        }
        r.push(v as u8);
        r
    }
    fn bytes(n: u32, b: &[u8]) -> Vec<u8> {
        let mut r = vi(((n << 3) | 2) as u64);
        r.extend(vi(b.len() as u64));
        r.extend(b);
        r
    }
    fn uint(n: u32, v: u64) -> Vec<u8> {
        let mut r = vi((n << 3) as u64);
        r.extend(vi(v));
        r
    }
    fn packed(v: &[u64]) -> Vec<u8> {
        let mut r = vi(v.len() as u64);
        for v in v {
            r.extend(vi(*v));
        }
        r
    }
    fn floats(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }
    fn doubles(v: &[f64]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }
    fn matrix() -> [f64; 16] {
        [
            2., 0., 0., 0., 0., 3., 0., 0., 0., 0., 4., 0., 6_000_000., 10., 20., 1.,
        ]
    }
    fn mesh(runs: &[u64], skirts: &[u8]) -> Vec<u8> {
        // Source absolute coordinates (0,0,0), (1,0,0), (0,1,1), (1,1,1).
        let mut m = bytes(1, &[0, 1, 255, 1, 0, 0, 1, 0, 0, 0, 1, 0]);
        m.extend(bytes(3, &packed(&[0, 0, 0, 0])));
        let mut texture = bytes(1, &[255, 216, 255, 217]);
        texture.extend(uint(2, 1));
        m.extend(bytes(6, &texture));
        m.extend(bytes(
            7,
            &[3, 0, 3, 0, 0, 1, 255, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ));
        m.extend(bytes(8, &packed(runs)));
        m.extend(bytes(13, skirts));
        m
    }
    fn node(m: &[u8]) -> Vec<u8> {
        let mut n = bytes(1, &doubles(&matrix()));
        n.extend(bytes(2, m));
        n
    }
    fn change(data: &[u8], field: u32, replacement: &[u8]) -> Vec<u8> {
        let mut w = Wire::new(data).unwrap();
        let mut out = vec![];
        while let Some((n, v)) = w.next().unwrap() {
            if n != field {
                out.extend(match v {
                    Value::Integer(v) => uint(n, v),
                    Value::Bytes(b) => bytes(n, b),
                });
            }
        }
        out.extend(bytes(field, replacement));
        out
    }
    fn f(v: &[u8], i: usize) -> f32 {
        f32::from_le_bytes(v[i * 4..i * 4 + 4].try_into().unwrap())
    }
    fn indices(m: &DecodedMesh) -> Vec<u32> {
        m.indices
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .collect()
    }
    #[test]
    fn planet_root_bulk_epoch() {
        let mut root = uint(2, 7);
        root.extend(uint(5, 9));
        let mut p = bytes(1, &root);
        p.extend([21]);
        p.extend(6_371_000f32.to_le_bytes());
        let p = planet(&p).unwrap();
        assert_eq!(p.epoch, 9);
        assert_eq!(p.radius, 6_371_000.);
    }
    #[test]
    fn bulk_paths_epochs_bounds() {
        // Depth2, octants3 then5, flags16.
        let mut n = uint(1, 1 | (3 << 2) | (5 << 5) | (16 << 8));
        n.extend(bytes(3, &[1, 0, 254, 255, 3, 0, 4, 5, 6, 0, 0, 0, 0, 0, 0]));
        n.extend(uint(2, 11));
        n.extend(uint(5, 12));
        let mut b = bytes(1, &n);
        b.extend(bytes(3, &doubles(&[10., 20., 30.])));
        b.extend(bytes(4, &floats(&[1., 2.])));
        b.extend(uint(5, 13));
        let ns = bulk(&b, "07", 8).unwrap();
        let n = &ns[0];
        assert_eq!(n.path, "0735");
        assert_eq!(
            (n.flags, n.epoch, n.bulk_epoch, n.imagery_epoch),
            (16, 11, 12, Some(13))
        );
        let b = n.bounds.as_ref().unwrap();
        assert_eq!(b.center, [12., 16., 36.]);
        assert_eq!(b.half, [8., 10., 12.]);
        assert_eq!(b.axes, [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]);
    }
    #[test]
    fn transform_uv_strip_winding_and_height() {
        let m = decode(&node(&mesh(&[4], &[])), 0, 6_000_000.)
            .unwrap()
            .remove(0);
        let s = 20. / 6_000_000.;
        assert_eq!(m.origin, [10. * s, 20. * s, 20.]);
        assert_eq!((m.vertex_count, m.triangle_count), (4, 2));
        assert_eq!(indices(&m), [0, 1, 2, 1, 3, 2]);
        assert!((f(&m.vertices, 8 + 2) as f64 - 2. * s).abs() < 1e-10);
        assert!((f(&m.vertices, 16) as f64 - 3. * s).abs() < 1e-10);
        assert!((f(&m.vertices, 17) as f64 - 4. * s).abs() < 1e-10);
        assert_eq!(f(&m.vertices, 6), 0.125);
        assert_eq!(f(&m.vertices, 14), 0.375);
        assert_eq!(f(&m.vertices, 22), 0.125);
        assert_eq!(f(&m.vertices, 23), 0.375);
        for v in m.vertices.chunks_exact(32) {
            let l = (f(v, 3).powi(2) + f(v, 4).powi(2) + f(v, 5).powi(2)).sqrt();
            assert!((l - 1.).abs() < 1e-5);
        }
    }
    #[test]
    fn masks_skirts_and_extra_layers() {
        assert!(decode(&node(&mesh(&[4], &[])), 1, 6_000_000.)
            .unwrap()
            .is_empty());
        let m = decode(&node(&mesh(&[4], &[1])), 0, 6_000_000.)
            .unwrap()
            .remove(0);
        assert_eq!(m.triangle_count, 1);
        let mut runs = vec![0; 24];
        runs.push(4);
        assert!(decode(&node(&mesh(&runs, &[])), 0, 6_000_000.)
            .unwrap()
            .is_empty());
        let m = decode(&node(&mesh(&[3, 1], &[])), 1, 6_000_000.)
            .unwrap()
            .remove(0);
        assert_eq!(m.triangle_count, 1);
    }
    #[test]
    fn explicit_uv_offsets_packed_and_unpacked() {
        let mut a = mesh(&[4], &[]);
        a.extend(bytes(10, &floats(&[0., 0., 0.5, 0.25])));
        let m = decode(&node(&a), 0, 6_000_000.).unwrap().remove(0);
        assert_eq!(f(&m.vertices, 14), 0.5);
        let mut b = mesh(&[4], &[]);
        for v in [0f32, 0., 0.5, 0.25] {
            b.extend(vi((10 << 3) | 5));
            b.extend(v.to_le_bytes());
        }
        assert_eq!(
            decode(&node(&b), 0, 6_000_000.).unwrap()[0].vertices,
            m.vertices
        );
    }
    #[test]
    fn malformed_and_unbounded_inputs_fail() {
        for bad in [
            &[0u8][..],
            &[10, 255][..],
            &[8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 2][..],
            &[11][..],
        ] {
            assert!(planet(bad).is_err());
            assert!(bulk(bad, "", 0).is_err());
            assert!(decode(bad, 0, 6_000_000.).is_err());
        }
        assert!(bulk(&[], "08", 0).is_err());
        assert!(decode(&node(&mesh(&[4], &[])), 0, f64::NAN).is_err());
        let mut m = mesh(&[4], &[]);
        m.extend(bytes(10, &floats(&[0., 0., f32::NAN, 1.])));
        assert!(decode(&node(&m), 0, 6_000_000.).is_err());
        let mut m = mesh(&[4], &[]);
        m.extend(bytes(10, &[0]));
        assert!(decode(&node(&m), 0, 6_000_000.).is_err());
        let mut n = bytes(1, &doubles(&[f64::NAN; 16]));
        n.extend(bytes(2, &mesh(&[4], &[])));
        assert!(decode(&n, 0, 6_000_000.).is_err());
        let mut m = mesh(&[4], &[]); // Replace index field by building a separate malformed mesh.
        m.splice(14..21, bytes(3, &packed(&[1, 0, 0, 0])));
        assert!(decode(&node(&m), 0, 6_000_000.).is_err());
        assert!(decode(&node(&mesh(&[u64::MAX], &[])), 0, 6_000_000.).is_err());
    }

    #[test]
    fn counts_indices_jpeg_and_preallocation_budgets_are_bounded() {
        let original = mesh(&[4], &[]);
        for bad in [
            packed(&[4, 0, 0, 0]),
            packed(&[0, 0, 0, 0, 0]),
            vec![5, 0, 0, 0, 0],
            vec![0, 0],
            vi(u64::MAX),
        ] {
            assert!(decode(&node(&change(&original, 3, &bad)), 0, 6_000_000.).is_err());
        }
        let mut invalid_texture = bytes(1, &[255, 216, 255, 217]);
        invalid_texture.extend(uint(2, 2));
        assert!(decode(
            &node(&change(&original, 6, &invalid_texture)),
            0,
            6_000_000.
        )
        .is_err());
        assert!(decode(
            &node(&change(&original, 6, &bytes(1, &[0; 4]))),
            0,
            6_000_000.
        )
        .is_err());
        assert!(decode(&node(&change(&original, 7, &[0; 19])), 0, 6_000_000.).is_err());
        assert!(decode(
            &node(&change(&original, 1, &vec![0; (MAX_VERTICES + 1) * 3])),
            0,
            6_000_000.
        )
        .is_err());
        let mut excessive = node(&original);
        for _ in 0..MAX_MESHES {
            excessive.extend(bytes(2, &original));
        }
        assert!(decode(&excessive, 0, 6_000_000.).is_err());
        let mut budget = Budget {
            output: MAX_OUTPUT - 155,
            ..Default::default()
        };
        assert!(decode_mesh(
            &original,
            &matrix(),
            [0.; 3],
            20. / 6_000_000.,
            0,
            &mut budget
        )
        .is_err());
        let mut budget = Budget {
            vertices: MAX_TOTAL_VERTICES - 3,
            ..Default::default()
        };
        assert!(decode_mesh(
            &original,
            &matrix(),
            [0.; 3],
            20. / 6_000_000.,
            0,
            &mut budget
        )
        .is_err());
        let mut budget = Budget {
            strip: MAX_TOTAL_STRIP - 3,
            ..Default::default()
        };
        assert!(decode_mesh(
            &original,
            &matrix(),
            [0.; 3],
            20. / 6_000_000.,
            0,
            &mut budget
        )
        .is_err());
        let too_large = vec![0; MAX_INPUT + 1];
        assert!(planet(&too_large).is_err());
        assert!(bulk(&too_large, "", 0).is_err());
        assert!(decode(&too_large, 0, 6_000_000.).is_err());
    }

    #[test]
    fn uv_high_planes_wrap_and_high_water_backreferences() {
        let mut uv = vec![0, 1, 0, 1];
        uv.extend([0, 1, 255, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0]);
        let m = decode(&node(&change(&mesh(&[4], &[]), 7, &uv)), 0, 6_000_000.)
            .unwrap()
            .remove(0);
        for (i, u) in [256., 0., 254., 255.].iter().enumerate() {
            assert!((f(&m.vertices, i * 8 + 6) as f64 - (u + 0.5) / 257.).abs() < 1e-7);
        }
        // The fifth strip entry reuses vertex2 (high4 minus2), yielding a degenerate tail.
        let m = change(&mesh(&[5], &[]), 3, &packed(&[0, 0, 0, 0, 2]));
        let m = decode(&node(&m), 0, 6_000_000.).unwrap().remove(0);
        assert_eq!(m.triangle_count, 2);
    }

    #[test]
    fn malformed_bounds_and_transforms_fail() {
        let mut b = bytes(1, &uint(1, 0));
        b.extend(bytes(4, &floats(&[f32::NAN])));
        let n = change(&uint(1, 0), 3, &[0; 15]);
        b = change(&b, 1, &n);
        b.extend(bytes(3, &doubles(&[0.; 3])));
        assert!(bulk(&b, "", 1).is_err());
        assert!(node_box(&[0; 14], [0.; 3], 1.).is_err());
        let mut m = matrix();
        m[3] = 0.01;
        let n = change(&node(&mesh(&[4], &[])), 1, &doubles(&m));
        assert!(decode(&n, 0, 6_000_000.).is_err());
        let mut m = matrix();
        m[0] = f64::MAX;
        let n = change(&node(&mesh(&[4], &[])), 1, &doubles(&m));
        assert!(decode(&n, 0, 6_000_000.).is_err());
    }

    #[test]
    fn bounded_arbitrary_wire_data_never_panics() {
        // Deterministic synthetic bytes only; no downloaded payload fixtures.
        let mut seed = 0x27d4eb2du64;
        for len in 0..256 {
            let mut b = vec![0; len];
            for x in &mut b {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *x = seed as u8;
            }
            let _ = planet(&b);
            let _ = bulk(&b, "012", 1);
            let _ = decode(&b, 0, 6_000_000.);
        }
    }
}
