//! Builds the face of every NPC of Skyrim.esm that has a pre-built face
//! (FaceGen NIF) from its records and the base files, and compares each
//! shape with the pre-built one of the same name: vertex count, positions,
//! triangles, texture paths and tint colour.
//!
//!     cargo run --release -p apoc-character --example facegen [-- <8 hex id>...]
//!
//! The game files are the unpacked archives under `scratch/samples/skyrim`,
//! with `scratch/samples/skyrim-ussep` over them; plugins come from the
//! game's Data folder (`SKYRIM_DATA` to point elsewhere).

mod common;

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use apoc_character::build::{tint, Options};
use apoc_character::files::Files;
use apoc_character::{build, plan, Look, Records};
use apoc_nif::{mesh, Nif};
use apoc_plugin::FormRef;

const FACEGEN: &str = "meshes/actors/character/facegendata/facegeom/skyrim.esm/";

#[derive(Default)]
struct Tally {
    npcs: usize,
    npcs_e3: usize,
    npcs_e5: usize,
    shapes: usize,
    shapes_e3: usize,
    shapes_e5: usize,
    shapes_whole: usize,
    causes: BTreeMap<String, Vec<String>>,
    worst: Vec<(f32, String)>,
}

fn main() {
    let only: Vec<u32> = std::env::args()
        .skip(1)
        .filter_map(|a| u32::from_str_radix(&a, 16).ok())
        .collect();
    let t = Instant::now();
    let order = common::load_order();
    let index = common::index();
    println!(
        "plugins {:.0} ms, {} sample files",
        t.elapsed().as_secs_f64() * 1e3,
        index.len()
    );
    let mut files = Files::new();
    let mut loaded: HashMap<String, bool> = HashMap::new();
    let mut tally = Tally::default();
    let mut time_plan = Duration::ZERO;
    let mut time_build = Duration::ZERO;
    let mut time_write = Duration::ZERO;
    let mut builds = 0usize;
    let mut slowest = (Duration::ZERO, String::new());
    let mut missing: BTreeMap<String, usize> = BTreeMap::new();
    let mut trailing = (0usize, 0usize);

    let mut ids: Vec<u32> = index
        .keys()
        .filter_map(|k| k.strip_prefix(FACEGEN))
        .filter_map(|k| k.strip_suffix(".nif"))
        .filter_map(|k| u32::from_str_radix(k, 16).ok())
        .collect();
    ids.sort();
    if !only.is_empty() {
        ids.retain(|i| only.contains(&(i & 0xFF_FFFF)));
    }
    for id in ids {
        let fg_key = format!("{FACEGEN}{id:08x}.nif");
        let fg_bytes = std::fs::read(&index[&fg_key]).unwrap();
        let fg = match Nif::read(&fg_bytes) {
            Ok(n) => n,
            Err(e) => {
                cause(
                    &mut tally,
                    format!("the pre-built face does not read: {e}"),
                    id,
                );
                continue;
            }
        };
        if !fg.trailing.is_empty() {
            trailing.0 += 1;
            if fg.write() == fg_bytes {
                trailing.1 += 1;
            }
        }
        let form = FormRef::new("Skyrim.esm", id);
        if order.npc(&form).is_none() {
            cause(&mut tally, "no NPC_ of Skyrim.esm".into(), id);
            continue;
        }
        let label = format!(
            "{id:08X} {}",
            order
                .npc(&form)
                .and_then(|n| n.editor_id)
                .unwrap_or_default()
        );
        tally.npcs += 1;
        let t = Instant::now();
        let look = Look::of_form(&order, &form);
        let mut p = match look.and_then(|l| plan(&l, &order)) {
            Ok(p) => p,
            Err(e) => {
                cause(&mut tally, format!("no plan: {e}"), id);
                continue;
            }
        };
        // The pre-built faces hold the head only.
        p.body.clear();
        time_plan += t.elapsed();
        for k in p.files() {
            if loaded.contains_key(&k) {
                continue;
            }
            let have = index.get(&k).map(|path| std::fs::read(path).unwrap());
            loaded.insert(k.clone(), have.is_some());
            match have {
                Some(b) => files.insert(&k, b),
                None => *missing.entry(k).or_default() += 1,
            }
        }
        let options = Options {
            face_tint: Some(format!(
                "data\\Textures\\Actors\\Character\\FaceGenData\\FaceTint\\Skyrim.esm\\{:08X}.dds",
                id
            )),
        };
        let t = Instant::now();
        let built = match build(&p, &files, &options) {
            Ok(b) => b,
            Err(e) => {
                cause(&mut tally, format!("no build: {e}"), id);
                continue;
            }
        };
        let took = t.elapsed();
        time_build += took;
        builds += 1;
        if took > slowest.0 {
            slowest = (took, label.clone());
        }
        let t = Instant::now();
        let bytes = built.nif.write();
        time_write += t.elapsed();
        if Nif::read(&bytes).map(|n| n.write()) != Ok(bytes.clone()) {
            cause(
                &mut tally,
                "the build does not read back as written".into(),
                id,
            );
        }
        for n in &built.notes {
            cause(&mut tally, format!("note: {}", generalise(n)), id);
        }

        // Shape by shape.
        let ours: HashMap<String, usize> = built
            .shapes
            .iter()
            .map(|s| (s.name.to_ascii_lowercase(), s.block))
            .collect();
        let mut worst_npc = 0.0f32;
        let mut whole_npc = true;
        for fb in mesh::shapes(&fg) {
            let theirs = mesh::read(&fg, fb).unwrap();
            tally.shapes += 1;
            let Some(&ob) = ours.get(&theirs.name.to_ascii_lowercase()) else {
                cause(
                    &mut tally,
                    format!("the build has no shape {}", theirs.name),
                    id,
                );
                whole_npc = false;
                worst_npc = f32::INFINITY;
                continue;
            };
            let mine = mesh::read(&built.nif, ob).unwrap();
            let kind = p
                .head
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case(&theirs.name))
                .and_then(|h| h.kind)
                .map(|k| format!("{k:?}"))
                .unwrap_or_default();
            let mut whole = true;
            if mine.positions.len() != theirs.positions.len() {
                cause(&mut tally, format!("{kind}: vertex counts differ"), id);
                worst_npc = f32::INFINITY;
                whole_npc = false;
                continue;
            }
            let (mut most, mut sum) = (0.0f32, 0.0f64);
            for (a, b) in mine.positions.iter().zip(&theirs.positions) {
                let e = (0..3).map(|c| (a[c] - b[c]).abs()).fold(0.0, f32::max);
                most = most.max(e);
                sum += e as f64;
            }
            let mean = sum / mine.positions.len().max(1) as f64;
            if most <= 1e-3 {
                tally.shapes_e3 += 1;
            }
            if most <= 1e-5 {
                tally.shapes_e5 += 1;
            }
            if most > 1e-3 {
                let band = if most > 0.1 {
                    "over 0.1"
                } else if most > 0.01 {
                    "0.01 to 0.1"
                } else {
                    "1e-3 to 0.01"
                };
                // Where the pre-built face left vertices at the model's
                // place that the morphs move.
                let base = p
                    .head
                    .iter()
                    .find(|h| h.name.eq_ignore_ascii_case(&theirs.name))
                    .and_then(|h| apoc_character::Source::read(&files, &h.model))
                    .and_then(|b| Nif::read(b).ok())
                    .and_then(|n| {
                        mesh::shapes(&n)
                            .first()
                            .and_then(|b| mesh::read(&n, *b).ok())
                    });
                let left = base.map_or(0, |b| {
                    (0..mine.positions.len())
                        .filter(|v| {
                            let e = (0..3)
                                .map(|c| (mine.positions[*v][c] - theirs.positions[*v][c]).abs())
                                .fold(0.0, f32::max);
                            e > 1e-3 && theirs.positions[*v] == b.positions[*v]
                        })
                        .count()
                });
                let bad = (0..mine.positions.len())
                    .filter(|v| {
                        (0..3)
                            .map(|c| (mine.positions[*v][c] - theirs.positions[*v][c]).abs())
                            .fold(0.0, f32::max)
                            > 1e-3
                    })
                    .count();
                let why = if left == bad {
                    "every vertex off is one the pre-built face left at the model's place"
                } else if left > 0 {
                    "some vertices off are ones the pre-built face left at the model's place"
                } else {
                    "vertices off"
                };
                cause(
                    &mut tally,
                    format!("{kind}: positions off by {band}: {why}"),
                    id,
                );
                tally.worst.push((
                    most,
                    format!("{label} {} max {most:.5} mean {mean:.6}", theirs.name),
                ));
                whole = false;
            }
            worst_npc = worst_npc.max(most);
            if mine.triangles != theirs.triangles {
                cause(&mut tally, format!("{kind}: triangles differ"), id);
                whole = false;
            }
            let same_textures = mine.textures.len() == theirs.textures.len()
                && mine
                    .textures
                    .iter()
                    .zip(&theirs.textures)
                    .all(|(a, b)| texture(a) == texture(b));
            if !same_textures {
                for (i, (a, b)) in mine.textures.iter().zip(&theirs.textures).enumerate() {
                    if texture(a) != texture(b) {
                        cause(
                            &mut tally,
                            format!(
                                "{kind}: texture slot {i}: ours {}, theirs {}",
                                shape_of(a),
                                shape_of(b)
                            ),
                            id,
                        );
                    }
                }
                if mine.textures.len() != theirs.textures.len() {
                    cause(
                        &mut tally,
                        format!("{kind}: texture slot counts differ"),
                        id,
                    );
                }
                whole = false;
            }
            match (tint(&built.nif, ob), tint(&fg, fb)) {
                (Some((k1, a)), Some((k2, b))) => {
                    if k1 != k2 || (0..3).any(|c| (a[c] - b[c]).abs() > 1e-3) {
                        cause(
                            &mut tally,
                            format!("{kind}: tint kind {k1}/{k2} colour differs"),
                            id,
                        );
                        whole = false;
                    }
                }
                (None, None) => {}
                (a, b) => {
                    cause(
                        &mut tally,
                        format!("{kind}: tint ours {a:?} theirs {:?}", b.map(|b| b.0)),
                        id,
                    );
                    whole = false;
                }
            }
            if whole {
                tally.shapes_whole += 1;
            } else {
                whole_npc = false;
            }
        }
        let _ = whole_npc;
        // A face far off: would the records before a later plugin make it?
        if worst_npc > 0.05 && worst_npc.is_finite() {
            for n in (1..order.plugins.len()).rev() {
                let shorter = apoc_plugin::LoadOrder::new(order.plugins[..n].to_vec());
                let e = max_error(&shorter, &form, &fg, &mut files, &index, &mut loaded);
                if e.is_some_and(|e| e < worst_npc / 10.0) {
                    cause(
                        &mut tally,
                        format!(
                            "the face was made from the records before {} (off by {:.4} with them)",
                            order.plugins[n].name(),
                            e.unwrap()
                        ),
                        id,
                    );
                    break;
                }
            }
        }
        if worst_npc <= 1e-3 {
            tally.npcs_e3 += 1;
        }
        if worst_npc <= 1e-5 {
            tally.npcs_e5 += 1;
        }
    }

    println!(
        "{} NPCs: {} within 1e-3 in every shape, {} within 1e-5",
        tally.npcs, tally.npcs_e3, tally.npcs_e5
    );
    println!(
        "{} shapes: {} within 1e-3, {} within 1e-5; {} match in everything (positions within 1e-3, triangles, textures, tint)",
        tally.shapes, tally.shapes_e3, tally.shapes_e5, tally.shapes_whole
    );
    println!(
        "{} pre-built faces have bytes after the footer; {} of them write back byte for byte",
        trailing.0, trailing.1
    );
    let per = |d: Duration| d.as_secs_f64() * 1e3 / builds.max(1) as f64;
    println!(
        "per NPC: plan {:.3} ms, build {:.3} ms, write {:.3} ms; slowest build {:.2} ms ({})",
        per(time_plan),
        per(time_build),
        per(time_write),
        slowest.0.as_secs_f64() * 1e3,
        slowest.1
    );
    if !missing.is_empty() {
        println!("files the records name that are not in the samples:");
        for (k, n) in &missing {
            println!("  {n:>4}  {k}");
        }
    }
    let mut causes: Vec<_> = tally.causes.into_iter().collect();
    causes.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    println!("causes:");
    for (c, who) in causes {
        println!("  {:>5}  {c}", who.len());
        println!(
            "         {}",
            who.iter().take(5).cloned().collect::<Vec<_>>().join(" ")
        );
    }
    tally.worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("worst shapes:");
    for (_, w) in tally.worst.iter().take(15) {
        println!("  {w}");
    }
}

