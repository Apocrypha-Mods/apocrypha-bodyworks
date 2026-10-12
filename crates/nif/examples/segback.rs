//! Whether every Fallout 4 shape's segments read and write back as they
//! were, and again after being dealt out anew with the same counts.
//!
//!     cargo run --release -p apoc-nif --example segback -- <file.nif>...

use apoc_nif::geometry::TriShape;
use apoc_nif::segments::Segments;
use apoc_nif::Nif;

fn main() {
    let (mut same, mut regrouped, mut ordered, mut all, mut first) =
        (0usize, 0usize, 0usize, 0usize, String::new());
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        for at in 0..nif.blocks.len() {
            if nif.kind_of(at) != "BSSubIndexTriShape" {
                continue;
            }
            let Ok(s) = TriShape::read(&nif.blocks[at].data, nif.header.stream.version) else {
                continue;
            };
            all += 1;
            match Segments::read(&s.rest, s.triangles.len()) {
                Ok(seg) => {
                    same += (seg.write() == s.rest) as usize;
                    let of = seg.group_of();
                    let mut counts = vec![0usize; seg.groups()];
                    for g in &of {
                        counts[*g] += 1;
                    }
                    // Already laid down group after group: dealt out anew, the same bytes.
                    let in_order = of.windows(2).all(|w| w[0] <= w[1]);
                    ordered += in_order as usize;
                    regrouped += (in_order
                        && seg.regrouped(&counts).is_some_and(|r| r.write() == s.rest))
                        as usize;
                }
                Err(e) => {
                    if first.is_empty() {
                        first = format!("{path}: {e}");
                    }
                }
            }
        }
    }
    println!("{same} of {all} write back the same; {ordered} lie group after group, and {regrouped} of those are the same dealt out anew {first}");
}
