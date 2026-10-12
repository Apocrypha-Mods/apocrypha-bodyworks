//! The first few vertices of each shape of a file, and where its node puts
//! it: to hold a mesh against another file of the same vertices.
//!
//!     cargo run --release -p apoc-nif --example firstverts -- <file.nif>

use apoc_nif::{mesh, Nif};

fn main() {
    for path in std::env::args().skip(1) {
        let nif = Nif::read(&std::fs::read(&path).unwrap()).unwrap();
        for at in mesh::shapes(&nif) {
            let m = mesh::read(&nif, at).unwrap();
            let shape =
                apoc_nif::geometry::TriShape::read(&nif.blocks[at].data, nif.header.stream.version)
                    .unwrap();
            println!(
                "{}: {} vertices; placed {:?} scale {}; {:?} {:?} {:?}",
                m.name,
                m.positions.len(),
                shape.av.transform.translation,
                shape.av.transform.scale,
                m.positions[0],
                m.positions[1],
                m.positions.get(250)
            );
        }
    }
}
