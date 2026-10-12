//! Build every set found under some folders at every preset made for it,
//! and say how far each result is from the mesh that is already at the
//! set's output path (built elsewhere, by the original program).
//!
//!     cargo run --release -p apoc-bodyslide --example corpus -- <folder>...
//!
//! Each folder is laid out like a game's (`Data/CalienteTools/BodySlide`,
//! `Data/meshes`). A file in a later folder replaces one in an earlier.

use apoc_bodyslide::{build, group, preset, set};
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

type Shapes = Vec<(String, Vec<[f32; 3]>, usize)>;

fn shapes(bytes: &[u8]) -> Option<Shapes> {
    let nif = apoc_nif::Nif::read(bytes).ok()?;
    Some(
        apoc_nif::mesh::shapes(&nif)
            .into_iter()
            .filter_map(|at| apoc_nif::mesh::read(&nif, at).ok())
            .map(|m| (m.name, m.positions, m.triangles.len()))
            .collect(),
    )
}

enum Fit {
    /// Largest and mean distance between vertices.
    Near(f32, f32),
    /// Not the same shapes, vertices or triangles: what differs.
    Differs(String),
}

fn fit(ours: &Shapes, theirs: &Shapes) -> Fit {
    if ours.len() != theirs.len() {
        return Fit::Differs(format!("{} shapes, theirs {}", ours.len(), theirs.len()));
    }
    let (mut worst, mut sum, mut n) = (0.0f32, 0.0f64, 0usize);
    for (name, pa, ta) in ours {
        let Some((_, pb, tb)) = theirs.iter().find(|(nb, _, _)| nb == name) else {
            return Fit::Differs(format!("theirs has no shape {name}"));
        };
        if pa.len() != pb.len() || ta != tb {
            return Fit::Differs(format!(
                "{name}: {} vertices and {ta} triangles, theirs {} and {tb}",
                pa.len(),
                pb.len()
            ));
        }
        for (x, y) in pa.iter().zip(pb) {
            let d = ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt();
            worst = worst.max(d);
            sum += d as f64;
            n += 1;
        }
    }
    Fit::Near(worst, if n > 0 { (sum / n as f64) as f32 } else { 0.0 })
}

