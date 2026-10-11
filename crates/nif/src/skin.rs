//! Skinning: which bones a shape follows, and how its vertices are divided
//! among them for the game to draw.
//!
//! In Special Edition the skin partition holds a second copy of the shape's
//! vertices, which is the copy the game draws a skinned shape from. Anything
//! that moves a vertex has to move it in both.

use crate::io::{Reader, Writer};
use crate::node::{read_refs, write_refs, Ref, Transform};
use crate::NifError;

/// `NiSkinInstance`, and `BSDismemberSkinInstance` which adds body parts.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinInstance {
    pub data: Ref,
    pub partition: Ref,
    pub skeleton_root: Ref,
    pub bones: Vec<Ref>,
    /// `BSDismemberSkinInstance` only: a flag pair and a body part for each
    /// partition.
    pub body_parts: Option<Vec<(u16, u16)>>,
}

impl SkinInstance {
    pub fn read(data: &[u8], dismember: bool) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let skin_data = r.i32()?;
        let partition = r.i32()?;
        let skeleton_root = r.i32()?;
        let bones = read_refs(&mut r)?;
        let body_parts = if dismember {
            let n = r.count(4)?;
            let mut parts = Vec::with_capacity(n);
            for _ in 0..n {
                parts.push((r.u16()?, r.u16()?));
            }
            Some(parts)
        } else {
            None
        };
        if r.left() != 0 {
            return Err(NifError::Corrupt("bytes after a skin instance".into()));
        }
        Ok(SkinInstance {
            data: skin_data,
            partition,
            skeleton_root,
            bones,
            body_parts,
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.i32(self.data);
        w.i32(self.partition);
        w.i32(self.skeleton_root);
        write_refs(&mut w, &self.bones);
        if let Some(parts) = &self.body_parts {
            w.u32(parts.len() as u32);
            for (flags, part) in parts {
                w.u16(*flags);
                w.u16(*part);
            }
        }
        w.out
    }
}

/// A skin transform is stored turned round: rotation, then translation.
fn read_skin_transform(r: &mut Reader) -> Result<Transform, NifError> {
    let mut rotation = [0.0; 9];
    for v in &mut rotation {
        *v = r.f32()?;
    }
    let mut translation = [0.0; 3];
    for v in &mut translation {
        *v = r.f32()?;
    }
    Ok(Transform {
        translation,
        rotation,
        scale: r.f32()?,
    })
}

fn write_skin_transform(w: &mut Writer, t: &Transform) {
    for v in t.rotation.iter().chain(&t.translation) {
        w.f32(*v);
    }
    w.f32(t.scale);
}

/// One bone of a `NiSkinData`.
#[derive(Debug, Clone, PartialEq)]
pub struct BoneData {
    pub transform: Transform,
    pub bound_center: [f32; 3],
    pub bound_radius: f32,
    /// Each vertex this bone moves and by how much. Empty when the block
    /// keeps no weights (they are in the partition or the vertices then),
    /// though the count the file stated is kept in `stated_vertices`.
    pub weights: Vec<(u16, f32)>,
    stated_vertices: u16,
}

/// `NiSkinData`: where each bone is, and the weights by bone.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinData {
    pub transform: Transform,
    pub has_weights: u8,
    pub bones: Vec<BoneData>,
}

