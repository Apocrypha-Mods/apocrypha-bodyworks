//! Which shapes of some files carry vertex colours.
//!
//!     cargo run -p apoc-nif --example colours -- <file.nif>...

use apoc_nif::{mesh, Nif};

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        for at in mesh::shapes(&nif) {
            let colors = mesh::colors(&nif, at).unwrap_or_default();
            if !colors.is_empty() {
                println!(
                    "{path}: {} ({} vertices)",
                    mesh::name(&nif, at).unwrap_or_default(),
                    colors.len()
                );
            }
        }
    }
}
