//! Moving every shape into one space: the skeleton's, standing as the body
//! does.
//!
//! A body model is stored where it stands (Skyrim's `malebody_1.nif` from 0
//! to about 120 up). A head part is stored about its bone: `femalehead.nif`'s
//! vertices lie round the origin and its `NPC Head [Head]` bind is the
//! identity, and the game puts it on the head through the skeleton. For one
//! mesh of every shape, each head part is moved to where its bone stands in
//! the race's skeleton, and its binds made again so that skinning still
//! takes each vertex where it was relative to its bones. After that a head
//! part's bind for a bone the body also follows is the body's
//! (`--example character` measures it).

use std::collections::HashMap;

use apoc_nif::copy::{composed, inverse, parents, placed};

use apoc_nif::geometry::TriShape;
use apoc_nif::node::{is_node, Node, Transform};
use apoc_nif::skin::{SkinData, SkinInstance};
use apoc_nif::{mesh, Nif};

/// The bone a head part is placed by, when it follows it.
const HEAD: &str = "NPC Head [Head]";

/// Each named bone of a skeleton and where it stands.
pub fn bone_places(skeleton: &Nif) -> HashMap<String, Transform> {
    let stream = skeleton.header.stream.version;
    let up = parents(skeleton);
    let mut out = HashMap::new();
    for at in 0..skeleton.blocks.len() {
        if !is_node(skeleton.kind_of(at)) {
            continue;
        }
        let Ok(n) = Node::read(&skeleton.blocks[at].data, stream) else {
            continue;
        };
        let Some(name) = skeleton.string(n.av.name) else {
            continue;
        };
        if let Ok(t) = placed(skeleton, at, &up) {
            out.entry(String::from_utf8_lossy(name).into_owned())
                .or_insert(t);
        }
    }
    out
}

fn turn(m: &[f32; 9], v: [f32; 3]) -> [f32; 3] {
    [
        m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
        m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
        m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
    ]
}

/// The skin data block of the shape at `block`.
fn skin_data(nif: &Nif, block: usize) -> Option<usize> {
    let stream = nif.header.stream.version;
    let shape = TriShape::read(&nif.blocks.get(block)?.data, stream).ok()?;
    let at = usize::try_from(shape.skin)
        .ok()
        .filter(|a| *a < nif.blocks.len())?;
    let kind = nif.kind_of(at);
    if kind != "NiSkinInstance" && kind != "BSDismemberSkinInstance" {
        return None;
    }
    let inst = SkinInstance::read(&nif.blocks[at].data, kind != "NiSkinInstance").ok()?;
    let d = usize::try_from(inst.data)
        .ok()
        .filter(|d| *d < nif.blocks.len())?;
    (nif.kind_of(d) == "NiSkinData").then_some(d)
}

