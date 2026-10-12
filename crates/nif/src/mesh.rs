//! A shape as something to draw and to move: its vertices and triangles,
//! wherever in the file they are kept, and writing moved vertices back to
//! every place that keeps them.
//!
//! A Special Edition shape's vertices can be in the shape, in its skin
//! partition, or both; a face part (`BSDynamicTriShape`) keeps its positions
//! in a list of its own. This is the one place that knows.

use crate::geometry::{self, attr, Layout, TriShape};
use crate::geometry_le::{NiTriShape, TriShapeData};
use crate::shader::{LightingShader, TextureSet};
use crate::skin::{SkinInstance, SkinPartition};
use crate::{Nif, NifError};

/// A shape read out of a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// The block the shape is.
    pub block: usize,
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    /// Empty when the shape carries none.
    pub uvs: Vec<[f32; 2]>,
    pub normals: Vec<[f32; 3]>,
    pub triangles: Vec<[u16; 3]>,
    /// Texture files by slot (colour, normal map, …), as the file names them.
    pub textures: Vec<String>,
    pub skinned: bool,
}

/// The blocks of a file that are shapes this can read.
pub fn shapes(nif: &Nif) -> Vec<usize> {
    (0..nif.blocks.len())
        .filter(|i| {
            let kind = nif.kind_of(*i);
            geometry::is_tri_shape(kind) || kind == "NiTriShape"
        })
        .collect()
}

