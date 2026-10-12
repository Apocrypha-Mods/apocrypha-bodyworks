//! Build one set at one preset and list its blocks beside those of the
//! original program's file: each one's kind and size, and whether its
//! bytes are the same.
//!
//!     cargo run --release -p apoc-bodyslide --example kindsdiff -- <set> <preset or -> <their.nif> <folder>...
//!
//! `TOOLS` says where the folders keep the slider sets
//! (`data/calientetools/bodyslide/`; Fallout 4 has `data/tools/bodyslide/`).

use apoc_bodyslide::{build, morph, preset, set};
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
    let mut all = BTreeMap::new();
    for root in &args[3..] {
        walk(Path::new(root), "", &mut all);
    }
    let tools = std::env::var("TOOLS").unwrap_or_else(|_| "data/calientetools/bodyslide/".into());
    let files = |path: &str| {
        all.get(&format!("{tools}{}", path.to_lowercase()))
            .and_then(|p| std::fs::read(p).ok())
    };
    let read_all = |folder: &str, ext: &str| -> Vec<Vec<u8>> {
        all.iter()
            .filter(|(k, _)| k.starts_with(&format!("{tools}{folder}/")) && k.ends_with(ext))
            .filter_map(|(_, p)| std::fs::read(p).ok())
            .collect()
    };
    let sets: Vec<set::SliderSet> = read_all("slidersets", ".osp")
        .iter()
        .flat_map(|b| set::read(b).unwrap_or_default())
        .collect();
    let presets: Vec<preset::Preset> = read_all("sliderpresets", ".xml")
        .iter()
        .flat_map(|b| preset::read(b).unwrap_or_default())
        .collect();
    let s = sets
        .iter()
        .find(|s| s.name == args[0])
        .expect("no such set");
    let p = presets.iter().find(|p| p.name == args[1]);
    let built = build::build_with(
        s,
        &morph::values(s, p, false),
        &morph::values(s, p, true),
        &files,
        true,
    )
    .unwrap();
    for note in &built.notes {
        println!("note: {note}");
    }
    let ours = apoc_nif::Nif::read(&built.outputs.last().unwrap().bytes).unwrap();
    let theirs = apoc_nif::Nif::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    println!(
        "blocks {} / {}; outputs {:?}",
        ours.blocks.len(),
        theirs.blocks.len(),
        built.outputs.iter().map(|o| &o.path).collect::<Vec<_>>()
    );
    for i in 0..ours.blocks.len().max(theirs.blocks.len()) {
        let of = |n: &apoc_nif::Nif| {
            if i < n.blocks.len() {
                format!("{} {}", n.kind_of(i), n.blocks[i].data.len())
            } else {
                Default::default()
            }
        };
        let same = i < ours.blocks.len()
            && i < theirs.blocks.len()
            && ours.blocks[i].data == theirs.blocks[i].data;
        println!(
            "{i:3} {:44} {:44} {}",
            of(&ours),
            of(&theirs),
            if same { "same" } else { "DIFFERS" }
        );
        if !same
            && i < ours.blocks.len()
            && i < theirs.blocks.len()
            && apoc_nif::geometry::is_tri_shape(ours.kind_of(i))
        {
            use apoc_nif::geometry::{Layout, TriShape};
            let stream = ours.header.stream.version;
            let (Ok(a), Ok(b)) = (
                TriShape::read(&ours.blocks[i].data, stream),
                TriShape::read(&theirs.blocks[i].data, stream),
            ) else {
                continue;
            };
            let stride = Layout::of(a.vertex_desc, stream).map_or(1, |l| l.stride.max(1));
            let mut by = vec![0usize; stride];
            for (k, (x, y)) in a.vertex_data.iter().zip(&b.vertex_data).enumerate() {
                by[k % stride] += (x != y) as usize;
            }
            println!("      desc {:016x}/{:016x} stride {stride}; bytes differing by place in a vertex: {by:?}", a.vertex_desc, b.vertex_desc);
            println!(
                "      bounds {:?} {} / {:?} {}; triangles same {}; rest same {} ({} / {} bytes)",
                a.bound_center,
                a.bound_radius,
                b.bound_center,
                b.bound_radius,
                a.triangles == b.triangles,
                a.rest == b.rest,
                a.rest.len(),
                b.rest.len()
            );
            // The first vertex that differs, as each has it.
            if let Some(v) = (0..a.vertex_data.len() / stride).find(|v| {
                a.vertex_data[v * stride..(v + 1) * stride]
                    != b.vertex_data[v * stride..(v + 1) * stride]
            }) {
                println!(
                    "      vertex {v}: {:02x?}",
                    &a.vertex_data[v * stride..(v + 1) * stride]
                );
                println!(
                    "      theirs    : {:02x?}",
                    &b.vertex_data[v * stride..(v + 1) * stride]
                );
            }
        }
    }
}