impl SkinData {
    pub fn read(data: &[u8]) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let transform = read_skin_transform(&mut r)?;
        let bone_count = r.count(1)?;
        let has_weights = r.u8()?;
        let mut bones = Vec::with_capacity(bone_count);
        for _ in 0..bone_count {
            let transform = read_skin_transform(&mut r)?;
            let bound_center = [r.f32()?, r.f32()?, r.f32()?];
            let bound_radius = r.f32()?;
            let stated_vertices = r.u16()?;
            let mut weights = Vec::new();
            if has_weights != 0 {
                weights.reserve(stated_vertices as usize);
                for _ in 0..stated_vertices {
                    weights.push((r.u16()?, r.f32()?));
                }
            }
            bones.push(BoneData {
                transform,
                bound_center,
                bound_radius,
                weights,
                stated_vertices,
            });
        }
        if r.left() != 0 {
            return Err(NifError::Corrupt("bytes after skin data".into()));
        }
        Ok(SkinData {
            transform,
            has_weights,
            bones,
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        write_skin_transform(&mut w, &self.transform);
        w.u32(self.bones.len() as u32);
        w.u8(self.has_weights);
        for b in &self.bones {
            write_skin_transform(&mut w, &b.transform);
            for v in &b.bound_center {
                w.f32(*v);
            }
            w.f32(b.bound_radius);
            if self.has_weights != 0 {
                w.u16(b.weights.len() as u16);
                for (vertex, weight) in &b.weights {
                    w.u16(*vertex);
                    w.f32(*weight);
                }
            } else {
                w.u16(b.stated_vertices);
            }
        }
        w.out
    }
}

/// One partition of a `NiSkinPartition`: a group of triangles that share a
/// small set of bones.
#[derive(Debug, Clone, PartialEq)]
pub struct Partition {
    pub weights_per_vertex: u16,
    pub bones: Vec<u16>,
    /// The shape's vertex each of this partition's vertices is.
    pub vertex_map: Option<Vec<u16>>,
    /// `weights_per_vertex` weights a vertex.
    pub vertex_weights: Option<Vec<f32>>,
    pub strips: Vec<Vec<u16>>,
    /// Triangles over this partition's own vertices, when not in strips.
    pub triangles: Option<Vec<[u16; 3]>>,
    /// `weights_per_vertex` bones a vertex, as indices into `bones`.
    pub bone_indices: Option<Vec<u8>>,
    pub lod_level: Option<u16>,
    /// Special Edition: the vertex description again, and the triangles
    /// over the shape's vertices.
    pub vertex_desc: Option<u64>,
    pub true_triangles: Option<Vec<[u16; 3]>>,
    /// How many vertices and triangles the file said, kept for writing back
    /// a partition whose lists are absent.
    vertex_count: u16,
    triangle_count: u16,
    has_faces: u8,
}

impl Partition {
    pub fn vertex_count(&self) -> usize {
        self.vertex_count as usize
    }