fn textures_of(nif: &Nif, shader: i32) -> Vec<String> {
    let stream = nif.header.stream.version;
    block_of(nif, shader, &["BSLightingShaderProperty"])
        .and_then(|at| LightingShader::read(&nif.blocks[at].data, stream).ok())
        .and_then(|s| block_of(nif, s.texture_set, &["BSShaderTextureSet"]))
        .and_then(|at| TextureSet::read(&nif.blocks[at].data).ok())
        .map(|set| {
            set.textures
                .iter()
                .map(|t| String::from_utf8_lossy(t).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Name the texture files of the shape at `block`, slot by slot: as many
/// as it has slots for, no more and no fewer.
pub fn set_textures(nif: &mut Nif, block: usize, textures: &[String]) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    let shader = if nif.kind_of(block) == "NiTriShape" {
        NiTriShape::read(&nif.blocks[block].data, stream)?
            .shader
            .unwrap_or(-1)
    } else {
        TriShape::read(&nif.blocks[block].data, stream)?.shader
    };
    let at = block_of(nif, shader, &["BSLightingShaderProperty"])
        .and_then(|at| LightingShader::read(&nif.blocks[at].data, stream).ok())
        .and_then(|s| block_of(nif, s.texture_set, &["BSShaderTextureSet"]))
        .ok_or_else(|| NifError::Unsupported("the shape names no textures".into()))?;
    let mut set = TextureSet::read(&nif.blocks[at].data)?;
    if set.textures.len() != textures.len() {
        return Err(NifError::Unsupported(format!(
            "the shape has {} texture slots",
            set.textures.len()
        )));
    }
    set.textures = textures.iter().map(|t| t.as_bytes().to_vec()).collect();
    nif.blocks[at].data = set.write();
    Ok(())
}

fn name_of(nif: &Nif, index: i32) -> String {
    nif.string(index)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .unwrap_or_default()
}

/// A shape of the original Skyrim's kind, with its data block.
fn le_shape(nif: &Nif, block: usize) -> Result<(NiTriShape, usize, TriShapeData), NifError> {
    let stream = nif.header.stream.version;
    let shape = NiTriShape::read(&nif.blocks[block].data, stream)?;
    let at = block_of(nif, shape.data, &["NiTriShapeData"])
        .ok_or_else(|| NifError::Unsupported("a shape without triangle data".into()))?;
    let data = TriShapeData::read(&nif.blocks[at].data, stream)?;
    Ok((shape, at, data))
}

fn block_of(nif: &Nif, link: i32, kinds: &[&str]) -> Option<usize> {
    usize::try_from(link)
        .ok()
        .filter(|i| kinds.contains(&nif.kind_of(*i)))
}

fn partition_of(nif: &Nif, shape: &TriShape) -> Option<(usize, SkinPartition)> {
    let stream = nif.header.stream.version;
    let at = block_of(
        nif,
        shape.skin,
        &["NiSkinInstance", "BSDismemberSkinInstance"],
    )?;
    let instance =
        SkinInstance::read(&nif.blocks[at].data, nif.kind_of(at) != "NiSkinInstance").ok()?;
    let at = block_of(nif, instance.partition, &["NiSkinPartition"])?;
    Some((at, SkinPartition::read(&nif.blocks[at].data, stream).ok()?))
}

/// A face part keeps its positions after the shape, four floats a vertex.
fn dynamic_positions(shape: &TriShape) -> Option<Vec<[f32; 3]>> {
    let n = shape.vertex_count as usize;
    let size = u32::from_le_bytes(shape.rest.get(..4)?.try_into().ok()?) as usize;
    let data = shape.rest.get(4..4 + size)?;
    (size == n * 16).then(|| {
        data.as_chunks::<16>()
            .0
            .iter()
            .map(|c| {
                [
                    f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                    f32::from_le_bytes([c[8], c[9], c[10], c[11]]),
                ]
            })
            .collect()
    })
}

/// The names of the bones a shape is skinned to, in the order its skin
/// lists them. Empty for a shape that is not skinned.
pub fn bone_names(nif: &Nif, block: usize) -> Vec<String> {
    let stream = nif.header.stream.version;
    let skin = if nif.kind_of(block) == "NiTriShape" {
        NiTriShape::read(&nif.blocks[block].data, stream).map(|s| s.skin)
    } else {
        TriShape::read(&nif.blocks[block].data, stream).map(|s| s.skin)
    };
    let Some(at) = skin
        .ok()
        .and_then(|l| block_of(nif, l, &["NiSkinInstance", "BSDismemberSkinInstance"]))
    else {
        return Vec::new();
    };
    let Ok(instance) =
        SkinInstance::read(&nif.blocks[at].data, nif.kind_of(at) != "NiSkinInstance")
    else {
        return Vec::new();
    };
    instance
        .bones
        .iter()
        .map(|bone| {
            usize::try_from(*bone)
                .ok()
                .filter(|i| crate::node::is_node(nif.kind_of(*i)))
                .and_then(|i| crate::node::Node::read(&nif.blocks[i].data, stream).ok())
                .map(|n| name_of(nif, n.av.name))
                .unwrap_or_default()
        })
        .collect()
}

/// Read the shape at `block`.
pub fn read(nif: &Nif, block: usize) -> Result<Mesh, NifError> {
    let stream = nif.header.stream.version;
    let kind = nif.kind_of(block).to_string();
    if kind == "NiTriShape" {
        let (shape, _, data) = le_shape(nif, block)?;
        if data
            .triangles
            .iter()
            .flatten()
            .any(|i| *i as usize >= data.positions.len())
        {
            return Err(NifError::Corrupt(
                "a triangle names a vertex past the last".into(),
            ));
        }
        return Ok(Mesh {
            block,
            name: name_of(nif, shape.av.name),
            positions: data.positions,
            uvs: data.uvs,
            normals: data.normals,
            triangles: data.triangles,
            textures: textures_of(nif, shape.shader.unwrap_or(-1)),
            skinned: shape.skin >= 0,
        });
    }
    if !geometry::is_tri_shape(&kind) {
        return Err(NifError::Unsupported(format!("a {kind} is not a shape")));
    }
    let shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let layout = Layout::of(shape.vertex_desc, stream)
        .ok_or_else(|| NifError::Unsupported(format!("vertex layout {:#x}", shape.vertex_desc)))?;
    let partition = partition_of(nif, &shape);
    // A shape whose vertices are only in its skin partition often states
    // none of its own: the partition's bytes say how many there are.
    let n = match &partition {
        Some((_, p)) if !shape.has_vertices() && layout.stride > 0 && !p.vertex_data.is_empty() => {
            p.vertex_data.len() / layout.stride
        }
        _ => shape.vertex_count as usize,
    };

    // The shape's own copy when it has one, else the skin partition's.
    let (data, triangles): (&[u8], Vec<[u16; 3]>) = if shape.has_vertices() {
        (&shape.vertex_data, shape.triangles.clone())
    } else if let Some((_, p)) = &partition {
        (
            &p.vertex_data,
            p.partitions
                .iter()
                .flat_map(|part| part.true_triangles.iter().flatten().copied())
                .collect(),
        )
    } else {
        (&[], Vec::new())
    };
    if data.len() != n * layout.stride {
        return Err(NifError::Corrupt(format!(
            "{} bytes of vertices for {n} of {} each",
            data.len(),
            layout.stride
        )));
    }
    // Read through the shape's own accessors, over whichever copy it is.
    let mut view = shape.clone();
    view.vertex_data = data.to_vec();
    let positions = if kind == "BSDynamicTriShape" {
        dynamic_positions(&shape)
            .ok_or_else(|| NifError::Corrupt("a face part without its positions".into()))?
    } else if layout.has(attr::POSITION) {
        (0..n).filter_map(|v| view.position(&layout, v)).collect()
    } else {
        return Err(NifError::Unsupported("a shape with no positions".into()));
    };
    let uvs = (0..n).filter_map(|v| view.uv(&layout, v)).collect();
    let normals = (0..n).filter_map(|v| view.normal(&layout, v)).collect();
    if triangles.iter().flatten().any(|i| *i as usize >= n) {
        return Err(NifError::Corrupt(
            "a triangle names a vertex past the last".into(),
        ));
    }

    let textures = textures_of(nif, shape.shader);

    Ok(Mesh {
        block,
        name: name_of(nif, shape.av.name),
        positions,
        uvs,
        normals,
        triangles,
        textures,
        skinned: shape.skin >= 0,
    })
}

/// The name of the shape at `block`.
pub fn name(nif: &Nif, block: usize) -> Option<String> {
    let stream = nif.header.stream.version;
    // Read from the start of the block: looking a shape up by its name
    // does not cost a copy of its vertices.
    let name = if nif.kind_of(block) == "NiTriShape" {
        NiTriShape::read(&nif.blocks[block].data, stream)
            .ok()?
            .av
            .name
    } else {
        TriShape::name_of(&nif.blocks[block].data, stream)?
    };
    Some(name_of(nif, name))
}

/// Give the shape at `block` another name.
pub fn rename(nif: &mut Nif, block: usize, to: &str) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    let name = nif.intern(to.as_bytes());
    if nif.kind_of(block) == "NiTriShape" {
        let mut shape = NiTriShape::read(&nif.blocks[block].data, stream)?;
        shape.av.name = name;
        nif.blocks[block].data = shape.write(stream);
    } else {
        let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
        shape.av.name = name;
        nif.blocks[block].data = shape.write(stream);
    }
    Ok(())
}

/// Move a shape's vertices: `positions` in every place the file keeps them,
/// and the shape's bounding sphere around where they now are. Nothing else
/// in the file changes.
pub fn set_positions(nif: &mut Nif, block: usize, positions: &[[f32; 3]]) -> Result<(), NifError> {
    move_vertices(nif, block, positions, true)
}

/// [`set_positions`], with the bounding sphere left as the file states it
/// when `rebound` is false.
pub fn move_vertices(
    nif: &mut Nif,
    block: usize,
    positions: &[[f32; 3]],
    rebound: bool,
) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    let kind = nif.kind_of(block).to_string();
    if kind == "NiTriShape" {
        let (_, at, mut data) = le_shape(nif, block)?;
        if positions.len() != data.positions.len() {
            return Err(NifError::Corrupt(format!(
                "{} positions for a shape of {} vertices",
                positions.len(),
                data.positions.len()
            )));
        }
        data.positions = positions.to_vec();
        if rebound {
            let (center, radius) = bounding_sphere(positions);
            data.bound_center = center;
            data.bound_radius = radius;
        }
        nif.blocks[at].data = data.write();
        return Ok(());
    }
    let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let layout = Layout::of(shape.vertex_desc, stream)
        .ok_or_else(|| NifError::Unsupported(format!("vertex layout {:#x}", shape.vertex_desc)))?;
    let partition = partition_of(nif, &shape);
    let count = match &partition {
        Some((_, p)) if !shape.has_vertices() && layout.stride > 0 && !p.vertex_data.is_empty() => {
            p.vertex_data.len() / layout.stride
        }
        _ => shape.vertex_count as usize,
    };
    if positions.len() != count {
        return Err(NifError::Corrupt(format!(
            "{} positions for a shape of {count} vertices",
            positions.len()
        )));
    }
    if kind == "BSDynamicTriShape" {
        let size = positions.len() * 16;
        if shape.rest.len() < 4 + size {
            return Err(NifError::Corrupt(
                "a face part without its positions".into(),
            ));
        }
        for (v, p) in positions.iter().enumerate() {
            for (i, c) in p.iter().enumerate() {
                let at = 4 + v * 16 + i * 4;
                shape.rest[at..at + 4].copy_from_slice(&c.to_le_bytes());
            }
        }
    } else if shape.has_vertices() {
        for (v, p) in positions.iter().enumerate() {
            shape.set_position(&layout, v, *p);
        }
    }
    if let Some((at, mut partition)) = partition {
        if partition.vertex_data.len() == positions.len() * layout.stride
            && layout.has(attr::POSITION)
            && kind != "BSDynamicTriShape"
        {
            for (v, p) in positions.iter().enumerate() {
                geometry::set_position_in(&mut partition.vertex_data, &layout, v, *p);
            }
            nif.blocks[at].data = partition.write(stream);
        }
    }
    if rebound {
        let (center, radius) = bounding_sphere(positions);
        shape.bound_center = center;
        shape.bound_radius = radius;
    }
    nif.blocks[block].data = shape.write(stream);
    Ok(())
}

/// A sphere that holds every point: centred on the middle of their box.
pub fn bounding_sphere(points: &[[f32; 3]]) -> ([f32; 3], f32) {
    let Some(first) = points.first() else {
        return ([0.0; 3], 0.0);
    };
    let (mut lo, mut hi) = (*first, *first);
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let center = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let radius = points
        .iter()
        .map(|p| {
            ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2) + (p[2] - center[2]).powi(2))
                .sqrt()
        })
        .fold(0.0f32, f32::max);
    (center, radius)
}

