//! Shapes as Skyrim and the games before it keep them: a `NiTriShape` that
//! points at a `NiTriShapeData` holding the vertices as plain floats.

use crate::io::{Reader, Writer};
use crate::node::{AvObject, Ref};
use crate::NifError;

/// `NiTriShape`: the scene object, and its links to its data, skin and
/// shader.
#[derive(Debug, Clone, PartialEq)]
pub struct NiTriShape {
    pub av: AvObject,
    pub data: Ref,
    pub skin: Ref,
    /// Material names and their extra data, kept as they were.
    materials: Vec<u8>,
    /// Skyrim on: the shader and alpha properties.
    pub shader: Option<Ref>,
    pub alpha: Option<Ref>,
    pub rest: Vec<u8>,
}

impl NiTriShape {
    pub fn read(data: &[u8], stream: u32) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let av = AvObject::read(&mut r, stream)?;
        let geometry_data = r.i32()?;
        let skin = r.i32()?;
        let start = r.pos();
        let count = r.count(8)?;
        r.take(count * 8)?;
        r.i32()?; // active material
        r.u8()?; // material needs update
        let materials = data[start..r.pos()].to_vec();
        let (shader, alpha) = if stream > 34 {
            (Some(r.i32()?), Some(r.i32()?))
        } else {
            (None, None)
        };
        let rest = r.take(r.left())?.to_vec();
        Ok(NiTriShape {
            av,
            data: geometry_data,
            skin,
            materials,
            shader,
            alpha,
            rest,
        })
    }

    pub fn write(&self, stream: u32) -> Vec<u8> {
        let mut w = Writer::default();
        self.av.write(&mut w, stream);
        w.i32(self.data);
        w.i32(self.skin);
        w.bytes(&self.materials);
        if let (Some(s), Some(a)) = (self.shader, self.alpha) {
            w.i32(s);
            w.i32(a);
        }
        w.bytes(&self.rest);
        w.out
    }
}

/// `NiTriShapeData`: the vertices and triangles themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct TriShapeData {
    pub group_id: i32,
    pub keep_flags: u8,
    pub compress_flags: u8,
    pub positions: Vec<[f32; 3]>,
    /// The count of UV sets in its low bit, and whether there are tangents.
    pub vector_flags: u16,
    pub material_crc: Option<u32>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
    pub bitangents: Vec<[f32; 3]>,
    pub bound_center: [f32; 3],
    pub bound_radius: f32,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub consistency: u16,
    pub additional_data: Ref,
    pub triangles: Vec<[u16; 3]>,
    /// Groups of vertices that are one point drawn several times.
    pub match_groups: Vec<Vec<u16>>,
    /// The vertex count as stated when the block holds no positions.
    stated_vertices: u16,
    stated_triangles: u16,
    stated_points: u32,
    has: [u8; 4],
}

const HAS_TANGENTS: u16 = 0x1000;

fn vec3s(r: &mut Reader, n: usize) -> Result<Vec<[f32; 3]>, NifError> {
    if n.checked_mul(12).is_none_or(|need| need > r.left()) {
        return Err(NifError::Truncated);
    }
    (0..n).map(|_| Ok([r.f32()?, r.f32()?, r.f32()?])).collect()
}

fn put_vec3s(w: &mut Writer, v: &[[f32; 3]]) {
    for p in v {
        for c in p {
            w.f32(*c);
        }
    }
}

