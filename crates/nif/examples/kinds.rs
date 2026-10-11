//! List a mesh file's blocks in order, with each one's size.
//!
//!     cargo run --release -p apoc-nif --example kinds -- <file.nif>

fn main() {
    for path in std::env::args().skip(1) {
        let nif = apoc_nif::Nif::read(&std::fs::read(&path).unwrap()).unwrap();
        println!("{path}");
        for i in 0..nif.blocks.len() {
            println!("{i:3} {} {}", nif.kind_of(i), nif.blocks[i].data.len());
        }
    }
}