/// A tangent and a bitangent for every vertex.
pub type TangentSpace = (Vec<[f32; 3]>, Vec<[f32; 3]>);

/// The tangent and bitangent each vertex of a shape stores, when it stores
/// them.
pub fn stored_tangents(nif: &Nif, block: usize) -> Option<TangentSpace> {
    let stream = nif.header.stream.version;
    if nif.kind_of(block) == "NiTriShape" {
        let (_, _, data) = le_shape(nif, block).ok()?;
        return (!data.tangents.is_empty()).then_some((data.tangents, data.bitangents));
    }
    let mut shape = TriShape::read(&nif.blocks[block].data, stream).ok()?;
    let layout = Layout::of(shape.vertex_desc, stream)?;
    if !shape.has_vertices() {
        shape.vertex_data = partition_of(nif, &shape)?.1.vertex_data;
    }
    let n = shape.vertex_data.len() / layout.stride.max(1);
    let both: Vec<_> = (0..n)
        .filter_map(|v| shape.tangent_space(&layout, v))
        .collect();
    (both.len() == n && n > 0).then(|| both.into_iter().unzip())
}

/// Give a shape new texture coordinates, in every place the file keeps
/// them. A shape that has none is left as it is.
pub fn set_uvs(nif: &mut Nif, block: usize, uvs: &[[f32; 2]]) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    if nif.kind_of(block) == "NiTriShape" {
        let (_, at, mut data) = le_shape(nif, block)?;
        // The first list of coordinates is the one a slider moves.
        if data.uvs.len() >= uvs.len() && !uvs.is_empty() {
            data.uvs[..uvs.len()].copy_from_slice(uvs);
            nif.blocks[at].data = data.write();
        }
        return Ok(());
    }
    let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let layout = Layout::of(shape.vertex_desc, stream)
        .ok_or_else(|| NifError::Unsupported(format!("vertex layout {:#x}", shape.vertex_desc)))?;
    if !layout.has(attr::UV) {
        return Ok(());
    }
    let apply = |target: &mut TriShape| {
        let n = target.vertex_data.len() / layout.stride.max(1);
        for (v, uv) in uvs.iter().enumerate().take(n) {
            target.set_uv(&layout, v, *uv);
        }
    };
    if let Some((at, mut p)) = partition_of(nif, &shape) {
        if !p.vertex_data.is_empty() {
            let mut view = shape.clone();
            view.vertex_data = std::mem::take(&mut p.vertex_data);
            apply(&mut view);
            p.vertex_data = view.vertex_data;
            nif.blocks[at].data = p.write(stream);
        }
    }
    if shape.has_vertices() {
        apply(&mut shape);
        nif.blocks[block].data = shape.write(stream);
    }
    Ok(())
}