impl TriShapeData {
    pub fn read(data: &[u8], stream: u32) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let group_id = r.i32()?;
        let stated_vertices = r.u16()?;
        let n = stated_vertices as usize;
        let keep_flags = r.u8()?;
        let compress_flags = r.u8()?;
        let has_vertices = r.u8()?;
        let positions = if has_vertices != 0 {
            vec3s(&mut r, n)?
        } else {
            Vec::new()
        };
        let vector_flags = r.u16()?;
        let material_crc = if stream > 34 { Some(r.u32()?) } else { None };
        let has_normals = r.u8()?;
        let (mut normals, mut tangents, mut bitangents) = (Vec::new(), Vec::new(), Vec::new());
        if has_normals != 0 {
            normals = vec3s(&mut r, n)?;
            if vector_flags & HAS_TANGENTS != 0 {
                tangents = vec3s(&mut r, n)?;
                bitangents = vec3s(&mut r, n)?;
            }
        }
        let bound_center = [r.f32()?, r.f32()?, r.f32()?];
        let bound_radius = r.f32()?;
        let has_colors = r.u8()?;
        let mut colors = Vec::new();
        if has_colors != 0 {
            if n.checked_mul(16).is_none_or(|need| need > r.left()) {
                return Err(NifError::Truncated);
            }
            for _ in 0..n {
                colors.push([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
            }
        }
        let mut uvs = Vec::new();
        if vector_flags & 1 != 0 {
            if n.checked_mul(8).is_none_or(|need| need > r.left()) {
                return Err(NifError::Truncated);
            }
            for _ in 0..n {
                uvs.push([r.f32()?, r.f32()?]);
            }
        }
        let consistency = r.u16()?;
        let additional_data = r.i32()?;
        let stated_triangles = r.u16()?;
        let stated_points = r.u32()?;
        let has_triangles = r.u8()?;
        let mut triangles = Vec::new();
        if has_triangles != 0 {
            let nt = stated_triangles as usize;
            if nt.checked_mul(6).is_none_or(|need| need > r.left()) {
                return Err(NifError::Truncated);
            }
            for _ in 0..nt {
                triangles.push([r.u16()?, r.u16()?, r.u16()?]);
            }
        }
        let group_count = r.u16()? as usize;
        let mut match_groups = Vec::with_capacity(group_count.min(4096));
        for _ in 0..group_count {
            let len = r.u16()? as usize;
            let mut group = Vec::with_capacity(len.min(4096));
            for _ in 0..len {
                group.push(r.u16()?);
            }
            match_groups.push(group);
        }
        if r.left() != 0 {
            return Err(NifError::Corrupt("bytes after shape data".into()));
        }
        Ok(TriShapeData {
            group_id,
            keep_flags,
            compress_flags,
            positions,
            vector_flags,
            material_crc,
            normals,
            tangents,
            bitangents,
            bound_center,
            bound_radius,
            colors,
            uvs,
            consistency,
            additional_data,
            triangles,
            match_groups,
            stated_vertices,
            stated_triangles,
            stated_points,
            has: [has_vertices, has_normals, has_colors, has_triangles],
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.i32(self.group_id);
        let n = if self.positions.is_empty() {
            self.stated_vertices
        } else {
            self.positions.len() as u16
        };
        w.u16(n);
        w.u8(self.keep_flags);
        w.u8(self.compress_flags);
        w.u8(self.has[0]);
        put_vec3s(&mut w, &self.positions);
        w.u16(self.vector_flags);
        if let Some(crc) = self.material_crc {
            w.u32(crc);
        }
        w.u8(self.has[1]);
        put_vec3s(&mut w, &self.normals);
        put_vec3s(&mut w, &self.tangents);
        put_vec3s(&mut w, &self.bitangents);
        for c in &self.bound_center {
            w.f32(*c);
        }
        w.f32(self.bound_radius);
        w.u8(self.has[2]);
        for c in self.colors.iter().flatten() {
            w.f32(*c);
        }
        for c in self.uvs.iter().flatten() {
            w.f32(*c);
        }
        w.u16(self.consistency);
        w.i32(self.additional_data);
        let same_count = self.triangles.len() == self.stated_triangles as usize;
        if self.has[3] != 0 && !same_count {
            w.u16(self.triangles.len() as u16);
            w.u32(self.triangles.len() as u32 * 3);
        } else {
            w.u16(self.stated_triangles);
            w.u32(self.stated_points);
        }
        w.u8(self.has[3]);
        for t in &self.triangles {
            for i in t {
                w.u16(*i);
            }
        }
        w.u16(self.match_groups.len() as u16);
        for g in &self.match_groups {
            w.u16(g.len() as u16);
            for i in g {
                w.u16(*i);
            }
        }
        w.out
    }
}
