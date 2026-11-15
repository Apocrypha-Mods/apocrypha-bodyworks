//! The sample presets in `scratch/samples/jslot` read into a look with the
//! records of the game's `UBE_AllRace.esp` (`SKYRIM_DATA`, or Steam's
//! folder), and written back. Skipped when the presets or the plugin are not
//! there; the presets are third-party and never tracked.
//!
//! The first sample (Frau Peach, a UBE preset) names seven head parts: three
//! of `UBE_AllRace.esp`, which is installed here, and four of light plugins
//! that are not.

use std::path::{Path, PathBuf};

use apoc_character::preset::{apply_look, look_of_preset, LoadOrder, Note, Plugin};
use apoc_jslot::{parse, write, Document, Preset};
use apoc_plugin::native::read_file;
use apoc_plugin::FormRef;

const WANTED: [[u8; 4]; 4] = [*b"RACE", *b"HDPT", *b"TXST", *b"CLFM"];

/// `UBE_NordRace`; its women have the sample's 108 tint masks.
fn race() -> FormRef {
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

fn samples() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("jslot"))
            {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/jslot"),
        &mut out,
    );
    out.sort();
    out
}

fn records() -> Option<apoc_plugin::LoadOrder> {
    let path = data_folder().join("UBE_AllRace.esp");
    if !path.is_file() {
        return None;
    }
    let (plugin, _) = read_file(&path, &WANTED).expect("UBE_AllRace.esp");
    Some(apoc_plugin::LoadOrder::new(vec![plugin]))
}

/// The load order a preset was saved with, as far as its head parts and
/// `mods` show it, with stand-ins for the plugins it does not name.
fn saved_order(doc: &Document) -> LoadOrder {
    let mut full: Vec<Option<String>> = Vec::new();
    let mut light: Vec<Option<String>> = Vec::new();
    let put = |list: &mut Vec<Option<String>>, at: usize, name: String| {
        if list.len() <= at {
            list.resize(at + 1, None);
        }
        list[at] = Some(name);
    };
    let p = Preset::of(doc);
    for h in p.head_parts.into_iter().flatten().flatten() {
        let (Ok(id), Ok(f)) = (h.form_id, h.form_identifier) else {
            continue;
        };
        if id >> 24 == 0xFE {
            put(&mut light, ((id >> 12) & 0xFFF) as usize, f.plugin);
        } else {
            put(&mut full, (id >> 24) as usize, f.plugin);
        }
    }
    for m in p.mods.into_iter().flatten().flatten() {
        if let (Ok(i), Ok(n)) = (m.index, m.name) {
            put(&mut full, i as usize, n);
        }
    }
    let mut plugins: Vec<Plugin> = Vec::new();
    for (i, n) in full.into_iter().enumerate() {
        let name = n.unwrap_or_else(|| format!("Stand-in {i}.esp"));
        plugins.push(Plugin::new(&name, false));
    }
    for (i, n) in light.into_iter().enumerate() {
        let name = n.unwrap_or_else(|| format!("Stand-in {i}.esl"));
        plugins.push(Plugin::new(&name, true));
    }
    LoadOrder::new(plugins)
}

#[test]
fn every_sample_written_back_from_its_look_is_unchanged() {
    let Some(records) = records() else {
        eprintln!("skipped: no UBE_AllRace.esp");
        return;
    };
    let samples = samples();
    eprintln!("{} sample presets", samples.len());
    for path in samples {
        let bytes = std::fs::read(&path).unwrap();
        let mut doc = parse(&bytes).unwrap();
        let order = saved_order(&doc);
        let (look, _) = look_of_preset(&doc, &race(), true, &records, &order);
        apply_look(&mut doc, &look, &records, &order).unwrap();
        assert!(
            write(&doc).unwrap() == bytes,
            "{} changed when written back",
            path.display()
        );
    }
}

fn frau_peach() -> Option<Document> {
    let path = samples()
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == "Frau Peach.jslot"))?;
    Some(parse(&std::fs::read(path).unwrap()).unwrap())
}

