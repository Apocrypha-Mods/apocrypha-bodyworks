//! Print the bounding sphere each shape of a mesh file states.
//!
//!     cargo run --release -p apoc-nif --example stated -- <file.nif>...

fn main() {
    for path in std::env::args().skip(1) {
        let nif = apoc_nif::Nif::read(&std::fs::read(&path).unwrap()).unwrap();
        println!("{path}");
        for at in apoc_nif::mesh::shapes(&nif) {
            if let Ok(s) =
                apoc_nif::geometry::TriShape::read(&nif.blocks[at].data, nif.header.stream.version)
            {
                let name = nif
                    .string(s.av.name)
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_default();
                println!("  {name}: {:?} r {}", s.bound_center, s.bound_radius);
            }
        }
    }
}
