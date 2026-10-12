//! Shapes: the vertices and triangles of a mesh.
//!
//! Skyrim Special Edition and Fallout 4 keep a shape's vertices in the shape
//! block itself (`BSTriShape` and the kinds built on it), packed to a layout
//! the block describes. The vertex bytes are kept as they are and read and
//! changed in place through that layout, so a value this does not touch is
//! never rewritten.

use crate::io::{Reader, Writer};
use crate::node::{AvObject, Ref};
use crate::NifError;

/// What a packed vertex carries, as the bits of a shape's vertex description.
pub mod attr {
    pub const POSITION: u16 = 0x001;
    pub const UV: u16 = 0x002;
    pub const UV2: u16 = 0x004;
    pub const NORMAL: u16 = 0x008;
    pub const TANGENT: u16 = 0x010;
    pub const COLOR: u16 = 0x020;
    pub const SKINNED: u16 = 0x040;
    pub const LAND: u16 = 0x080;
    pub const EYE: u16 = 0x100;
    pub const INSTANCE: u16 = 0x200;
    pub const FULL_PRECISION: u16 = 0x400;
}

/// A sixteen-bit float as the value it stands for.
pub fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (h >> 10) & 0x1f;
    let frac = (h & 0x3ff) as f32;
    let value = match exp {
        0 => frac * (1.0 / 1024.0) * (1.0 / 16384.0),
        31 if frac == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + frac / 1024.0) * 2f32.powi(exp as i32 - 15),
    };
    sign * value
}

