//! RaceMenu's named sliders on the owner's installed game (`SKYRIM_DATA`, or
//! Steam's folder): `UBE_AllRace.esp`'s and `Expressive Facegen
//! Morphs.esl`'s loose folders under `Meshes\Actors\Character\
//! FaceGenMorphs`, and the first sample preset (`scratch/samples/jslot`,
//! third-party, never tracked). Skipped when any of it is not there.
//!
//! Only loose files are read: Bodyworks has no archive reader, so
//! RaceMenu's own folder (`racemenu.esp`, inside `RaceMenu.bsa`) is not.

use std::path::{Path, PathBuf};

use apoc_character::build::{build, Options};
use apoc_character::files::key;
use apoc_character::preset::{look_of_preset, LoadOrder as PresetOrder, Plugin};
use apoc_character::racemenu::{weights, SliderNote};
use apoc_character::{plan, plan_with, Files, Look, RaceMenuMorphs};
use apoc_jslot::parse;
use apoc_nif::mesh;
use apoc_plugin::native::read_file;
use apoc_plugin::{FormRef, LoadOrder};

const WANTED: [[u8; 4]; 4] = [*b"RACE", *b"HDPT", *b"TXST", *b"CLFM"];

/// The plugins whose folders are looked in, as a load order would list
/// them; most have none.
const PLUGINS: [&str; 6] = [
    "Skyrim.esm",
    "Dawnguard.esm",
    "RaceCompatibility.esm",
    "UBE_AllRace.esp",
    "Expressive Facegen Morphs.esl",
    "RaceMenu.esp",
];

/// `UBE_NordRace`.
fn ube_nord() -> FormRef {
    FormRef::new("UBE_AllRace.esp", 0x05_A184)
}

fn data_folder() -> PathBuf {
    std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(format!(
                "{}/.local/share/Steam/steamapps/common/Skyrim Special Edition/Data",
                std::env::var("HOME").unwrap_or_default()
            ))
        })
}

/// A file under the Data folder by [`key`], each part matched without case
/// as Windows would.
fn loose(data: &Path, k: &str) -> Option<Vec<u8>> {
    let mut at = data.to_path_buf();
    for part in key(k).split('/') {
        let found = std::fs::read_dir(&at).ok()?.flatten().find(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(part))
        })?;
        at = found.path();
    }
    std::fs::read(at).ok()
}

/// Fetch `keys` into `files`; those not there are left out.
fn fetch(data: &Path, keys: &[String], files: &mut Files) {
    for k in keys {
        if let Some(b) = loose(data, k) {
            files.insert(k, b);
        }
    }
}

/// The folders, fetched as a host would: ask, fetch, ask again.
fn racemenu(data: &Path) -> (RaceMenuMorphs, Files) {
    let mut files = Files::new();
    let mut rounds = 0;
    loop {
        let (needed, notes) = RaceMenuMorphs::needed(&PLUGINS, &files);
        assert!(notes.is_empty(), "{notes:?}");
        if needed.is_empty() {
            break;
        }
        rounds += 1;
        assert!(rounds <= 3, "{needed:?}");
        // What isn't there is given as nothing.
        for k in needed {
            let b = loose(data, &k).unwrap_or_default();
            files.insert(&k, b);
        }
    }
    (RaceMenuMorphs::read(&PLUGINS, &files), files)
}

fn records(data: &Path) -> LoadOrder {
    let plugins = [
        "Skyrim.esm",
        "Dawnguard.esm",
        "RaceCompatibility.esm",
        "UBE_AllRace.esp",
    ]
    .iter()
    .filter(|n| data.join(n).is_file())
    .map(|n| read_file(&data.join(n), &WANTED).expect(n).0)
    .collect();
    LoadOrder::new(plugins)
}

/// The first sample read as a UBE Nord woman.
fn sample_look(records: &LoadOrder) -> Option<Look> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../scratch/samples/jslot/frau-peach/SKSE/Plugins/CharGen/Presets/Frau Peach.jslot",
    );
    let doc = parse(&std::fs::read(path).ok()?).unwrap();
    let order = PresetOrder::new(
        [
            "Skyrim.esm",
            "Dawnguard.esm",
            "RaceCompatibility.esm",
            "UBE_AllRace.esp",
        ]
        .iter()
        .map(|n| Plugin::new(n, false))
        .collect(),
    );
    Some(look_of_preset(&doc, &ube_nord(), true, records, &order).0)
}

