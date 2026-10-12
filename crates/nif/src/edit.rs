//! Changing what a file is made of: taking vertices out of a shape, and
//! taking a shape out of the file.
//!
//! Blocks name each other by position, so taking one out means rewriting
//! every link in every block that is left. That is only done when every
//! block's links are known ([`links_known`]); a file with a kind of block
//! this crate keeps as bytes is left as it is rather than guessed at.

use crate::geometry::{is_tri_shape, TriShape};
use crate::geometry_le::{NiTriShape, TriShapeData};
use crate::io::{Reader, Writer};
use crate::node::{is_node, read_refs, write_refs, Node};
use crate::shader::LightingShader;
use crate::skin::{SkinData, SkinInstance, SkinPartition};
use crate::{Nif, NifError};
use std::collections::BTreeSet;

/// Kinds of block that link to no other.
fn has_no_links(kind: &str) -> bool {
    matches!(
        kind,
        "NiSkinData"
            | "NiSkinPartition"
            | "BSShaderTextureSet"
            | "NiStringExtraData"
            | "NiStringsExtraData"
            | "NiIntegerExtraData"
            | "NiIntegersExtraData"
            | "NiFloatExtraData"
            | "NiFloatsExtraData"
            | "NiBooleanExtraData"
            | "NiBinaryExtraData"
            | "NiVectorExtraData"
            | "NiColorExtraData"
            | "NiTextKeyExtraData"
            | "BSXFlags"
            | "BSInvMarker"
            | "BSBound"
            | "BSBehaviorGraphExtraData"
            | "BSFurnitureMarker"
            | "BSFurnitureMarkerNode"
            | "BSDecalPlacementVectorExtraData"
            | "BSClothExtraData"
            | "BSConnectPoint::Parents"
            | "BSConnectPoint::Children"
            | "BSSkin::BoneData"
    )
}

/// Kinds that start as every named object does (name, extra data,
/// controller) and link to nothing after that.
pub(crate) fn is_plain_object(kind: &str) -> bool {
    matches!(
        kind,
        "NiAlphaProperty"
            | "BSEffectShaderProperty"
            | "NiMaterialProperty"
            | "NiSpecularProperty"
            | "NiStencilProperty"
            | "NiZBufferProperty"
            | "NiVertexColorProperty"
            | "NiShadeProperty"
            | "NiWireframeProperty"
            | "NiDitherProperty"
    )
}

/// Whether every link in a block of this kind can be found.
pub fn links_known(kind: &str) -> bool {
    has_no_links(kind)
        || is_plain_object(kind)
        || (is_node(kind)
            && !matches!(
                kind,
                "BSTreeNode" | "BSMultiBoundNode" | "BSMasterParticleSystem" | "NiLODNode"
            ))
        || is_tri_shape(kind)
        || matches!(
            kind,
            "NiTriShape"
                | "NiTriShapeData"
                | "NiSkinInstance"
                | "BSDismemberSkinInstance"
                | "BSLightingShaderProperty"
        )
}

/// What becomes of a link: `Some` is where it points now, `None` that what
/// it pointed at is gone.
type Remap<'a> = &'a mut dyn FnMut(i32) -> Option<i32>;

fn one(link: &mut i32, map: Remap) {
    if *link >= 0 {
        *link = map(*link).unwrap_or(-1);
    }
}

/// A list drops a link to something gone; an empty slot stays an empty slot.
fn many(links: &mut Vec<i32>, map: Remap) {
    *links = links
        .iter()
        .filter_map(|l| if *l < 0 { Some(*l) } else { map(*l) })
        .collect();
}

/// A block of bones keeps its length: a bone that is gone is an empty slot.
fn fixed(links: &mut [i32], map: Remap) {
    for l in links {
        one(l, map);
    }
}

fn av(av: &mut crate::node::AvObject, map: Remap) {
    many(&mut av.extra_data, map);
    one(&mut av.controller, map);
    if let Some(p) = &mut av.properties {
        many(p, map);
    }
    one(&mut av.collision, map);
}

