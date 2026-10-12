//! Whether Fallout 4's skins are laid out as supposed: an instance as the
//! skeleton's root, its bone data, a count of bones and those, then a count
//! of scales with three numbers each; bone data as a count and, for each
//! bone, a sphere round what it moves (four numbers) and where it stands
//! (nine of a turn, three of a place, one of a scale).
//!
//!     cargo run --release -p apoc-nif --example fo4skin -- <file.nif>...

use apoc_nif::Nif;

fn main() {
    let (mut ok, mut odd, mut first) = ([0usize; 2], [0usize; 2], String::new());
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        for at in 0..nif.blocks.len() {
            let d = &nif.blocks[at].data;
            let word = |i: usize| {
                d.get(i..i + 4)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            };
            let (which, fits) = match nif.kind_of(at) {
                "BSSkin::Instance" => {
                    let n = word(8).unwrap_or(usize::MAX / 8);
                    let m = word(12 + n * 4).unwrap_or(usize::MAX / 16);
                    let named = |l: Option<usize>, kinds: &[&str]| {
                        l.is_some_and(|l| {
                            l == u32::MAX as usize
                                || (l < nif.blocks.len() && kinds.contains(&nif.kind_of(l)))
                        })
                    };
                    (
                        0,
                        d.len() == 16 + n * 4 + m * 12
                            && named(word(0), &["NiNode", "BSFadeNode"])
                            && named(word(4), &["BSSkin::BoneData"])
                            && (0..n).all(|i| named(word(12 + i * 4), &["NiNode", "BSFadeNode"])),
                    )
                }
                "BSSkin::BoneData" => (1, word(0).is_some_and(|n| d.len() == 4 + n * 68)),
                _ => continue,
            };
            if fits {
                ok[which] += 1;
            } else {
                odd[which] += 1;
                if first.is_empty() {
                    first = format!(
                        "{path} block {at} ({} bytes, words {:?} {:?} {:?})",
                        d.len(),
                        word(0),
                        word(4),
                        word(8)
                    );
                }
            }
        }
    }
    println!(
        "instances: {} as supposed, {} not; bone data: {} as supposed, {} not {first}",
        ok[0], odd[0], ok[1], odd[1]
    );
}
