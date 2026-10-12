//! How a file's shapes are skinned: bones, partitions, and where each
//! vertex's bone numbers point.
//!
//!     cargo run -p apoc-nif --example skin -- <file.nif>...

use apoc_nif::geometry::{is_tri_shape, Layout, TriShape};
use apoc_nif::skin::{SkinData, SkinInstance, SkinPartition};
use apoc_nif::Nif;

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            println!("{path}: unreadable");
            continue;
        };
        let stream = nif.header.stream.version;
        println!("{path} (stream {stream})");
        for at in 0..nif.blocks.len() {
            if !is_tri_shape(nif.kind_of(at)) {
                continue;
            }
            let Ok(shape) = TriShape::read(&nif.blocks[at].data, stream) else {
                continue;
            };
            let Ok(skin) = usize::try_from(shape.skin) else {
                continue;
            };
            let kind = nif.kind_of(skin).to_string();
            let Ok(instance) = SkinInstance::read(&nif.blocks[skin].data, kind != "NiSkinInstance")
            else {
                println!("  shape {at}: skin is a {kind}");
                continue;
            };
            let data = usize::try_from(instance.data)
                .ok()
                .and_then(|d| SkinData::read(&nif.blocks[d].data).ok());
            let part = usize::try_from(instance.partition)
                .ok()
                .and_then(|p| SkinPartition::read(&nif.blocks[p].data, stream).ok());
            println!(
                "  shape {at}: {} vertices, own vertex bytes {}, {} bones, skin data bones {:?} has_weights {:?} weights in data {:?}",
                shape.vertex_count,
                shape.vertex_data.len(),
                instance.bones.len(),
                data.as_ref().map(|d| d.bones.len()),
                data.as_ref().map(|d| d.has_weights),
                data.as_ref().map(|d| d.bones.iter().map(|b| b.weights.len()).sum::<usize>()),
            );
            if let Some(d) = &data {
                for (i, b) in d.bones.iter().take(3).enumerate() {
                    println!(
                        "    bone {i}: centre {:?} radius {} transform {:?}",
                        b.bound_center, b.bound_radius, b.transform
                    );
                }
                println!("    skin transform {:?}", d.transform);
            }
            let Some(part) = part else { continue };
            println!(
                "    partition block: vertex bytes {}, size {:?}, same as shape's: {}",
                part.vertex_data.len(),
                part.vertex_size,
                part.vertex_data == shape.vertex_data
            );
            let layout = part.vertex_desc.and_then(|d| Layout::of(d, stream));
            for (i, p) in part.partitions.iter().enumerate() {
                let mut most = 0u8;
                let mut used = std::collections::BTreeSet::new();
                if let (Some(layout), Some(tris)) = (&layout, &p.true_triangles) {
                    for v in tris.iter().flatten() {
                        if let Some((w, b)) = apoc_nif::geometry::skin_weights_in(
                            &part.vertex_data,
                            layout,
                            *v as usize,
                        ) {
                            for k in 0..4 {
                                if w[k] > 0.0 {
                                    most = most.max(b[k]);
                                    used.insert(b[k]);
                                }
                            }
                        }
                    }
                }
                println!(
                    "    part {i}: {} bones {:?}..., weights/vertex {}, map {:?}, weights {:?}, indices {:?}, highest bone number used {most}, distinct used {}",
                    p.bones.len(),
                    &p.bones[..p.bones.len().min(8)],
                    p.weights_per_vertex,
                    p.vertex_map.as_ref().map(Vec::len),
                    p.vertex_weights.as_ref().map(Vec::len),
                    p.bone_indices.as_ref().map(Vec::len),
                    used.len(),
                );
                let _ = used;
            }
        }
    }
}