/// A value as the nearest sixteen-bit float.
pub fn f32_to_half(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let frac = bits & 0x007f_ffff;
    if v.is_nan() {
        return sign | 0x7e00;
    }
    if exp >= 31 {
        return sign | 0x7c00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        // Too small for a normal half: kept as a denormal, rounded.
        let m = (frac | 0x0080_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    // Rounded to nearest; a carry out of the fraction raises the exponent,
    // which the addition does by itself.
    let half = ((exp as u32) << 10) | (frac >> 13);
    sign | (half + ((frac >> 12) & 1)) as u16
}

/// A direction packed into a byte a part, as the direction.
fn unpack_dir(b: u8) -> f32 {
    b as f32 / 255.0 * 2.0 - 1.0
}

fn pack_dir(v: f32) -> u8 {
    ((v.clamp(-1.0, 1.0) + 1.0) / 2.0 * 255.0).round() as u8
}

/// Where each thing a vertex carries sits within its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub stride: usize,
    pub attributes: u16,
    /// Positions (and the first bitangent part) are thirty-two-bit floats.
    pub full_precision: bool,
    position: Option<usize>,
    uv: Option<usize>,
    normal: Option<usize>,
    tangent: Option<usize>,
    color: Option<usize>,
    skin: Option<usize>,
}

impl Layout {
    /// The layout a vertex description stands for. `None` when what it says
    /// the vertex carries does not add up to the size it states, which is a
    /// layout this does not know and must not guess at.
    pub fn of(desc: u64, stream: u32) -> Option<Layout> {
        let stride = (desc & 0xf) as usize * 4;
        let attributes = ((desc >> 44) & 0xfff) as u16;
        let has = |a: u16| attributes & a != 0;
        // Special Edition always stores full floats; Fallout 4 says when.
        let full_precision = stream < 130 || has(attr::FULL_PRECISION);
        let mut at = 0usize;
        let mut next = |size: usize, present: bool| {
            present.then(|| {
                let here = at;
                at += size;
                here
            })
        };
        let position = next(if full_precision { 16 } else { 8 }, has(attr::POSITION));
        let uv = next(4, has(attr::UV));
        let _uv2 = next(4, has(attr::UV2));
        let normal = next(4, has(attr::NORMAL));
        let tangent = next(4, has(attr::NORMAL) && has(attr::TANGENT));
        let color = next(4, has(attr::COLOR));
        let skin = next(12, has(attr::SKINNED));
        let _eye = next(4, has(attr::EYE));
        (at == stride && !has(attr::LAND) && !has(attr::INSTANCE)).then_some(Layout {
            stride,
            attributes,
            full_precision,
            position,
            uv,
            normal,
            tangent,
            color,
            skin,
        })
    }

    pub fn has(&self, a: u16) -> bool {
        self.attributes & a != 0
    }
}

/// A shape with its vertices in the block: `BSTriShape` and the kinds built
/// on it.
#[derive(Debug, Clone, PartialEq)]
pub struct TriShape {
    pub av: AvObject,
    pub bound_center: [f32; 3],
    pub bound_radius: f32,
    /// Fallout 76 only: the box around the shape.
    pub bound_box: Option<[f32; 6]>,
    pub skin: Ref,
    pub shader: Ref,
    pub alpha: Ref,
    pub vertex_desc: u64,
    pub vertex_count: u16,
    /// `vertex_count` vertices of the layout's stride each. Empty for a
    /// shape whose vertices are elsewhere (a skinned Special Edition shape
    /// keeps them in its skin partition).
    pub vertex_data: Vec<u8>,
    pub triangles: Vec<[u16; 3]>,
    /// Special Edition's particle data, kept as it was.
    pub particle: Option<Vec<u8>>,
    /// What a kind built on this adds after it, kept as it was.
    pub rest: Vec<u8>,
    /// `data_size` as the file stated it when that was not what the counts
    /// give, so a file that states it wrongly is written back as it was.
    stated_size: Option<u32>,
    /// The triangle count as stated, when there was no data to hold them.
    stated_triangles: u32,
}

/// The kinds of block that start as a `BSTriShape` does.
pub fn is_tri_shape(kind: &str) -> bool {
    matches!(
        kind,
        "BSTriShape" | "BSDynamicTriShape" | "BSSubIndexTriShape" | "BSMeshLODTriShape"
    )
}

impl TriShape {
    pub fn read(data: &[u8], stream: u32) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let av = AvObject::read(&mut r, stream)?;
        let mut bound_center = [0.0; 3];
        for v in &mut bound_center {
            *v = r.f32()?;
        }
        let bound_radius = r.f32()?;
        let bound_box = if stream == 155 {
            let mut b = [0.0; 6];
            for v in &mut b {
                *v = r.f32()?;
            }
            Some(b)
        } else {
            None
        };
        let skin = r.i32()?;
        let shader = r.i32()?;
        let alpha = r.i32()?;
        let vertex_desc = r.u64()?;
        let stated_triangles = if stream < 130 {
            r.u16()? as u32
        } else {
            r.u32()?
        };
        let vertex_count = r.u16()?;
        let data_size = r.u32()?;
        let stride = (vertex_desc & 0xf) as usize * 4;
        let (vertex_data, triangles) = if data_size > 0 {
            let vertex_data = r.take(vertex_count as usize * stride)?.to_vec();
            let mut triangles = Vec::with_capacity(stated_triangles as usize);
            if (stated_triangles as usize)
                .checked_mul(6)
                .is_none_or(|n| n > r.left())
            {
                return Err(NifError::Truncated);
            }
            for _ in 0..stated_triangles {
                triangles.push([r.u16()?, r.u16()?, r.u16()?]);
            }
            (vertex_data, triangles)
        } else {
            (Vec::new(), Vec::new())
        };
        let expected = (vertex_data.len() + triangles.len() * 6) as u32;
        let particle = if stream == 100 {
            let size = r.u32()?;
            let mut bytes = size.to_le_bytes().to_vec();
            if size > 0 {
                // Positions and normals a vertex, then the triangles again.
                let n = vertex_count as usize * 12 + stated_triangles as usize * 6;
                bytes.extend_from_slice(r.take(n)?);
            }
            Some(bytes)
        } else {
            None
        };
        let rest = r.take(r.left())?.to_vec();
        Ok(TriShape {
            av,
            bound_center,
            bound_radius,
            bound_box,
            skin,
            shader,
            alpha,
            vertex_desc,
            vertex_count,
            vertex_data,
            triangles,
            particle,
            rest,
            stated_size: (data_size != expected).then_some(data_size),
            stated_triangles,
        })
    }

    pub fn write(&self, stream: u32) -> Vec<u8> {
        let mut w = Writer::default();
        self.av.write(&mut w, stream);
        for v in &self.bound_center {
            w.f32(*v);
        }
        w.f32(self.bound_radius);
        if let Some(b) = &self.bound_box {
            for v in b {
                w.f32(*v);
            }
        }
        w.i32(self.skin);
        w.i32(self.shader);
        w.i32(self.alpha);
        w.u64(self.vertex_desc);
        let has_data = !self.vertex_data.is_empty() || !self.triangles.is_empty();
        let triangle_count = if has_data {
            self.triangles.len() as u32
        } else {
            self.stated_triangles
        };
        if stream < 130 {
            w.u16(triangle_count as u16);
        } else {
            w.u32(triangle_count);
        }
        w.u16(self.vertex_count);
        let size = (self.vertex_data.len() + self.triangles.len() * 6) as u32;
        w.u32(self.stated_size.unwrap_or(size));
        w.bytes(&self.vertex_data);
        for t in &self.triangles {
            for i in t {
                w.u16(*i);
            }
        }
        if let Some(p) = &self.particle {
            w.bytes(p);
        }
        w.bytes(&self.rest);
        w.out
    }

    /// Keep only some vertices: `now[v]` is where vertex `v` is now, or
    /// `None` when it is removed. A triangle goes when any of its vertices
    /// does. For a shape whose vertices are elsewhere this changes nothing
    /// here; its skin partition holds them.
    pub fn retain(&mut self, now: &[Option<u16>], dynamic: bool) {
        let stride = (self.vertex_desc & 0xf) as usize * 4;
        let kept = now.iter().filter(|n| n.is_some()).count();
        if dynamic {
            // A face part's positions follow the shape, 16 bytes a vertex.
            if let Some(size) = self.rest.get(..4) {
                let size = u32::from_le_bytes([size[0], size[1], size[2], size[3]]) as usize;
                if size == now.len() * 16 && self.rest.len() >= 4 + size {
                    let rows: Vec<u8> = self.rest[4..4 + size]
                        .chunks(16)
                        .zip(now)
                        .filter(|(_, n)| n.is_some())
                        .flat_map(|(row, _)| row.iter().copied())
                        .collect();
                    let mut rest = (rows.len() as u32).to_le_bytes().to_vec();
                    rest.extend(rows);
                    rest.extend_from_slice(&self.rest[4 + size..]);
                    self.rest = rest;
                }
            }
        }
        if stride == 0 || self.vertex_data.len() != now.len() * stride {
            return;
        }
        self.vertex_data = self
            .vertex_data
            .chunks(stride)
            .zip(now)
            .filter(|(_, n)| n.is_some())
            .flat_map(|(row, _)| row.iter().copied())
            .collect();
        let at = |v: u16| now.get(v as usize).copied().flatten();
        self.triangles = self
            .triangles
            .iter()
            .filter_map(|t| Some([at(t[0])?, at(t[1])?, at(t[2])?]))
            .collect();
        self.vertex_count = kept as u16;
        self.stated_size = None;
        self.stated_triangles = self.triangles.len() as u32;
        // What the editor keeps for particles is positions and triangles
        // again; it is dropped, not kept wrong.
        if let Some(p) = &mut self.particle {
            *p = 0u32.to_le_bytes().to_vec();
        }
    }

    pub fn layout(&self, stream: u32) -> Option<Layout> {
        Layout::of(self.vertex_desc, stream)
    }

    /// Whether the vertices are here to be read (not in a skin partition).
    pub fn has_vertices(&self) -> bool {
        !self.vertex_data.is_empty()
    }

    fn at(
        &self,
        layout: &Layout,
        vertex: usize,
        offset: Option<usize>,
        size: usize,
    ) -> Option<usize> {
        let at = vertex.checked_mul(layout.stride)?.checked_add(offset?)?;
        (at + size <= self.vertex_data.len()).then_some(at)
    }

    pub fn position(&self, layout: &Layout, vertex: usize) -> Option<[f32; 3]> {
        position_in(&self.vertex_data, layout, vertex)
    }

    pub fn set_position(&mut self, layout: &Layout, vertex: usize, p: [f32; 3]) {
        set_position_in(&mut self.vertex_data, layout, vertex, p);
    }

    pub fn uv(&self, layout: &Layout, vertex: usize) -> Option<[f32; 2]> {
        let at = self.at(layout, vertex, layout.uv, 4)?;
        let d = &self.vertex_data;
        Some([
            half_to_f32(u16::from_le_bytes([d[at], d[at + 1]])),
            half_to_f32(u16::from_le_bytes([d[at + 2], d[at + 3]])),
        ])
    }

    pub fn set_uv(&mut self, layout: &Layout, vertex: usize, uv: [f32; 2]) {
        if let Some(at) = self.at(layout, vertex, layout.uv, 4) {
            self.vertex_data[at..at + 2].copy_from_slice(&f32_to_half(uv[0]).to_le_bytes());
            self.vertex_data[at + 2..at + 4].copy_from_slice(&f32_to_half(uv[1]).to_le_bytes());
        }
    }

    pub fn normal(&self, layout: &Layout, vertex: usize) -> Option<[f32; 3]> {
        let at = self.at(layout, vertex, layout.normal, 4)?;
        let d = &self.vertex_data;
        Some([
            unpack_dir(d[at]),
            unpack_dir(d[at + 1]),
            unpack_dir(d[at + 2]),
        ])
    }

    pub fn set_normal(&mut self, layout: &Layout, vertex: usize, n: [f32; 3]) {
        if let Some(at) = self.at(layout, vertex, layout.normal, 4) {
            for (i, v) in n.iter().enumerate() {
                self.vertex_data[at + i] = pack_dir(*v);
            }
        }
    }

    /// The tangent and bitangent a vertex stores. The bitangent's three
    /// parts are kept beside the position, the normal and the tangent.
    pub fn tangent_space(&self, layout: &Layout, vertex: usize) -> Option<([f32; 3], [f32; 3])> {
        let t = self.at(layout, vertex, layout.tangent, 4)?;
        let n = self.at(layout, vertex, layout.normal, 4)?;
        let p = self.at(
            layout,
            vertex,
            layout.position,
            if layout.full_precision { 16 } else { 8 },
        )?;
        let d = &self.vertex_data;
        let bx = if layout.full_precision {
            f32::from_le_bytes([d[p + 12], d[p + 13], d[p + 14], d[p + 15]])
        } else {
            half_to_f32(u16::from_le_bytes([d[p + 6], d[p + 7]]))
        };
        Some((
            [unpack_dir(d[t]), unpack_dir(d[t + 1]), unpack_dir(d[t + 2])],
            [bx, unpack_dir(d[n + 3]), unpack_dir(d[t + 3])],
        ))
    }

    pub fn set_tangent_space(
        &mut self,
        layout: &Layout,
        vertex: usize,
        tangent: [f32; 3],
        bitangent: [f32; 3],
    ) {
        let (Some(t), Some(n), Some(p)) = (
            self.at(layout, vertex, layout.tangent, 4),
            self.at(layout, vertex, layout.normal, 4),
            self.at(
                layout,
                vertex,
                layout.position,
                if layout.full_precision { 16 } else { 8 },
            ),
        ) else {
            return;
        };
        let d = &mut self.vertex_data;
        for (i, v) in tangent.iter().enumerate() {
            d[t + i] = pack_dir(*v);
        }
        if layout.full_precision {
            d[p + 12..p + 16].copy_from_slice(&bitangent[0].to_le_bytes());
        } else {
            d[p + 6..p + 8].copy_from_slice(&f32_to_half(bitangent[0]).to_le_bytes());
        }
        d[n + 3] = pack_dir(bitangent[1]);
        d[t + 3] = pack_dir(bitangent[2]);
    }

    pub fn color(&self, layout: &Layout, vertex: usize) -> Option<[u8; 4]> {
        let at = self.at(layout, vertex, layout.color, 4)?;
        let d = &self.vertex_data;
        Some([d[at], d[at + 1], d[at + 2], d[at + 3]])
    }

    /// The four bones a vertex follows and how much it follows each.
    pub fn skin_weights(&self, layout: &Layout, vertex: usize) -> Option<([f32; 4], [u8; 4])> {
        let at = self.at(layout, vertex, layout.skin, 12)?;
        let d = &self.vertex_data;
        let mut weights = [0.0; 4];
        for (i, w) in weights.iter_mut().enumerate() {
            *w = half_to_f32(u16::from_le_bytes([d[at + i * 2], d[at + i * 2 + 1]]));
        }
        Some((weights, [d[at + 8], d[at + 9], d[at + 10], d[at + 11]]))
    }

    pub fn set_skin_weights(
        &mut self,
        layout: &Layout,
        vertex: usize,
        weights: [f32; 4],
        bones: [u8; 4],
    ) {
        if let Some(at) = self.at(layout, vertex, layout.skin, 12) {
            for (i, w) in weights.iter().enumerate() {
                self.vertex_data[at + i * 2..at + i * 2 + 2]
                    .copy_from_slice(&f32_to_half(*w).to_le_bytes());
            }
            self.vertex_data[at + 8..at + 12].copy_from_slice(&bones);
        }
    }
}

