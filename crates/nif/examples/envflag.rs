//! Whether the switch taken to mean "environment map" goes with the kind
//! of lighting that carries one: how many lighting blocks of each kind
//! have the first set's switch 7 on.
//!
//!     cargo run --release -p apoc-nif --example envflag -- <file.nif>...

use apoc_nif::shader::LightingShader;
use apoc_nif::Nif;
use std::collections::BTreeMap;

fn main() {
    let mut seen: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        for at in 0..nif.blocks.len() {
            if nif.kind_of(at) != "BSLightingShaderProperty" {
                continue;
            }
            if let Ok(s) = LightingShader::read(&nif.blocks[at].data, nif.header.stream.version) {
                let t = seen.entry(s.shader_type.unwrap_or(u32::MAX)).or_default();
                t.0 += 1;
                t.1 += (s.flags1 >> 7 & 1) as usize;
            }
        }
    }
    for (kind, (all, on)) in seen {
        println!("kind {kind}: {on} of {all} have it on");
    }
}