#[test]
fn the_first_sample_reports_what_it_cannot_use() {
    let (Some(records), Some(doc)) = (records(), frau_peach()) else {
        eprintln!("skipped: no UBE_AllRace.esp or no sample");
        return;
    };
    let order = saved_order(&doc);
    let (look, report) = look_of_preset(&doc, &race(), true, &records, &order);
    assert_eq!(look.head_parts.len(), 7);
    assert_eq!(look.weight, 21.0);
    assert_eq!(
        look.face_texture,
        Some(FormRef::new("UBE_AllRace.esp", 0x01_1D49))
    );
    assert_eq!(look.face_parts, vec![0, -1, 0, 0]);
    assert_eq!(look.face_morphs.len(), 19);
    assert_eq!(look.face_morphs[18], f32::MAX);
    assert_eq!(look.custom_morphs.len(), 59);
    // The light parts' plugins are in the saved order but not installed.
    let not_parts: Vec<&FormRef> = report
        .notes
        .iter()
        .filter_map(|n| match n {
            Note::NotAHeadPart { form, .. } => Some(form),
            _ => None,
        })
        .collect();
    assert_eq!(not_parts.len(), 4, "{:?}", report.notes);
    assert!(not_parts.iter().all(|f| f.id <= 0xFFF));
    assert!(not_parts.contains(&&FormRef::new("Tullius Hair 3 SMP2.esp", 0xB1A)));
    // Nine of the 108 layers are not UBE_NordRace's masks here, six of
    // them in use.
    let foreign: Vec<u8> = report
        .notes
        .iter()
        .filter_map(|n| match n {
            Note::TintNotOfRace { alpha, .. } => Some(*alpha),
            _ => None,
        })
        .collect();
    assert_eq!(foreign.len(), 9);
    assert_eq!(foreign.iter().filter(|a| **a > 0).count(), 6);
    assert_eq!(report.notes.len(), 13, "{:?}", report.notes);
    // In use and the race's: the skin tone and three others.
    assert_eq!(look.tints.len(), 4);
    assert!(look.skin_tone.is_some());

    // Each layer's index is its mask's place in the race's list, not the
    // mask's TINI.
    let r = apoc_character::Records::race(&records, &race()).unwrap();
    let masks = &r.sexes[1].head.tints;
    let tints = doc.root.as_object().unwrap().get("tintInfo").unwrap();
    let mut by_place = 0;
    for e in tints.as_array().unwrap() {
        let o = e.as_object().unwrap();
        let index = o
            .get("index")
            .unwrap()
            .as_number()
            .unwrap()
            .as_u64()
            .unwrap() as usize;
        let texture = o.get("texture").unwrap().as_str().unwrap().text().unwrap();
        let Some(place) = masks.iter().position(|m| {
            m.mask
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case(&texture))
        }) else {
            continue;
        };
        assert_eq!(index, place, "{texture}");
        by_place += 1;
    }
    assert_eq!(by_place, 99);
    assert_eq!(masks[0].index, 0x18);
}

#[test]
fn the_first_sample_under_another_load_order() {
    let (Some(records), Some(doc)) = (records(), frau_peach()) else {
        eprintln!("skipped: no UBE_AllRace.esp or no sample");
        return;
    };
    // UBE_AllRace.esp seventh, and none of the light plugins.
    let order = LoadOrder::new(
        [
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
            "RaceCompatibility.esm",
            "UBE_AllRace.esp",
        ]
        .iter()
        .map(|n| Plugin::new(n, false))
        .collect(),
    );
    let (look, report) = look_of_preset(&doc, &race(), true, &records, &order);
    let gone = report
        .notes
        .iter()
        .filter(|n| matches!(n, Note::PartNotLoaded { .. }))
        .count();
    assert_eq!(gone, 4);
    // Unedited, nothing is written, the old form ids included.
    let mut out = doc.clone();
    apply_look(&mut out, &look, &records, &order).unwrap();
    assert!(write(&out).unwrap() == write(&doc).unwrap());
    // With the parts edited (the last, not loaded, taken out), the parts are
    // written under this load order.
    let mut look = look;
    look.head_parts.pop();
    apply_look(&mut out, &look, &records, &order).unwrap();
    let p = Preset::of(&out);
    let ids: Vec<u32> = p
        .head_parts
        .unwrap()
        .into_iter()
        .map(|h| h.unwrap().form_id.unwrap())
        .collect();
    // UBE_AllRace.esp's parts take its new index; the others are as read.
    assert_eq!(ids[..3], [0x060144EA, 0x06019426, 0x06025ADC]);
    assert!(ids[3..].iter().all(|i| i >> 24 == 0xFE));
    assert_eq!(ids.len(), 6);
    let mods = p.mods.unwrap();
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].as_ref().unwrap().index, Ok(6));
    // Plugin names are only added to: the part taken out stays named.
    assert_eq!(p.mod_names.unwrap().len(), 5);
    // Nothing else moved.
    let put_back = |o: &mut Document, key: &str| {
        let old = doc.root.as_object().unwrap().get(key).unwrap().clone();
        o.root.as_object_mut().unwrap().insert(key, old);
    };
    put_back(&mut out, "headParts");
    put_back(&mut out, "mods");
    assert!(write(&out).unwrap() == write(&doc).unwrap());
}
