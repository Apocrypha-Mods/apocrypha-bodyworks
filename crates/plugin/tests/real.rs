//! Tests on the real plugins of a Skyrim Special Edition install. Each one
//! is skipped (passes, saying so) when the files are not there.

use std::path::PathBuf;

use apoc_plugin::native::read_file;
use apoc_plugin::records::*;
use apoc_plugin::walk::read;
use apoc_plugin::{FormRef, LoadOrder, Plugin};

const WANTED: [[u8; 4]; 7] = [
    *b"RACE", *b"HDPT", *b"TXST", *b"ARMO", *b"ARMA", *b"CLFM", *b"NPC_",
];

fn data(file: &str) -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var("HOME").ok()?)
        .join(".local/share/Steam/steamapps/common/Skyrim Special Edition/Data")
        .join(file);
    if p.exists() {
        Some(p)
    } else {
        eprintln!("skipped: no {}", p.display());
        None
    }
}

fn s(id: u32) -> FormRef {
    FormRef::new("Skyrim.esm", id)
}

/// Reads every record of a plugin as its type; none may fail.
fn type_all(p: &Plugin) -> usize {
    let mut n = 0;
    for r in &p.records {
        let ok = match &r.kind {
            b"RACE" => Race::read(r, &p.names).is_ok(),
            b"HDPT" => HeadPart::read(r, &p.names).is_ok(),
            b"TXST" => TextureSet::read(r, &p.names).is_ok(),
            b"ARMO" => Armor::read(r, &p.names).is_ok(),
            b"ARMA" => ArmorAddon::read(r, &p.names).is_ok(),
            b"CLFM" => Colour::read(r, &p.names).is_ok(),
            b"NPC_" => Npc::read(r, &p.names).is_ok(),
            _ => true,
        };
        n += ok as usize;
    }
    n
}

#[test]
fn skyrim_esm_by_ranges() {
    let Some(path) = data("Skyrim.esm") else {
        return;
    };
    let (p, stats) = read_file(&path, &WANTED).unwrap();
    let len = std::fs::metadata(&path).unwrap().len();
    assert!(p.header.esm() && p.header.localized());
    assert!(p.header.masters.is_empty());
    // A few percent of the file, in about one read per top-level group.
    assert!(stats.bytes * 20 < len, "{} of {len}", stats.bytes);
    assert!(stats.reads <= stats.groups + 2);
    assert_eq!(stats.groups_read, 7);
    assert!(p.failures.is_empty());
    assert_eq!(type_all(&p), p.records.len());

    let nord: Race = p.typed(&s(0x13746)).unwrap().unwrap();
    assert_eq!(nord.editor_id.as_deref(), Some("NordRace"));
    assert_eq!(nord.skin, Some(s(0xD64)));
    let male = &nord.sexes[0];
    assert_eq!(
        male.skeleton.as_deref(),
        Some("Actors\\Character\\Character Assets\\skeleton.nif")
    );
    assert_eq!(
        male.body_lighting.as_deref(),
        Some("Actors\\Character\\UpperBodyHumanMale.egt")
    );
    assert!(male.head.parts.iter().any(|(_, f)| *f == s(0x5162F)));
    assert_eq!(male.head.morphs.len(), 4);
    assert_eq!(male.head.morphs[0].flags[0], 0x001F_FF3E);
    assert_eq!(male.head.presets.len(), 10);
    assert_eq!(male.head.hair_colours.len(), 15);
    assert_eq!(male.head.default_face_texture, Some(s(0x3B521)));
    assert_eq!(male.head.tints[0].presets[0].number, 0x4B);
    assert_eq!(nord.sexes[1].head.parts.len(), 5);

    let head: HeadPart = p.typed(&s(0x5162F)).unwrap().unwrap();
    assert_eq!(head.editor_id.as_deref(), Some("MaleHeadNord"));
    assert_eq!(head.kind, Some(HeadPartKind::Face));
    assert_eq!(head.files.len(), 3);
    assert_eq!(head.files[0].kind, PartFileKind::RaceMorph);
    assert!(head.files[2].path.ends_with("MaleHeadCustomizations.tri"));

    let ulfric: Npc = p.typed(&s(0x1414D)).unwrap().unwrap();
    assert_eq!(ulfric.race, Some(s(0x13746)));
    assert_eq!(ulfric.head_parts.len(), 5);
    assert_eq!(ulfric.tints[0].preset, 0x4B);
    assert!(!ulfric.female());

    let skin: Armor = p.typed(&s(0xD64)).unwrap().unwrap();
    assert!(!skin.addons.is_empty());
    let torso: ArmorAddon = p.typed(&skin.addons[0]).unwrap().unwrap();
    assert!(torso.models[0].is_some());
}

#[test]
fn overrides_across_the_official_files_and_ussep() {
    let names = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
        "unofficial skyrim special edition patch.esp",
    ];
    let Some(paths) = names.iter().map(|n| data(n)).collect::<Option<Vec<_>>>() else {
        return;
    };
    let plugins: Vec<Plugin> = paths
        .iter()
        .map(|p| read_file(p, &WANTED).unwrap().0)
        .collect();
    let ussep = &plugins[5];
    assert_eq!(ussep.header.masters[..5], names[..5]);
    let order = LoadOrder::new(plugins);
    // USSEP changes CWBattleGalmar to take his face from Galmar.
    let (p, _) = order.winner(&s(0xD0570)).unwrap();
    assert!(p.name().starts_with("unofficial"));
    let galmar: Npc = order.get(&s(0xD0570)).unwrap().unwrap();
    assert!(galmar.face_from_template());
    // Dawnguard's own forms are named by Dawnguard.esm.
    let dawnguard = order.plugin("dawnguard.esm").unwrap();
    assert!(dawnguard
        .forms()
        .any(|(f, _)| f.plugin == "Dawnguard.esm" && f.id >= 0x800));
}

/// A cheap source of positions.
fn lcg(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

#[test]
fn truncated_real_plugins_give_errors() {
    let Some(path) = data("HearthFires.esm") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let (whole, _) = read("HearthFires.esm", &bytes, &WANTED).unwrap();
    let mut seed = 1;
    for _ in 0..400 {
        let n = (lcg(&mut seed) as usize) % bytes.len();
        // A cut at a group's end leaves a smaller, whole plugin.
        if let Ok((p, _)) = read("HearthFires.esm", &bytes[..n], &WANTED) {
            assert!(p.records.len() <= whole.records.len());
        }
    }
    for n in 0..200 {
        assert!(read("HearthFires.esm", &bytes[..n], &WANTED).is_err());
    }
}

#[test]
fn flipped_real_plugins_never_panic() {
    for file in ["ccBGSSSE037-Curios.esl", "HearthFires.esm"] {
        let Some(path) = data(file) else {
            continue;
        };
        let bytes = std::fs::read(path).unwrap();
        let mut seed = 7;
        let mut errors = 0;
        for i in 0..300 {
            let mut b = bytes.clone();
            for _ in 0..1 + i % 8 {
                let at = (lcg(&mut seed) as usize) % b.len();
                b[at] ^= 1 << (lcg(&mut seed) % 8);
            }
            match read(file, &b, &WANTED) {
                Ok((p, _)) => {
                    type_all(&p);
                }
                Err(_) => errors += 1,
            }
        }
        eprintln!("{file}: {errors} of 300 flipped copies refused");
    }
}