fn ready() -> Option<(PathBuf, RaceMenuMorphs, Files)> {
    let data = data_folder();
    if !data.join("UBE_AllRace.esp").is_file() {
        eprintln!("skipped: no UBE_AllRace.esp");
        return None;
    }
    let (rm, files) = racemenu(&data);
    if rm.sliders_for("00UBE_NordRace", true).is_empty() {
        eprintln!("skipped: no UBE slider files");
        return None;
    }
    Some((data, rm, files))
}

#[test]
fn the_samples_named_sliders_all_resolve_for_every_ube_race() {
    let Some((data, rm, files)) = ready() else {
        return;
    };
    eprintln!("{} ini files read; notes: {:#?}", files.len(), rm.notes);
    let ube: Vec<&str> = rm
        .races()
        .into_iter()
        .filter(|r| r.starts_with("00UBE_"))
        .collect();
    eprintln!("UBE races with sliders ({}): {ube:?}", ube.len());
    assert_eq!(ube.len(), 16);
    let others: Vec<&str> = rm
        .races()
        .into_iter()
        .filter(|r| !r.starts_with("00UBE_"))
        .collect();
    eprintln!("other races with sliders ({}): {others:?}", others.len());

    let records = records(&data);
    let Some(look) = sample_look(&records) else {
        eprintln!("skipped: no sample preset");
        return;
    };
    assert_eq!(look.custom_morphs.len(), 59);
    for race in &ube {
        let sliders = rm.sliders_for(race, true);
        let (on, notes) = weights(&look.custom_morphs, &sliders);
        let unknown: Vec<&SliderNote> = notes
            .iter()
            .filter(|n| matches!(n, SliderNote::Unknown { .. }))
            .collect();
        assert!(unknown.is_empty(), "{race}: {unknown:?}");
        if *race == "00UBE_NordRace" {
            eprintln!(
                "{race}: {} sliders, {} morphs turned on, notes {notes:?}",
                sliders.len(),
                on.len()
            );
        }
    }
    let negative = look.custom_morphs.iter().filter(|(_, v)| *v < 0.0).count();
    assert_eq!(negative, 15);
}

/// Max and mean distance each head shape's vertices moved between two
/// builds.
fn moved(a: &apoc_character::Built, b: &apoc_character::Built) -> Vec<(String, f32, f32, usize)> {
    let mut out = Vec::new();
    for s in a.shapes.iter().filter(|s| s.head) {
        let Some(t) = b.shapes.iter().find(|t| t.name == s.name) else {
            continue;
        };
        let p = mesh::read(&a.nif, s.block).unwrap().positions;
        let q = mesh::read(&b.nif, t.block).unwrap().positions;
        assert_eq!(p.len(), q.len(), "{}", s.name);
        let d: Vec<f32> = p
            .iter()
            .zip(&q)
            .map(|(x, y)| {
                ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
            })
            .collect();
        let max = d.iter().copied().fold(0.0, f32::max);
        let mean = d.iter().sum::<f32>() / d.len().max(1) as f32;
        out.push((s.name.clone(), max, mean, p.len()));
    }
    out
}

#[test]
fn the_samples_named_sliders_move_the_ube_head_and_none_change_nothing() {
    let Some((data, rm, _)) = ready() else {
        return;
    };
    let records = records(&data);
    let Some(look) = sample_look(&records) else {
        eprintln!("skipped: no sample preset");
        return;
    };
    let options = Options {
        face_tint: None,
        keep_head_space: true,
    };
    let fetch_all = |keys: Vec<String>| {
        let mut f = Files::new();
        fetch(&data, &keys, &mut f);
        f
    };

    let with = plan_with(&look, &records, Some(&rm)).unwrap();
    let extras: Vec<(&str, usize)> = with
        .head
        .iter()
        .map(|h| (h.name.as_str(), h.extras.len()))
        .collect();
    eprintln!("head parts and their extra tris: {extras:?}");
    eprintln!("slider notes: {:#?}", with.sliders);
    assert!(with.head.iter().any(|h| !h.extras.is_empty()));
    let files = fetch_all(with.files());
    let built = build(&with, &files, &options).unwrap();
    eprintln!("missing: {:?}", built.missing);
    eprintln!("notes: {:#?}", built.notes);
    assert!(
        !built.notes.iter().any(|n| n.contains("left off")),
        "{:#?}",
        built.notes
    );

    let mut plain = look.clone();
    plain.custom_morphs.clear();
    let before = plan(&plain, &records).unwrap();
    let before_built = build(&before, &fetch_all(before.files()), &options).unwrap();

    let moves = moved(&built, &before_built);
    for (name, max, mean, n) in &moves {
        eprintln!("{name}: {n} vertices, moved max {max:.3}, mean {mean:.3}");
    }
    let face = moves
        .iter()
        .find(|(name, ..)| name.to_ascii_lowercase().contains("head"))
        .expect("a head shape");
    assert!(face.1 > 0.1, "{face:?}");

    // The same look without named sliders builds as before, with RaceMenu
    // or without.
    let same = plan_with(&plain, &records, Some(&rm)).unwrap();
    assert_eq!(same, before);
    let same_built = build(&same, &fetch_all(same.files()), &options).unwrap();
    assert!(same_built == before_built);
    assert!(same_built.nif.write() == before_built.nif.write());
}

