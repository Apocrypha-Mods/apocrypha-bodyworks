//! For one head part, across every pre-built face that has it: which
//! vertices the face leaves at the model's place although the morphs move
//! them, and whether that set is the same for every face.
mod common;
use apoc_character::{build, plan, Files, Look, Options, Source};
use apoc_nif::{mesh, Nif};
use apoc_plugin::FormRef;
use std::collections::BTreeSet;
fn main() {
    let part = std::env::args().nth(1).unwrap().to_ascii_lowercase();
    let order = common::load_order();
    let index = common::index();
    let mut files = Files::new();
    let pre = "meshes/actors/character/facegendata/facegeom/skyrim.esm/";
    let mut sets: Vec<(u32, BTreeSet<usize>, usize)> = vec![];
    let mut keys: Vec<&String> = index.keys().filter(|k| k.starts_with(pre)).collect();
    keys.sort();
    for k in keys {
        let id = u32::from_str_radix(&k[pre.len()..pre.len() + 8], 16).unwrap();
        let Ok(look) = Look::of_form(&order, &FormRef::new("Skyrim.esm", id)) else {
            continue;
        };
        let Ok(mut p) = plan(&look, &order) else {
            continue;
        };
        p.body.clear();
        p.head.retain(|h| h.name.to_ascii_lowercase() == part);
        if p.head.is_empty() {
            continue;
        }
        for f in p.files() {
            if files.read(&f).is_none() {
                if let Some(x) = index.get(&f) {
                    files.insert(&f, std::fs::read(x).unwrap());
                }
            }
        }
        let Ok(b) = build(&p, &files, &Options::default()) else {
            continue;
        };
        let fg = Nif::read(&std::fs::read(&index[k]).unwrap()).unwrap();
        let Some(fb) = mesh::shapes(&fg)
            .into_iter()
            .find(|b| mesh::name(&fg, *b).unwrap().to_ascii_lowercase() == part)
        else {
            continue;
        };
        let t = mesh::read(&fg, fb).unwrap();
        let m = mesh::read(&b.nif, b.shapes[0].block).unwrap();
        let base_nif = Nif::read(files.read(&p.head[0].model).unwrap()).unwrap();
        let base = mesh::read(&base_nif, mesh::shapes(&base_nif)[0]).unwrap();
        let mut s = BTreeSet::new();
        let mut other = 0;
        for v in 0..m.positions.len() {
            let e = (0..3)
                .map(|c| (m.positions[v][c] - t.positions[v][c]).abs())
                .fold(0.0, f32::max);
            if e > 1e-5 {
                if t.positions[v] == base.positions[v] {
                    s.insert(v);
                } else {
                    other += 1;
                }
            }
        }
        sets.push((id, s, other));
    }
    let union: BTreeSet<usize> = sets.iter().flat_map(|s| s.1.iter().copied()).collect();
    println!("{} faces; union of unmoved {}", sets.len(), union.len());
    for (id, s, o) in sets.iter().take(30) {
        println!(
            "  {id:08X}: {} unmoved, {} other off; missing from union {}",
            s.len(),
            o,
            union.len() - s.len()
        );
    }
    println!("union {:?}", union.iter().take(80).collect::<Vec<_>>());
}
