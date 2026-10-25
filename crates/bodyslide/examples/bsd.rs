//! How the older slider data files (`.bsd`) and the sets that name them
//! fare: every file read and written back, and every set that names one
//! built, opened as a project, saved, and built again from what was saved.
//!
//!     cargo run --release -p apoc-bodyslide --example bsd -- <folder>...
//!
//! Each folder holds a `CalienteTools/BodySlide` somewhere under it, as a
//! mod's own files do. A file in a later folder replaces one in an earlier.

use apoc_bodyslide::bsd::{self, Bsd};
use apoc_bodyslide::project::Project;
use apoc_bodyslide::{build, morph, set};
use apoc_nif::{mesh, Nif};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

const TOOLS: &str = "calientetools/bodyslide/";

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
            // By the path under the BodySlide folder.
            let lower = rel.to_lowercase();
            if let Some(at) = lower.find(TOOLS) {
                into.insert(lower[at + TOOLS.len()..].to_string(), e.path());
            }
        }
    }
}

fn main() {
    let mut all = BTreeMap::new();
    for root in std::env::args().skip(1) {
        walk(Path::new(&root), "", &mut all);
    }
    let files = |path: &str| {
        all.get(&path.to_lowercase())
            .and_then(|p| std::fs::read(p).ok())
    };

    // The files themselves.
    let (mut read, mut same, mut refused, mut padded) = (0, 0, 0, 0);
    for (rel, path) in all.iter().filter(|(r, _)| bsd::named(r)) {
        let bytes = std::fs::read(path).unwrap_or_default();
        match Bsd::read(&bytes) {
            Ok(b) => {
                read += 1;
                padded += usize::from(!b.unused.is_empty());
                if b.write() == bytes {
                    same += 1;
                } else {
                    println!("CHANGED {rel}");
                }
            }
            Err(e) => {
                refused += 1;
                println!("REFUSED {rel}: {e}");
            }
        }
    }
    println!(
        "{read} BSDs read ({padded} with something past a vertex's number), \
         {same} write back byte for byte, {refused} refused"
    );

    // The sets that name them.
    let (mut sets, mut from_xml) = (0, 0);
    let (mut before_whole, mut after_whole, mut built) = (0, 0, 0);
    let (mut refs, mut missing, mut beyond, mut all_found) = (0, 0, 0, 0);
    let mut gone: BTreeSet<String> = BTreeSet::new();
    let (mut resaved, mut differ, mut failed) = (0, 0, 0);
    // As the build was before: a `.bsd` named alone was taken for a list
    // with no file, so its data was never found.
    let without = |path: &str| (!bsd::named(path)).then(|| files(path)).flatten();
    for (rel, path) in &all {
        let xml = rel.ends_with(".xml");
        if !rel.starts_with("slidersets/") || !(xml || rel.ends_with(".osp")) {
            continue;
        }
        let Ok(found) = std::fs::read(path)
            .map_err(drop)
            .and_then(|b| set::read(&b).map_err(drop))
        else {
            println!("UNREAD  {rel}");
            continue;
        };
        for s in found {
            if !s
                .sliders
                .iter()
                .flat_map(|x| &x.data)
                .any(|d| bsd::named(&d.file))
            {
                continue;
            }
            sets += 1;
            from_xml += usize::from(xml);
            let mesh_at =
                format!("ShapeData/{}/{}", s.data_folder, s.source_file).replace('\\', "/");
            let counts: HashMap<String, usize> = files(&mesh_at)
                .and_then(|b| Nif::read(&b).ok())
                .map(|nif| {
                    mesh::shapes(&nif)
                        .into_iter()
                        .filter_map(|at| mesh::read(&nif, at).ok())
                        .map(|m| (m.name, m.positions.len()))
                        .collect()
                })
                .unwrap_or_default();
            let missed = missing;
            for slider in &s.sliders {
                for d in &slider.data {
                    refs += 1;
                    let shape = s.shapes.iter().find(|x| x.target == d.target);
                    let folder = match shape.and_then(|x| x.data_folder.as_ref()) {
                        Some(f) if !d.local => f.as_str(),
                        _ => s.data_folder.as_str(),
                    };
                    let at = format!("ShapeData/{folder}/{}", d.file).replace('\\', "/");
                    let Some(bytes) = files(&at) else {
                        missing += 1;
                        gone.insert(folder.to_string());
                        if !folder.eq_ignore_ascii_case("CalienteBody") {
                            println!("MISSING {}: {at}", s.name);
                        }
                        continue;
                    };
                    let count = shape.and_then(|x| counts.get(&x.name)).copied();
                    let data = apoc_bodyslide::osd::SliderData::read_file(&at, &bytes);
                    if let (Some(count), Ok(data)) = (count, data) {
                        if let Some(list) = data.list(&d.list) {
                            if list.iter().any(|x| x.vertex as usize >= count) {
                                beyond += 1;
                                println!("BEYOND  {}: {at} for {count} vertices", s.name);
                            }
                        }
                    }
                }
            }
            all_found += usize::from(missing == missed);
            println!("SET     {}: {} not found", s.name, missing - missed);
            // Built at the set's own values: whether the build said data
            // was missing.
            let whole = |b: &build::Built| !b.notes.iter().any(|n| n.contains("is missing"));
            let (small, big) = (
                morph::values(&s, None, false),
                morph::values(&s, None, true),
            );
            if build::build_with(&s, &small, &big, &without, true).is_ok_and(|b| whole(&b)) {
                before_whole += 1;
            }
            let Ok(direct) = build::build_with(&s, &small, &big, &files, true) else {
                println!("NO BUILD {}", s.name);
                continue;
            };
            built += 1;
            after_whole += usize::from(whole(&direct));

            // Opened as a project and saved: built from the saved files,
            // the same bytes.
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
            let both = |path: &str| {
                held.get(&path.to_lowercase())
                    .cloned()
                    .or_else(|| files(path))
            };
            match build::build_with(&again, &small, &big, &both, true) {
                Ok(after) if after == direct => resaved += 1,
                Ok(_) => {
                    differ += 1;
                    println!("DIFFERS {}", s.name);
                }
                Err(e) => {
                    failed += 1;
                    println!("FAILED  {}: {e}", s.name);
                }
            }
        }
    }
    println!(
        "{sets} sets name BSDs ({from_xml} from older .xml sets), {built} build; \
         at their own values with no data missing: {before_whole} before, {after_whole} now; \
         {all_found} with every reference found"
    );
    println!(
        "{refs} references, {missing} not found (in {}), {beyond} moving a vertex \
         their shape has not got",
        gone.into_iter().collect::<Vec<_>>().join(", ")
    );
    println!("saved as projects: {resaved} build the same bytes, {differ} differ, {failed} failed");
}