/// A texture path as the game finds it: under `textures`, whether or not
/// it says so, without case.
fn texture(p: &str) -> String {
    let k = apoc_character::files::key(p);
    k.strip_prefix("textures/").map(str::to_string).unwrap_or(k)
}

/// The largest position error over a face's shapes, built from `order`.
fn max_error(
    order: &apoc_plugin::LoadOrder,
    form: &FormRef,
    fg: &Nif,
    files: &mut Files,
    index: &HashMap<String, std::path::PathBuf>,
    loaded: &mut HashMap<String, bool>,
) -> Option<f32> {
    let look = Look::of_form(order, form).ok()?;
    let mut p = plan(&look, order).ok()?;
    p.body.clear();
    for k in p.files() {
        if loaded.contains_key(&k) {
            continue;
        }
        let have = index.get(&k).map(|path| std::fs::read(path).unwrap());
        loaded.insert(k.clone(), have.is_some());
        if let Some(b) = have {
            files.insert(&k, b);
        }
    }
    let built = build(&p, files, &Options::default()).ok()?;
    let mut worst = 0.0f32;
    for fb in mesh::shapes(fg) {
        let theirs = mesh::read(fg, fb).ok()?;
        let s = built
            .shapes
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&theirs.name))?;
        let mine = mesh::read(&built.nif, s.block).ok()?;
        if mine.positions.len() != theirs.positions.len() {
            return None;
        }
        for (a, b) in mine.positions.iter().zip(&theirs.positions) {
            worst = worst.max((0..3).map(|c| (a[c] - b[c]).abs()).fold(0.0, f32::max));
        }
    }
    Some(worst)
}

fn cause(t: &mut Tally, why: String, id: u32) {
    t.causes.entry(why).or_default().push(format!("{id:08X}"));
}

/// A texture path with its file name kept and the rest shortened, so like
/// differences group together.
fn shape_of(p: &str) -> String {
    if p.is_empty() {
        return "nothing".into();
    }
    let lower = p.to_ascii_lowercase().replace('/', "\\");
    if lower.contains("facetint") {
        return "a FaceTint path".into();
    }
    let prefix = if lower.starts_with("textures\\") {
        "textures\\…\\"
    } else if lower.starts_with("data\\") {
        "data\\…\\"
    } else {
        "…\\"
    };
    format!("{prefix}{}", lower.rsplit('\\').next().unwrap_or(""))
}

/// A note with its numbers taken out.
fn generalise(n: &str) -> String {
    n.chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}