    /// Replace this partition's triangles over the shape's vertices, and
    /// with them the vertex map and the triangles over its own vertices.
    pub fn set_true_triangles(&mut self, triangles: Vec<[u16; 3]>) {
        self.triangle_count = triangles.len() as u16;
        self.true_triangles = Some(triangles);
    }
}

/// The rows of `values`, `per` to a vertex, for the vertices kept.
fn kept_rows<T: Copy>(values: &[T], per: usize, kept: &[bool]) -> Vec<T> {
    if per == 0 {
        return Vec::new();
    }
    values
        .chunks(per)
        .zip(kept)
        .filter(|(_, k)| **k)
        .flat_map(|(row, _)| row.iter().copied())
        .collect()
}

impl Partition {
    /// Keep only the vertices still in the shape: `now[v]` is where the
    /// shape's vertex `v` is now, or `None` when it was removed. A triangle
    /// goes when any of its vertices did. Says whether a triangle is left.
    pub fn retain(&mut self, now: &[Option<u16>]) -> Result<bool, NifError> {
        if self.strips.iter().any(|s| !s.is_empty()) {
            return Err(NifError::Unsupported(
                "removing vertices from a partition kept as strips".into(),
            ));
        }
        let at = |v: u16| now.get(v as usize).copied().flatten();
        // Each of this partition's own vertices: whether it stays, and
        // where among its own it then is.
        let kept: Vec<bool> = match &self.vertex_map {
            Some(map) => map.iter().map(|v| at(*v).is_some()).collect(),
            None => (0..self.vertex_count).map(|v| at(v).is_some()).collect(),
        };
        let mut next = 0u16;
        let own: Vec<Option<u16>> = kept
            .iter()
            .map(|k| {
                k.then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        let per = self.weights_per_vertex as usize;
        if let Some(w) = &mut self.vertex_weights {
            *w = kept_rows(w, per, &kept);
        }
        if let Some(b) = &mut self.bone_indices {
            *b = kept_rows(b, per, &kept);
        }
        // Some files state the triangles over the shape's vertices in both
        // places, some over the partition's own in the first.
        let same = self.triangles.is_some() && self.triangles == self.true_triangles;
        if let Some(t) = &mut self.true_triangles {
            *t = t
                .iter()
                .filter_map(|t| Some([at(t[0])?, at(t[1])?, at(t[2])?]))
                .collect();
        }
        if same {
            self.triangles = self.true_triangles.clone();
        } else if let Some(t) = &mut self.triangles {
            let own = |v: u16| own.get(v as usize).copied().flatten();
            *t = t
                .iter()
                .filter_map(|t| Some([own(t[0])?, own(t[1])?, own(t[2])?]))
                .collect();
        }
        if let Some(map) = &mut self.vertex_map {
            *map = map.iter().filter_map(|v| at(*v)).collect();
        }
        self.vertex_count = next;
        self.triangle_count = self
            .true_triangles
            .as_ref()
            .or(self.triangles.as_ref())
            .map_or(0, Vec::len) as u16;
        Ok(self.triangle_count > 0)
    }

    pub fn triangle_count(&self) -> usize {
        self.triangle_count as usize
    }
}

impl SkinData {
    /// Keep only the weights of vertices still in the shape (see
    /// [`Partition::retain`]).
    pub fn retain(&mut self, now: &[Option<u16>]) {
        for bone in &mut self.bones {
            bone.weights = bone
                .weights
                .iter()
                .filter_map(|(v, w)| Some((now.get(*v as usize).copied().flatten()?, *w)))
                .collect();
        }
    }
}

/// `NiSkinPartition`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinPartition {
    /// Special Edition: the shape's vertices again, in the shape's layout.
    pub vertex_size: Option<u32>,
    pub vertex_desc: Option<u64>,
    pub vertex_data: Vec<u8>,
    pub partitions: Vec<Partition>,
    stated_data_size: Option<u32>,
}

fn read_triangles(r: &mut Reader, n: usize) -> Result<Vec<[u16; 3]>, NifError> {
    if n.checked_mul(6).is_none_or(|need| need > r.left()) {
        return Err(NifError::Truncated);
    }
    (0..n).map(|_| Ok([r.u16()?, r.u16()?, r.u16()?])).collect()
}

fn write_triangles(w: &mut Writer, triangles: &[[u16; 3]]) {
    for t in triangles {
        for i in t {
            w.u16(*i);
        }
    }
}

impl SkinPartition {
    /// Keep only the vertices still in the shape (see [`Partition::retain`]),
    /// and only the partitions that still have a triangle. Says which of the
    /// partitions were kept, in their old order.
    pub fn retain(&mut self, now: &[Option<u16>]) -> Result<Vec<bool>, NifError> {
        if let Some(size) = self.vertex_size.map(|s| s as usize).filter(|s| *s > 0) {
            if !self.vertex_data.is_empty() {
                if self.vertex_data.len() != now.len() * size {
                    return Err(NifError::Corrupt(
                        "a skin partition whose vertices are not the shape's".into(),
                    ));
                }
                self.vertex_data = self
                    .vertex_data
                    .chunks(size)
                    .zip(now)
                    .filter(|(_, n)| n.is_some())
                    .flat_map(|(row, _)| row.iter().copied())
                    .collect();
            }
        }
        let mut kept = Vec::with_capacity(self.partitions.len());
        for p in &mut self.partitions {
            kept.push(p.retain(now)?);
        }
        let mut flags = kept.iter();
        self.partitions.retain(|_| *flags.next().unwrap_or(&true));
        Ok(kept)
    }

