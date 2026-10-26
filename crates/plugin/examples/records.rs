//! Reads the records a character needs from every plugin in a Data folder,
//! by ranges, and checks them against the game's own pre-built faces.
//!
//!     cargo run --release -p apoc-plugin --example records -- [Data folder] [FaceGen folder]...
//!
//! The FaceGen check: for every `NPC_` of Skyrim.esm that has a FaceGen NIF,
//! work out its head parts as the game would (its own `PNAM`s; the race's
//! defaults for its sex for each type it lacks; each part's `HNAM` extras,
//! recursively; all of it from the template when the template flags say the
//! traits come from there), and compare their editor ids with the names of
//! the NIF's shapes. Later FaceGen folders win over earlier ones (USSEP's
//! replacements over vanilla).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use apoc_nif::geometry::{is_tri_shape, TriShape};
use apoc_nif::{Nif, NifError};
use apoc_plugin::native::read_file;
use apoc_plugin::records::{
    Armor, ArmorAddon, Colour, HeadPart, HeadPartKind, Npc, Race, TextureSet, Typed, EXTRA_PART,
};
use apoc_plugin::{FormRef, LoadOrder, Plugin};

const WANTED: [[u8; 4]; 7] = [
    *b"RACE", *b"HDPT", *b"TXST", *b"ARMO", *b"ARMA", *b"CLFM", *b"NPC_",
];

const ORDER: [&str; 6] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
    "unofficial skyrim special edition patch.esp",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = PathBuf::from(args.first().cloned().unwrap_or_else(|| {
        format!(
            "{}/.local/share/Steam/steamapps/common/Skyrim Special Edition/Data",
            std::env::var("HOME").unwrap_or_default()
        )
    }));
    let facegen: Vec<PathBuf> = if args.len() > 1 {
        args[1..].iter().map(PathBuf::from).collect()
    } else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples");
        let tail = "meshes/actors/character/facegendata/facegeom/skyrim.esm";
        vec![
            root.join("skyrim").join(tail),
            root.join("skyrim-ussep").join(tail),
        ]
    };

    // Every plugin in the folder.
    let mut files: Vec<PathBuf> = std::fs::read_dir(&data)
        .expect("the Data folder")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["esm", "esp", "esl"].contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    files.sort();
    let mut plugins: HashMap<String, Plugin> = HashMap::new();
    let (mut total_bytes, mut total_size, mut total_reads) = (0u64, 0u64, 0u32);
    let all = Instant::now();
    println!(
        "{:<60} {:>11} {:>11} {:>6} {:>5} {:>8}",
        "plugin", "size", "read", "%", "reads", "ms"
    );
    for f in &files {
        let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        let t = Instant::now();
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        match read_file(f, &WANTED) {
            Ok((p, s)) => {
                println!(
                    "{:<60} {:>11} {:>11} {:>6.2} {:>5} {:>8.1}",
                    name,
                    size,
                    s.bytes,
                    100.0 * s.bytes as f64 / size.max(1) as f64,
                    s.reads,
                    t.elapsed().as_secs_f64() * 1e3
                );
                total_bytes += s.bytes;
                total_size += size;
                total_reads += s.reads;
                plugins.insert(name.to_ascii_lowercase(), p);
            }
            Err(e) => println!("{name:<60} failed: {e}"),
        }
    }
    println!(
        "all {} plugins: {} of {} bytes ({:.2}%), {} reads, {:.0} ms",
        plugins.len(),
        total_bytes,
        total_size,
        100.0 * total_bytes as f64 / total_size.max(1) as f64,
        total_reads,
        all.elapsed().as_secs_f64() * 1e3
    );

    // Every record of every plugin read as its type.
    let t = Instant::now();
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = 0;
    let mut names: Vec<&String> = plugins.keys().collect();
    names.sort();
    for n in names {
        let p = &plugins[n];
        for f in &p.failures {
            println!("  {n}: record {:08X} unread: {}", f.0, f.1);
            failures += 1;
        }
        for r in &p.records {
            let ok = match &r.kind {
                b"RACE" => Race::read(r, &p.names).map(|_| ()),
                b"HDPT" => HeadPart::read(r, &p.names).map(|_| ()),
                b"TXST" => TextureSet::read(r, &p.names).map(|_| ()),
                b"ARMO" => Armor::read(r, &p.names).map(|_| ()),
                b"ARMA" => ArmorAddon::read(r, &p.names).map(|_| ()),
                b"CLFM" => Colour::read(r, &p.names).map(|_| ()),
                b"NPC_" => Npc::read(r, &p.names).map(|_| ()),
                _ => Ok(()),
            };
            let c = counts
                .entry(String::from_utf8_lossy(&r.kind).into_owned())
                .or_default();
            c.0 += 1;
            if let Err(e) = ok {
                c.1 += 1;
                failures += 1;
                if c.1 <= 3 {
                    println!(
                        "  {n}: {:08X} {}: {e}",
                        r.id,
                        String::from_utf8_lossy(&r.kind)
                    );
                }
            }
        }
    }
    for (k, (n, bad)) in &counts {
        println!("{k}: {n} read, {bad} failed");
    }
    println!(
        "{failures} failures; typed in {:.0} ms",
        t.elapsed().as_secs_f64() * 1e3
    );

    // The load order for the check.
    let order: Vec<Plugin> = ORDER
        .iter()
        .filter_map(|n| plugins.remove(&n.to_ascii_lowercase()))
        .collect();
    let order = LoadOrder::new(order);
    facegen_check(&order, &facegen);
}

