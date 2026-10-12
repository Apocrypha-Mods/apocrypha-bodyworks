//! How many shapes' skins write back as the file had them: each read, then
//! written with what was read, and the file compared byte for byte.
//!
//!     cargo run --release -p apoc-nif --example skinsame -- <file.nif>...

use apoc_nif::{mesh, weights, Nif};

fn main() {
    let (mut same, mut differ, mut refused, mut first) = (0usize, 0usize, 0usize, String::new());
    let (mut bones, mut loose) = (0usize, 0usize);
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        for at in mesh::shapes(&nif) {
            let Ok(skin) = weights::read(&nif, at) else {
                continue;
            };
            bones += skin.bones.len();
            loose += skin
                .vertices
                .iter()
                .filter(|v| (v.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() > 0.01)
                .count();
            let mut again = nif.clone();
            match weights::write(&mut again, at, &skin, None) {
                Ok(()) if again.write() == nif.write() => same += 1,
                Ok(()) => {
                    differ += 1;
                    if first.is_empty() {
                        first = path.clone();
                    }
                }
                Err(e) => {
                    refused += 1;
                    if first.is_empty() {
                        first = format!("{path}: {e}");
                    }
                }
            }
        }
    }
    println!("{same} skins write back the same, {differ} differ, {refused} are refused; {bones} bones read, {loose} vertices whose weights do not add up to one {first}");
}