/// A vertex's position in packed vertex bytes of `layout`.
pub fn position_in(data: &[u8], layout: &Layout, vertex: usize) -> Option<[f32; 3]> {
    let at = vertex
        .checked_mul(layout.stride)?
        .checked_add(layout.position?)?;
    if layout.full_precision {
        let d = data.get(at..at + 12)?;
        Some([
            f32::from_le_bytes([d[0], d[1], d[2], d[3]]),
            f32::from_le_bytes([d[4], d[5], d[6], d[7]]),
            f32::from_le_bytes([d[8], d[9], d[10], d[11]]),
        ])
    } else {
        let d = data.get(at..at + 6)?;
        Some([
            half_to_f32(u16::from_le_bytes([d[0], d[1]])),
            half_to_f32(u16::from_le_bytes([d[2], d[3]])),
            half_to_f32(u16::from_le_bytes([d[4], d[5]])),
        ])
    }
}

/// Set a vertex's position in packed vertex bytes of `layout`.
pub fn set_position_in(data: &mut [u8], layout: &Layout, vertex: usize, p: [f32; 3]) {
    let Some(at) = vertex
        .checked_mul(layout.stride)
        .and_then(|v| v.checked_add(layout.position?))
    else {
        return;
    };
    if layout.full_precision {
        if let Some(d) = data.get_mut(at..at + 12) {
            for (i, v) in p.iter().enumerate() {
                d[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
    } else if let Some(d) = data.get_mut(at..at + 6) {
        for (i, v) in p.iter().enumerate() {
            d[i * 2..i * 2 + 2].copy_from_slice(&f32_to_half(*v).to_le_bytes());
        }
    }
}

/// The four bones a vertex follows and how much it follows each, in packed
/// vertex bytes of `layout`.
pub fn skin_weights_in(data: &[u8], layout: &Layout, vertex: usize) -> Option<([f32; 4], [u8; 4])> {
    let at = vertex
        .checked_mul(layout.stride)?
        .checked_add(layout.skin?)?;
    let d = data.get(at..at + 12)?;
    let mut weights = [0.0; 4];
    for (i, w) in weights.iter_mut().enumerate() {
        *w = half_to_f32(u16::from_le_bytes([d[i * 2], d[i * 2 + 1]]));
    }
    Some((weights, [d[8], d[9], d[10], d[11]]))
}

/// Set the four bones a vertex follows in packed vertex bytes of `layout`.
/// Says whether there was room for them.
pub fn set_skin_weights_in(
    data: &mut [u8],
    layout: &Layout,
    vertex: usize,
    weights: [f32; 4],
    bones: [u8; 4],
) -> bool {
    let Some(d) = vertex
        .checked_mul(layout.stride)
        .and_then(|v| v.checked_add(layout.skin?))
        .and_then(|at| data.get_mut(at..at + 12))
    else {
        return false;
    };
    for (i, w) in weights.iter().enumerate() {
        d[i * 2..i * 2 + 2].copy_from_slice(&f32_to_half(*w).to_le_bytes());
    }
    d[8..12].copy_from_slice(&bones);
    true
}
