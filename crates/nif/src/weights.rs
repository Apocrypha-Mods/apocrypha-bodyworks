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
use crate::node::{is_node, Node, Transform};
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

/// Whether the shape at `block` has its skin to itself. A skin two shapes
/// share is not one shape's to change: what is written of it for one would
/// be wrong for the other. Reading one is no harm, and is not stopped.
pub fn owned(nif: &Nif, block: usize) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    let at = blocks(nif, block)?;
    let mine = SkinInstance::read(&nif.blocks[at.instance].data, at.dismember)?;
    for other in 0..nif.blocks.len() {
        let kind = nif.kind_of(other);
        let shares = if other == block {
            false
        } else if is_tri_shape(kind) {
            // Its link to a skin follows what every scene object starts
            // with, and is read without the vertices after it.
            TriShape::skin_of(&nif.blocks[other].data, stream) == Some(at.instance as i32)
        } else if kind == "NiTriShape" {
            crate::geometry_le::NiTriShape::read(&nif.blocks[other].data, stream)
                .is_ok_and(|s| s.skin == at.instance as i32)
        } else if other != at.instance
            && (kind == "NiSkinInstance" || kind == "BSDismemberSkinInstance")
        {
            SkinInstance::read(&nif.blocks[other].data, kind != "NiSkinInstance")
                .is_ok_and(|theirs| theirs.data == mine.data || theirs.partition == mine.partition)
        } else {
            false
        };
        if shares {
            return unsupported("the shape shares its skin with another shape");
        }
    }
    Ok(())
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
            if weights[k] > 0.0 && weights[k].is_finite() {
                slots[k] = (bones[k] as u16, weights[k]);
            }
        }
        vertices.push(slots);
    }
    let bones = mesh::bone_names(nif, block);
    // A vertex following a bone the skin does not list is a broken file,
    // said here once and not found out by whatever reads the skin next.
    if vertices
        .iter()
        .flatten()
        .any(|(b, w)| *w > 0.0 && *b as usize >= bones.len())
    {
        return Err(NifError::Corrupt(
            "a vertex follows a bone the skin does not list".into(),
        ));
    }
    Ok(Skin { bones, vertices })
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
    owned(nif, block)?;
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
    // Each bone's vertices, gathered in one pass over the vertices.
    let mut by_bone: Vec<Vec<(u16, f32)>> = vec![Vec::new(); skin.bones.len()];
    if data.has_weights != 0 {
        for (v, slots) in vertices.iter().enumerate() {
            for (bone, w) in slots {
                if *w > 0.0 {
                    by_bone[*bone as usize].push((v as u16, *w));
                }
            }
        }
    }
    let mut nodes = Vec::with_capacity(skin.bones.len());
    let mut bones = Vec::with_capacity(skin.bones.len());
    for (i, name) in skin.bones.iter().enumerate() {
        let node = names
            .get(name.as_bytes())
            .copied()
            .filter(|n| is_node(nif.kind_of(*n)))
            .ok_or_else(|| NifError::Unsupported(format!("the mesh has no bone named {name}")))?;
        nodes.push(node as i32);
        let weights = std::mem::take(&mut by_bone[i]);
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
        // Failing both, from where the bone's node is: a skin places a
        // bone as the undoing of that (so 26,114 of 26,204 real bones are
        // placed, `examples/bindfit.rs`), for a skin that is itself placed
        // nowhere in particular.
        let from_node =
            (old.is_none() && theirs.is_none() && data.transform == Transform::IDENTITY)
                .then(|| crate::copy::placed(nif, node, &crate::copy::parents(nif)).ok())
                .flatten()
                .and_then(|p| crate::copy::inverse(&p))
                .map(|transform| BoneData::new(transform, [0.0; 3], 0.0, Vec::new()));
        let Some(place) = old.or(theirs).or(from_node.as_ref()) else {
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
    // The shape's own copy of its vertices, where it keeps one as well.
    let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
    if shape.has_vertices() {
        let own = Layout::of(shape.vertex_desc, stream)
            .filter(|l| shape.vertex_data.len() / l.stride.max(1) == vertices.len());
        let Some(own) = own else {
            return unsupported("the shape and its skin disagree about its vertices");
        };
        for (v, slots) in vertices.iter().enumerate() {
            let weights = [slots[0].1, slots[1].1, slots[2].1, slots[3].1];
            let numbers = [0, 1, 2, 3].map(|k| {
                if slots[k].1 > 0.0 {
                    slots[k].0 as u8
                } else {
                    0
                }
            });
            set_skin_weights_in(&mut shape.vertex_data, &own, v, weights, numbers);
        }
        nif.blocks[block].data = shape.write(stream);
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
    owned(nif, block)?;
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
fn retriangled(p: &mut crate::skin::Partition, triangles: Vec<[u16; 3]>) -> Result<(), NifError> {
    p.retriangle(triangles)
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
    owned(nif, block)?;
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
        retriangled(p, staying)?;
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
            retriangled(p, all)?;
        }
        None => {
            let mut p = partition.partitions[0].clone();
            retriangled(&mut p, moved)?;
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
    if skinned {
        owned(nif, block)?;
    }
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
        retriangled(p, all)?;
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

/// Take the `index`th part out of a shape, its triangles going to the
/// `into`th. The parts after it move up by one.
pub fn remove_part(nif: &mut Nif, block: usize, index: usize, into: usize) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    owned(nif, block)?;
    let at = blocks(nif, block)?;
    let mut instance = SkinInstance::read(&nif.blocks[at.instance].data, at.dismember)?;
    let mut partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    let Some(named) = instance.body_parts.as_mut() else {
        return unsupported("the shape's skin does not divide it into parts");
    };
    let count = partition.partitions.len();
    if named.len() != count || index >= count || into >= count || index == into {
        return unsupported("the shape has no such two parts");
    }
    let (Some(going), Some(mut all)) = (
        partition.partitions[index].true_triangles.clone(),
        partition.partitions[into].true_triangles.clone(),
    ) else {
        return unsupported("a part keeps its triangles in a way this cannot change");
    };
    all.extend(going);
    retriangled(&mut partition.partitions[into], all)?;
    partition.partitions.remove(index);
    named.remove(index);
    nif.blocks[at.instance].data = instance.write();
    nif.blocks[at.partition].data = partition.write(stream);
    settle(nif, block)
}

/// Put the shape at `from` into the shape at `into`: its vertices after
/// the other's, its triangles as parts of their own, its bones added to
/// the other's. The two must keep the same things for each vertex, and
/// both keep their vertices in their skins, as skinned shapes of Special
/// Edition do. The shape at `from` is left as it was, for the caller to
/// take out. Returns how many vertices `into` had before: where the
/// other's now begin.
pub fn merge(nif: &mut Nif, into: usize, from: usize) -> Result<usize, NifError> {
    let stream = nif.header.stream.version;
    owned(nif, into)?;
    owned(nif, from)?;
    let (a, b) = (blocks(nif, into)?, blocks(nif, from)?);
    for shape in [into, from] {
        if TriShape::read(&nif.blocks[shape].data, stream)?.has_vertices() {
            return unsupported("a shape that keeps its vertices outside its skin is not merged");
        }
    }
    let mut pa = SkinPartition::read(&nif.blocks[a.partition].data, stream)?;
    let pb = SkinPartition::read(&nif.blocks[b.partition].data, stream)?;
    let same = pa.vertex_desc.is_some()
        && pa.vertex_desc == pb.vertex_desc
        && pa.vertex_size == pb.vertex_size;
    let Some(layout) = pa
        .vertex_desc
        .and_then(|d| Layout::of(d, stream))
        .filter(|_| same)
    else {
        return unsupported(
            "the two shapes keep different things for each vertex, and cannot be one shape",
        );
    };
    let stride = layout.stride.max(1);
    let (na, nb) = (pa.vertex_data.len() / stride, pb.vertex_data.len() / stride);
    if na + nb > u16::MAX as usize {
        return unsupported("together they have more vertices than one shape can number");
    }
    let (sa, sb) = (read(nif, into)?, read(nif, from)?);
    let mut ia = SkinInstance::read(&nif.blocks[a.instance].data, a.dismember)?;
    let ib = SkinInstance::read(&nif.blocks[b.instance].data, b.dismember)?;

    pa.vertex_data.extend_from_slice(&pb.vertex_data);
    for (i, p) in pb.partitions.iter().enumerate() {
        let Some(triangles) = &p.true_triangles else {
            return unsupported("a part keeps its triangles in a way this cannot change");
        };
        if triangles.iter().flatten().any(|v| *v as usize >= nb) {
            return Err(NifError::Corrupt(
                "a triangle names a vertex the shape does not have".into(),
            ));
        }
        // Within what one shape can number: the two counts were added up above.
        let moved = triangles
            .iter()
            .map(|t| t.map(|v| (v as usize + na) as u16))
            .collect();
        let mut q = p.clone();
        q.retriangle(moved)?;
        pa.partitions.push(q);
        if let Some(parts) = &mut ia.body_parts {
            let theirs = ib.body_parts.as_ref().and_then(|x| x.get(i)).copied();
            let Some(part) = theirs.or_else(|| parts.first().copied()) else {
                return unsupported("the shape has no part to put the other's triangles in");
            };
            parts.push(part);
        }
    }
    nif.blocks[a.partition].data = pa.write(stream);
    nif.blocks[a.instance].data = ia.write();

    // One list of bones, the first shape's and then any only the second has.
    let mut bones = sa.bones.clone();
    let place: Vec<u16> = sb
        .bones
        .iter()
        .map(|name| {
            bones.iter().position(|b| b == name).unwrap_or_else(|| {
                bones.push(name.clone());
                bones.len() - 1
            }) as u16
        })
        .collect();
    let mut vertices = sa.vertices;
    vertices.extend(sb.vertices.iter().map(|slots| {
        slots.map(|(bone, w)| {
            if w > 0.0 {
                (place[bone as usize], w)
            } else {
                (0, 0.0)
            }
        })
    }));
    write(nif, into, &Skin { bones, vertices }, Some(from))?;
    Ok(na)
}

/// Give a skinned shape other vertices and triangles altogether: its
/// vertices as packed bytes in the shape's own layout, its triangles part
/// by part (as many parts as it had, or fewer, and at least one), and the
/// skin its new vertices have. For a shape that keeps its vertices in its
/// skin, as skinned shapes of Special Edition do.
pub fn rebuild(
    nif: &mut Nif,
    block: usize,
    vertex_data: Vec<u8>,
    parts: Vec<Vec<[u16; 3]>>,
    skin: &Skin,
    like: Option<usize>,
) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    owned(nif, block)?;
    let at = blocks(nif, block)?;
    if TriShape::read(&nif.blocks[block].data, stream)?.has_vertices() {
        return unsupported("a shape that keeps its vertices outside its skin is not rebuilt");
    }
    let mut instance = SkinInstance::read(&nif.blocks[at.instance].data, at.dismember)?;
    let mut partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    let Some(layout) = partition.vertex_desc.and_then(|d| Layout::of(d, stream)) else {
        return unsupported("the skin keeps no vertices this can read");
    };
    let stride = layout.stride.max(1);
    let count = vertex_data.len() / stride;
    if !vertex_data.len().is_multiple_of(stride) || count == 0 || count > u16::MAX as usize {
        return unsupported("that is not a number of vertices one shape can hold");
    }
    if parts.is_empty() || parts.len() > partition.partitions.len() {
        return unsupported("the shape has not that many parts");
    }
    if parts
        .iter()
        .flatten()
        .flatten()
        .any(|v| *v as usize >= count)
    {
        return unsupported("a triangle names a vertex the shape would not have");
    }
    if skin.vertices.len() != count {
        return unsupported("the skin is for another number of vertices");
    }
    if instance
        .body_parts
        .as_ref()
        .is_some_and(|named| named.len() != partition.partitions.len())
    {
        return unsupported("the shape's parts and their names are out of step");
    }
    partition.vertex_data = vertex_data;
    partition.partitions.truncate(parts.len());
    for (p, triangles) in partition.partitions.iter_mut().zip(parts) {
        p.retriangle(triangles)?;
    }
    if let Some(named) = &mut instance.body_parts {
        named.truncate(partition.partitions.len());
    }
    // All of it worked out before anything is put in the file: a skin that
    // cannot be written leaves the shape as it was.
    let (was_instance, was_partition) = (
        nif.blocks[at.instance].data.clone(),
        nif.blocks[at.partition].data.clone(),
    );
    nif.blocks[at.instance].data = instance.write();
    nif.blocks[at.partition].data = partition.write(stream);
    let done = write(nif, block, skin, like);
    if done.is_err() {
        nif.blocks[at.instance].data = was_instance;
        nif.blocks[at.partition].data = was_partition;
    }
    done
}

/// A shape's vertices as packed bytes, their layout, and its triangles part
/// by part.
pub type Packed = (Vec<u8>, Layout, Vec<Vec<[u16; 3]>>);

/// A shape's vertices as packed bytes, its layout, and its triangles part
/// by part: what [`rebuild`] takes back.
pub fn packed(nif: &Nif, block: usize) -> Result<Packed, NifError> {
    let stream = nif.header.stream.version;
    let at = blocks(nif, block)?;
    let partition = SkinPartition::read(&nif.blocks[at.partition].data, stream)?;
    let Some(layout) = partition.vertex_desc.and_then(|d| Layout::of(d, stream)) else {
        return unsupported("the skin keeps no vertices this can read");
    };
    // Whole vertices, no more of them than can be numbered, and no
    // triangle naming one that is not there: checked once here, for all
    // that goes on to index by them.
    let stride = layout.stride.max(1);
    let count = partition.vertex_data.len() / stride;
    if !partition.vertex_data.len().is_multiple_of(stride) || count > u16::MAX as usize {
        return Err(NifError::Corrupt(
            "the skin's vertices are not a whole number a shape can hold".into(),
        ));
    }
    let mut parts = Vec::with_capacity(partition.partitions.len());
    for p in &partition.partitions {
        match &p.true_triangles {
            Some(t) if t.iter().flatten().all(|v| (*v as usize) < count) => parts.push(t.clone()),
            Some(_) => {
                return Err(NifError::Corrupt(
                    "a triangle names a vertex the shape does not have".into(),
                ))
            }
            None => return unsupported("a part keeps its triangles in a way this cannot change"),
        }
    }
    Ok((partition.vertex_data, layout, parts))
}

/// Refine a shape: every triangle made four, a new vertex in the middle of
/// each edge. The vertices it had keep their numbers; each new one is
/// halfway between two of them in place, in texture and in the bones it
/// follows, and is otherwise as the first of the two. Returns, for each new
/// vertex in order, the two it lies between.
pub fn refine(nif: &mut Nif, block: usize) -> Result<Vec<(u16, u16)>, NifError> {
    let stream = nif.header.stream.version;
    let (mut bytes, layout, parts) = packed(nif, block)?;
    let stride = layout.stride.max(1);
    let had = bytes.len() / stride;
    let skin = read(nif, block)?;
    if skin.vertices.len() != had {
        return unsupported("the shape and its skin disagree about its vertices");
    }
    let shape = TriShape::read(&nif.blocks[block].data, stream)?;
    let mut between: Vec<(u16, u16)> = Vec::new();
    let mut middle: std::collections::HashMap<(u16, u16), u16> = std::collections::HashMap::new();
    let mut refined = Vec::with_capacity(parts.len());
    for triangles in &parts {
        let mut out = Vec::with_capacity(triangles.len() * 4);
        for t in triangles {
            let mut mid = [0u16; 3];
            for (k, slot) in mid.iter_mut().enumerate() {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let edge = (a.min(b), a.max(b));
                *slot = match middle.get(&edge) {
                    Some(m) => *m,
                    None => {
                        let at = had + between.len();
                        if at > u16::MAX as usize {
                            return unsupported(
                                "refined, the shape would have more vertices than can be numbered",
                            );
                        }
                        between.push(edge);
                        middle.insert(edge, at as u16);
                        at as u16
                    }
                };
            }
            out.extend([
                [t[0], mid[0], mid[2]],
                [mid[0], t[1], mid[1]],
                [mid[2], mid[1], t[2]],
                [mid[0], mid[1], mid[2]],
            ]);
        }
        if out.len() > u16::MAX as usize {
            return unsupported(
                "refined, a part of the shape would have more triangles than it can count",
            );
        }
        refined.push(out);
    }
    // Each new vertex: the first's bytes, then its own place and texture.
    let mut view = shape.clone();
    view.vertex_data = std::mem::take(&mut bytes);
    view.vertex_data.reserve(between.len() * stride);
    for (a, _) in &between {
        let from = *a as usize * stride;
        let row = view.vertex_data[from..from + stride].to_vec();
        view.vertex_data.extend(row);
    }
    let halfway = |x: [f32; 3], y: [f32; 3]| {
        [
            (x[0] + y[0]) / 2.0,
            (x[1] + y[1]) / 2.0,
            (x[2] + y[2]) / 2.0,
        ]
    };
    for (i, (a, b)) in between.iter().enumerate() {
        let v = had + i;
        if let (Some(pa), Some(pb)) = (
            view.position(&layout, *a as usize),
            view.position(&layout, *b as usize),
        ) {
            view.set_position(&layout, v, halfway(pa, pb));
        }
        if let (Some(ua), Some(ub)) = (view.uv(&layout, *a as usize), view.uv(&layout, *b as usize))
        {
            view.set_uv(&layout, v, [(ua[0] + ub[0]) / 2.0, (ua[1] + ub[1]) / 2.0]);
        }
    }
    let mut vertices = skin.vertices.clone();
    for (a, b) in &between {
        let both: Vec<(u16, f32)> = skin.vertices[*a as usize]
            .iter()
            .chain(&skin.vertices[*b as usize])
            .filter(|x| x.1 > 0.0)
            .map(|x| (x.0, x.1 / 2.0))
            .collect();
        vertices.push(settled(&both));
    }
    let skin = Skin {
        bones: skin.bones,
        vertices,
    };
    rebuild(nif, block, view.vertex_data, refined, &skin, None)?;
    Ok(between)
}

/// Weld a shape: vertices at one place, with one place in the texture,
/// become one vertex (the first of them), and a triangle left with two
/// corners at one vertex goes. Returns where each old vertex now is.
pub fn weld(nif: &mut Nif, block: usize) -> Result<Vec<u16>, NifError> {
    let stream = nif.header.stream.version;
    let (bytes, layout, parts) = packed(nif, block)?;
    let stride = layout.stride.max(1);
    let count = bytes.len() / stride;
    let skin = read(nif, block)?;
    if skin.vertices.len() != count {
        return unsupported("the shape and its skin disagree about its vertices");
    }
    let mut view = TriShape::read(&nif.blocks[block].data, stream)?;
    view.vertex_data = bytes;
    // Told apart to a two-thousandth of a unit in place, and a
    // four-thousandth of the texture.
    let mut first: std::collections::HashMap<[i32; 5], u16> = std::collections::HashMap::new();
    let mut now = Vec::with_capacity(count);
    let mut kept: Vec<usize> = Vec::new();
    for v in 0..count {
        let p = view.position(&layout, v).unwrap_or([0.0; 3]);
        let uv = view.uv(&layout, v).unwrap_or([0.0; 2]);
        let key = [
            (p[0] * 2000.0).round() as i32,
            (p[1] * 2000.0).round() as i32,
            (p[2] * 2000.0).round() as i32,
            (uv[0] * 4096.0).round() as i32,
            (uv[1] * 4096.0).round() as i32,
        ];
        let at = *first.entry(key).or_insert_with(|| {
            kept.push(v);
            (kept.len() - 1) as u16
        });
        now.push(at);
    }
    if kept.len() == count {
        return Ok(now);
    }
    let mut data = Vec::with_capacity(kept.len() * stride);
    let mut vertices = Vec::with_capacity(kept.len());
    for v in &kept {
        data.extend_from_slice(&view.vertex_data[v * stride..(v + 1) * stride]);
        vertices.push(skin.vertices[*v]);
    }
    let parts: Vec<Vec<[u16; 3]>> = parts
        .iter()
        .map(|triangles| {
            triangles
                .iter()
                .map(|t| t.map(|v| now[v as usize]))
                .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
                .collect()
        })
        .collect();
    if parts.iter().any(Vec::is_empty) {
        return unsupported("welded, a part of the shape would have no triangle left");
    }
    let skin = Skin {
        bones: skin.bones,
        vertices,
    };
    rebuild(nif, block, data, parts, &skin, None)?;
    Ok(now)
}