/// The colour each vertex of the shape at `block` carries (red, green,
/// blue, and how solid), or nothing for a shape whose vertices carry none.
pub fn colors(nif: &Nif, block: usize) -> Result<Vec<[u8; 4]>, NifError> {
    let stream = nif.header.stream.version;
    if !geometry::is_tri_shape(nif.kind_of(block)) {
        return Ok(Vec::new());
    }
    let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let Some(layout) = Layout::of(shape.vertex_desc, stream).filter(|l| l.has(attr::COLOR)) else {
        return Ok(Vec::new());
    };
    if !shape.has_vertices() {
        match partition_of(nif, &shape) {
            Some((_, p)) => shape.vertex_data = p.vertex_data,
            None => return Ok(Vec::new()),
        }
    }
    let count = shape.vertex_data.len() / layout.stride.max(1);
    Ok((0..count).filter_map(|v| shape.color(&layout, v)).collect())
}

/// Give vertices of a shape colours, in every place the file keeps them:
/// each `(vertex, colour)`. A shape whose vertices carry no colour is
/// refused: giving it room for one would change every vertex of it.
pub fn set_colors(nif: &mut Nif, block: usize, colors: &[(u16, [u8; 4])]) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    if !geometry::is_tri_shape(nif.kind_of(block)) {
        return Err(NifError::Unsupported(
            "the shape's vertices carry no colour".into(),
        ));
    }
    let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let layout = Layout::of(shape.vertex_desc, stream)
        .filter(|l| l.has(attr::COLOR))
        .ok_or_else(|| NifError::Unsupported("the shape's vertices carry no colour".into()))?;
    // Where its vertices are in a skin, the skin must be its own.
    if shape.skin >= 0 && partition_of(nif, &shape).is_some() {
        crate::weights::owned(nif, block)?;
    }
    let apply = |target: &mut TriShape| {
        for (v, c) in colors {
            target.set_color(&layout, *v as usize, *c);
        }
    };
    if let Some((at, mut p)) = partition_of(nif, &shape) {
        if !p.vertex_data.is_empty() {
            let mut view = shape.clone();
            view.vertex_data = std::mem::take(&mut p.vertex_data);
            apply(&mut view);
            p.vertex_data = view.vertex_data;
            nif.blocks[at].data = p.write(stream);
        }
    }
    if shape.has_vertices() {
        apply(&mut shape);
        nif.blocks[block].data = shape.write(stream);
    }
    Ok(())
}

