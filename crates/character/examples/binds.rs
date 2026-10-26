//! For a head part's model: each bone's bind, and where the skeleton has
//! the bone, so how each bone would place the shape.
//!
//!     cargo run --release -p apoc-character --example binds -- <model> <skeleton>
mod common;
use apoc_nif::copy::composed;
use apoc_nif::{mesh, skin::SkinData, Nif};
fn main() {
    let idx = common::index();
    let a: Vec<String> = std::env::args().skip(1).collect();
    let nif = Nif::read(&std::fs::read(&idx[&a[0].to_ascii_lowercase()]).unwrap()).unwrap();
    let sk = Nif::read(&std::fs::read(&idx[&a[1].to_ascii_lowercase()]).unwrap()).unwrap();
    let places = apoc_character::place::bone_places(&sk);
    let b = mesh::shapes(&nif)[0];
    let names = mesh::bone_names(&nif, b);
    let d = (0..nif.blocks.len())
        .find(|i| nif.kind_of(*i) == "NiSkinData")
        .unwrap();
    let data = SkinData::read(&nif.blocks[d].data).unwrap();
    println!("overall {:?}", data.transform);
    for sb in mesh::shapes(&nif) {
        let t = apoc_nif::geometry::TriShape::read(&nif.blocks[sb].data, nif.header.stream.version)
            .unwrap();
        println!(
            "shape {} at {:?} scale {} range {:?}",
            mesh::name(&nif, sb).unwrap(),
            t.av.transform.translation,
            t.av.transform.scale,
            range(&mesh::read(&nif, sb).unwrap())
        );
    }
    println!("vertices from {:?}", range(&mesh::read(&nif, b).unwrap()));
    for (n, bone) in names.iter().zip(&data.bones) {
        let w = places.get(n);
        println!(
            "{n}: bind t {:?} r {:?}\n   world {:?}",
            bone.transform.translation,
            &bone.transform.rotation[..3],
            w.map(|w| w.translation)
        );
        if let Some(w) = w {
            let m = composed(w, &composed(&bone.transform, &data.transform));
            println!("   places by {:?} r {:?}", m.translation, &m.rotation[..3]);
        }
    }
}
#[allow(dead_code)]
fn range(m: &apoc_nif::mesh::Mesh) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in &m.positions {
        for c in 0..3 {
            lo[c] = lo[c].min(p[c]);
            hi[c] = hi[c].max(p[c]);
        }
    }
    (lo, hi)
}
