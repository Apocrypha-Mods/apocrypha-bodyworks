//! Every way shapes of some files say what a vertex carries, and how often:
//! the description as the file has it, and the same on the skin's own copy.
//!
//!     cargo run --release -p apoc-nif --example descs -- <file.nif>...

use apoc_nif::geometry::{is_tri_shape, TriShape};
use apoc_nif::skin::SkinPartition;
use apoc_nif::Nif;
use std::collections::BTreeMap;

fn main() {
    let mut seen: BTreeMap<(String, u64), usize> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        let stream = nif.header.stream.version;
        for at in 0..nif.blocks.len() {
            let kind = nif.kind_of(at);
            if is_tri_shape(kind) {
                if let Ok(s) = TriShape::read(&nif.blocks[at].data, stream) {
                    let how = if s.has_vertices() {
                        "in the shape"
                    } else {
                        "shape, vertices elsewhere"
                    };
                    *seen
                        .entry((format!("{kind} {how}"), s.vertex_desc))
                        .or_default() += 1;
                }
            } else if kind == "NiSkinPartition" {
                if let Ok(p) = SkinPartition::read(&nif.blocks[at].data, stream) {
                    if let Some(d) = p.vertex_desc {
                        *seen.entry(("skin".into(), d)).or_default() += 1;
                    }
                }
            }
        }
    }
    for ((what, desc), n) in seen {
        println!("{desc:016x} {n:6} {what}");
    }
}
