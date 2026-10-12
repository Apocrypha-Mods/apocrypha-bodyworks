//! Which bones each vertex of a shape follows, and how much: read as one
//! list, and written back to every place a file keeps it.
//!
//! A skinned shape of Special Edition keeps its weights three times: with
//! each vertex (bone numbers into the skin's list of bones), by bone in the
//! skin's data, and by partition (bone numbers into the partition's own
//! list). The rules each copy keeps were counted over real files
//! (`examples/skinfit.rs`): a vertex's weights are heaviest first, a slot
//! not used is bone 0 with a weight of nothing, and a partition lists its
//! bones in rising order.

use crate::copy::node_names;
use crate::geometry::{
    f32_to_half, half_to_f32, is_tri_shape, set_skin_weights_in, skin_weights_in, Layout, TriShape,
};
use crate::mesh;
use crate::node::{is_node, Node};
use crate::skin::{BoneData, SkinData, SkinInstance, SkinPartition};
use crate::{Nif, NifError};
use std::collections::BTreeSet;

/// How many bones a vertex can follow.
pub const MOST: usize = 4;

/// A shape's skin: its bones by name, and for each vertex up to four of
/// them (by place in `bones`) with how much of each. A slot not used has a
/// weight of nothing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skin {
    pub bones: Vec<String>,
    pub vertices: Vec<[(u16, f32); MOST]>,
}

struct Blocks {
    instance: usize,
    dismember: bool,
    data: usize,
    partition: usize,
}

fn unsupported<T>(why: &str) -> Result<T, NifError> {
    Err(NifError::Unsupported(why.into()))
}

fn blocks(nif: &Nif, block: usize) -> Result<Blocks, NifError> {
    let stream = nif.header.stream.version;
    if !is_tri_shape(nif.kind_of(block)) {
        return unsupported("bone weights are read from a shape of Special Edition's kind");
    }
    let shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let of = |link: i32, kinds: &[&str]| {
        usize::try_from(link)
            .ok()
            .filter(|i| *i < nif.blocks.len() && kinds.contains(&nif.kind_of(*i)))
    };
    let Some(instance) = of(shape.skin, &["NiSkinInstance", "BSDismemberSkinInstance"]) else {
        return unsupported("the shape is not skinned");
    };
    let dismember = nif.kind_of(instance) != "NiSkinInstance";
    let read = SkinInstance::read(&nif.blocks[instance].data, dismember)?;
    match (
        of(read.data, &["NiSkinData"]),
        of(read.partition, &["NiSkinPartition"]),
    ) {
        (Some(data), Some(partition)) => Ok(Blocks {
            instance,
            dismember,
            data,
            partition,
        }),
        _ => unsupported("the shape's skin has no data or no partitions"),
    }
}

/// The skin of the shape at `block`.
pub fn read(nif: &Nif, block: usize) -> Result<Skin, NifError> {
    let stream = nif.header.stream.version;
    let at = blocks(nif, block)?;
    let partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    let Some(layout) = partition.vertex_desc.and_then(|d| Layout::of(d, stream)) else {
        return unsupported("the skin keeps no vertices this can read");
    };
    let count = partition.vertex_data.len() / layout.stride.max(1);
    let mut vertices = Vec::with_capacity(count);
    for v in 0..count {
        let Some((weights, bones)) = skin_weights_in(&partition.vertex_data, &layout, v) else {
            return unsupported("the shape's vertices carry no bone weights");
        };
        let mut slots = [(0u16, 0.0f32); MOST];
        for k in 0..MOST {
            if weights[k] > 0.0 {
                slots[k] = (bones[k] as u16, weights[k]);
            }
        }
        vertices.push(slots);
    }
    Ok(Skin {
        bones: mesh::bone_names(nif, block),
        vertices,
    })
}

