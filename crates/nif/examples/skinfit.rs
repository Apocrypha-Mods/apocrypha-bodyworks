//! Over many files: which rules a skinned shape's three copies of its
//! weights keep with each other, counted.
//!
//!     cargo run --release -p apoc-nif --example skinfit -- <folder>

use apoc_nif::geometry::{is_tri_shape, skin_weights_in, Layout, TriShape};
use apoc_nif::mesh;
use apoc_nif::skin::{SkinData, SkinInstance, SkinPartition};
use apoc_nif::Nif;
use std::collections::BTreeMap;
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            walk(&e.path(), out);
        } else if e
            .path()
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("nif"))
        {
            out.push(e.path());
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for root in std::env::args().skip(1) {
        walk(Path::new(&root), &mut files);
    }
    let mut tally: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let mut say = |what: &'static str, yes: bool| {
        let t = tally.entry(what).or_default();
        t.0 += yes as usize;
        t.1 += 1;
    };
    let mut shown = 0;
    for path in files {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        let stream = nif.header.stream.version;
        for at in 0..nif.blocks.len() {
            if !is_tri_shape(nif.kind_of(at)) {
                continue;
            }
            let Ok(shape) = TriShape::read(&nif.blocks[at].data, stream) else {
                continue;
            };
            let Ok(skin) = usize::try_from(shape.skin) else {
                continue;
            };
            let kind = nif.kind_of(skin).to_string();
            let Ok(instance) = SkinInstance::read(&nif.blocks[skin].data, kind != "NiSkinInstance")
            else {
                continue;
            };
            let Some(data) = usize::try_from(instance.data)
                .ok()
                .and_then(|d| SkinData::read(&nif.blocks[d].data).ok())
            else {
                continue;
            };
            let Some(part) = usize::try_from(instance.partition)
                .ok()
                .and_then(|p| SkinPartition::read(&nif.blocks[p].data, stream).ok())
            else {
                continue;
            };
            let Some(layout) = part.vertex_desc.and_then(|d| Layout::of(d, stream)) else {
                continue;
            };
            let Ok(m) = mesh::read(&nif, at) else {
                continue;
            };
            let n = m.positions.len();
            // The vertices' own copy: bone numbers into the skin's list.
            let own: Vec<([f32; 4], [u8; 4])> = (0..n)
                .filter_map(|v| skin_weights_in(&part.vertex_data, &layout, v))
                .collect();
            if own.len() != n {
                continue;
            }
            say(
                "vertex bone numbers are within the skin's list",
                own.iter().all(|(w, b)| {
                    (0..4).all(|k| w[k] == 0.0 || (b[k] as usize) < instance.bones.len())
                }),
            );
            say(
                "a vertex's weights add up to one (to 0.01)",
                own.iter()
                    .all(|(w, _)| (w.iter().sum::<f32>() - 1.0).abs() < 0.01),
            );
            say(
                "a vertex's weights are heaviest first",
                own.iter().all(|(w, _)| w.windows(2).all(|x| x[0] >= x[1])),
            );
            say(
                "an unused slot's bone number is 0",
                own.iter()
                    .all(|(w, b)| (0..4).all(|k| w[k] != 0.0 || b[k] == 0)),
            );
            // The skin data's copy, by bone.
            say(
                "skin data has as many bones as the skin",
                data.bones.len() == instance.bones.len(),
            );
            let mut from_data: Vec<Vec<(u8, f32)>> = vec![Vec::new(); n];
            let mut sorted = true;
            let mut zeros = false;
            for (b, bone) in data.bones.iter().enumerate() {
                sorted &= bone.weights.windows(2).all(|x| x[0].0 < x[1].0);
                for (v, w) in &bone.weights {
                    zeros |= *w == 0.0;
                    if let Some(l) = from_data.get_mut(*v as usize) {
                        l.push((b as u8, *w));
                    }
                }
            }
            say("skin data lists a bone's vertices in order", sorted);
            say("skin data lists no weight of nothing", !zeros);
            let agree = (0..n).all(|v| {
                let (w, b) = own[v];
                let mut a: Vec<(u8, f32)> = (0..4)
                    .filter(|k| w[*k] > 0.0)
                    .map(|k| (b[k], w[k]))
                    .collect();
                let mut d = from_data[v].clone();
                a.sort_by_key(|x| x.0);
                d.sort_by_key(|x| x.0);
                a.len() == d.len()
                    && a.iter()
                        .zip(&d)
                        .all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 0.002)
            });
            say("skin data and the vertices agree (to 0.002)", agree);
            let exact = (0..n).all(|v| {
                let (w, b) = own[v];
                from_data[v]
                    .iter()
                    .all(|(bone, weight)| (0..4).any(|k| b[k] == *bone && w[k] == *weight))
            });
            say(
                "skin data's weights are the vertices' exactly (halved floats)",
                exact,
            );
            // Each bone's sphere, against two ways of making one.
            for bone in &data.bones {
                let pts: Vec<[f32; 3]> = bone
                    .weights
                    .iter()
                    .filter_map(|(v, _)| m.positions.get(*v as usize))
                    .map(|p| bone.transform.apply(*p))
                    .collect();
                if pts.is_empty() {
                    say(
                        "a bone moving nothing has a sphere of nothing",
                        bone.bound_radius == 0.0,
                    );
                    continue;
                }
                let (c, r) = mesh::bounding_sphere(&pts);
                let near = |a: [f32; 3], b: [f32; 3], ra: f32, rb: f32| {
                    (0..3).all(|k| (a[k] - b[k]).abs() < 0.01) && (ra - rb).abs() < 0.01
                };
                let boxed = near(c, bone.bound_center, r, bone.bound_radius);
                say(
                    "a bone's sphere is the box's middle, in the bone's space",
                    boxed,
                );
                let mean =
                    [0, 1, 2].map(|k| pts.iter().map(|p| p[k]).sum::<f32>() / pts.len() as f32);
                let far = pts
                    .iter()
                    .map(|p| {
                        ((p[0] - mean[0]).powi(2)
                            + (p[1] - mean[1]).powi(2)
                            + (p[2] - mean[2]).powi(2))
                        .sqrt()
                    })
                    .fold(0.0f32, f32::max);
                say(
                    "a bone's sphere is the mean point, in the bone's space",
                    near(mean, bone.bound_center, far, bone.bound_radius),
                );
                if !boxed && shown < 6 {
                    shown += 1;
                    println!(
                        "  {}: stored {:?} r {}, box {:?} r {}, mean {:?} r {}, {} points",
                        path.file_name().unwrap().to_string_lossy(),
                        bone.bound_center,
                        bone.bound_radius,
                        c,
                        r,
                        mean,
                        far,
                        pts.len()
                    );
                }
            }
            // Each partition's copy.
            for p in &part.partitions {
                say(
                    "a partition's bones are in rising order",
                    p.bones.windows(2).all(|x| x[0] < x[1]),
                );
                let (Some(map), Some(weights), Some(indices)) =
                    (&p.vertex_map, &p.vertex_weights, &p.bone_indices)
                else {
                    say("a partition has its own weights", false);
                    continue;
                };
                say("a partition has its own weights", true);
                let mut used = std::collections::BTreeSet::new();
                let mut same = true;
                let mut same_order = true;
                let mut unused_zero = true;
                for (i, v) in map.iter().enumerate() {
                    let (w, b) = own[*v as usize];
                    for k in 0..4 {
                        let (pw, pi) = (weights[i * 4 + k], indices[i * 4 + k]);
                        let global = p.bones.get(pi as usize).copied().unwrap_or(u16::MAX);
                        if w[k] > 0.0 {
                            used.insert(b[k] as u16);
                        }
                        same_order &=
                            (pw - w[k]).abs() < 0.002 && (pw == 0.0 || global == b[k] as u16);
                        if pw == 0.0 {
                            unused_zero &= pi == 0;
                        }
                    }
                    let mut a: Vec<(u16, f32)> = (0..4)
                        .filter(|k| w[*k] > 0.0)
                        .map(|k| (b[k] as u16, w[k]))
                        .collect();
                    let mut q: Vec<(u16, f32)> = (0..4)
                        .filter(|k| weights[i * 4 + k] > 0.0)
                        .map(|k| {
                            (
                                p.bones
                                    .get(indices[i * 4 + k] as usize)
                                    .copied()
                                    .unwrap_or(u16::MAX),
                                weights[i * 4 + k],
                            )
                        })
                        .collect();
                    a.sort_by_key(|x| x.0);
                    q.sort_by_key(|x| x.0);
                    same &= a.len() == q.len()
                        && a.iter()
                            .zip(&q)
                            .all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 0.002);
                }
                say(
                    "a partition's weights are its vertices', through its bones",
                    same,
                );
                say("... and in the same slots", same_order);
                say("a partition's unused slot is its bone 0", unused_zero);
                say(
                    "a partition's bones are just those its vertices use",
                    used.iter().copied().collect::<Vec<_>>() == p.bones,
                );
            }
        }
    }
    for (what, (yes, of)) in tally {
        println!("{yes:6} of {of:6}  {what}");
    }
}
