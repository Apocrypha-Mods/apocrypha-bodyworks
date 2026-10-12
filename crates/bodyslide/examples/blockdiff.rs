//! Build one set at one preset with morphs and say where each shape differs
//! from the original program's file: bounds, and each stored attribute.
//!
//!     cargo run --release -p apoc-bodyslide --example blockdiff -- <set> <preset or -> <their _1.nif> <folder>...

use apoc_bodyslide::{build, morph, preset, set};
use apoc_nif::geometry::{attr, Layout, TriShape};
use apoc_nif::skin::{SkinInstance, SkinPartition};
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
    let tools = tools.as_str();
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
    let s = sets.iter().find(|s| s.name == args[0]).unwrap();
    let p = presets.iter().find(|p| p.name == args[1]);
    let built = build::build_with(
        s,
        &morph::values(s, p, false),
        &morph::values(s, p, true),
        &files,
        true,
    )
    .unwrap();
    let ours = apoc_nif::Nif::read(&built.outputs.last().unwrap().bytes).unwrap();
    let theirs = apoc_nif::Nif::read(&std::fs::read(&args[2]).unwrap()).unwrap();
    println!("blocks {} / {}", ours.blocks.len(), theirs.blocks.len());
    for i in 0..ours.blocks.len().min(theirs.blocks.len()) {
        if ours.blocks[i].data == theirs.blocks[i].data {
            continue;
        }
        let kind = ours.kind_of(i);
        println!(
            "[{i}] {kind} differs ({} / {} bytes)",
            ours.blocks[i].data.len(),
            theirs.blocks[i].data.len()
        );
        // A partition is looked at through the shape it belongs to.
        let owner = |nif: &apoc_nif::Nif| {
            (0..nif.blocks.len()).find(|j| {
                nif.kind_of(*j) == "BSTriShape"
                    && TriShape::read(&nif.blocks[*j].data, 100)
                        .ok()
                        .is_some_and(|s| {
                            s.skin >= 0
                                && SkinInstance::read(&nif.blocks[s.skin as usize].data, true)
                                    .is_ok_and(|x| x.partition == i as i32)
                        })
            })
        };
        let (ia, ib) = match kind {
            "BSTriShape" => (i, i),
            "NiSkinPartition" => match (owner(&ours), owner(&theirs)) {
                (Some(x), Some(y)) => (x, y),
                _ => continue,
            },
            _ => continue,
        };
        let (a, b) = (
            TriShape::read(&ours.blocks[ia].data, 100).unwrap(),
            TriShape::read(&theirs.blocks[ib].data, 100).unwrap(),
        );
        println!(
            "    bound ours {:?} r {}   theirs {:?} r {}",
            a.bound_center, a.bound_radius, b.bound_center, b.bound_radius
        );
        let layout = Layout::of(a.vertex_desc, 100).unwrap();
        let part = |nif: &apoc_nif::Nif, s: &TriShape| {
            let inst = SkinInstance::read(&nif.blocks[s.skin as usize].data, true).unwrap();
            SkinPartition::read(&nif.blocks[inst.partition as usize].data, 100).unwrap()
        };
        let (pa, pb) = (part(&ours, &a), part(&theirs, &b));
        let (mut va, mut vb) = (a.clone(), b.clone());
        va.vertex_data = pa.vertex_data.clone();
        vb.vertex_data = pb.vertex_data.clone();
        let n = va.vertex_data.len() / layout.stride;
        if vb.vertex_data.len() != va.vertex_data.len() {
            println!("    vertex data sizes differ");
            continue;
        }
        let (mut dp, mut dn, mut dt, mut duv, mut dw) = (0, 0, 0, 0, 0);
        let (mut maxp, mut maxn, mut maxt) = (0.0f32, 0.0f32, 0.0f32);
        let dist = |x: [f32; 3], y: [f32; 3]| {
            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
        };
        let mut shown = 0;
        for v in 0..n {
            let (x, y) = (
                va.position(&layout, v).unwrap(),
                vb.position(&layout, v).unwrap(),
            );
            if x != y {
                dp += 1;
                maxp = maxp.max(dist(x, y));
            }
            if va.uv(&layout, v) != vb.uv(&layout, v) {
                duv += 1;
            }
            if layout.has(attr::NORMAL) {
                let (x, y) = (
                    va.normal(&layout, v).unwrap(),
                    vb.normal(&layout, v).unwrap(),
                );
                if x != y {
                    dn += 1;
                    maxn = maxn.max(dist(x, y));
                    if shown < 3 {
                        shown += 1;
                        println!("      normal v{v}: ours {x:?} theirs {y:?}");
                    }
                }
            }
            if layout.has(attr::TANGENT) {
                let (x, y) = (
                    va.tangent_space(&layout, v).unwrap(),
                    vb.tangent_space(&layout, v).unwrap(),
                );
                if x != y {
                    dt += 1;
                    maxt = maxt.max(dist(x.0, y.0)).max(dist(x.1, y.1));
                    if shown < 6 {
                        shown += 1;
                        println!("      tangent v{v}: ours {x:?}\n               theirs {y:?}");
                    }
                }
            }
            if va.skin_weights(&layout, v) != vb.skin_weights(&layout, v) {
                dw += 1;
            }
        }
        println!("    {n} vertices: positions differ {dp} (max {maxp}), uvs {duv}, normals {dn} (max {maxn}), tangents {dt} (max {maxt}), weights {dw}");
        println!(
            "    partitions the same apart from vertices: {}",
            pa.partitions == pb.partitions
        );
    }
}
