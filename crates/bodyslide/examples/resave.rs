//! Open every set found under some folders as a project, save it, and say
//! whether a build from the saved files is the build from the originals,
//! byte for byte.
//!
//!     cargo run --release -p apoc-bodyslide --example resave -- <folder>...
//!
//! Each folder is laid out like a game's (`Data/CalienteTools/BodySlide`).
//! A file in a later folder replaces one in an earlier.

use apoc_bodyslide::project::Project;
use apoc_bodyslide::{build, morph, set};
use std::collections::{BTreeMap, HashMap};
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
    let mut all = BTreeMap::new();
    for root in std::env::args().skip(1) {
        walk(Path::new(&root), "", &mut all);
    }
    const TOOLS: &str = "data/calientetools/bodyslide/";
    let files = |path: &str| {
        all.get(&format!("{TOOLS}{}", path.to_lowercase()))
            .and_then(|p| std::fs::read(p).ok())
    };
    let (mut same, mut differ, mut failed) = (0, 0, 0);
    for (rel, path) in &all {
        if !rel.starts_with(&format!("{TOOLS}slidersets/")) || !rel.ends_with(".osp") {
            continue;
        }
        let Ok(sets) = std::fs::read(path)
            .map_err(drop)
            .and_then(|b| set::read(&b).map_err(drop))
        else {
            continue;
        };
        for s in sets {
            let (small, big) = (
                morph::values(&s, None, false),
                morph::values(&s, None, true),
            );
            let Ok(before) = build::build_with(&s, &small, &big, &files, true) else {
                continue;
            };
            let saved = Project::open(&s, &files).and_then(|p| p.save(&s.name));
            let (again, written) = match saved {
                Ok(x) => x,
                Err(e) => {
                    failed += 1;
                    println!("FAILED  {}: {e}", s.name);
                    continue;
                }
            };
            let held: HashMap<String, Vec<u8>> = written
                .into_iter()
                .map(|o| (o.path.to_lowercase(), o.bytes))
                .collect();
            // What was saved reads back as the set it was saved from.
            let osp = held.keys().find(|k| k.ends_with(".osp")).unwrap();
            let reread = set::read(&held[osp]).map(|v| v == vec![again.clone()]);
            let both = |path: &str| {
                held.get(&path.to_lowercase())
                    .cloned()
                    .or_else(|| files(path))
            };
            match build::build_with(&again, &small, &big, &both, true) {
                Ok(after) if after == before && reread == Ok(true) => same += 1,
                Ok(after) => {
                    differ += 1;
                    println!(
                        "DIFFERS {}: {} outputs, before {}; reads back the same: {reread:?}",
                        s.name,
                        after.outputs.len(),
                        before.outputs.len()
                    );
                }
                Err(e) => {
                    failed += 1;
                    println!("FAILED  {}: {e}", s.name);
                }
            }
        }
    }
    println!("{same} the same, {differ} differ, {failed} failed");
}