/// A vertex's weights as a file keeps them: the heaviest four, heaviest
/// first, adding up to one, each a number a half-size float can hold.
/// Nothing at all stays nothing.
pub fn settled(weights: &[(u16, f32)]) -> [(u16, f32); MOST] {
    let mut all: Vec<(u16, f32)> = weights
        .iter()
        .copied()
        .filter(|(_, w)| w.is_finite() && *w > 0.0)
        .collect();
    // The same bone twice is one bone.
    all.sort_by_key(|x| x.0);
    all.dedup_by(|b, a| {
        let same = a.0 == b.0;
        if same {
            a.1 += b.1;
        }
        same
    });
    all.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    all.truncate(MOST);
    let sum: f32 = all.iter().map(|x| x.1).sum();
    let mut out = [(0u16, 0.0f32); MOST];
    if sum <= 0.0 {
        return out;
    }
    for (slot, (bone, w)) in out.iter_mut().zip(&all) {
        let w = half_to_f32(f32_to_half(w / sum));
        if w > 0.0 {
            *slot = (*bone, w);
        }
    }
    out
}

/// Give the shape at `block` this skin, in every place the file keeps one.
/// Each bone is the node of that name, which the file must have. A bone the
/// shape did not follow before is placed as `like` places it: another shape
/// of the file that follows it and stands where this one does (the body the
/// weights were taken from).
pub fn write(
    nif: &mut Nif,
    block: usize,
    skin: &Skin,
    like: Option<usize>,
) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    let at = blocks(nif, block)?;
    if skin.bones.len() > 255 {
        return unsupported("a shape follows 255 bones at most");
    }
    let mut instance = SkinInstance::read(&nif.blocks[at.instance].data, at.dismember)?;
    let mut data = SkinData::read(&nif.blocks[at.data].data)?;
    let mut partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    let Some(layout) = partition.vertex_desc.and_then(|d| Layout::of(d, stream)) else {
        return unsupported("the skin keeps no vertices this can read");
    };
    let count = partition.vertex_data.len() / layout.stride.max(1);
    if skin.vertices.len() != count {
        return unsupported("the skin is for another number of vertices");
    }
    let vertices: Vec<[(u16, f32); MOST]> = skin.vertices.iter().map(|v| settled(v)).collect();
    if vertices
        .iter()
        .flatten()
        .any(|(b, w)| *w > 0.0 && *b as usize >= skin.bones.len())
    {
        return unsupported("a vertex follows a bone the skin does not list");
    }

    // Where each bone is: as this shape had it, or as the other has it.
    let names = node_names(nif);
    let had = mesh::bone_names(nif, block);
    let other = match like {
        Some(shape) => {
            let theirs = blocks(nif, shape)?;
            Some((
                mesh::bone_names(nif, shape),
                SkinData::read(&nif.blocks[theirs.data].data)?,
            ))
        }
        None => None,
    };
    let positions = mesh::read(nif, block)?.positions;
    let mut nodes = Vec::with_capacity(skin.bones.len());
    let mut bones = Vec::with_capacity(skin.bones.len());
    for (i, name) in skin.bones.iter().enumerate() {
        let node = names
            .get(name.as_bytes())
            .copied()
            .filter(|n| is_node(nif.kind_of(*n)))
            .ok_or_else(|| NifError::Unsupported(format!("the mesh has no bone named {name}")))?;
        nodes.push(node as i32);
        let weights: Vec<(u16, f32)> = if data.has_weights != 0 {
            vertices
                .iter()
                .enumerate()
                .filter_map(|(v, slots)| {
                    slots
                        .iter()
                        .find(|(b, w)| *b as usize == i && *w > 0.0)
                        .map(|(_, w)| (v as u16, *w))
                })
                .collect()
        } else {
            Vec::new()
        };
        let old = had
            .iter()
            .position(|n| n == name)
            .and_then(|k| data.bones.get(k));
        let theirs = other.as_ref().and_then(|(names, data)| {
            names
                .iter()
                .position(|n| n == name)
                .and_then(|k| data.bones.get(k))
        });
        let Some(place) = old.or(theirs) else {
            return Err(NifError::Unsupported(format!(
                "nothing here says where the bone {name} is"
            )));
        };
        // A bone it followed keeps its sphere, as a build leaves every
        // bone's; a new one gets the sphere round what it now moves.
        let (center, radius) = match old {
            Some(old) => (old.bound_center, old.bound_radius),
            None => {
                let moved: Vec<[f32; 3]> = weights
                    .iter()
                    .filter_map(|(v, _)| positions.get(*v as usize))
                    .map(|p| place.transform.apply(*p))
                    .collect();
                if data.has_weights != 0 && !moved.is_empty() {
                    mesh::bounding_sphere(&moved)
                } else {
                    (place.bound_center, place.bound_radius)
                }
            }
        };
        bones.push(BoneData::new(place.transform, center, radius, weights));
    }
    // Checked before anything is changed: every node there is a node.
    for node in &nodes {
        Node::read(&nif.blocks[*node as usize].data, stream)?;
    }

    for (v, slots) in vertices.iter().enumerate() {
        let weights = [slots[0].1, slots[1].1, slots[2].1, slots[3].1];
        let numbers = [0, 1, 2, 3].map(|k| {
            if slots[k].1 > 0.0 {
                slots[k].0 as u8
            } else {
                0
            }
        });
        if !set_skin_weights_in(&mut partition.vertex_data, &layout, v, weights, numbers) {
            return unsupported("the shape's vertices carry no bone weights");
        }
    }
    for p in &mut partition.partitions {
        let Some(map) = p.vertex_map.clone() else {
            return unsupported("a partition does not say which vertices are its own");
        };
        let used: BTreeSet<u16> = map
            .iter()
            .filter_map(|v| vertices.get(*v as usize))
            .flatten()
            .filter(|(_, w)| *w > 0.0)
            .map(|(b, _)| *b)
            .collect();
        p.bones = used.into_iter().collect();
        if p.weights_per_vertex as usize != MOST {
            return unsupported("a partition keeps other than four weights a vertex");
        }
        let mut weights = Vec::with_capacity(map.len() * MOST);
        let mut numbers = Vec::with_capacity(map.len() * MOST);
        for v in &map {
            let slots = vertices.get(*v as usize).copied().unwrap_or_default();
            for (bone, w) in slots {
                weights.push(w);
                numbers.push(if w > 0.0 {
                    p.bones.iter().position(|b| *b == bone).unwrap_or(0) as u8
                } else {
                    0
                });
            }
        }
        if p.vertex_weights.is_some() {
            p.vertex_weights = Some(weights);
        }
        if p.bone_indices.is_some() {
            p.bone_indices = Some(numbers);
        }
    }
    data.bones = bones;
    instance.bones = nodes;
    nif.blocks[at.instance].data = instance.write();
    nif.blocks[at.data].data = data.write();
    nif.blocks[at.partition].data = partition.write(stream);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bone_has_its_twin_across_the_body() {
        assert_eq!(across("NPC L Thigh [LThg]"), "NPC R Thigh [RThg]");
        assert_eq!(
            across("NPC R UpperarmTwist1 [RUt1]"),
            "NPC L UpperarmTwist1 [LUt1]"
        );
        assert_eq!(across("NPC Spine2 [Spn2]"), "NPC Spine2 [Spn2]");
        assert_eq!(across("LeftArm"), "RightArm");
        assert_eq!(across(&across("NPC L Breast")), "NPC L Breast");
    }

    #[test]
    fn weights_settle_to_the_heaviest_four_adding_up_to_one() {
        let got = settled(&[(3, 0.1), (1, 0.4), (2, 0.2), (7, 0.05), (9, 0.2), (1, 0.05)]);
        assert_eq!(got.map(|x| x.0), [1, 2, 9, 3]);
        assert!(got.windows(2).all(|x| x[0].1 >= x[1].1));
        assert!((got.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 0.002);
        // Each is a number a half-size float holds.
        assert!(got.iter().all(|(_, w)| half_to_f32(f32_to_half(*w)) == *w));
        assert_eq!(settled(&[]), [(0, 0.0); MOST]);
        assert_eq!(settled(&[(4, 0.0), (5, f32::NAN)]), [(0, 0.0); MOST]);
        assert_eq!(settled(&[(6, 3.0)])[0], (6, 1.0));
    }
}