/// Move the skinned shape at `block` to where its bones stand in `places`,
/// its vertices in the skeleton's space and its binds the bones' places
/// undone.
pub fn place_shape(
    nif: &mut Nif,
    block: usize,
    places: &HashMap<String, Transform>,
) -> Result<f32, String> {
    let bones = mesh::bone_names(nif, block);
    let at = skin_data(nif, block).ok_or("it has no skin to place it by")?;
    let mut data = SkinData::read(&nif.blocks[at].data).map_err(|e| e.to_string())?;
    if data.bones.len() != bones.len() || bones.is_empty() {
        return Err("its skin lists its bones twice over differently".into());
    }
    let by = bones.iter().position(|b| b == HEAD).unwrap_or(0);
    let world = places
        .get(&bones[by])
        .ok_or_else(|| format!("the skeleton has no {}", bones[by]))?;
    // Found from the files (`--example binds`): a bone takes a vertex to
    // where it stands by its place in the skeleton after its bind, W·T·v.
    // Hair, eyes and mouths keep vertices about the head bone and binds
    // whose W·T is the head's place (120 up); faces and bodies keep their
    // vertices 120 below that, with the shape's own transform 120 up and the
    // skin's overall transform 120 down, and W·T is that 120 up again. The
    // skin's overall transform is the shape's undone and takes no part.
    let m = composed(world, &data.bones[by].transform);
    // How far another bone would place it otherwise: what moving it by one
    // bone costs the vertices that follow the rest.
    let mut spread = 0.0f32;
    for (bone, name) in data.bones.iter().zip(&bones) {
        if let Some(w) = places.get(name) {
            let p = composed(w, &bone.transform);
            for c in 0..3 {
                spread = spread.max((p.translation[c] - m.translation[c]).abs());
            }
            for c in 0..9 {
                spread = spread.max((p.rotation[c] - m.rotation[c]).abs());
            }
        }
    }
    let shape = mesh::read(nif, block).map_err(|e| e.to_string())?;
    let moved: Vec<[f32; 3]> = shape.positions.iter().map(|p| m.apply(*p)).collect();
    // What turns with it: normals and the tangent space.
    let turned = (0..9).any(|i| (m.rotation[i] - Transform::IDENTITY.rotation[i]).abs() > 1e-6);
    let tangents = turned.then(|| mesh::stored_tangents(nif, block)).flatten();
    mesh::move_vertices(nif, block, &moved, true).map_err(|e| e.to_string())?;
    if turned && shape.normals.len() == moved.len() {
        let r = &m.rotation;
        let normals: Vec<_> = shape.normals.iter().map(|n| turn(r, *n)).collect();
        let (t, b) = tangents.unwrap_or_default();
        let t: Vec<_> = t.iter().map(|v| turn(r, *v)).collect();
        let b: Vec<_> = b.iter().map(|v| turn(r, *v)).collect();
        mesh::set_shading(nif, block, &normals, &t, &b).map_err(|e| e.to_string())?;
    }
    // Each bind made again for the moved vertices, so every bone takes
    // each vertex where it took it before. Where a shape's binds agree with
    // the skeleton (every one of Ulfric's within 7e-5) that is the bone's
    // place undone, as the body's are.
    let back = inverse(&m).ok_or("its bone scales it to nothing")?;
    for bone in &mut data.bones {
        bone.bound_center = m.apply(
            inverse(&bone.transform)
                .map(|t| t.apply(bone.bound_center))
                .unwrap_or(bone.bound_center),
        );
        bone.transform = composed(&bone.transform, &back);
        bone.bound_center = bone.transform.apply(bone.bound_center);
    }
    data.transform = Transform::IDENTITY;
    nif.blocks[at].data = data.write();
    let stream = nif.header.stream.version;
    let mut shape = TriShape::read(&nif.blocks[block].data, stream).map_err(|e| e.to_string())?;
    shape.av.transform = Transform::IDENTITY;
    nif.blocks[block].data = shape.write(stream);
    Ok(spread)
}

/// Stand every bare bone hung from the root (as a copied shape brings
/// them) where the skeleton has it.
pub fn place_bones(nif: &mut Nif, places: &HashMap<String, Transform>) {
    let stream = nif.header.stream.version;
    let Some(root) = nif.roots.first().and_then(|r| usize::try_from(*r).ok()) else {
        return;
    };
    let Ok(top) = Node::read(&nif.blocks[root].data, stream) else {
        return;
    };
    for c in top.children.iter().filter_map(|c| usize::try_from(*c).ok()) {
        if c >= nif.blocks.len() || !is_node(nif.kind_of(c)) {
            continue;
        }
        let Ok(mut n) = Node::read(&nif.blocks[c].data, stream) else {
            continue;
        };
        if !n.children.is_empty() {
            continue;
        }
        let name = nif
            .string(n.av.name)
            .map(|s| String::from_utf8_lossy(s).into_owned());
        if let Some(t) = name.and_then(|s| places.get(&s)) {
            n.av.transform = *t;
            nif.blocks[c].data = n.write(stream);
        }
    }
}