/// A block's bytes with every link passed through `map`.
pub(crate) fn relinked(
    kind: &str,
    data: &[u8],
    stream: u32,
    map: Remap,
) -> Result<Vec<u8>, NifError> {
    if has_no_links(kind) {
        return Ok(data.to_vec());
    }
    if is_plain_object(kind) {
        let mut r = Reader::new(data);
        let name = r.i32()?;
        let mut extra = read_refs(&mut r)?;
        let mut controller = r.i32()?;
        let rest = r.take(r.left())?;
        many(&mut extra, map);
        one(&mut controller, map);
        let mut w = Writer::default();
        w.i32(name);
        write_refs(&mut w, &extra);
        w.i32(controller);
        w.bytes(rest);
        return Ok(w.out);
    }
    if is_node(kind) {
        // A node of a kind that links to more than a node does, after what
        // is read of it here, is not one whose links are all known.
        if matches!(
            kind,
            "BSTreeNode" | "BSMultiBoundNode" | "BSMasterParticleSystem" | "NiLODNode"
        ) {
            return Err(NifError::Unsupported(format!(
                "the links of a {kind} are not known"
            )));
        }
        let mut n = Node::read(data, stream)?;
        av(&mut n.av, map);
        many(&mut n.children, map);
        if let Some(e) = &mut n.effects {
            many(e, map);
        }
        return Ok(n.write(stream));
    }
    if is_tri_shape(kind) {
        let mut s = TriShape::read(data, stream)?;
        av(&mut s.av, map);
        one(&mut s.skin, map);
        one(&mut s.shader, map);
        one(&mut s.alpha, map);
        return Ok(s.write(stream));
    }
    match kind {
        "NiTriShape" => {
            let mut s = NiTriShape::read(data, stream)?;
            av(&mut s.av, map);
            one(&mut s.data, map);
            one(&mut s.skin, map);
            if let Some(l) = &mut s.shader {
                one(l, map);
            }
            if let Some(l) = &mut s.alpha {
                one(l, map);
            }
            Ok(s.write(stream))
        }
        "NiTriShapeData" => {
            let mut d = TriShapeData::read(data, stream)?;
            one(&mut d.additional_data, map);
            Ok(d.write())
        }
        "NiSkinInstance" | "BSDismemberSkinInstance" => {
            let mut s = SkinInstance::read(data, kind != "NiSkinInstance")?;
            one(&mut s.data, map);
            one(&mut s.partition, map);
            one(&mut s.skeleton_root, map);
            fixed(&mut s.bones, map);
            Ok(s.write())
        }
        "BSLightingShaderProperty" => {
            let mut s = LightingShader::read(data, stream)?;
            many(&mut s.extra_data, map);
            one(&mut s.controller, map);
            one(&mut s.texture_set, map);
            Ok(s.write())
        }
        _ => Err(NifError::Unsupported(format!(
            "the links of a {kind} are not known"
        ))),
    }
}

/// Every block each block links to.
fn links_of(nif: &Nif) -> Result<Vec<Vec<usize>>, NifError> {
    let stream = nif.header.stream.version;
    (0..nif.blocks.len())
        .map(|i| {
            let mut found = Vec::new();
            relinked(nif.kind_of(i), &nif.blocks[i].data, stream, &mut |l| {
                found.push(l as usize);
                Some(l)
            })?;
            Ok(found)
        })
        .collect()
}

/// Take blocks out of the file. Every link to one of them is dropped, and
/// every other link follows the block it names to its new place.
pub fn remove_blocks(nif: &mut Nif, gone: &BTreeSet<usize>) -> Result<(), NifError> {
    if gone.is_empty() {
        return Ok(());
    }
    let stream = nif.header.stream.version;
    let mut now = Vec::with_capacity(nif.blocks.len());
    let mut next = 0i32;
    for i in 0..nif.blocks.len() {
        now.push((!gone.contains(&i)).then(|| {
            next += 1;
            next - 1
        }));
    }
    let mut map = |l: i32| now.get(l as usize).copied().flatten();
    // All of them first, so a block that cannot be rewritten leaves the
    // file untouched.
    let mut rewritten = Vec::with_capacity(nif.blocks.len());
    for i in 0..nif.blocks.len() {
        if gone.contains(&i) {
            continue;
        }
        let data = relinked(nif.kind_of(i), &nif.blocks[i].data, stream, &mut map)?;
        rewritten.push(crate::Block {
            kind: nif.blocks[i].kind,
            data,
        });
    }
    nif.blocks = rewritten;
    many(&mut nif.roots, &mut map);
    Ok(())
}