/// One part of a skinned shape: the triangles the game shows or hides
/// together, as a piece of armour covers part of a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub flags: u16,
    /// Which part of the body it is, by the game's number for it.
    pub part: u16,
    pub triangles: Vec<[u16; 3]>,
}

/// The parts of the shape at `block`, in the file's order. None for a
/// shape whose skin does not divide it.
pub fn parts(nif: &Nif, block: usize) -> Result<Vec<Part>, NifError> {
    let stream = nif.header.stream.version;
    let at = blocks(nif, block)?;
    let instance = SkinInstance::read(&nif.blocks[at.instance].data, at.dismember)?;
    let partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    let Some(named) = instance.body_parts else {
        return Ok(Vec::new());
    };
    Ok(named
        .iter()
        .zip(&partition.partitions)
        .map(|((flags, part), p)| Part {
            flags: *flags,
            part: *part,
            triangles: p.true_triangles.clone().unwrap_or_default(),
        })
        .collect())
}

/// Say which part of the body the `index`th part of a shape is.
pub fn set_part(nif: &mut Nif, block: usize, index: usize, part: u16) -> Result<(), NifError> {
    let at = blocks(nif, block)?;
    let mut instance = SkinInstance::read(&nif.blocks[at.instance].data, at.dismember)?;
    match instance.body_parts.as_mut().and_then(|p| p.get_mut(index)) {
        Some(slot) => slot.1 = part,
        None => return unsupported("the shape has no such part"),
    }
    nif.blocks[at.instance].data = instance.write();
    Ok(())
}

