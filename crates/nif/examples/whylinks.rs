//! Why a file's blocks cannot be moved, if they cannot: what taking its
//! last block out is refused for, and the kinds whose links are not known.
//!
//!     cargo run --release -p apoc-nif --example whylinks -- <file.nif>

use apoc_nif::{edit, Nif};

fn main() {
    for path in std::env::args().skip(1) {
        let mut nif = Nif::read(&std::fs::read(&path).unwrap()).unwrap();
        let mut unknown: Vec<&str> = (0..nif.blocks.len())
            .map(|i| nif.kind_of(i))
            .filter(|k| !edit::links_known(k))
            .collect();
        unknown.sort();
        unknown.dedup();
        println!("{path}: kinds not known {unknown:?}");
        let last = nif.blocks.len() - 1;
        println!(
            "  taking out the last block: {:?}",
            edit::remove_blocks(&mut nif, &[last].into()).map_err(|e| e.to_string())
        );
    }
}
