//! Write a file's skins back and say which blocks came out different, and
//! where: what is left to learn of how the original lays weights down.
//!
//!     cargo run -p apoc-nif --example skinback -- <file.nif>

use apoc_nif::skin::SkinData;
use apoc_nif::{mesh, weights, Nif};

fn main() {
    for path in std::env::args().skip(1) {
        let nif = Nif::read(&std::fs::read(&path).unwrap()).unwrap();
        for at in mesh::shapes(&nif) {
            let Ok(skin) = weights::read(&nif, at) else {
                continue;
            };
            let mut again = nif.clone();
            weights::write(&mut again, at, &skin, None).unwrap();
            for (i, (a, b)) in nif.blocks.iter().zip(&again.blocks).enumerate() {
                if a.data == b.data {
                    continue;
                }
                let first = a.data.iter().zip(&b.data).position(|(x, y)| x != y);
                let differing = a.data.iter().zip(&b.data).filter(|(x, y)| x != y).count();
                println!("shape {at}: block {i} ({}) differs: {} → {} bytes, first at {first:?}, {differing} bytes differ", nif.kind_of(i), a.data.len(), b.data.len());
                if nif.kind_of(i) == "NiSkinData" {
                    let (x, y) = (
                        SkinData::read(&a.data).unwrap(),
                        SkinData::read(&b.data).unwrap(),
                    );
                    for (k, (p, q)) in x.bones.iter().zip(&y.bones).enumerate() {
                        if p.weights != q.weights {
                            println!(
                                "  bone {k}: {} weights, ours {}; theirs starts {:?}; ours {:?}",
                                p.weights.len(),
                                q.weights.len(),
                                &p.weights[..p.weights.len().min(5)],
                                &q.weights[..q.weights.len().min(5)]
                            );
                            let mut sorted = p.weights.clone();
                            sorted.sort_by_key(|w| w.0);
                            println!(
                                "  theirs sorted is ours: {}; sorted starts {:?}",
                                sorted == q.weights,
                                &sorted[..sorted.len().min(5)]
                            );
                            break;
                        }
                    }
                }
            }
        }
    }
}
