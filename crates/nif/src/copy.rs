//! Copying a shape, with everything that is its own, into another file of
//! the same game or into its own: a body brought into an outfit, or a
//! shape made twice.
//!
//! A block names other blocks by where they are and its texts by where
//! they are in the file's table, so both are rewritten for the file the
//! copy lands in. A shape is only copied when that can be done for every
//! block it brings; one of a kind kept as bytes stops the copy.

use crate::edit::{is_plain_object, relinked};
use crate::geometry::{is_tri_shape, TriShape};
use crate::geometry_le::NiTriShape;
use crate::io::{Reader, Writer};
use crate::node::{is_node, Node};
use crate::shader::LightingShader;
use crate::{Block, Nif, NifError};
use std::collections::{BTreeSet, HashMap};

/// Extra data that is a name and then something with no text from the
/// file's table in it.
fn is_named_extra(kind: &str) -> bool {
    matches!(
        kind,
        "NiIntegerExtraData"
            | "NiIntegersExtraData"
            | "NiFloatExtraData"
            | "NiFloatsExtraData"
            | "NiBooleanExtraData"
            | "NiBinaryExtraData"
            | "NiStringsExtraData"
            | "BSXFlags"
    )
}

/// A block's bytes with every text it names passed through `map`.
fn restrung(
    kind: &str,
    data: &[u8],
    stream: u32,
    map: &mut dyn FnMut(i32) -> i32,
) -> Result<Vec<u8>, NifError> {
    let first = |data: &[u8], count: usize, map: &mut dyn FnMut(i32) -> i32| {
        let mut r = Reader::new(data);
        let mut w = Writer::default();
        for _ in 0..count {
            w.i32(map(r.i32()?));
        }
        w.bytes(r.take(r.left())?);
        Ok(w.out)
    };
    if is_tri_shape(kind) {
        let mut s = TriShape::read(data, stream)?;
        s.av.name = map(s.av.name);
        return Ok(s.write(stream));
    }
    if is_node(kind) {
        let mut n = Node::read(data, stream)?;
        n.av.name = map(n.av.name);
        return Ok(n.write(stream));
    }
    if is_plain_object(kind) || is_named_extra(kind) {
        return first(data, 1, map);
    }
    match kind {
        "NiTriShape" => {
            let mut s = NiTriShape::read(data, stream)?;
            s.av.name = map(s.av.name);
            Ok(s.write(stream))
        }
        "BSLightingShaderProperty" => {
            let mut s = LightingShader::read(data, stream)?;
            s.name = map(s.name);
            Ok(s.write())
        }
        "NiStringExtraData" => first(data, 2, map),
        "NiTriShapeData"
        | "NiSkinData"
        | "NiSkinPartition"
        | "NiSkinInstance"
        | "BSDismemberSkinInstance"
        | "BSShaderTextureSet" => Ok(data.to_vec()),
        _ => Err(NifError::Unsupported(format!(
            "the texts of a {kind} are not known"
        ))),
    }
}

/// Every node of a file by its name: the first, where two share one.
pub(crate) fn node_names(nif: &Nif) -> HashMap<Vec<u8>, usize> {
    let stream = nif.header.stream.version;
    let mut names = HashMap::new();
    for i in 0..nif.blocks.len() {
        if !is_node(nif.kind_of(i)) {
            continue;
        }
        if let Ok(n) = Node::read(&nif.blocks[i].data, stream) {
            if let Some(name) = nif.string(n.av.name) {
                names.entry(name.to_vec()).or_insert(i);
            }
        }
    }
    names
}

