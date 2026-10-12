//! Which lighting switches go with which kind of lighting: for each kind
//! and each switch of either set, how many blocks of the kind have it on,
//! shown where it is on for all of one kind and none of some other.
//!
//!     cargo run --release -p apoc-nif --example kindflags -- <file.nif>...

use apoc_nif::shader::LightingShader;
use apoc_nif::Nif;
use std::collections::BTreeMap;

fn main() {
    let mut seen: BTreeMap<u32, (usize, [usize; 64])> = BTreeMap::new();
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
                let t = seen
                    .entry(s.shader_type.unwrap_or(u32::MAX))
                    .or_insert((0, [0; 64]));
                t.0 += 1;
                let both = (s.flags1 as u64) | ((s.flags2 as u64) << 32);
                for (bit, n) in t.1.iter_mut().enumerate() {
                    *n += (both >> bit & 1) as usize;
                }
            }
        }
    }
    for bit in 0..64 {
        let all: Vec<u32> = seen
            .iter()
            .filter(|(_, t)| t.1[bit] == t.0)
            .map(|(k, _)| *k)
            .collect();
        let none: Vec<u32> = seen
            .iter()
            .filter(|(_, t)| t.1[bit] == 0)
            .map(|(k, _)| *k)
            .collect();
        if !all.is_empty() && !none.is_empty() {
            println!(
                "set {} switch {}: on for all of kinds {all:?}, none of {none:?}",
                bit / 32 + 1,
                bit % 32
            );
        }
    }
    for (kind, t) in &seen {
        println!("kind {kind}: {} blocks", t.0);
    }
}
