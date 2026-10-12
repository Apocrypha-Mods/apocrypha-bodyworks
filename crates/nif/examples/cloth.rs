//! Whether Fallout 4's cloth data blocks are laid out as supposed: how many
//! bytes, and those, which are a Havok file; and how many a file has.
//!
//!     cargo run --release -p apoc-nif --example cloth -- <file.nif>...

use apoc_nif::Nif;
use std::collections::BTreeMap;

fn main() {
    let (mut ok, mut odd) = (0usize, 0usize);
    let mut per_file: BTreeMap<usize, usize> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        let mut here = 0;
        for at in 0..nif.blocks.len() {
            if nif.kind_of(at) != "BSClothExtraData" {
                continue;
            }
            here += 1;
            let d = &nif.blocks[at].data;
            let stated = d
                .get(..4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
            let fits = stated == Some(d.len().saturating_sub(4))
                && d.get(4..12) == Some(&[0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10]);
            if fits {
                ok += 1;
            } else {
                odd += 1;
            }
        }
        if here > 0 {
            *per_file.entry(here).or_default() += 1;
        }
    }
    println!("{ok} as supposed, {odd} not; how many a file has: {per_file:?}");
}