/// Where each bone of a shape is, in the shape's own space, in the order
/// its skin lists them.
pub fn bone_places(nif: &Nif, block: usize) -> Result<Vec<[f32; 3]>, NifError> {
    let at = blocks(nif, block)?;
    let data = SkinData::read(&nif.blocks[at.data].data)?;
    Ok(data
        .bones
        .iter()
        .map(|b| {
            // The bone's transform takes the shape's space into the bone's:
            // the bone itself is what it takes to nothing.
            let (t, m, s) = (
                b.transform.translation,
                b.transform.rotation,
                b.transform.scale,
            );
            let s = if s == 0.0 { 1.0 } else { s };
            [
                -(m[0] * t[0] + m[3] * t[1] + m[6] * t[2]) / s,
                -(m[1] * t[0] + m[4] * t[1] + m[7] * t[2]) / s,
                -(m[2] * t[0] + m[5] * t[1] + m[8] * t[2]) / s,
            ]
        })
        .collect())
}

/// Give a partition new triangles; [`settle`] then fills its copy of the
/// weights.
fn retriangled(p: &mut crate::skin::Partition, triangles: Vec<[u16; 3]>) {
    p.retriangle(triangles);
}

/// Write a shape's skin back as it reads: after its partitions' vertices
/// have changed, what each partition keeps of the weights is made again.
fn settle(nif: &mut Nif, block: usize) -> Result<(), NifError> {
    let skin = read(nif, block)?;
    write(nif, block, &skin, None)
}

