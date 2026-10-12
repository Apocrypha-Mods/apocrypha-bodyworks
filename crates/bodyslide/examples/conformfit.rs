//! Conform every outfit found under some folders to its reference body, and
//! say how far the result is from the slider data the outfit came with
//! (made by the original program, and sometimes touched up by hand).
//!
//!     cargo run --release -p apoc-bodyslide --example conformfit -- <folder>...
//!
//! `NEAREST`, `REACH` and `FALLOFF` in the environment try other settings.
//! With `WHOLE` set to a set's name, every outfit is conformed to that
//! set's first shape instead of to the body inside the outfit, which is
//! often only the part of the body the outfit leaves showing.

use apoc_bodyslide::conform::{carried, tie, Options};
use apoc_bodyslide::osd::{Diff, SliderData};
use apoc_bodyslide::set;
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

/// A body: where its vertices are, and what each slider does to them by name.
type Body = (Vec<[f32; 3]>, HashMap<String, Vec<Diff>>);

/// A way to a slider's list: its folder, its file and its name.
type Lists<'a> = dyn FnMut(&str, &str, &str) -> Option<Vec<Diff>> + 'a;

fn number(name: &str, or: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(or)
}

fn main() {
    let mut all = BTreeMap::new();
    for root in std::env::args().skip(1) {
        walk(Path::new(&root), "", &mut all);
    }
    const TOOLS: &str = "data/calientetools/bodyslide/";
    let files = |path: &str| {
        all.get(&format!(
            "{TOOLS}{}",
            path.replace('\\', "/").to_lowercase()
        ))
        .and_then(|p| std::fs::read(p).ok())
    };
    let defaults = Options::default();
    let options = Options {
        nearest: number("NEAREST", defaults.nearest as f32) as usize,
        reach: number("REACH", defaults.reach),
        falloff: number("FALLOFF", defaults.falloff),
    };
    let mut data: HashMap<String, Option<SliderData>> = HashMap::new();
    let mut list = |folder: &str, file: &str, name: &str| -> Option<Vec<Diff>> {
        let path = format!("ShapeData/{folder}/{file}");
        data.entry(path.clone())
            .or_insert_with(|| files(&path).and_then(|b| SliderData::read(&b).ok()))
            .as_ref()
            .and_then(|d| d.list(name).map(<[_]>::to_vec))
    };
    // Over every vertex of every list compared: how far ours is from
    // theirs, and how far theirs moves at all.
    // The whole body, when asked for: where its vertices are and what each
    // slider does to them, by the slider's name.
    let mut every: Vec<set::SliderSet> = Vec::new();
    for (rel, path) in &all {
        if rel.starts_with(&format!("{TOOLS}slidersets/")) && rel.ends_with(".osp") {
            every.extend(
                std::fs::read(path)
                    .ok()
                    .and_then(|b| set::read(&b).ok())
                    .unwrap_or_default(),
            );
        }
    }
    let whole: Option<Body> = std::env::var("WHOLE").ok().and_then(|name| {
        let s = every.iter().find(|s| s.name == name)?;
        let nif = files(&format!("ShapeData/{}/{}", s.data_folder, s.source_file))
            .and_then(|b| apoc_nif::Nif::read(&b).ok())?;
        let shape = s.shapes.first()?;
        let m = apoc_nif::mesh::shapes(&nif)
            .into_iter()
            .filter_map(|at| apoc_nif::mesh::read(&nif, at).ok())
            .find(|m| m.name == shape.name)?;
        let mut by_name = HashMap::new();
        for slider in &s.sliders {
            if let Some(d) = slider.data.iter().find(|d| d.target == shape.target) {
                if let Some(l) = list(&s.data_folder, &d.file, &d.list) {
                    by_name.insert(slider.name.clone(), l);
                }
            }
        }
        Some((m.positions, by_name))
    });
    let (mut off, mut size, mut count) = (0.0f64, 0.0f64, 0usize);
    let (mut shapes, mut lists) = (0usize, 0usize);
    let mut worst: Vec<(f32, String)> = Vec::new();
    for (rel, path) in &all {
        if !rel.starts_with(&format!("{TOOLS}slidersets/")) || !rel.ends_with(".osp") {
            continue;
        }
        let Some(sets) = std::fs::read(path).ok().and_then(|b| set::read(&b).ok()) else {
            continue;
        };
        for s in sets {
            // The body is the shape marked as a reference, or failing that
            // the one named as CBBE's bodies are.
            let Some(body) = s
                .shapes
                .iter()
                .find(|x| x.data_folder.is_some())
                .or_else(|| {
                    s.shapes
                        .iter()
                        .find(|x| x.name == "CBBE" || x.name == "BaseShape")
                })
            else {
                continue;
            };
            let Some(nif) = files(&format!("ShapeData/{}/{}", s.data_folder, s.source_file))
                .and_then(|b| apoc_nif::Nif::read(&b).ok())
            else {
                continue;
            };
            let meshes: Vec<apoc_nif::mesh::Mesh> = apoc_nif::mesh::shapes(&nif)
                .into_iter()
                .filter_map(|at| apoc_nif::mesh::read(&nif, at).ok())
                .collect();
            let Some(on) = meshes.iter().find(|m| m.name == body.name) else {
                continue;
            };
            let mut scratch = Vec::new();
            for shape in s.shapes.iter().filter(|x| x.name != body.name) {
                let Some(m) = meshes.iter().find(|m| m.name == shape.name) else {
                    continue;
                };
                let ties = match &whole {
                    Some((positions, _)) => tie(positions, &m.positions, options),
                    None => tie(&on.positions, &m.positions, options),
                };
                shapes += 1;
                let (mut shape_off, mut shape_size) = (0.0f64, 0.0f64);
                for slider in s.sliders.iter().filter(|x| !x.zap && !x.uv) {
                    let of = |target: &str, folder: &str, list: &mut Lists| {
                        slider
                            .data
                            .iter()
                            .find(|d| d.target == target)
                            .and_then(|d| list(folder, &d.file, &d.list))
                    };
                    let from = body.data_folder.as_deref().unwrap_or(&s.data_folder);
                    let bodys = match &whole {
                        Some((_, by_name)) => by_name.get(&slider.name).cloned(),
                        None => of(&body.target, from, &mut list),
                    };
                    let (Some(theirs), Some(bodys)) =
                        (of(&shape.target, &s.data_folder, &mut list), bodys)
                    else {
                        continue;
                    };
                    lists += 1;
                    let ours = carried(&ties, &bodys, 0.0, &mut scratch);
                    let mut both: BTreeMap<u16, ([f32; 3], [f32; 3])> = BTreeMap::new();
                    for d in &ours {
                        both.entry(d.vertex).or_default().0 = d.offset;
                    }
                    for d in &theirs {
                        both.entry(d.vertex).or_default().1 = d.offset;
                    }
                    for (a, b) in both.values() {
                        let d =
                            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2))
                                .sqrt();
                        let l = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
                        shape_off += d as f64;
                        shape_size += l as f64;
                        count += 1;
                    }
                }
                off += shape_off;
                size += shape_size;
                if shape_size > 0.0 {
                    worst.push((
                        (shape_off / shape_size) as f32,
                        format!("{} / {}", s.name, shape.name),
                    ));
                }
            }
        }
    }
    worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    let middle = worst.get(worst.len() / 2).map_or(0.0, |x| x.0);
    println!(
        "{options:?}\n{shapes} shapes, {lists} lists, {count} vertices compared\n\
         ours is off theirs by {:.1}% of how far theirs moves overall; the middle shape by {:.1}%",
        100.0 * off / size.max(1e-9),
        100.0 * middle
    );
    for (far, name) in worst.iter().take(8) {
        println!("  {:5.1}%  {name}", far * 100.0);
    }
}