/// The FaceGen NIFs by formID, later folders replacing earlier ones.
fn facegen_files(dirs: &[PathBuf]) -> BTreeMap<u32, PathBuf> {
    let mut out = BTreeMap::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else {
            println!("no FaceGen folder at {}", d.display());
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Ok(id) = u32::from_str_radix(stem, 16) {
                out.insert(id & 0xFF_FFFF, p);
            }
        }
    }
    out
}

fn shape_names(path: &Path) -> Result<BTreeSet<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    // 23 of the game's own FaceGen files have bytes past the NIF's footer,
    // which apoc-nif refuses; the NIF before them is read.
    let nif = match Nif::read(&bytes) {
        Err(NifError::Corrupt(m)) if m.ends_with("bytes after the end") => {
            let n: usize = m
                .split(' ')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            Nif::read(&bytes[..bytes.len().saturating_sub(n)])
        }
        r => r,
    }
    .map_err(|e| e.to_string())?;
    let stream = nif.header.stream.version;
    let mut out = BTreeSet::new();
    for i in 0..nif.blocks.len() {
        if !is_tri_shape(nif.kind_of(i)) {
            continue;
        }
        let s = TriShape::read(&nif.blocks[i].data, stream).map_err(|e| e.to_string())?;
        if let Some(n) = nif.string(s.av.name) {
            out.insert(String::from_utf8_lossy(n).to_ascii_lowercase());
        }
    }
    Ok(out)
}

/// Why an NPC_'s parts could not be worked out.
#[derive(Debug)]
enum Unresolved {
    LeveledTemplate,
    MissingTemplate,
    TemplateLoop,
    NoRace,
}

/// The NPC_ a face comes from: itself, or its template's, followed.
fn face_source(order: &LoadOrder, form: &FormRef) -> Result<Npc, Unresolved> {
    let mut npc = order
        .get::<Npc>(form)
        .and_then(Result::ok)
        .ok_or(Unresolved::MissingTemplate)?;
    for _ in 0..32 {
        if !npc.face_from_template() {
            return Ok(npc);
        }
        let t = npc.template.clone().unwrap();
        match order.winner(&t) {
            Some((_, r)) if &r.kind == b"NPC_" => {
                npc = order
                    .get::<Npc>(&t)
                    .and_then(Result::ok)
                    .ok_or(Unresolved::MissingTemplate)?;
            }
            Some(_) => return Err(Unresolved::LeveledTemplate),
            None => return Err(Unresolved::MissingTemplate),
        }
    }
    Err(Unresolved::TemplateLoop)
}

/// The head parts the game would give `npc`, by form.
fn head_parts(order: &LoadOrder, npc: &Npc) -> Result<Vec<FormRef>, Unresolved> {
    let race = npc
        .race
        .as_ref()
        .and_then(|r| order.get::<Race>(r))
        .and_then(Result::ok)
        .ok_or(Unresolved::NoRace)?;
    let part = |f: &FormRef| order.get::<HeadPart>(f).and_then(Result::ok);
    // A part that is not an extra stands for its type, the first of a type
    // listed is kept, and a type the NPC_ lacks is filled from the race's
    // defaults. Found from the files: 2 NPC_s list an extra (a beard's
    // `_1bit` part, PNAM 0) themselves and still get the race's default
    // mouth (also PNAM 0); WEAdventurerWarriorDualKhajiitM lists the
    // model-less HairKhajiit00 before KhajiitMaleEarTufts and its face has no
    // tufts (keeping the last of a type loses it). Scars are not one to a
    // type: dunAlftandValie and dunAlftandSullaTrebatius each list a left
    // and a right gash, and their faces have both.
    let mut by_kind: Vec<(HeadPartKind, FormRef)> = Vec::new();
    let mut parts: Vec<FormRef> = Vec::new();
    for f in &npc.head_parts {
        match part(f) {
            Some(h)
                if h.flags & EXTRA_PART == 0 && h.kind.is_some_and(|k| k != HeadPartKind::Scar) =>
            {
                let k = h.kind.unwrap();
                if !by_kind.iter().any(|(have, _)| *have == k) {
                    by_kind.push((k, f.clone()));
                }
            }
            _ => parts.push(f.clone()),
        }
    }
    for (_, d) in &race.sexes[npc.female() as usize].head.parts {
        if let Some(k) = part(d).and_then(|h| h.kind) {
            if !by_kind.iter().any(|(have, _)| *have == k) {
                by_kind.push((k, d.clone()));
            }
        }
    }
    parts.extend(by_kind.into_iter().map(|(_, f)| f));
    // The extras, recursively.
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while let Some(p) = parts.pop() {
        if !seen.insert(p.clone()) {
            continue;
        }
        if let Some(Ok(h)) = order.get::<HeadPart>(&p) {
            parts.extend(h.extra_parts.iter().cloned());
        }
        out.push(p);
    }
    Ok(out)
}

