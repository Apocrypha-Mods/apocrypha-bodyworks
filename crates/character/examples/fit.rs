//! Fits each head part of an NPC's pre-built face: the weights of every
//! morph its tris have (race, `SkinnyMorph`, chargen) that best explain the
//! face minus the part's model, by least squares, with what is left over.
//! How the rules in `morph.rs` were found.
//!
//!     cargo run --release -p apoc-character --example fit -- <8 hex id>...
//!
//! `ONLY=a,b` fits only those morphs; `PART=Head` only the parts whose
//! editor id has that in it.
#![allow(clippy::needless_range_loop, clippy::ptr_arg)]

mod common;
use apoc_bodyslide::headtri;
use apoc_character::{parts, Look, Records};
use apoc_nif::{mesh, Nif};
use apoc_plugin::records::PartFileKind;
use apoc_plugin::FormRef;
use std::collections::HashMap;
use std::path::PathBuf;

fn read_nif(p: &PathBuf) -> Nif {
    let b = std::fs::read(p).unwrap();
    match Nif::read(&b) {
        Ok(n) => n,
        Err(apoc_nif::NifError::Corrupt(m)) => {
            let n: usize = m.split(' ').next().unwrap().parse().unwrap();
            Nif::read(&b[..b.len() - n]).unwrap()
        }
        Err(e) => panic!("{e}"),
    }
}

fn solve(a: &mut Vec<Vec<f64>>, b: &mut Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for i in 0..n {
        a[i][i] += 1e-9;
    }
    for c in 0..n {
        let p = (c..n)
            .max_by(|x, y| a[*x][c].abs().partial_cmp(&a[*y][c].abs()).unwrap())
            .unwrap();
        a.swap(c, p);
        b.swap(c, p);
        let d = a[c][c];
        if d.abs() < 1e-14 {
            continue;
        }
        for r in 0..n {
            if r != c {
                let f = a[r][c] / d;
                if f != 0.0 {
                    for k in c..n {
                        a[r][k] -= f * a[c][k];
                    }
                    b[r] -= f * b[c];
                }
            }
        }
    }
    (0..n)
        .map(|i| {
            if a[i][i].abs() < 1e-14 {
                0.0
            } else {
                b[i] / a[i][i]
            }
        })
        .collect()
}