/// Give a shape new normals, and with them new tangents and bitangents
/// where it stores those, in every place the file keeps them. A shape that
/// stores no normals (a body lit from a normal map) is left as it is.
pub fn set_shading(
    nif: &mut Nif,
    block: usize,
    normals: &[[f32; 3]],
    tangents: &[[f32; 3]],
    bitangents: &[[f32; 3]],
) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    if nif.kind_of(block) == "NiTriShape" {
        let (_, at, mut data) = le_shape(nif, block)?;
        if data.normals.len() == normals.len() {
            data.normals = normals.to_vec();
        }
        if data.tangents.len() == tangents.len() && data.bitangents.len() == bitangents.len() {
            data.tangents = tangents.to_vec();
            data.bitangents = bitangents.to_vec();
        }
        nif.blocks[at].data = data.write();
        return Ok(());
    }
    let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let layout = Layout::of(shape.vertex_desc, stream)
        .ok_or_else(|| NifError::Unsupported(format!("vertex layout {:#x}", shape.vertex_desc)))?;
    if !layout.has(attr::NORMAL) {
        return Ok(());
    }
    let with_tangents = layout.has(attr::TANGENT) && tangents.len() == normals.len();
    let partition = partition_of(nif, &shape);
    let apply = |target: &mut TriShape| {
        let n = target.vertex_data.len() / layout.stride.max(1);
        for v in 0..n.min(normals.len()) {
            target.set_normal(&layout, v, normals[v]);
            if with_tangents {
                target.set_tangent_space(&layout, v, tangents[v], bitangents[v]);
            }
        }
    };
    if let Some((at, mut p)) = partition {
        if !p.vertex_data.is_empty() {
            let mut view = shape.clone();
            view.vertex_data = std::mem::take(&mut p.vertex_data);
            apply(&mut view);
            p.vertex_data = view.vertex_data;
            nif.blocks[at].data = p.write(stream);
        }
    }
    if shape.has_vertices() {
        apply(&mut shape);
        nif.blocks[block].data = shape.write(stream);
    }
    Ok(())
}
