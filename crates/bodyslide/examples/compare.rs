//! Build a set at every preset in a folder and say how far each result is
//! from a mesh built elsewhere (by the original program).
//!
//!     cargo run --release -p apoc-bodyslide --example compare -- \
//!         <BodySlide folder> <set name> <reference _0.nif> <reference _1.nif>

use apoc_bodyslide::{build, preset, set};
use std::path::{Path, PathBuf};

fn find(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for part in rel.split('/') {
        let entries = std::fs::read_dir(&at).ok()?;
        at = entries
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?
            .path();
    }
    Some(at)
}

fn positions(bytes: &[u8]) -> Vec<(String, Vec<[f32; 3]>)> {
    let nif = apoc_nif::Nif::read(bytes).unwrap();
    apoc_nif::mesh::shapes(&nif)
        .into_iter()
        .filter_map(|at| apoc_nif::mesh::read(&nif, at).ok())
        .map(|m| (m.name, m.positions))
        .collect()
}

/// The largest and the mean distance between two meshes' vertices, shape by
/// shape, or `None` when they are not the same meshes.
fn distance(a: &[(String, Vec<[f32; 3]>)], b: &[(String, Vec<[f32; 3]>)]) -> Option<(f32, f32)> {
    let (mut worst, mut sum, mut n) = (0.0f32, 0.0f64, 0usize);
    for (name, pa) in a {
        let (_, pb) = b.iter().find(|(nb, _)| nb == name)?;
        if pa.len() != pb.len() {
            return None;
        }
        for (x, y) in pa.iter().zip(pb) {
            let d = ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt();
            worst = worst.max(d);
            sum += d as f64;
            n += 1;
        }
    }
    (n > 0).then(|| (worst, (sum / n as f64) as f32))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(&args[0]);
    let files = |path: &str| find(&root, path).and_then(|p| std::fs::read(p).ok());
    let mut sets = Vec::new();
    for e in std::fs::read_dir(find(&root, "SliderSets").unwrap())
        .unwrap()
        .flatten()
    {
        sets.extend(set::read(&std::fs::read(e.path()).unwrap()).unwrap_or_default());
    }
    let the_set = sets
        .iter()
        .find(|s| s.name == args[1])
        .expect("no such set");
    let mut presets = Vec::new();
    for e in std::fs::read_dir(find(&root, "SliderPresets").unwrap())
        .unwrap()
        .flatten()
    {
        presets.extend(preset::read(&std::fs::read(e.path()).unwrap()).unwrap_or_default());
    }
    let reference: Vec<_> = args[2..4]
        .iter()
        .map(|p| positions(&std::fs::read(p).unwrap()))
        .collect();
    println!(
        "{} sliders, {} presets",
        the_set.sliders.len(),
        presets.len()
    );
    let mut results = Vec::new();
    for p in std::iter::once(None).chain(presets.iter().map(Some)) {
        let built = build::build(the_set, p, &files).unwrap();
        let d: Vec<_> = built
            .outputs
            .iter()
            .zip(&reference)
            .map(|(o, r)| distance(&positions(&o.bytes), r))
            .collect();
        results.push((
            p.map_or("(the set's defaults)", |p| p.name.as_str()),
            d,
            built.notes,
        ));
    }
    results.sort_by(|a, b| {
        let key = |r: &Vec<Option<(f32, f32)>>| {
            r.iter()
                .map(|d| d.map_or(f32::MAX, |d| d.0))
                .fold(0.0f32, f32::max)
        };
        key(&a.1).total_cmp(&key(&b.1))
    });
    for (name, d, notes) in results.iter().take(6) {
        println!("{name}: {d:?}  notes: {}", notes.len());
    }
    if let Some((_, _, notes)) = results.first() {
        for n in notes.iter().take(5) {
            println!("  {n}");
        }
    }
}