/// Put a new block in the file at `at`, moving the blocks from there on up
/// by one and every link to them with them. When some block's links are not
/// known the new block goes at the end instead, where nothing has to move.
/// Returns where it went.
pub fn insert_block(nif: &mut Nif, at: usize, kind: &str, data: Vec<u8>) -> usize {
    let stream = nif.header.stream.version;
    let kind = nif.kind_index(kind);
    let at = at.min(nif.blocks.len());
    let mut map = |l: i32| Some(if l as usize >= at { l + 1 } else { l });
    let moved: Result<Vec<Vec<u8>>, NifError> = (0..nif.blocks.len())
        .map(|i| relinked(nif.kind_of(i), &nif.blocks[i].data, stream, &mut map))
        .collect();
    match moved {
        Ok(moved) => {
            for (block, data) in nif.blocks.iter_mut().zip(moved) {
                block.data = data;
            }
            many(&mut nif.roots, &mut map);
            nif.blocks.insert(at, crate::Block { kind, data });
            at
        }
        Err(_) => {
            nif.blocks.push(crate::Block { kind, data });
            nif.blocks.len() - 1
        }
    }
}

/// Give the shape at `block` a named piece of text (a `NiStringExtraData`),
/// in a new block placed after it.
pub fn add_text(nif: &mut Nif, block: usize, name: &str, value: &str) -> Result<(), NifError> {
    let stream = nif.header.stream.version;
    let kind = nif.kind_of(block).to_string();
    let mut w = Writer::default();
    w.i32(nif.intern(name.as_bytes()));
    w.i32(nif.intern(value.as_bytes()));
    let at = insert_block(nif, block + 1, "NiStringExtraData", w.out) as i32;
    if kind == "NiTriShape" {
        let mut shape = NiTriShape::read(&nif.blocks[block].data, stream)?;
        shape.av.extra_data.push(at);
        nif.blocks[block].data = shape.write(stream);
    } else if is_tri_shape(&kind) {
        let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
        shape.av.extra_data.push(at);
        nif.blocks[block].data = shape.write(stream);
    } else {
        return Err(NifError::Unsupported(format!("a {kind} is not a shape")));
    }
    Ok(())
}

/// Take shapes out of the file, each with everything that was only its
/// own: its skin, what it is drawn with, its extra data. Something another
/// block still uses stays.
pub fn remove_shapes(nif: &mut Nif, shapes: &[usize]) -> Result<(), NifError> {
    if shapes.is_empty() {
        return Ok(());
    }
    let links = links_of(nif)?;
    let mut gone: BTreeSet<usize> = shapes.iter().copied().collect();
    // What the shapes reach, as far as it goes, short of nodes: a skin's
    // bones are the skeleton's, not the shape's.
    let mut reach: Vec<usize> = shapes.to_vec();
    while let Some(at) = reach.pop() {
        for l in links.get(at).into_iter().flatten() {
            if *l < nif.blocks.len() && !is_node(nif.kind_of(*l)) && gone.insert(*l) {
                reach.push(*l);
            }
        }
    }
    // Anything among those which a block that is staying links to is
    // shared, and stays; then so does what only it kept in.
    loop {
        let shared: Vec<usize> = gone
            .iter()
            .copied()
            .filter(|g| {
                !shapes.contains(g)
                    && links
                        .iter()
                        .enumerate()
                        .any(|(from, to)| !gone.contains(&from) && to.contains(g))
            })
            .collect();
        if shared.is_empty() {
            break;
        }
        for s in shared {
            gone.remove(&s);
        }
    }
    remove_blocks(nif, &gone)
}

