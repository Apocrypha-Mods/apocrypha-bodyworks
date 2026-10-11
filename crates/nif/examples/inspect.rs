//! Say how a mesh file's shapes and their skinning are laid out.
//!
//!     cargo run --release -p apoc-nif --example inspect -- <file.nif>...

use apoc_nif::geometry::{is_tri_shape, TriShape};
use apoc_nif::skin::{SkinData, SkinInstance, SkinPartition};
use apoc_nif::Nif;

fn main() {
    for path in std::env::args().skip(1) {
        let nif = Nif::read(&std::fs::read(&path).unwrap()).unwrap();
        let stream = nif.header.stream.version;
        println!("{path}: {} blocks", nif.blocks.len());
        for i in 0..nif.blocks.len() {
            let kind = nif.kind_of(i).to_string();
            if !is_tri_shape(&kind) {
                continue;
            }
            let s = TriShape::read(&nif.blocks[i].data, stream).unwrap();
            let name = nif
                .string(s.av.name)
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .unwrap_or_default();
            println!(
                "  [{i}] {kind} {name:?}: {} vertices ({} bytes), {} triangles, desc {:#x}, skin {} shader {} alpha {}",
                s.vertex_count,
                s.vertex_data.len(),
                s.triangles.len(),
                s.vertex_desc,
                s.skin,
                s.shader,
                s.alpha
            );
            let Ok(at) = usize::try_from(s.skin) else {
                continue;
            };
            let inst =
                SkinInstance::read(&nif.blocks[at].data, nif.kind_of(at) != "NiSkinInstance")
                    .unwrap();
            println!(
                "      {}: {} bones, parts {:?}",
                nif.kind_of(at),
                inst.bones.len(),
                inst.body_parts
            );
            if let Ok(d) = usize::try_from(inst.data) {
                let data = SkinData::read(&nif.blocks[d].data).unwrap();
                println!(
                    "      NiSkinData: has_weights {}, {} bones, weights per bone {:?}",
                    data.has_weights,
                    data.bones.len(),
                    data.bones
                        .iter()
                        .take(6)
                        .map(|b| b.weights.len())
                        .collect::<Vec<_>>()
                );
            }
            if let Ok(p) = usize::try_from(inst.partition) {
                let part = SkinPartition::read(&nif.blocks[p].data, stream).unwrap();
                println!(
                    "      NiSkinPartition: vertex data {} bytes, size {:?}, desc {:x?}",
                    part.vertex_data.len(),
                    part.vertex_size,
                    part.vertex_desc
                );
                for q in &part.partitions {
                    println!(
                        "        part: {} vertices, {} bones, per vertex {}, map {:?}, weights {:?}, indices {:?}, tris {:?}, true tris {:?}, strips {}",
                        q.vertex_count(),
                        q.bones.len(),
                        q.weights_per_vertex,
                        q.vertex_map.as_ref().map(Vec::len),
                        q.vertex_weights.as_ref().map(Vec::len),
                        q.bone_indices.as_ref().map(Vec::len),
                        q.triangles.as_ref().map(Vec::len),
                        q.true_triangles.as_ref().map(Vec::len),
                        q.strips.len()
                    );
                }
            }
        }
    }
}
