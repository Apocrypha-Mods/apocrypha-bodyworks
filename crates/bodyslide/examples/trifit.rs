//! Build every set's morph file and say how it compares with the one the
//! original program wrote at the same path.
//!
//!     cargo run --release -p apoc-bodyslide --example trifit -- <folder>... [--show <set name>]

use apoc_bodyslide::{build, morph, set, tri};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, rel: &str, into: &mut BTreeMap<String, PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        if e.path().is_dir() {
            walk(&e.path(), &rel, into);
        } else {
            into.insert(rel.to_lowercase(), e.path());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show = args
        .iter()
        .position(|a| a == "--show")
        .map(|i| args[i + 1].clone());
    let mut all = BTreeMap::new();
    for root in args.iter().take_while(|a| *a != "--show") {
        walk(Path::new(root), "", &mut all);
    }
    let tools = "data/calientetools/bodyslide/";
    let files = |path: &str| {
        all.get(&format!("{tools}{}", path.to_lowercase()))
            .and_then(|p| std::fs::read(p).ok())
    };
    let sets: Vec<set::SliderSet> = all
        .iter()
        .filter(|(k, _)| k.starts_with(&format!("{tools}slidersets/")) && k.ends_with(".osp"))
        .flat_map(|(_, p)| set::read(&std::fs::read(p).unwrap()).unwrap_or_default())
        .collect();
    let (mut same, mut alike, mut differ, mut none) = (0, 0, 0, 0);
    let mut reasons: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for s in &sets {
        if show.as_ref().is_some_and(|n| *n != s.name) {
            continue;
        }
        let path = format!(
            "data/{}/{}.tri",
            s.output_path.replace('\\', "/"),
            s.output_file
        )
        .to_lowercase();
        let Some(theirs) = all.get(&path).and_then(|p| std::fs::read(p).ok()) else {
            none += 1;
            continue;
        };
        let small = morph::values(s, None, false);
        let big = morph::values(s, None, true);
        let Ok(built) = build::build_with(s, &small, &big, &files, true) else {
            differ += 1;
            continue;
        };
        let ours = built.morphs.unwrap().bytes;
        if ours == theirs {
            same += 1;
            continue;
        }
        let (Ok(a), Ok(b)) = (
            tri::read(&ours).map(|m| [m.shapes, m.uv].concat()),
            tri::read(&theirs).map(|m| [m.shapes, m.uv].concat()),
        ) else {
            *reasons.entry("theirs is not read".into()).or_default() = (
                reasons.get("theirs is not read").map_or(0, |r| r.0) + 1,
                s.name.clone(),
            );
            differ += 1;
            continue;
        };
        let why = if a.len() != b.len() {
            format!(
                "shapes: ours {:?}, theirs {:?}",
                a.iter().map(|x| &x.name).collect::<Vec<_>>(),
                b.iter().map(|x| &x.name).collect::<Vec<_>>()
            )
        } else {
            let mut why = String::new();
            for (x, y) in a.iter().zip(&b) {
                if x.name != y.name {
                    why = format!("shape order: {} / {}", x.name, y.name);
                    break;
                }
                let xn: Vec<&str> = x.morphs.iter().map(|m| m.name.as_str()).collect();
                let yn: Vec<&str> = y.morphs.iter().map(|m| m.name.as_str()).collect();
                if xn != yn {
                    let only_ours: Vec<_> = xn.iter().filter(|n| !yn.contains(n)).collect();
                    let only_theirs: Vec<_> = yn.iter().filter(|n| !xn.contains(n)).collect();
                    why = if only_ours.is_empty() && only_theirs.is_empty() {
                        "morph order".into()
                    } else {
                        format!(
                            "morphs only ours {} only theirs {}",
                            only_ours.len(),
                            only_theirs.len()
                        )
                    };
                    if show.is_some() {
                        println!(
                            "{}: only ours {only_ours:?}\n  only theirs {only_theirs:?}",
                            x.name
                        );
                    }
                    break;
                }
                for (m, n) in x.morphs.iter().zip(&y.morphs) {
                    if m.offsets.len() != n.offsets.len() {
                        why = "vertex counts in a morph".into();
                        if show.is_some() {
                            println!(
                                "{} {}: {} / {}",
                                x.name,
                                m.name,
                                m.offsets.len(),
                                n.offsets.len()
                            );
                        }
                    } else if m.multiplier != n.multiplier {
                        why = "a multiplier".into();
                    } else if m.offsets != n.offsets {
                        let off = m
                            .offsets
                            .iter()
                            .zip(&n.offsets)
                            .filter(|(p, q)| p != q)
                            .count();
                        let order = m
                            .offsets
                            .iter()
                            .map(|o| o.0)
                            .ne(n.offsets.iter().map(|o| o.0));
                        why = if order {
                            "vertex order".into()
                        } else {
                            "numbers off by rounding".into()
                        };
                        if show.is_some() {
                            println!(
                                "{} {}: {off} rows differ, e.g. {:?}",
                                x.name,
                                m.name,
                                m.offsets.iter().zip(&n.offsets).find(|(p, q)| p != q)
                            );
                        }
                    }
                    if !why.is_empty() && show.is_none() {
                        break;
                    }
                }
                if !why.is_empty() {
                    break;
                }
            }
            why
        };
        if why.is_empty() {
            alike += 1;
        } else {
            differ += 1;
            let e = reasons.entry(why).or_insert((0, s.name.clone()));
            e.0 += 1;
        }
    }
    println!("{same} byte for byte, {alike} the same content, {differ} differ, {none} have no file to compare with");
    for (why, (n, eg)) in &reasons {
        println!("  {n:4}  {why}   e.g. {eg}");
    }
}