    pub fn read(data: &[u8], stream: u32) -> Result<Self, NifError> {
        let mut r = Reader::new(data);
        let count = r.count(1)?;
        let sse = stream == 100;
        let (stated_data_size, vertex_size, vertex_desc, vertex_data) = if sse {
            let data_size = r.u32()?;
            let vertex_size = r.u32()?;
            let desc = r.u64()?;
            let bytes = if data_size > 0 {
                r.take(data_size as usize)?.to_vec()
            } else {
                Vec::new()
            };
            (Some(data_size), Some(vertex_size), Some(desc), bytes)
        } else {
            (None, None, None, Vec::new())
        };
        let mut partitions = Vec::with_capacity(count);
        for _ in 0..count {
            let vertex_count = r.u16()?;
            let triangle_count = r.u16()?;
            let bone_count = r.u16()? as usize;
            let strip_count = r.u16()? as usize;
            let weights_per_vertex = r.u16()?;
            let mut bones = Vec::with_capacity(bone_count);
            for _ in 0..bone_count {
                bones.push(r.u16()?);
            }
            let (nv, wpv) = (vertex_count as usize, weights_per_vertex as usize);
            let vertex_map = if r.u8()? != 0 {
                let mut map = Vec::with_capacity(nv);
                for _ in 0..nv {
                    map.push(r.u16()?);
                }
                Some(map)
            } else {
                None
            };
            let vertex_weights = if r.u8()? != 0 {
                let mut weights = Vec::with_capacity(nv * wpv);
                for _ in 0..nv * wpv {
                    weights.push(r.f32()?);
                }
                Some(weights)
            } else {
                None
            };
            let mut strip_lengths = Vec::with_capacity(strip_count);
            for _ in 0..strip_count {
                strip_lengths.push(r.u16()? as usize);
            }
            let has_faces = r.u8()?;
            let mut strips = Vec::new();
            let mut triangles = None;
            if has_faces != 0 {
                if strip_count != 0 {
                    for len in &strip_lengths {
                        let mut strip = Vec::with_capacity(*len);
                        for _ in 0..*len {
                            strip.push(r.u16()?);
                        }
                        strips.push(strip);
                    }
                } else {
                    triangles = Some(read_triangles(&mut r, triangle_count as usize)?);
                }
            } else {
                // No faces, but the strips' lengths were stated: kept so
                // the block is written back as it was.
                strips = strip_lengths.iter().map(|_| Vec::new()).collect();
                if strip_lengths.iter().any(|l| *l != 0) {
                    return Err(NifError::Unsupported("strip lengths with no faces".into()));
                }
            }
            let bone_indices = if r.u8()? != 0 {
                Some(r.take(nv * wpv)?.to_vec())
            } else {
                None
            };
            let lod_level = if stream > 34 { Some(r.u16()?) } else { None };
            let (vertex_desc, true_triangles) = if sse {
                let desc = r.u64()?;
                (
                    Some(desc),
                    Some(read_triangles(&mut r, triangle_count as usize)?),
                )
            } else {
                (None, None)
            };
            partitions.push(Partition {
                weights_per_vertex,
                bones,
                vertex_map,
                vertex_weights,
                strips,
                triangles,
                bone_indices,
                lod_level,
                vertex_desc,
                true_triangles,
                vertex_count,
                triangle_count,
                has_faces,
            });
        }
        if r.left() != 0 {
            return Err(NifError::Corrupt("bytes after a skin partition".into()));
        }
        Ok(SkinPartition {
            vertex_size,
            vertex_desc,
            vertex_data,
            partitions,
            stated_data_size,
        })
    }

    pub fn write(&self, stream: u32) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.partitions.len() as u32);
        if let (Some(size), Some(desc)) = (self.vertex_size, self.vertex_desc) {
            // The size as stated, unless the vertices were changed in number.
            let stated = self
                .stated_data_size
                .filter(|s| *s as usize == self.vertex_data.len() || self.vertex_data.is_empty());
            w.u32(stated.unwrap_or(self.vertex_data.len() as u32));
            w.u32(size);
            w.u64(desc);
            w.bytes(&self.vertex_data);
        }
        for p in &self.partitions {
            w.u16(p.vertex_count);
            w.u16(p.triangle_count);
            w.u16(p.bones.len() as u16);
            w.u16(p.strips.len() as u16);
            w.u16(p.weights_per_vertex);
            for b in &p.bones {
                w.u16(*b);
            }
            w.u8(p.vertex_map.is_some() as u8);
            for v in p.vertex_map.iter().flatten() {
                w.u16(*v);
            }
            w.u8(p.vertex_weights.is_some() as u8);
            for v in p.vertex_weights.iter().flatten() {
                w.f32(*v);
            }
            for strip in &p.strips {
                w.u16(strip.len() as u16);
            }
            w.u8(p.has_faces);
            if p.has_faces != 0 {
                for strip in &p.strips {
                    for i in strip {
                        w.u16(*i);
                    }
                }
                if let Some(t) = &p.triangles {
                    write_triangles(&mut w, t);
                }
            }
            w.u8(p.bone_indices.is_some() as u8);
            if let Some(b) = &p.bone_indices {
                w.bytes(b);
            }
            if let Some(l) = p.lod_level {
                w.u16(l);
            }
            if let Some(d) = p.vertex_desc {
                w.u64(d);
            }
            if let Some(t) = &p.true_triangles {
                write_triangles(&mut w, t);
            }
        }
        let _ = stream;
        w.out
    }
}