/// Take vertices out of the shape at `block`: `gone[v]` says vertex `v`
/// goes. Every triangle with one of them goes too, and so does a part of
/// the skin left with no triangle. Returns how many triangles the shape
/// has left.
pub fn remove_vertices(nif: &mut Nif, block: usize, gone: &[bool]) -> Result<usize, NifError> {
    let stream = nif.header.stream.version;
    let kind = nif.kind_of(block).to_string();
    // A vertex is numbered in sixteen bits: more than that many is not a
    // shape these files can hold, and would be counted wrongly here.
    if gone.len() > u16::MAX as usize + 1 {
        return Err(NifError::Unsupported(
            "a shape of more vertices than can be numbered".into(),
        ));
    }
    let mut next = 0u16;
    let now: Vec<Option<u16>> = gone
        .iter()
        .map(|g| {
            (!g).then(|| {
                // The last of 65,536 kept is numbered 65,535, and there is
                // no next after it to count to.
                let at = next;
                next = next.wrapping_add(1);
                at
            })
        })
        .collect();
    let block_of = |nif: &Nif, link: i32, kinds: &[&str]| {
        usize::try_from(link)
            .ok()
            .filter(|i| kinds.contains(&nif.kind_of(*i)))
    };
    let instances = ["NiSkinInstance", "BSDismemberSkinInstance"];
    let (skin, mut left) = if kind == "NiTriShape" {
        let shape = NiTriShape::read(&nif.blocks[block].data, stream)?;
        let at = block_of(nif, shape.data, &["NiTriShapeData"])
            .ok_or_else(|| NifError::Unsupported("a shape without triangle data".into()))?;
        let mut data = TriShapeData::read(&nif.blocks[at].data, stream)?;
        if data.positions.len() != gone.len() {
            return Err(NifError::Corrupt(format!(
                "{} marks for a shape of {} vertices",
                gone.len(),
                data.positions.len()
            )));
        }
        data.retain(&now);
        let left = data.triangles.len();
        nif.blocks[at].data = data.write();
        (shape.skin, left)
    } else if is_tri_shape(&kind) {
        let mut shape = TriShape::read(&nif.blocks[block].data, stream)?;
        shape.retain(&now, kind == "BSDynamicTriShape");
        let left = shape.triangles.len();
        nif.blocks[block].data = shape.write(stream);
        (shape.skin, left)
    } else {
        return Err(NifError::Unsupported(format!("a {kind} is not a shape")));
    };
    let Some(at) = block_of(nif, skin, &instances) else {
        return Ok(left);
    };
    let dismember = nif.kind_of(at) != "NiSkinInstance";
    let mut instance = SkinInstance::read(&nif.blocks[at].data, dismember)?;
    if let Some(d) = block_of(nif, instance.data, &["NiSkinData"]) {
        let mut data = SkinData::read(&nif.blocks[d].data)?;
        data.retain(&now);
        nif.blocks[d].data = data.write();
    }
    if let Some(p) = block_of(nif, instance.partition, &["NiSkinPartition"]) {
        let mut partition = SkinPartition::read(&nif.blocks[p].data, stream)?;
        let kept = partition.retain(&now)?;
        nif.blocks[p].data = partition.write(stream);
        // A skinned shape of Special Edition has its triangles here alone.
        let here: usize = partition
            .partitions
            .iter()
            .map(|q| q.triangle_count())
            .sum();
        left = left.max(here);
        if let Some(parts) = &mut instance.body_parts {
            if parts.len() == kept.len() {
                let mut flags = kept.iter();
                parts.retain(|_| *flags.next().unwrap_or(&true));
                nif.blocks[at].data = instance.write();
            }
        }
    }
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{AvObject, Transform};

    /// A partition of the older kind: its own vertices, mapped to the
    /// shape's, with triangles over its own.
    fn partition(map: &[u16], triangles: &[[u16; 3]]) -> SkinPartition {
        let mut w = Writer::default();
        w.u32(1);
        w.u16(map.len() as u16);
        w.u16(triangles.len() as u16);
        w.u16(2);
        w.u16(0);
        w.u16(1);
        w.u16(0);
        w.u16(1);
        w.u8(1);
        for v in map {
            w.u16(*v);
        }
        w.u8(1);
        for v in map {
            w.f32(*v as f32);
        }
        w.u8(1);
        for t in triangles {
            for i in t {
                w.u16(*i);
            }
        }
        w.u8(1);
        for v in map {
            w.u8(*v as u8);
        }
        w.u16(0);
        SkinPartition::read(&w.out, 83).unwrap()
    }

    fn now(gone: &[bool]) -> Vec<Option<u16>> {
        let mut next = 0;
        gone.iter()
            .map(|g| {
                (!g).then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect()
    }

    #[test]
    fn a_partition_keeps_what_is_left_in_step() {
        // Shape vertices 10..15 are this partition's 0..5.
        let mut p = partition(&[10, 11, 12, 13, 14], &[[0, 1, 2], [2, 3, 4]]);
        let mut gone = vec![false; 16];
        gone[2] = true; // not this partition's: only moves the rest down
        gone[13] = true;
        let kept = p.retain(&now(&gone)).unwrap();
        assert_eq!(kept, vec![true]);
        let q = &p.partitions[0];
        assert_eq!(q.vertex_count(), 4);
        // 10, 11, 12 and 14, each one lower for the vertex taken before them.
        assert_eq!(q.vertex_map.as_deref(), Some(&[9, 10, 11, 12][..]));
        // Each vertex still has its own weight and bone.
        assert_eq!(
            q.vertex_weights.as_deref(),
            Some(&[10.0, 11.0, 12.0, 14.0][..])
        );
        assert_eq!(q.bone_indices.as_deref(), Some(&[10, 11, 12, 14][..]));
        // The triangle that used the vertex is gone; the other is as it was.
        assert_eq!(q.triangles.as_deref(), Some(&[[0, 1, 2]][..]));
        // And it reads back as what was written.
        assert_eq!(SkinPartition::read(&p.write(83), 83).unwrap(), p);
    }

    #[test]
    fn a_partition_with_no_triangle_left_goes() {
        let mut p = partition(&[0, 1, 2], &[[0, 1, 2]]);
        let kept = p.retain(&now(&[false, true, false])).unwrap();
        assert_eq!(kept, vec![false]);
        assert!(p.partitions.is_empty());
    }

    fn node(children: &[i32], extra: &[i32]) -> Vec<u8> {
        Node {
            av: AvObject {
                name: 0,
                extra_data: extra.to_vec(),
                controller: -1,
                flags: 14,
                transform: Transform::IDENTITY,
                properties: None,
                collision: -1,
            },
            children: children.to_vec(),
            effects: Some(Vec::new()),
            rest: Vec::new(),
        }
        .write(100)
    }

    /// Root → [child node, extra data, another node], by position.
    fn scene() -> Nif {
        let mut nif = Nif::read(&crate::tests::sample(100)).unwrap();
        let node_kind = nif.kind_index("NiNode");
        let extra_kind = nif.kind_index("NiStringExtraData");
        nif.blocks = vec![
            crate::Block {
                kind: node_kind,
                data: node(&[1, -1, 3], &[2]),
            },
            crate::Block {
                kind: node_kind,
                data: node(&[], &[2]),
            },
            crate::Block {
                kind: extra_kind,
                data: vec![0; 8],
            },
            crate::Block {
                kind: node_kind,
                data: node(&[], &[]),
            },
        ];
        nif.roots = vec![0];
        nif
    }

    #[test]
    fn links_follow_blocks_to_their_new_places() {
        let mut nif = scene();
        remove_blocks(&mut nif, &BTreeSet::from([1])).unwrap();
        assert_eq!(nif.blocks.len(), 3);
        let root = Node::read(&nif.blocks[0].data, 100).unwrap();
        // The link to the block that went is dropped, the empty slot stays,
        // and the others name the same blocks where they now are.
        assert_eq!(root.children, vec![-1, 2]);
        assert_eq!(root.av.extra_data, vec![1]);
        assert_eq!(nif.roots, vec![0]);
        // The whole file still reads.
        assert_eq!(Nif::read(&nif.write()).unwrap(), nif);
    }

    #[test]
    fn a_new_block_moves_the_links_past_it_and_not_the_others() {
        let mut nif = scene();
        let at = insert_block(&mut nif, 1, "NiStringExtraData", vec![0; 8]);
        assert_eq!(at, 1);
        let root = Node::read(&nif.blocks[0].data, 100).unwrap();
        assert_eq!(root.children, vec![2, -1, 4]);
        assert_eq!(root.av.extra_data, vec![3]);
        assert_eq!(Nif::read(&nif.write()).unwrap(), nif);
        // With a block whose links are not known, nothing moves: it goes last.
        let mut nif = scene();
        let odd = nif.kind_index("bhkCollisionObject");
        nif.blocks.push(crate::Block {
            kind: odd,
            data: vec![0; 12],
        });
        assert_eq!(
            insert_block(&mut nif, 1, "NiStringExtraData", vec![0; 8]),
            5
        );
        assert_eq!(
            Node::read(&nif.blocks[0].data, 100).unwrap().children,
            vec![1, -1, 3]
        );
    }

    #[test]
    fn a_file_with_a_block_whose_links_are_not_known_is_left_alone() {
        let mut nif = scene();
        let odd = nif.kind_index("bhkCollisionObject");
        nif.blocks.push(crate::Block {
            kind: odd,
            data: vec![0; 12],
        });
        let before = nif.clone();
        assert!(remove_blocks(&mut nif, &BTreeSet::from([1])).is_err());
        assert_eq!(nif, before);
        assert!(!links_known("bhkCollisionObject"));
        assert!(links_known("BSDismemberSkinInstance"));
    }

    #[test]
    fn what_another_block_still_uses_stays_when_a_shape_goes() {
        // Block 1 stands in for a shape here: it and block 0 both use the
        // extra data at 2.
        let mut nif = scene();
        remove_shapes(&mut nif, &[1]).unwrap();
        assert_eq!(nif.blocks.len(), 3);
        assert_eq!(nif.kind_of(1), "NiStringExtraData");
        // With nothing else using it, it goes with the shape.
        let mut nif = scene();
        nif.blocks[0].data = node(&[1, -1, 3], &[]);
        remove_shapes(&mut nif, &[1]).unwrap();
        assert_eq!(nif.blocks.len(), 2);
        assert_eq!(nif.kind_of(1), "NiNode");
    }
}
