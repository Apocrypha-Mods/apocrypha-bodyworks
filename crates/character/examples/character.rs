//! Builds one NPC whole, body and head, from its records and the base
//! files, writes it, reads it back, and opens it as a Bodyworks project as
//! a mesh dropped in would be. Says what each shape is and how long a build
//! takes.
//!
//!     cargo run --release -p apoc-character --example character -- <8 hex id> [out.nif]

mod common;

use std::time::Instant;

use apoc_bodyslide::project::Project;
use apoc_character::build::tint;
use apoc_character::{build, needed, plan, Files, Look, Options};
use apoc_nif::{mesh, Nif};
use apoc_plugin::FormRef;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let id = u32::from_str_radix(args.first().map_or("1414D", String::as_str), 16).unwrap();
    let order = common::load_order();
    let index = common::index();
    let look = Look::of_form(&order, &FormRef::new("Skyrim.esm", id)).unwrap();

    let t = Instant::now();
    let wanted = needed(&look, &order).unwrap();
    println!(
        "{} files needed ({:.3} ms):",
        wanted.len(),
        t.elapsed().as_secs_f64() * 1e3
    );
    let mut files = Files::new();
    for k in &wanted {
        match index.get(k) {
            Some(p) => files.insert(k, std::fs::read(p).unwrap()),
            None => println!("  not in the samples: {k}"),
        }
    }

    let p = plan(&look, &order).unwrap();
    let options = Options::default();
    let built = build(&p, &files, &options).unwrap();
    // Steady timings: plan and build together, many times.
    let runs = 200;
    let t = Instant::now();
    for _ in 0..runs {
        let p = plan(&look, &order).unwrap();
        std::hint::black_box(build(&p, &files, &options).unwrap());
    }
    let each = t.elapsed().as_secs_f64() * 1e3 / runs as f64;
    println!("plan and build: {each:.3} ms each over {runs}");

    for s in &built.shapes {
        let m = mesh::read(&built.nif, s.block).unwrap();
        println!(
            "  {:<36} {} v{:<5} t{:<5} skinned {} bones {} tint {:?}\n      {:?}",
            s.name,
            nif_kind(&built.nif, s.block),
            m.positions.len(),
            m.triangles.len(),
            m.skinned,
            mesh::bone_names(&built.nif, s.block).len(),
            tint(&built.nif, s.block),
            m.textures
                .iter()
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        );
    }
    for m in &built.missing {
        println!("  missing: {m}");
    }
    for n in &built.notes {
        println!("  note: {n}");
    }

    // The body at the weight lies between its lightest and heaviest.
    for piece in &p.body {
        let Some((light, heavy)) = &piece.weights else {
            continue;
        };
        let read =
            |k: &str| apoc_character::Source::read(&files, k).and_then(|b| Nif::read(b).ok());
        let (Some(l), Some(h)) = (read(light), read(heavy)) else {
            continue;
        };
        let (lm, hm) = (
            mesh::read(&l, mesh::shapes(&l)[0]).unwrap(),
            mesh::read(&h, mesh::shapes(&h)[0]).unwrap(),
        );
        let Some(ours) = built
            .shapes
            .iter()
            .find(|s| s.name == hm.name && !s.head)
            .and_then(|s| mesh::read(&built.nif, s.block).ok())
        else {
            continue;
        };
        let worst = ours
            .positions
            .iter()
            .zip(lm.positions.iter().zip(&hm.positions))
            .map(|(o, (a, b))| {
                (0..3)
                    .map(|c| (o[c] - (a[c] + (b[c] - a[c]) * p.weight)).abs())
                    .fold(0.0f32, f32::max)
            })
            .fold(0.0f32, f32::max);
        println!(
            "  {}: weight {:.2}, off the blend of {} and {} by {worst:.2e}",
            piece.name, p.weight, light, heavy
        );
    }

    let bytes = built.nif.write();
    let back = Nif::read(&bytes).unwrap();
    assert_eq!(back.write(), bytes, "reads back as written");
    println!("written: {} bytes, reads back the same", bytes.len());
    match Project::from_mesh("Character", "meshes/character.nif", &bytes) {
        Ok(project) => println!(
            "opens as a project: {} shapes",
            mesh::shapes(&project.nif).len()
        ),
        Err(e) => println!("does not open as a project: {e}"),
    }
    if let Some(out) = args.get(1) {
        std::fs::write(out, &bytes).unwrap();
        println!("written to {out}");
    }
}

fn nif_kind(nif: &Nif, block: usize) -> &str {
    nif.kind_of(block)
}