fn main() {
    let mut all = BTreeMap::new();
    for root in std::env::args().skip(1) {
        walk(Path::new(&root), "", &mut all);
    }
    // Fallout 4 keeps them one folder up: `TOOLS=data/tools/bodyslide/`.
    let tools = std::env::var("TOOLS").unwrap_or_else(|_| "data/calientetools/bodyslide/".into());
    let tools = tools.as_str();
    let files = |path: &str| {
        all.get(&format!("{tools}{}", path.to_lowercase()))
            .and_then(|p| std::fs::read(p).ok())
    };
    let under = |folder: &str, ext: &str| -> Vec<Vec<u8>> {
        let prefix = format!("{tools}{folder}/");
        all.range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .filter(|(k, _)| k.ends_with(ext))
            .filter_map(|(_, p)| std::fs::read(p).ok())
            .collect()
    };
    let sets: Vec<set::SliderSet> = under("slidersets", ".osp")
        .iter()
        .flat_map(|b| set::read(b).unwrap_or_default())
        .collect();
    let presets: Vec<preset::Preset> = under("sliderpresets", ".xml")
        .iter()
        .flat_map(|b| preset::read(b).unwrap_or_default())
        .collect();
    let groups: Vec<group::Group> = under("slidergroups", ".xml")
        .iter()
        .flat_map(|b| group::read(b).unwrap_or_default())
        .collect();
    println!(
        "{} sets, {} presets, {} groups",
        sets.len(),
        presets.len(),
        groups.len()
    );

    let (mut built_ok, mut compared, mut near, mut differs, mut failed, mut no_reference) =
        (0, 0, 0, 0, 0, 0);
    let mut worst_overall = 0.0f32;
    let mut by_preset: BTreeMap<String, usize> = BTreeMap::new();
    let mut far: Vec<(f32, String)> = Vec::new();
    let mut unlike: Vec<String> = Vec::new();
    let mut zaps = 0;
    let with_morphs = std::env::var("MORPHS").is_ok();
    let (mut files_same, mut files_all) = (0, 0);
    let mut why_not: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut kinds: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for s in &sets {
        if s.sliders.iter().any(|x| x.zap) {
            zaps += 1;
        }
        let mine: Vec<&str> = groups
            .iter()
            .filter(|g| g.members.contains(&s.name))
            .map(|g| g.name.as_str())
            .collect();
        let suffixes: &[&str] = if s.gen_weights { &["_0", "_1"] } else { &[""] };
        let reference: Vec<Option<Shapes>> = suffixes
            .iter()
            .map(|suffix| {
                let path = format!(
                    "data/{}/{}{suffix}.nif",
                    s.output_path.replace('\\', "/"),
                    s.output_file
                )
                .to_lowercase();
                all.get(&path)
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|b| shapes(&b))
            })
            .collect();
        let mut best: Option<(f32, f32, String)> = None;
        let mut best_bytes: Vec<(bool, Vec<(String, bool)>)> = Vec::new();
        let mut why = None;
        let mut any = false;
        for p in std::iter::once(None).chain(
            presets
                .iter()
                .filter(|p| p.groups.iter().any(|g| mine.contains(&g.as_str())))
                .map(Some),
        ) {
            let built = match build::build_with(
                s,
                &apoc_bodyslide::morph::values(s, p, false),
                &apoc_bodyslide::morph::values(s, p, true),
                &files,
                with_morphs,
            ) {
                Ok(b) => b,
                Err(e) => {
                    why = Some(e.to_string());
                    continue;
                }
            };
            any = true;
            let mut bytes_same = Vec::new();
            for (o, suffix) in built.outputs.iter().zip(suffixes) {
                let path = format!(
                    "data/{}/{}{suffix}.nif",
                    s.output_path.replace('\\', "/"),
                    s.output_file
                )
                .to_lowercase();
                let theirs = all
                    .get(&path)
                    .and_then(|p| std::fs::read(p).ok())
                    .unwrap_or_default();
                let mut per: Vec<(String, bool)> = Vec::new();
                if let (Ok(a), Ok(b)) =
                    (apoc_nif::Nif::read(&o.bytes), apoc_nif::Nif::read(&theirs))
                {
                    if a.blocks.len() == b.blocks.len() {
                        for i in 0..a.blocks.len() {
                            per.push((
                                a.kind_of(i).to_string(),
                                a.kind_of(i) == b.kind_of(i)
                                    && a.blocks[i].data == b.blocks[i].data,
                            ));
                        }
                    }
                }
                bytes_same.push((o.bytes == theirs, per));
            }
            let (mut worst, mut mean, mut ok) = (0.0f32, 0.0f32, true);
            for (o, r) in built.outputs.iter().zip(&reference) {
                let (Some(r), Some(ours)) = (r, shapes(&o.bytes)) else {
                    ok = false;
                    break;
                };
                match fit(&ours, r) {
                    Fit::Near(w, m) => {
                        worst = worst.max(w);
                        mean = mean.max(m);
                    }
                    Fit::Differs(d) => {
                        why = Some(d);
                        ok = false;
                        break;
                    }
                }
            }
            if ok && best.as_ref().is_none_or(|b| worst < b.0) {
                best_bytes = bytes_same;
                best = Some((
                    worst,
                    mean,
                    p.map_or("(defaults)".to_string(), |p| p.name.clone()),
                ));
            }
        }
        if !any {
            failed += 1;
            println!("FAILED  {}: {}", s.name, why.unwrap_or_default());
            continue;
        }
        built_ok += 1;
        if reference.iter().any(Option::is_none) {
            no_reference += 1;
            continue;
        }
        compared += 1;
        match best {
            Some((worst, _mean, preset)) => {
                near += 1;
                for (same, per) in &best_bytes {
                    files_all += 1;
                    files_same += *same as usize;
                    if !*same {
                        let mut d: Vec<&str> = per
                            .iter()
                            .filter(|(_, eq)| !eq)
                            .map(|(k, _)| k.as_str())
                            .collect();
                        d.sort();
                        d.dedup();
                        let key = if per.is_empty() {
                            "a different number of blocks".to_string()
                        } else if d.is_empty() {
                            "header or footer only".to_string()
                        } else {
                            d.join(" + ")
                        };
                        if std::env::var("LIST").is_ok_and(|l| key.starts_with(&l))
                            && !s.name.contains("(Physics)")
                        {
                            println!("    {key}: {}", s.name);
                        }
                        let e = why_not.entry(key).or_insert((0, s.name.clone()));
                        e.0 += 1;
                    }
                    for (kind, eq) in per {
                        let e = kinds.entry(kind.clone()).or_default();
                        e.0 += *eq as usize;
                        e.1 += 1;
                    }
                }
                worst_overall = worst_overall.max(worst);
                *by_preset.entry(preset.clone()).or_default() += 1;
                if worst > 0.05 {
                    far.push((worst, format!("{} at {preset}", s.name)));
                }
            }
            None => {
                differs += 1;
                unlike.push(format!("{}: {}", s.name, why.unwrap_or_default()));
            }
        }
    }
    println!("{built_ok} built, {failed} failed, {zaps} have zap sliders");
    println!("{compared} have a mesh to compare with ({no_reference} do not)");
    println!("{near} are the same shapes, vertices and triangles; {differs} are not");
    println!("largest distance of any vertex, at each set's best preset: {worst_overall}");
    println!("best preset: {by_preset:?}");
    for (why, (n, eg)) in &why_not {
        println!("  not the same, {n:4}: {why}   e.g. {eg}");
    }
    println!(
        "{files_same} of {files_all} files are theirs byte for byte; blocks the same, by kind:"
    );
    for (kind, (same, of)) in &kinds {
        println!("  {kind}: {same} of {of}");
    }
    far.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("{} sets with a vertex further than 0.05:", far.len());
    for (d, name) in far.iter().take(15) {
        println!("  {d:.4}  {name}");
    }
    for u in unlike.iter().take(25) {
        println!("  UNLIKE {u}");
    }
}