fn main() {
    let order = common::load_order();
    let idx = common::index();
    let get = |p: &str, pre: &str| -> Option<PathBuf> {
        idx.get(&format!(
            "{pre}{}",
            p.replace('\\', "/").to_ascii_lowercase()
        ))
        .cloned()
    };
    for a in std::env::args().skip(1) {
        let id = u32::from_str_radix(&a, 16).unwrap();
        let look = Look::of_form(&order, &FormRef::new("Skyrim.esm", id)).unwrap();
        println!(
            "=== {a} NAM9 {:?}\n    NAMA {:?} weight {}",
            look.face_morphs, look.face_parts, look.weight
        );
        let fg = read_nif(
            &get(
                &format!("actors/character/facegendata/facegeom/skyrim.esm/{id:08x}.nif"),
                "meshes/",
            )
            .unwrap(),
        );
        let fgs: HashMap<String, usize> = mesh::shapes(&fg)
            .into_iter()
            .map(|b| (mesh::name(&fg, b).unwrap().to_ascii_lowercase(), b))
            .collect();
        let race = order.race(look.race.as_ref().unwrap()).unwrap();
        for p in parts::head_parts(&order, &race, look.female, &look.head_parts) {
            let h = order.head_part(&p).unwrap();
            if !parts::has_model(&h) {
                continue;
            }
            if let Ok(pp) = std::env::var("PART") {
                if !h.editor_id.as_deref().unwrap_or("").contains(&pp) {
                    continue;
                }
            }
            let ed = h.editor_id.clone().unwrap();
            let Some(&fb) = fgs.get(&ed.to_ascii_lowercase()) else {
                println!("{ed}: not in facegen");
                continue;
            };
            let target = mesh::read(&fg, fb).unwrap();
            let base_nif = read_nif(&get(h.model.as_ref().unwrap(), "meshes/").unwrap());
            let bb = mesh::shapes(&base_nif)[0];
            let base = mesh::read(&base_nif, bb).unwrap();
            let same_uv = base.uvs == target.uvs;
            let same_tri = base.triangles == target.triangles;
            let mut morphs: Vec<(String, Vec<[f64; 3]>)> = Vec::new();
            for f in &h.files {
                let Some(path) = get(&f.path, "meshes/") else {
                    println!("  missing {}", f.path);
                    continue;
                };
                let t = headtri::read(&std::fs::read(path).unwrap()).unwrap();
                let tag = match f.kind {
                    PartFileKind::RaceMorph => "R:",
                    PartFileKind::ChargenMorph => "C:",
                    _ => "T:",
                };
                if f.kind == PartFileKind::Tri {
                    // tri base vs nif base
                    let d = t
                        .vertices
                        .iter()
                        .zip(&base.positions)
                        .map(|(a, b)| (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0f32, f32::max))
                        .fold(0.0f32, f32::max);
                    println!("  tri base vs nif max {d}");
                }
                let only: Option<Vec<String>> = std::env::var("ONLY")
                    .ok()
                    .map(|s| s.split(',').map(String::from).collect());
                for m in t.morphs {
                    if f.kind == PartFileKind::Tri && m.name != "SkinnyMorph" {
                        continue;
                    }
                    if let Some(o) = &only {
                        if !o.contains(&m.name) {
                            continue;
                        }
                    }
                    let o = m
                        .offsets
                        .iter()
                        .map(|o| {
                            [
                                o[0] as f64 * m.scale as f64,
                                o[1] as f64 * m.scale as f64,
                                o[2] as f64 * m.scale as f64,
                            ]
                        })
                        .collect();
                    morphs.push((format!("{tag}{}", m.name), o));
                }
            }
            let n = base.positions.len();
            let diff: Vec<[f64; 3]> = (0..n)
                .map(|v| [0, 1, 2].map(|i| (target.positions[v][i] - base.positions[v][i]) as f64))
                .collect();
            let k = morphs.len();
            let mut ata = vec![vec![0.0; k]; k];
            let mut atb = vec![0.0; k];
            for i in 0..k {
                for j in i..k {
                    let s: f64 = (0..n)
                        .map(|v| {
                            (0..3)
                                .map(|c| morphs[i].1[v][c] * morphs[j].1[v][c])
                                .sum::<f64>()
                        })
                        .sum();
                    ata[i][j] = s;
                    ata[j][i] = s;
                }
                atb[i] = (0..n)
                    .map(|v| (0..3).map(|c| morphs[i].1[v][c] * diff[v][c]).sum::<f64>())
                    .sum();
            }
            let w = solve(&mut ata, &mut atb);
            let mut resid = 0.0f64;
            for v in 0..n {
                for c in 0..3 {
                    let p: f64 = (0..k).map(|i| w[i] * morphs[i].1[v][c]).sum();
                    resid = resid.max((diff[v][c] - p).abs());
                }
            }
            let maxd = diff.iter().flatten().fold(0.0f64, |m, x| m.max(x.abs()));
            println!("{ed} v{n}/{} uv same {same_uv} tri same {same_tri} max diff {maxd:.4} resid {resid:.6}", target.positions.len());
            let mut ws: Vec<(String, f64)> = morphs
                .iter()
                .zip(&w)
                .filter(|(_, w)| w.abs() > 1e-3)
                .map(|(m, w)| (m.0.clone(), *w))
                .collect();
            ws.sort_by(|a, b| a.0.cmp(&b.0));
            for (m, w) in ws {
                print!(" {m}={w:.4}");
            }
            println!();
        }
    }
}
