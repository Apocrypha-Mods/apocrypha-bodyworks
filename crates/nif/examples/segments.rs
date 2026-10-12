//! Whether what Fallout 4's shapes keep after their triangles is laid out
//! as supposed: a count of triangles, segments each a run of them with
//! runs of its own inside it, then (where there are any of those) which
//! is which, something for each, and the name of a file.
//!
//!     cargo run --release -p apoc-nif --example segments -- <file.nif>...

use apoc_nif::geometry::TriShape;
use apoc_nif::Nif;

fn parse(rest: &[u8], triangles: usize) -> Result<String, String> {
    let mut at = 0usize;
    let mut word = |what: &str| -> Result<u32, String> {
        let b = rest.get(at..at + 4).ok_or(format!("ends at {what}"))?;
        at += 4;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    };
    let stated = word("count")? as usize;
    if stated != triangles {
        return Err(format!("says {stated} triangles, has {triangles}"));
    }
    let (segments, total) = (word("segments")? as usize, word("total")? as usize);
    let (mut next, mut subs) = (0usize, 0usize);
    for s in 0..segments {
        let (start, count, parent, inner) = (
            word("start")? as usize,
            word("n")? as usize,
            word("parent")?,
            word("subs")? as usize,
        );
        let place = s + subs;
        if (start != next * 3 && count != 0) || parent != u32::MAX {
            return Err(format!(
                "segment {s} starts at {start}, after {next} triangles, parent {parent:x}"
            ));
        }
        let mut inside = 0usize;
        for k in 0..inner {
            let (a, n, p, _unused) = (
                word("sub start")? as usize,
                word("sub n")? as usize,
                word("sub parent")? as usize,
                word("unused")?,
            );
            // A run lies inside its segment, after the run before it.
            if n != 0 && (a < (next + inside) * 3 || a + n * 3 > (next + count) * 3) || p != place {
                return Err(format!(
                    "sub {k} of {s} is {a}+{n}, its segment {}+{count}, parent {p}",
                    next * 3
                ));
            }
            if n != 0 {
                inside = a / 3 + n - next;
            }
        }
        if inside > count {
            return Err(format!(
                "segment {s}: its runs reach {inside} of its {count}"
            ));
        }
        subs += inner;
        next += count;
    }
    if next != triangles {
        return Err(format!("segments hold {next} of {triangles}"));
    }
    if segments + subs != total {
        return Err(format!(
            "{segments} segments and {subs} runs, total said {total}"
        ));
    }
    let mut shape = format!("{segments} segments, {subs} runs");
    if subs > 0 {
        let (n, t) = (
            word("data segments")? as usize,
            word("data total")? as usize,
        );
        if n != segments || t != total {
            return Err(format!("data says {n} and {t}"));
        }
        let mut last = None;
        for _ in 0..n {
            let i = word("index")?;
            if last.is_some_and(|l| i <= l) {
                return Err("indices not rising".into());
            }
            last = Some(i);
        }
        for _ in 0..t {
            let (_user, _bone, cuts) = (word("user")?, word("bone")?, word("cuts")? as usize);
            if cuts > 64 {
                return Err(format!("{cuts} cuts"));
            }
            for _ in 0..cuts {
                word("cut")?;
            }
        }
        let len = rest
            .get(at..at + 2)
            .map(|b| u16::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or("ends at the name")?;
        let name = rest.get(at + 2..at + 2 + len).ok_or("ends in the name")?;
        at += 2 + len;
        shape += &format!(
            ", file {}",
            String::from_utf8_lossy(name).trim_end_matches('\0')
        );
    }
    if at != rest.len() {
        return Err(format!("{} bytes left of {}", rest.len() - at, rest.len()));
    }
    Ok(shape)
}

fn main() {
    let (mut ok, mut odd, mut shown, mut with) = (0usize, 0usize, 0, 0usize);
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
            match parse(&s.rest, s.triangles.len()) {
                Ok(what) => {
                    ok += 1;
                    with += what.contains("file") as usize;
                    if shown < 2 && what.contains("file") {
                        shown += 1;
                        println!("{path}: {what}");
                    }
                }
                Err(why) => {
                    odd += 1;
                    if odd <= 4 {
                        println!(
                            "ODD {path} block {at}: {why}; starts {:02x?}",
                            &s.rest[..s.rest.len().min(48)]
                        );
                    }
                }
            }
        }
    }
    println!("{ok} as supposed ({with} with runs inside segments), {odd} not");
}