/// Move triangles of a shape into its `to`th part, or with `None` into a
/// new part numbered `new_part`: every triangle whose three vertices are
/// all marked in `vertices`. A part is never left with nothing. Returns how
/// many triangles moved.
pub fn move_to_part(
    nif: &mut Nif,
    block: usize,
    vertices: &[bool],
    to: Option<usize>,
    new_part: u16,
) -> Result<usize, NifError> {
    let stream = nif.header.stream.version;
    let at = blocks(nif, block)?;
    let mut instance = SkinInstance::read(&nif.blocks[at.instance].data, at.dismember)?;
    let mut partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    let Some(named) = instance.body_parts.as_mut() else {
        return unsupported("the shape's skin does not divide it into parts");
    };
    if named.len() != partition.partitions.len() || to.is_some_and(|t| t >= named.len()) {
        return unsupported("the shape has no such part");
    }
    let marked = |t: &[u16; 3]| {
        t.iter()
            .all(|v| vertices.get(*v as usize).copied().unwrap_or(false))
    };
    let mut moved = Vec::new();
    for (i, p) in partition.partitions.iter_mut().enumerate() {
        if Some(i) == to {
            continue;
        }
        let Some(all) = p.true_triangles.clone() else {
            return unsupported("a part keeps its triangles in a way this cannot change");
        };
        let (going, staying): (Vec<_>, Vec<_>) = all.into_iter().partition(marked);
        if going.is_empty() {
            continue;
        }
        if staying.is_empty() {
            return unsupported("that would leave a part with nothing in it");
        }
        moved.extend(going);
        retriangled(p, staying);
    }
    if moved.is_empty() {
        return unsupported("no whole triangle is marked that is not in that part already");
    }
    let count = moved.len();
    match to {
        Some(t) => {
            let p = &mut partition.partitions[t];
            let mut all = p.true_triangles.clone().unwrap_or_default();
            all.extend(moved);
            retriangled(p, all);
        }
        None => {
            let mut p = partition.partitions[0].clone();
            retriangled(&mut p, moved);
            partition.partitions.push(p);
            let flags = named[0].0;
            named.push((flags, new_part));
        }
    }
    nif.blocks[at.instance].data = instance.write();
    nif.blocks[at.partition].data = partition.write(stream);
    settle(nif, block)?;
    Ok(count)
}

/// Turn every triangle of a shape to face the other way: what a shape
/// needs when it has been mirrored, or it would be seen from inside.
pub fn flip_facing(nif: &mut Nif, block: usize) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    if !is_tri_shape(nif.kind_of(block)) {
        return unsupported("triangles are turned on a shape of Special Edition's kind");
    }
    let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
    for t in &mut shape.triangles {
        t.swap(1, 2);
    }
    let skinned = shape.skin >= 0;
    nif.blocks[block].data = shape.write(stream);
    if !skinned {
        return Ok(());
    }
    let at = blocks(nif, block)?;
    let mut partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    for p in &mut partition.partitions {
        let Some(mut all) = p.true_triangles.clone() else {
            return unsupported("a part keeps its triangles in a way this cannot change");
        };
        for t in &mut all {
            t.swap(1, 2);
        }
        retriangled(p, all);
    }
    nif.blocks[at.partition].data = partition.write(stream);
    settle(nif, block)
}

/// The bone on the other side of the body from one named as the games name
/// theirs (`NPC L Thigh [LThg]` and `NPC R Thigh [RThg]`), or the same
/// name for a bone of the middle.
pub fn across(bone: &str) -> String {
    let swap = |text: &str, a: &str, b: &str| -> Option<String> {
        text.contains(a).then(|| text.replace(a, b))
    };
    for (l, r) in [(" L ", " R "), ("Left", "Right"), ("left", "right")] {
        if let Some(found) = swap(bone, l, r).or_else(|| swap(bone, r, l)) {
            // The short name in brackets follows the long one.
            let from_left = bone.contains(l);
            return if from_left {
                found.replace("[L", "[R")
            } else {
                found.replace("[R", "[L")
            };
        }
    }
    bone.to_string()
}