/// Copy the shape at `block` of `from` into `into`, with its skin, what it
/// is drawn with and its extra data. Its bones are the bones of the same
/// names in `into`; one that `into` does not have is brought along, bare.
/// The copy hangs from the first root of `into`. Returns where it is.
pub fn copy_shape(into: &mut Nif, from: &Nif, block: usize) -> Result<usize, NifError> {
    let stream = from.header.stream.version;
    if into.header.version != from.header.version
        || into.header.user_version != from.header.user_version
        || into.header.stream.version != stream
    {
        return Err(NifError::Unsupported(
            "the two meshes are made for different games".into(),
        ));
    }
    let kind = from.kind_of(block);
    if !(is_tri_shape(kind) || kind == "NiTriShape") {
        return Err(NifError::Unsupported(format!("a {kind} is not a shape")));
    }
    let root = into
        .roots
        .first()
        .and_then(|r| usize::try_from(*r).ok())
        .filter(|r| *r < into.blocks.len() && is_node(into.kind_of(*r)))
        .ok_or_else(|| NifError::Unsupported("the mesh has no node to hang a shape from".into()))?;

    // What the shape brings: everything it reaches, short of nodes, which
    // are the skeleton's.
    let mut brought: BTreeSet<usize> = BTreeSet::from([block]);
    let mut bones: BTreeSet<usize> = BTreeSet::new();
    let mut reach = vec![block];
    while let Some(at) = reach.pop() {
        let mut found = Vec::new();
        relinked(from.kind_of(at), &from.blocks[at].data, stream, &mut |l| {
            found.push(l as usize);
            Some(l)
        })?;
        for l in found {
            if l >= from.blocks.len() {
                continue;
            }
            if is_node(from.kind_of(l)) {
                bones.insert(l);
            } else if brought.insert(l) {
                reach.push(l);
            }
        }
    }
    // A kind with links that were not followed (a controller's, say) would
    // have stopped the walk above; one with texts not known stops it here,
    // before anything is changed.
    for at in &brought {
        restrung(
            from.kind_of(*at),
            &from.blocks[*at].data,
            stream,
            &mut |s| s,
        )?;
    }

    // Each bone, by name, in the file the copy lands in.
    let mut have = node_names(into);
    let from_root = from.roots.first().and_then(|r| usize::try_from(*r).ok());
    let mut place: HashMap<usize, i32> = HashMap::new();
    let mut hung = Vec::new();
    for bone in &bones {
        if Some(*bone) == from_root {
            place.insert(*bone, root as i32);
            continue;
        }
        let mut node = Node::read(&from.blocks[*bone].data, stream)?;
        let name = from.string(node.av.name).unwrap_or_default().to_vec();
        if let Some(at) = have.get(&name) {
            place.insert(*bone, *at as i32);
            continue;
        }
        // Bare: where it is, and nothing hanging from it.
        node.av.name = into.intern(&name);
        node.av.extra_data.clear();
        node.av.controller = -1;
        node.av.collision = -1;
        if let Some(p) = &mut node.av.properties {
            p.clear();
        }
        node.children.clear();
        if let Some(e) = &mut node.effects {
            e.clear();
        }
        let kind = into.kind_index(from.kind_of(*bone));
        into.blocks.push(Block {
            kind,
            data: node.write(stream),
        });
        let at = into.blocks.len() - 1;
        have.insert(name, at);
        place.insert(*bone, at as i32);
        hung.push(at as i32);
    }
    let base = into.blocks.len();
    for (i, at) in brought.iter().enumerate() {
        place.insert(*at, (base + i) as i32);
    }
    for at in &brought {
        let kind = from.kind_of(*at);
        let data = restrung(
            kind,
            &from.blocks[*at].data,
            stream,
            &mut |s| match from.string(s) {
                Some(text) => into.intern(text),
                None => -1,
            },
        )?;
        let data = relinked(kind, &data, stream, &mut |l| {
            place.get(&(l as usize)).copied()
        })?;
        let kind = into.kind_index(kind);
        into.blocks.push(Block { kind, data });
    }
    let copy = place[&block];
    hung.push(copy);
    let mut node = Node::read(&into.blocks[root].data, stream)?;
    node.children.extend(hung);
    into.blocks[root].data = node.write(stream);
    Ok(copy as usize)
}
