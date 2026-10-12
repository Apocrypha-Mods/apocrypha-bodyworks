//! Whether the blocks that say how a thing collides are laid out as
//! supposed: each kind's size, and whether what would be its links name
//! blocks of the kinds they should (or nothing).
//!
//!     cargo run --release -p apoc-nif --example havok -- <file.nif>...

use apoc_nif::Nif;
use std::collections::BTreeMap;

fn main() {
    let mut tally: BTreeMap<String, (usize, usize, String)> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        let word = |d: &[u8], at: usize| {
            d.get(at..at + 4)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
        };
        // A link to a block whose kind starts one of these ways, or to nothing.
        let names = |link: Option<i32>, kinds: &[&str]| match link {
            Some(-1) => true,
            Some(l) => usize::try_from(l)
                .ok()
                .filter(|i| *i < nif.blocks.len())
                .is_some_and(|i| kinds.iter().any(|k| nif.kind_of(i).starts_with(k))),
            None => false,
        };
        for at in 0..nif.blocks.len() {
            let (kind, d) = (nif.kind_of(at), &nif.blocks[at].data);
            let shapes = ["bhk"];
            let fits = match kind {
                "bhkCollisionObject" | "bhkSPCollisionObject" | "bhkBlendCollisionObject" => {
                    (kind != "bhkCollisionObject" || d.len() == 10)
                        && names(word(d, 0), &["Ni", "BS"])
                        && names(word(d, 6), &["bhkRigidBody", "bhkSimpleShapePhantom"])
                }
                "bhkConvexTransformShape" | "bhkTransformShape" => {
                    d.len() == 84 && names(word(d, 0), &shapes)
                }
                "bhkMoppBvTreeShape" => names(word(d, 0), &shapes),
                "bhkCompressedMeshShape" => {
                    d.len() == 56
                        && names(word(d, 0), &["Ni", "BS"])
                        && names(word(d, 52), &["bhkCompressedMeshShapeData"])
                }
                "bhkListShape" => word(d, 0).is_some_and(|n| {
                    let n = n as usize;
                    (0..n).all(|i| names(word(d, 4 + i * 4), &shapes))
                        && word(d, 4 + n * 4 + 28)
                            .is_some_and(|m| d.len() == 4 + n * 4 + 32 + m as usize * 4)
                }),
                "bhkRigidBody" | "bhkRigidBodyT" => {
                    // Its shape first; at its end how many things hold it to others, those, and two bytes.
                    names(word(d, 0), &shapes)
                        && (0..8).any(|n| {
                            d.len() >= 6 + n * 4
                                && word(d, d.len() - 6 - n * 4) == Some(n as i32)
                                && (0..n)
                                    .all(|i| names(word(d, d.len() - 2 - (n - i) * 4), &["bhk"]))
                        })
                }
                "bhkBoxShape"
                | "bhkSphereShape"
                | "bhkCapsuleShape"
                | "bhkConvexVerticesShape"
                | "bhkCompressedMeshShapeData" => true,
                k if k.starts_with("bhk") => false,
                _ => continue,
            };
            let t = tally
                .entry(format!(
                    "{kind} ({} bytes)",
                    if kind.contains("Vertices")
                        || kind.contains("Data")
                        || kind.contains("List")
                        || kind.contains("Mopp")
                    {
                        0
                    } else {
                        d.len()
                    }
                ))
                .or_default();
            if fits {
                t.0 += 1;
            } else {
                t.1 += 1;
                if t.2.is_empty() {
                    t.2 = path.clone();
                }
            }
        }
    }
    for (kind, (ok, not, first)) in tally {
        println!("{kind}: {ok} as supposed, {not} not {first}");
    }
}