/// The editor ids of the parts with a model the game would give `form`.
fn expected(order: &LoadOrder, form: &FormRef) -> Result<BTreeSet<String>, Unresolved> {
    let parts = face_source(order, form).and_then(|n| head_parts(order, &n))?;
    // A part with no model (the "no scar", "no beard", "no brow" parts a race
    // defaults to) has no shape.
    Ok(parts
        .iter()
        .filter_map(|p| order.get::<HeadPart>(p).and_then(Result::ok))
        .filter(|h| h.model.as_deref().is_some_and(|m| !m.is_empty()))
        .map(|h| h.editor_id.unwrap_or_default().to_ascii_lowercase())
        .collect())
}

fn facegen_check(order: &LoadOrder, dirs: &[PathBuf]) {
    let t = Instant::now();
    let files = facegen_files(dirs);
    let Some(skyrim) = order.plugin("Skyrim.esm") else {
        println!("no Skyrim.esm: no FaceGen check");
        return;
    };
    let (mut checked, mut exact) = (0, 0);
    let mut causes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut used = HashSet::new();
    for (form, r) in skyrim.forms() {
        if &r.kind != b"NPC_" {
            continue;
        }
        let Some(path) = files.get(&form.id) else {
            continue;
        };
        used.insert(form.id);
        checked += 1;
        let label = order
            .get::<Npc>(&form)
            .and_then(Result::ok)
            .and_then(|n| n.editor_id)
            .unwrap_or_default();
        let label = format!("{:08X} {label}", form.id);
        let shapes = match shape_names(path) {
            Ok(s) => s,
            Err(e) => {
                causes
                    .entry(format!("NIF unreadable: {e}"))
                    .or_default()
                    .push(label);
                continue;
            }
        };
        let want = match expected(order, &form) {
            Ok(p) => p,
            Err(e) => {
                causes
                    .entry(format!("unresolved: {e:?}"))
                    .or_default()
                    .push(label);
                continue;
            }
        };
        if want == shapes {
            exact += 1;
            continue;
        }
        // Would an earlier copy of the records have made this face?
        let earlier = (1..order.plugins.len()).rev().find(|n| {
            let shorter = LoadOrder::new(order.plugins[..*n].to_vec());
            expected(&shorter, &form).is_ok_and(|w| w == shapes)
        });
        if let Some(n) = earlier {
            let later: Vec<&str> = order.copies(&form)[1..].iter().map(|p| p.name()).collect();
            causes
                .entry(format!(
                    "the face was made from the records before {}: {later:?} changes them",
                    order.plugins[n].name()
                ))
                .or_default()
                .push(label);
            continue;
        }
        let missing: Vec<&String> = want.difference(&shapes).collect();
        let extra: Vec<&String> = shapes.difference(&want).collect();
        causes
            .entry(format!("records give {missing:?}, the NIF {extra:?}"))
            .or_default()
            .push(label);
    }
    println!(
        "FaceGen: {checked} NPC_ checked ({} files, {} with no NPC_ of Skyrim.esm), {exact} match exactly, {} do not; {:.0} ms",
        files.len(),
        files.keys().filter(|k| !used.contains(k)).count(),
        checked - exact,
        t.elapsed().as_secs_f64() * 1e3
    );
    let mut causes: Vec<_> = causes.into_iter().collect();
    causes.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (cause, who) in causes {
        println!("  {:>4}  {cause}", who.len());
        for w in who.iter().take(4) {
            println!("          {w}");
        }
    }
}
