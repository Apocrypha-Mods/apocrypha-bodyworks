//! For one set: which of the source mesh's vertices the original program's
//! output still has that the lists used to take vertices out mark, and what
//! each list says about them.
//!
//!     cargo run --release -p apoc-bodyslide --example kept -- <BodySlide folder> <set name> <shape name> <their _1.nif>

use apoc_bodyslide::{osd::SliderData, set};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn find(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        at = std::fs::read_dir(&at)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?
            .path();
    }
    Some(at)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(&args[0]);
    let mut sets = Vec::new();
    for e in std::fs::read_dir(find(&root, "SliderSets").unwrap())
        .unwrap()
        .flatten()
    {
        sets.extend(set::read(&std::fs::read(e.path()).unwrap()).unwrap_or_default());
    }
    let s = sets
        .iter()
        .find(|s| s.name == args[1])
        .expect("no such set");
    let shape = s
        .shapes
        .iter()
        .find(|x| x.name == args[2])
        .expect("no such shape");
    let read = |path: &Path| {
        let nif = apoc_nif::Nif::read(&std::fs::read(path).unwrap()).unwrap();
        apoc_nif::mesh::shapes(&nif)
            .into_iter()
            .filter_map(|at| apoc_nif::mesh::read(&nif, at).ok())
            .find(|m| m.name == args[2])
            .unwrap()
    };
    let base = read(
        &find(
            &root,
            &format!("ShapeData/{}/{}", s.data_folder, s.source_file),
        )
        .unwrap(),
    );
    let theirs = read(Path::new(&args[3]));
    // Texture coordinates do not move with the sliders: they say which of
    // the source's vertices each of theirs is.
    let key = |uv: [f32; 2]| (uv[0].to_bits(), uv[1].to_bits());
    let mut have: HashMap<(u32, u32), usize> = HashMap::new();
    for uv in &theirs.uvs {
        *have.entry(key(*uv)).or_default() += 1;
    }
    let mut used = vec![false; base.positions.len()];
    for t in &base.triangles {
        for i in t {
            used[*i as usize] = true;
        }
    }
    println!(
        "source {} vertices, theirs {}",
        base.positions.len(),
        theirs.positions.len()
    );
    for slider in s.sliders.iter().filter(|x| x.zap) {
        for d in slider.data.iter().filter(|d| d.target == shape.target) {
            let folder = if d.local {
                s.data_folder.as_str()
            } else {
                shape.data_folder.as_deref().unwrap_or(&s.data_folder)
            };
            let Some(path) = find(&root, &format!("ShapeData/{folder}/{}", d.file)) else {
                continue;
            };
            let data = SliderData::read(&std::fs::read(path).unwrap()).unwrap();
            let Some(list) = data.list(&d.list) else {
                continue;
            };
            let marked: Vec<_> = list.iter().filter(|x| x.offset != [0.0; 3]).collect();
            // Vertices can share coordinates: one of theirs at a marked
            // vertex's coordinates only counts when the unmarked vertices
            // there do not account for it.
            let is_marked: std::collections::HashSet<u16> =
                marked.iter().map(|x| x.vertex).collect();
            let mut unmarked: HashMap<(u32, u32), usize> = HashMap::new();
            for (v, uv) in base.uvs.iter().enumerate() {
                if !is_marked.contains(&(v as u16)) {
                    *unmarked.entry(key(*uv)).or_default() += 1;
                }
            }
            let still: Vec<_> = marked
                .iter()
                .filter(|x| {
                    let k = key(base.uvs[x.vertex as usize]);
                    have.get(&k).copied().unwrap_or(0) > unmarked.get(&k).copied().unwrap_or(0)
                })
                .collect();
            println!(
                "{} (low {} high {}): marks {}, of which theirs still has {}",
                slider.name,
                slider.small,
                slider.big,
                marked.len(),
                still.len()
            );
            for x in still.iter().take(12) {
                println!(
                    "    vertex {} offset {:?} in a triangle: {}",
                    x.vertex, x.offset, used[x.vertex as usize]
                );
            }
        }
    }
}