/// Every RaceMenu extra tri installed loose reads, Expressive Facegen
/// Morphs' among them (75 of its 91 have fewer places in the texture than
/// vertices, which the reader once refused).
#[test]
fn every_installed_extra_tri_reads() {
    let morphs = data_folder().join("Meshes/Actors/character/facegenmorphs/morphs");
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("tri")) {
                out.push(p);
            }
        }
    }
    let mut tris = Vec::new();
    walk(&morphs, &mut tris);
    if tris.is_empty() {
        eprintln!("skipped: no extra tris");
        return;
    }
    let mut failed = Vec::new();
    let mut efm = 0;
    for p in &tris {
        let is_efm = p.components().any(|c| c.as_os_str() == "EFM");
        efm += is_efm as usize;
        if let Err(e) = apoc_character::tri::read(&std::fs::read(p).unwrap()) {
            failed.push(format!("{}: {e}", p.display()));
        }
    }
    eprintln!("{} extra tris read, {efm} of them EFM's", tris.len());
    assert!(failed.is_empty(), "{failed:#?}");
}

/// An EFM slider on a vanilla Nord woman's head: Skyrim.esm's records, the
/// game's meshes unpacked in `scratch/samples/skyrim`, EFM's tris from the
/// Data folder. Skipped when any of it is not there.
#[test]
fn an_efm_slider_moves_a_vanilla_head() {
    let data = data_folder();
    let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/skyrim");
    if !data.join("Skyrim.esm").is_file()
        || !data.join("Expressive Facegen Morphs.esl").is_file()
        || !samples
            .join("meshes/actors/character/character assets/femalehead.nif")
            .is_file()
    {
        eprintln!("skipped: no Skyrim.esm, EFM or unpacked vanilla head");
        return;
    }
    let (rm, _) = racemenu(&data);
    let (plugin, _) = read_file(&data.join("Skyrim.esm"), &WANTED).unwrap();
    let records = LoadOrder::new(vec![plugin]);
    // NordRace.
    let look = Look {
        race: Some(FormRef::new("Skyrim.esm", 0x01_3746)),
        female: true,
        weight: 50.0,
        custom_morphs: vec![
            ("EFM_Nose_Depth".into(), 1.0),
            ("EFM_Eyes_Size".into(), -1.0),
        ],
        ..Look::default()
    };
    // The game's meshes from the samples; the folders' files from Data.
    let fetch_all = |keys: Vec<String>| {
        let mut f = Files::new();
        for k in keys {
            let b = if k.starts_with(apoc_character::racemenu::FOLDER) {
                loose(&data, &k)
            } else {
                std::fs::read(samples.join(&k)).ok()
            };
            if let Some(b) = b {
                f.insert(&k, b);
            }
        }
        f
    };
    let options = Options {
        face_tint: None,
        keep_head_space: true,
    };
    let with = plan_with(&look, &records, Some(&rm)).unwrap();
    assert!(with.sliders.is_empty(), "{:?}", with.sliders);
    let extras: Vec<(&str, &[String])> = with
        .head
        .iter()
        .map(|h| (h.name.as_str(), h.extras.as_slice()))
        .collect();
    eprintln!("parts and extras: {extras:#?}");
    let built = build(&with, &fetch_all(with.files()), &options).unwrap();
    eprintln!("missing: {:?}\nnotes: {:#?}", built.missing, built.notes);
    assert!(
        !built.notes.iter().any(|n| n.contains("left off")
            || n.contains("weren't drawn")
            || n.contains("No part's")),
        "{:#?}",
        built.notes
    );
    let mut plain = look.clone();
    plain.custom_morphs.clear();
    let before = plan(&plain, &records).unwrap();
    let before_built = build(&before, &fetch_all(before.files()), &options).unwrap();
    let moves = moved(&built, &before_built);
    for (name, max, mean, n) in &moves {
        eprintln!("{name}: {n} vertices, moved max {max:.3}, mean {mean:.3}");
    }
    let head = moves
        .iter()
        .find(|(name, ..)| name.eq_ignore_ascii_case("FemaleHeadNord"))
        .expect("the head");
    assert!(head.1 > 0.1, "{head:?}");
}
