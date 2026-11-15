//! The rules on presets and records made by hand.

use apoc_jslot::{parse, write, Preset};
use apoc_plugin::records::{Colour, HeadPart, HeadPartKind, Race, TextureSet, TintMask};
use apoc_plugin::FormRef;

use super::*;
use crate::tests::Fake;

/// Records that know plugins: [`Fake`]'s, for forms of the plugins named
/// only.
struct Known {
    w: Fake,
    plugins: Vec<&'static str>,
}

impl Known {
    fn knows(&self, f: &FormRef) -> bool {
        self.plugins
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&f.plugin))
    }
}

impl Records for Known {
    fn kind(&self, f: &FormRef) -> Option<[u8; 4]> {
        self.knows(f).then(|| self.w.kind(f)).flatten()
    }
    fn npc(&self, f: &FormRef) -> Option<apoc_plugin::records::Npc> {
        self.knows(f).then(|| self.w.npc(f)).flatten()
    }
    fn race(&self, f: &FormRef) -> Option<Race> {
        self.knows(f).then(|| self.w.race(f)).flatten()
    }
    fn head_part(&self, f: &FormRef) -> Option<HeadPart> {
        self.knows(f).then(|| self.w.head_part(f)).flatten()
    }
    fn texture_set(&self, f: &FormRef) -> Option<TextureSet> {
        self.knows(f).then(|| self.w.texture_set(f)).flatten()
    }
    fn armor(&self, f: &FormRef) -> Option<apoc_plugin::records::Armor> {
        self.knows(f).then(|| self.w.armor(f)).flatten()
    }
    fn armor_addon(&self, f: &FormRef) -> Option<apoc_plugin::records::ArmorAddon> {
        self.knows(f).then(|| self.w.armor_addon(f)).flatten()
    }
    fn colour(&self, f: &FormRef) -> Option<Colour> {
        self.knows(f).then(|| self.w.colour(f)).flatten()
    }
}

const SKIN: &str = "Actors\\Character\\Character Assets\\TintMasks\\SkinTone.dds";

fn mask(index: u16, path: Option<&str>, kind: u16) -> TintMask {
    TintMask {
        index,
        mask: path.map(str::to_string),
        kind: Some(kind),
        ..TintMask::default()
    }
}

fn part(name: &str, kind: HeadPartKind) -> HeadPart {
    HeadPart {
        editor_id: Some(name.into()),
        kind: Some(kind),
        ..HeadPart::default()
    }
}

/// Records by id: a race (100) whose women have five tint masks, the fourth
/// with no texture; a face and a mouth (`Race.esp`), two hairs (`Hair.esl`,
/// light, and `Hair2.esp`), a texture set and a colour.
fn world() -> Fake {
    let mut w = Fake::default();
    let mut race = Race::default();
    race.sexes[1].head.tints = vec![
        mask(0x18, Some(SKIN), 6),
        mask(0x1A, Some("Masks\\Lips.dds"), 1),
        mask(0x1B, Some("Masks\\Cheeks.dds"), 2),
        mask(0x1C, None, 7),
        mask(0x1D, Some("Masks/Brow.dds"), 3),
    ];
    w.races.insert(100, race);
    w.parts.insert(0x0144EA, part("Face", HeadPartKind::Face));
    w.parts.insert(0x019426, part("Mouth", HeadPartKind::Misc));
    w.parts.insert(0xB1A, part("HairA", HeadPartKind::Hair));
    w.parts.insert(0x801, part("HairB", HeadPartKind::Hair));
    w.parts.insert(0x802, part("Brows", HeadPartKind::Eyebrows));
    w.sets.insert(0x011D49, TextureSet::default());
    w.colours.insert(
        500,
        Colour {
            rgba: [57, 55, 40, 0],
            ..Colour::default()
        },
    );
    w
}

fn race() -> FormRef {
    FormRef::new("Test.esm", 100)
}

/// Race.esp is index 2, Hair2.esp 3; Hair.esl is light slot 1.
fn order() -> LoadOrder {
    LoadOrder::new(vec![
        Plugin::new("Skyrim.esm", false),
        Plugin::new("Update.esm", false),
        Plugin::new("Filler.esl", true),
        Plugin::new("Race.esp", false),
        Plugin::new("Hair.esl", true),
        Plugin::new("Hair2.esp", false),
    ])
}

/// A preset as RaceMenu would lay it out, with things a look does not own
/// (body morphs, a part's extra key, sculpting) and numbers written in ways
/// RaceMenu does not (`0.50`), which must come back as they are. Its fourth
/// head part is from a plugin that is not loaded (index 0x50), and its
/// fourth tint layer is not the race's.
const PRESET: &str = r#"{
   "actor" : {
      "hairColor" : 5196362,
      "headTexture" : "Race.esp|011D49",
      "weight" : 21
   },
   "bodyMorphs" : [ { "name" : "Kept", "value" : 1 } ],
   "headParts" : [
      {
         "formId" : 33637610,
         "formIdentifier" : "Race.esp|0144EA",
         "type" : 0
      },
      {
         "formId" : 33657894,
         "formIdentifier" : "race.esp|019426",
         "type" : 1
      },
      {
         "formId" : 4261419802,
         "formIdentifier" : "Hair.esl|001B1A",
         "type" : 3
      },
      {
         "extra" : true,
         "formId" : 1342179329,
         "formIdentifier" : "Gone.esp|000801",
         "type" : 9
      }
   ],
   "modNames" : [ "Gone.esp", "Hair.esl", "Race.esp" ],
   "mods" : [
      {
         "index" : 2,
         "name" : "Race.esp"
      },
      {
         "index" : 80,
         "name" : "Gone.esp"
      }
   ],
   "morphs" : {
      "custom" : [
         {
            "name" : "B",
            "value" : 0.3100000023841858
         },
         {
            "name" : "A",
            "value" : -1
         }
      ],
      "default" : {
         "morphs" : [ 0, 0.50, -1, 3.402823466385289e+38 ],
         "presets" : [ 0, 4294967295, 3, 0 ]
      },
      "sculpt" : [],
      "sculptDivisor" : 10000
   },
   "tintInfo" : [
      {
         "color" : 4294966524,
         "index" : 0,
         "texture" : "Actors\\Character\\Character Assets\\TintMasks\\SkinTone.dds"
      },
      {
         "color" : 16777215,
         "index" : 1,
         "texture" : "masks/lips.dds"
      },
      {
         "color" : 1621709906,
         "index" : 2,
         "texture" : "Masks\\Cheeks.dds"
      },
      {
         "color" : 1048379392,
         "index" : 3,
         "texture" : "Other\\Koralina.dds"
      }
   ],
   "version" : {
      "formatVersion" : 3,
      "runtimeVersion" : 17039392,
      "signature" : 1163086675,
      "skseVersion" : 33685552
   }
}
"#;

fn doc() -> Document {
    parse(PRESET.as_bytes()).unwrap()
}

fn text(d: &Document) -> String {
    String::from_utf8(write(d).unwrap()).unwrap()
}

#[test]
fn a_preset_is_read_into_a_look() {
    let w = world();
    let (look, report) = look_of_preset(&doc(), &race(), true, &w, &order());
    assert_eq!(look.race, Some(race()));
    assert!(look.female);
    assert_eq!(look.weight, 21.0);
    assert_eq!(
        look.hair_rgb,
        Some([
            0x4F as f32 / 255.0,
            0x4A as f32 / 255.0,
            0x4A as f32 / 255.0
        ])
    );
    assert_eq!(look.hair_colour, None);
    assert_eq!(look.face_texture, Some(FormRef::new("Race.esp", 0x011D49)));
    assert_eq!(
        look.head_parts,
        vec![
            FormRef::new("Race.esp", 0x0144EA),
            FormRef::new("Race.esp", 0x019426),
            // The light slot is not part of the form's id.
            FormRef::new("Hair.esl", 0xB1A),
            FormRef::new("Gone.esp", 0x801),
        ]
    );
    assert_eq!(look.face_morphs, vec![0.0, 0.5, -1.0, f32::MAX]);
    assert_eq!(look.face_parts, vec![0, -1, 3, 0]);
    assert_eq!(
        look.custom_morphs,
        vec![("B".to_string(), 0.31), ("A".to_string(), -1.0)]
    );
    // The lips layer has alpha 0 and is not in use. The cheeks' entry has
    // index 2, the mask's place in the race's list; its TINI is 0x1B.
    assert_eq!(
        look.tints,
        vec![
            Tint {
                mask: 0x18,
                colour: [0xFF, 0xFC, 0xFC, 0xFF],
                strength: 1.0
            },
            Tint {
                mask: 0x1B,
                colour: [0xA9, 0x54, 0x52, 0x60],
                strength: 0x60 as f32 / 255.0
            },
        ]
    );
    // Mid grey laid toward the skin tone layer's colour by its strength.
    let tone = look.skin_tone.unwrap();
    let want = [1.0, 252.0 / 255.0, 252.0 / 255.0];
    assert!((0..3).all(|c| (tone[c] - want[c]).abs() < 1e-6), "{tone:?}");
    assert_eq!(
        report.notes,
        vec![
            Note::PartNotLoaded {
                at: 3,
                form: FormRef::new("Gone.esp", 0x801)
            },
            Note::TintNotOfRace {
                at: 3,
                texture: "Other\\Koralina.dds".into(),
                alpha: 0x3E
            },
        ]
    );
    assert_eq!(
        report.notes[0].to_string(),
        "Head part 000801 (headParts[3]) is from Gone.esp, which isn't loaded."
    );
}

#[test]
fn a_look_written_back_unchanged_changes_no_byte() {
    let w = world();
    let mut d = doc();
    let before = text(&d);
    let (look, _) = look_of_preset(&d, &race(), true, &w, &order());
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(text(&d), before);
    assert!(before.contains("0.50,"));
}

#[test]
fn edits_change_only_what_they_touch() {
    let w = world();
    let mut d = doc();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    look.weight = 50.5;
    look.hair_rgb = Some([1.0, 0.0, 0.5]);
    // Swap the light hair for the other, and drop the part not loaded.
    look.head_parts[2] = FormRef::new("hair2.ESP", 0x801);
    look.head_parts.pop();
    look.face_morphs[1] = 0.25;
    look.face_parts[0] = -1;
    look.custom_morphs = vec![("A".into(), 0.5), ("C".into(), 0.31)];
    look.tints = vec![
        look.tints[0],
        Tint {
            mask: 0x1A,
            colour: [1, 2, 3, 0],
            strength: 0.5,
        },
        Tint {
            mask: 0x1D,
            colour: [4, 5, 6, 0],
            strength: 1.0,
        },
    ];
    let saved = apply_look(&mut d, &look, &w, &order()).unwrap();
    assert!(saved
        .notes
        .contains(&SaveNote::Changed("actor.weight".into())));
    assert!(saved.notes.contains(&SaveNote::Added {
        list: "modNames",
        what: "Hair2.esp".into()
    }));
    assert!(saved.notes.contains(&SaveNote::Added {
        list: "tintInfo",
        what: "Masks/Brow.dds".into()
    }));
    let out = text(&d);
    let p = Preset::of(&d);
    assert!(p.problems().is_empty(), "{:?}", p.problems());
    let a = p.actor.unwrap();
    assert_eq!(a.weight, Ok(50.5));
    assert_eq!(a.hair_color, Ok(0xFF0080));
    let parts: Vec<_> = p
        .head_parts
        .unwrap()
        .into_iter()
        .map(|h| {
            let h = h.unwrap();
            (
                h.kind.unwrap(),
                h.form_id.unwrap(),
                h.form_identifier.unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        parts,
        vec![
            (0, 0x020144EA, "Race.esp|0144EA".to_string()),
            // The identifier as read: its plugin's case is not a change.
            (1, 0x02019426, "race.esp|019426".to_string()),
            // The new hair takes the old one's type.
            (3, 0x03000801, "Hair2.esp|000801".to_string()),
        ]
    );
    // The dropped part's entry is gone, its extra key with it; keys a look
    // does not own are kept.
    assert!(!out.contains("\"extra\""));
    let top = |d: &Document, k: &str| d.root.as_object().unwrap().get(k).cloned();
    assert_eq!(top(&d, "bodyMorphs"), top(&doc(), "bodyMorphs"));
    assert_eq!(top(&d, "version"), top(&doc(), "version"));
    let sculpt = |d: &Document| {
        let m = top(d, "morphs").unwrap();
        let m = m.as_object().unwrap();
        (m.get("sculpt").cloned(), m.get("sculptDivisor").cloned())
    };
    assert_eq!(sculpt(&d), sculpt(&doc()));
    // Plugin names are only added to.
    assert_eq!(
        p.mod_names.unwrap(),
        ["Gone.esp", "Hair.esl", "Hair2.esp", "Race.esp"]
            .map(|n| Ok(n.to_string()))
            .to_vec()
    );
    let mods: Vec<_> = p
        .mods
        .unwrap()
        .into_iter()
        .map(|m| {
            let m = m.unwrap();
            (m.index.unwrap(), m.name.unwrap())
        })
        .collect();
    assert_eq!(
        mods,
        vec![
            (2, "Race.esp".into()),
            (3, "Hair2.esp".into()),
            (80, "Gone.esp".into())
        ]
    );
    let dm = p.default_morphs.unwrap();
    // Untouched sliders keep their text.
    assert!(out.contains("[ 0, 0.25, -1, 3.402823466385289e+38 ]"));
    assert_eq!(
        dm.presets.unwrap(),
        vec![Ok(u32::MAX), Ok(u32::MAX), Ok(3), Ok(0)]
    );
    let custom: Vec<_> = p
        .custom_morphs
        .unwrap()
        .into_iter()
        .map(|c| {
            let c = c.unwrap();
            (c.name.unwrap(), c.value.unwrap())
        })
        .collect();
    // B, which the look left out, is kept: named sliders are never taken
    // out.
    assert_eq!(
        custom,
        vec![
            ("B".into(), 0.3100000023841858),
            ("A".into(), 0.5),
            ("C".into(), 0.3100000023841858)
        ]
    );
    let tints: Vec<_> = p
        .tints
        .unwrap()
        .into_iter()
        .map(|t| {
            let t = t.unwrap();
            (t.index.unwrap(), t.color.unwrap(), t.texture.unwrap())
        })
        .collect();
    assert_eq!(
        tints,
        vec![
            (0, 0xFFFFFCFC, SKIN.to_string()),
            (1, 0x80010203, "masks/lips.dds".into()),
            // Out of the look: alpha 0, its colour kept.
            (2, 0x00A95452, "Masks\\Cheeks.dds".into()),
            // Not the race's: as it was.
            (3, 0x3E7D0000, "Other\\Koralina.dds".into()),
            // New, at its mask's place, the path as the race spells it.
            (4, 0xFF040506, "Masks/Brow.dds".into()),
        ]
    );
}

#[test]
fn a_part_from_a_plugin_not_loaded_is_kept_as_read_and_not_made_new() {
    let w = world();
    let mut d = doc();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    // Moved to the front: kept whole, its form id from the old load order.
    look.head_parts.rotate_right(1);
    apply_look(&mut d, &look, &w, &order()).unwrap();
    let p = Preset::of(&d);
    let first = p.head_parts.unwrap()[0].clone().unwrap();
    assert_eq!(first.form_id, Ok(0x50000801));
    assert!(text(&d).contains("\"extra\" : true"));
    // A part not in the preset whose plugin is not loaded cannot be written,
    // and the load order says so before the records are asked: records
    // that know plugins have no Gone.esp, so asking them first would call
    // it no head part.
    let known = Known {
        w: w.clone(),
        plugins: vec!["Race.esp", "Hair.esl", "Hair2.esp", "Test.esm"],
    };
    let mut d = doc();
    let before = text(&d);
    look.head_parts.push(FormRef::new("Gone.esp", 0x802));
    assert!(known.head_part(&FormRef::new("Gone.esp", 0x802)).is_none());
    assert_eq!(
        apply_look(&mut d, &look, &known, &order()),
        Err(PresetError::NotLoaded("Gone.esp|000802".into()))
    );
    assert_eq!(text(&d), before);
}

#[test]
fn a_new_part_must_be_a_head_part_and_takes_its_pnam_when_nothing_matches() {
    let w = world();
    let mut d = doc();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    look.head_parts.push(FormRef::new("Race.esp", 0x999));
    let before = text(&d);
    assert_eq!(
        apply_look(&mut d, &look, &w, &order()),
        Err(PresetError::NotAHeadPart("Race.esp|000999".into()))
    );
    assert_eq!(text(&d), before);
    look.head_parts.pop();
    look.head_parts.push(FormRef::new("Hair2.esp", 0x802));
    apply_look(&mut d, &look, &w, &order()).unwrap();
    let parts = Preset::of(&d).head_parts.unwrap();
    let brows = parts.last().unwrap().clone().unwrap();
    assert_eq!(brows.kind, Ok(6));
    assert_eq!(brows.form_id, Ok(0x03000802));
}

#[test]
fn what_cannot_be_read_is_reported() {
    let w = world();
    let d = parse(
        br#"{
        "actor" : { "headTexture" : "Gone.esp|000001", "weight" : "x" },
        "headParts" : [
            { "formId" : 4278190081, "type" : 0 },
            { "formId" : 33637610, "type" : 0 },
            { "formIdentifier" : "Race.esp|000123", "type" : 0 }
        ],
        "tintInfo" : [ { "color" : 0, "index" : 0, "texture" : "Masks\\Lips.dds" } ]
    }"#,
    )
    .unwrap();
    let (look, report) = look_of_preset(&d, &race(), true, &w, &order());
    // The second part's form id names Race.esp's face.
    assert_eq!(
        look.head_parts,
        vec![
            FormRef::new("Race.esp", 0x0144EA),
            FormRef::new("Race.esp", 0x123)
        ]
    );
    let notes: Vec<String> = report.notes.iter().map(|n| n.to_string()).collect();
    assert!(
        notes.contains(&"actor.weight is not a number.".to_string()),
        "{notes:?}"
    );
    assert!(notes.contains(&"actor.hairColor is missing.".to_string()));
    assert!(notes.contains(&"headParts[0].formIdentifier is missing.".to_string()));
    assert!(report.notes.contains(&Note::PartUnknown { at: 0 }));
    assert!(report.notes.contains(&Note::NotAHeadPart {
        at: 2,
        form: FormRef::new("Race.esp", 0x123)
    }));
    assert!(report
        .notes
        .contains(&Note::HeadTextureNotLoaded(FormRef::new("Gone.esp", 1))));
    assert!(notes.contains(&"modNames is missing.".to_string()));
    // Preview as a race with no record: one note for all the layers.
    let (_, report) = look_of_preset(&d, &FormRef::new("Test.esm", 7), true, &w, &order());
    assert!(report
        .notes
        .contains(&Note::NoRace(FormRef::new("Test.esm", 7))));
}

#[test]
fn a_new_preset_holds_the_race_masks_unused_and_takes_a_look() {
    let w = world();
    let mut d = new_document(&race(), true, &w).unwrap();
    let p = Preset::of(&d);
    let v = p.version.clone().unwrap();
    assert_eq!(
        (v.format, v.runtime, v.skse, v.signature),
        (Ok(3), Ok(17039392), Ok(33685552), Ok(1163086675))
    );
    let tints: Vec<_> = p
        .tints
        .unwrap()
        .into_iter()
        .map(|t| {
            let t = t.unwrap();
            (t.index.unwrap(), t.color.unwrap())
        })
        .collect();
    assert_eq!(
        tints,
        vec![(0, 0xFFFFFF), (1, 0xFFFFFF), (2, 0xFFFFFF), (4, 0xFFFFFF)]
    );
    let (mut look, _) = look_of_preset(&doc(), &race(), true, &w, &order());
    look.head_parts.pop();
    apply_look(&mut d, &look, &w, &order()).unwrap();
    let p = Preset::of(&d);
    assert!(p.problems().is_empty(), "{:?}", p.problems());
    let (again, report) = look_of_preset(&d, &race(), true, &w, &order());
    assert_eq!(report.notes, vec![]);
    assert_eq!(again, look);
    let parts: Vec<u32> = p
        .head_parts
        .unwrap()
        .into_iter()
        .map(|h| h.unwrap().kind.unwrap())
        .collect();
    // With nothing to take a type from, each part's PNAM.
    assert_eq!(parts, vec![1, 0, 3]);
    assert!(text(&d).contains("\"presets\" : [ 0, 4294967295, 3, 0 ]"));
    // A hair colour from a record when none is given directly.
    look.hair_rgb = None;
    look.hair_colour = Some(FormRef::new("Test.esm", 500));
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(
        Preset::of(&d).actor.unwrap().hair_color,
        Ok(u32::from_be_bytes([0, 57, 55, 40]))
    );
}

#[test]
fn load_order_ids_for_regular_and_light_plugins() {
    let o = order();
    assert_eq!(o.place("Race.esp"), Some(Place::Full(2)));
    assert_eq!(o.place("RACE.ESP"), Some(Place::Full(2)));
    assert_eq!(o.place("Hair.esl"), Some(Place::Light(1)));
    assert_eq!(o.place("Gone.esp"), None);
    // Regular: the index, then the form's 24 bits.
    let face = FormRef::new("race.esp", 0x0144EA);
    assert_eq!(o.form_id(&face), Some(0x020144EA));
    assert_eq!(o.identifier(&face).unwrap().to_string(), "Race.esp|0144EA");
    assert_eq!(o.form_of_id(0x020144EA), Some(face.clone()));
    // Light: 0xFE, the slot, the form's 12 bits; the identifier is the low
    // 24 bits, slot and all, as RaceMenu writes it.
    let hair = FormRef::new("Hair.esl", 0xB1A);
    assert_eq!(o.form_id(&hair), Some(0xFE001B1A));
    assert_eq!(o.identifier(&hair).unwrap().to_string(), "Hair.esl|001B1A");
    assert_eq!(o.form_of_id(0xFE001B1A), Some(hair.clone()));
    let ident = |t: &str| apoc_jslot::FormRef::parse(t).unwrap();
    assert_eq!(o.form_of_identifier(&ident("Hair.esl|001B1A"), None), hair);
    // Saved with the plugin in another slot: the slot is not the id.
    assert_eq!(o.form_of_identifier(&ident("hair.esl|14DB1A"), None), hair);
    // A light form's id must fit 12 bits.
    assert_eq!(o.form_id(&FormRef::new("Hair.esl", 0x1000)), None);
    assert_eq!(o.identifier(&FormRef::new("Hair.esl", 0x1000)), None);
    // Not loaded.
    assert_eq!(o.form_id(&FormRef::new("Gone.esp", 1)), None);
    assert_eq!(o.form_of_id(0x07000001), None);
    assert_eq!(o.form_of_id(0xFE005001), None);
    assert_eq!(o.form_of_id(0xFF000001), None);
    // A plugin not loaded is light when its form id says it was.
    assert_eq!(
        o.form_of_identifier(&ident("Gone.esl|14DB1A"), Some(0xFE14DB1A)),
        FormRef::new("Gone.esl", 0xB1A)
    );
    assert_eq!(
        o.form_of_identifier(&ident("Gone.esp|14DB1A"), Some(0x3014DB1A)),
        FormRef::new("Gone.esp", 0x14DB1A)
    );
}

#[test]
fn load_order_limits_and_repeats() {
    let mut plugins: Vec<Plugin> = (0..0xFF)
        .map(|i| Plugin::new(&format!("{i}.esp"), false))
        .collect();
    plugins.push(Plugin::new("0.ESP", false));
    let o = LoadOrder::new(plugins);
    assert_eq!(o.place("253.esp"), Some(Place::Full(0xFD)));
    // 0xFE is the light plugins' index.
    assert_eq!(o.place("254.esp"), None);
    // A name loaded twice has its first place.
    assert_eq!(o.place("0.esp"), Some(Place::Full(0)));
    assert_eq!(o.at(Place::Full(0)).unwrap().name, "0.esp");
    let lights: Vec<Plugin> = (0..0x1001)
        .map(|i| Plugin::new(&format!("{i}.esl"), true))
        .collect();
    let o = LoadOrder::new(lights);
    assert_eq!(o.place("4095.esl"), Some(Place::Light(0xFFF)));
    assert_eq!(o.place("4096.esl"), None);
    assert_eq!(
        o.form_id(&FormRef::new("4095.esl", 0xFFF)),
        Some(0xFEFFFFFF)
    );
}

#[test]
fn colours_pack_both_ways() {
    for v in 0..=255u8 {
        let c = u32::from_be_bytes([0, v, 255 - v, v / 2]);
        assert_eq!(pack_rgb(rgb_of(c)), c, "{v}");
        let argb = u32::from_be_bytes([v, 1, 2, 3]);
        let t = tint_of(9, argb);
        assert_eq!(t.colour, [1, 2, 3, v]);
        assert_eq!(pack_tint(&t), Ok(argb), "{v}");
    }
    assert_eq!(rgb_of(0x4F4A4A), [79.0 / 255.0, 74.0 / 255.0, 74.0 / 255.0]);
    // Out of range is clamped; the fourth byte of a look's colour is not
    // the alpha.
    assert_eq!(pack_rgb([2.0, -1.0, 0.5]), 0x00FF0080);
    let t = Tint {
        mask: 0,
        colour: [1, 2, 3, 200],
        strength: 0.0,
    };
    assert_eq!(pack_tint(&t), Ok(0x00010203));
    let t = Tint {
        strength: f32::NAN,
        ..t
    };
    assert_eq!(
        pack_tint(&t),
        Err(PresetError::NotANumber("a tint layer's strength"))
    );
}

#[test]
fn presets_of_none_are_minus_one_in_a_look_and_4294967295_in_a_preset() {
    let w = world();
    let d = parse(br#"{ "morphs" : { "default" : { "morphs" : [], "presets" : [ 4294967295, 2, 2147483648 ] } } }"#).unwrap();
    let (look, _) = look_of_preset(&d, &race(), true, &w, &order());
    assert_eq!(look.face_parts, vec![-1, 2, i32::MIN]);
    let mut d = new_document(&race(), true, &w).unwrap();
    let look = Look {
        race: Some(race()),
        female: true,
        face_parts: vec![-1, 2, i32::MIN],
        ..Look::default()
    };
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert!(text(&d).contains("\"presets\" : [ 4294967295, 2, 2147483648 ]"));
}

#[test]
fn plugin_names_go_in_byte_order_among_those_there() {
    let mut names: Vec<Value> = ["Gone.esp", "TW3Eyes.esp", "UBE_AllRace.esp"]
        .iter()
        .map(|n| Value::String(apoc_jslot::Str::new(n)))
        .collect();
    names.insert(1, Value::Bool(true));
    for n in ["Tullius.esp", "A.esp", "UBE 2.0.esp", "b.esp"] {
        insert_name(&mut names, n);
    }
    let text: Vec<String> = names
        .iter()
        .map(|v| v.as_str().map_or("?".into(), |s| s.text().unwrap()))
        .collect();
    assert_eq!(
        text,
        [
            "A.esp",
            "Gone.esp",
            "?",
            "TW3Eyes.esp",
            "Tullius.esp",
            "UBE 2.0.esp",
            "UBE_AllRace.esp",
            "b.esp"
        ]
    );
}

#[test]
fn layers_sharing_a_texture_pair_with_masks_by_place() {
    let masks = vec![
        mask(1, Some("A.dds"), 7),
        mask(2, Some("a.DDS"), 7),
        mask(3, Some("B.dds"), 7),
    ];
    let d = parse(
        br#"[ { "index" : 1, "texture" : "A.dds" }, { "index" : 0, "texture" : "A.dds" }, { "index" : 5, "texture" : "b.dds" }, { "index" : 2, "texture" : "B.dds" } ]"#,
    )
    .unwrap();
    let entries = d.root.as_array().unwrap();
    // By place first: mask 0 takes the entry of index 0 and mask 1 that of
    // index 1, though both have both textures; mask 2's entry of index 2
    // wins over the earlier one of index 5.
    assert_eq!(
        claim(entries, &masks),
        vec![Some(1), Some(0), None, Some(2)]
    );
}

#[test]
fn errors_leave_the_document_alone() {
    let w = world();
    let mut d = doc();
    let before = text(&d);
    let (look, _) = look_of_preset(&d, &race(), true, &w, &order());
    for bad in [
        Look {
            weight: f32::NAN,
            ..look.clone()
        },
        Look {
            race: None,
            ..look.clone()
        },
        Look {
            tints: vec![Tint {
                mask: 0x99,
                colour: [0; 4],
                strength: 1.0,
            }],
            ..look.clone()
        },
        Look {
            face_morphs: vec![f32::INFINITY],
            ..look.clone()
        },
    ] {
        assert!(apply_look(&mut d, &bad, &w, &order()).is_err());
        assert_eq!(text(&d), before);
    }
    let mut not_object = parse(b"[]").unwrap();
    assert_eq!(
        apply_look(&mut not_object, &look, &w, &order()),
        Err(PresetError::NotAnObject)
    );
    let mut wrong = parse(br#"{ "headParts" : 1 }"#).unwrap();
    assert_eq!(
        apply_look(&mut wrong, &look, &w, &order()),
        Err(PresetError::WrongKind("headParts", "a list"))
    );
}

/// The preset with `edit` made to its document first.
fn doc_with(edit: impl FnOnce(&mut Object)) -> Document {
    let mut d = doc();
    edit(d.root.as_object_mut().unwrap());
    d
}

fn layers(d: &Document) -> Vec<(u32, u32, String)> {
    Preset::of(d)
        .tints
        .unwrap()
        .into_iter()
        .map(|t| {
            let t = t.unwrap();
            (t.index.unwrap(), t.color.unwrap(), t.texture.unwrap())
        })
        .collect()
}

/// The preset without its lips entry (`tintInfo[1]`).
fn without_lips() -> Document {
    doc_with(|o| {
        o.get_mut("tintInfo")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .remove(1);
    })
}

fn lips() -> Tint {
    Tint {
        mask: 0x1A,
        colour: [1, 2, 3, 0],
        strength: 1.0,
    }
}

#[test]
fn a_layer_added_between_others_goes_at_its_place() {
    let w = world();
    let mut d = without_lips();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    look.tints.push(lips());
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(
        layers(&d),
        vec![
            (0, 0xFFFFFCFC, SKIN.to_string()),
            (1, 0xFF010203, "Masks\\Lips.dds".into()),
            (2, 0x60A95452, "Masks\\Cheeks.dds".into()),
            (3, 0x3E7D0000, "Other\\Koralina.dds".into()),
        ]
    );
}

#[test]
fn a_layer_added_and_a_later_one_dropped_each_reach_their_own_entry() {
    let w = world();
    let mut d = without_lips();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    // Drop the cheeks (TINI 0x1B, after the lips) and add the lips.
    look.tints.retain(|t| t.mask != 0x1B);
    look.tints.push(lips());
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(
        layers(&d),
        vec![
            (0, 0xFFFFFCFC, SKIN.to_string()),
            (1, 0xFF010203, "Masks\\Lips.dds".into()),
            (2, 0x00A95452, "Masks\\Cheeks.dds".into()),
            (3, 0x3E7D0000, "Other\\Koralina.dds".into()),
        ]
    );
}

#[test]
fn a_layer_of_a_mask_with_no_texture_is_refused() {
    let w = world();
    let mut d = doc();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    look.tints.push(Tint {
        mask: 0x1C,
        colour: [0; 4],
        strength: 1.0,
    });
    let before = text(&d);
    assert_eq!(
        apply_look(&mut d, &look, &w, &order()),
        Err(PresetError::MaskHasNoTexture(0x1C))
    );
    assert_eq!(text(&d), before);
}

#[test]
fn the_skin_tone_is_written_into_its_layer_and_reads_back() {
    let w = world();
    let mut d = doc();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    look.skin_tone = Some([0.1, 0.2, 0.3]);
    apply_look(&mut d, &look, &w, &order()).unwrap();
    // At full strength the layer's colour is the tone, to a 255th.
    assert_eq!(layers(&d)[0].1, 0xFF1A334D);
    let (back, _) = look_of_preset(&d, &race(), true, &w, &order());
    let tone = back.skin_tone.unwrap();
    for (c, want) in [0.1f32, 0.2, 0.3].into_iter().enumerate() {
        assert!((tone[c] - want).abs() <= 0.5 / 255.0, "{tone:?}");
    }
    // And the layer gives the tone by the face tint's rule.
    let layer = back.tints.iter().find(|t| t.mask == 0x18).unwrap();
    assert_eq!(tone_of(layer), tone);
    // Written again, nothing moves.
    let bytes = text(&d);
    apply_look(&mut d, &back, &w, &order()).unwrap();
    assert_eq!(text(&d), bytes);

    // Every tone comes back within half a 255th.
    for i in 0..=255u8 {
        let t = [i, 255 - i, i / 3].map(|v| (f32::from(v) + 0.3) / 256.0);
        let mut d = doc();
        let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
        look.skin_tone = Some(t);
        apply_look(&mut d, &look, &w, &order()).unwrap();
        let back = look_of_preset(&d, &race(), true, &w, &order())
            .0
            .skin_tone
            .unwrap();
        assert!(
            (0..3).all(|c| (back[c] - t[c]).abs() <= 0.5 / 255.0 + 1e-6),
            "{t:?} {back:?}"
        );
    }
}

#[test]
fn an_edited_skin_layer_wins_over_a_skin_tone_left_as_read() {
    let w = world();
    let mut d = doc();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    let skin = look.tints.iter_mut().find(|t| t.mask == 0x18).unwrap();
    skin.colour = [0x80, 0x40, 0x20, 0x80];
    skin.strength = 0x80 as f32 / 255.0;
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(layers(&d)[0].1, 0x80804020);
    // A skin tone of a layer the preset lacks is added at full strength.
    let mut d = doc_with(|o| {
        o.get_mut("tintInfo")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .remove(0);
    });
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    assert_eq!(look.skin_tone, None);
    look.skin_tone = Some([1.0, 0.5, 0.0]);
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(layers(&d)[0], (0, 0xFFFF8000, SKIN.to_string()));
}

#[test]
fn a_rewritten_part_keeps_its_other_keys() {
    let w = world();
    let mut d = doc_with(|o| {
        let parts = o.get_mut("headParts").unwrap().as_array_mut().unwrap();
        parts[0]
            .as_object_mut()
            .unwrap()
            .insert("zz", Value::Bool(true));
    });
    // Race.esp moved to index 4: the face's form id is rewritten when the
    // parts are, its own key kept.
    let moved = LoadOrder::new(vec![
        Plugin::new("Skyrim.esm", false),
        Plugin::new("Update.esm", false),
        Plugin::new("A.esm", false),
        Plugin::new("B.esm", false),
        Plugin::new("Race.esp", false),
        Plugin::new("Hair.esl", true),
    ]);
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &moved);
    look.head_parts.swap(0, 1);
    let saved = apply_look(&mut d, &look, &w, &moved).unwrap();
    let parts = Preset::of(&d).head_parts.unwrap();
    assert_eq!(parts[1].clone().unwrap().form_id, Ok(0x040144EA));
    let entry = d
        .root
        .as_object()
        .unwrap()
        .get("headParts")
        .unwrap()
        .as_array()
        .unwrap()[1]
        .as_object()
        .unwrap()
        .get("zz")
        .cloned();
    assert_eq!(entry, Some(Value::Bool(true)));
    // Race.esp's mods entry takes its index now.
    let mods = Preset::of(&d).mods.unwrap();
    assert_eq!(mods[0].clone().unwrap().index, Ok(4));
    assert!(saved
        .notes
        .contains(&SaveNote::Changed("mods[0].index".into())));
}

#[test]
fn a_plugin_whose_light_flag_changed_keeps_the_saved_ids() {
    let ident = |t: &str| apoc_jslot::FormRef::parse(t).unwrap();
    // Light when saved (0xFE), regular now: the slot is not part of the id.
    let o = LoadOrder::new(vec![Plugin::new("Was.esl", false)]);
    assert_eq!(
        o.form_of_identifier(&ident("Was.esl|14DB1A"), Some(0xFE14DB1A)),
        FormRef::new("Was.esl", 0xB1A)
    );
    // Regular when saved, light now: its ids are its own.
    let o = LoadOrder::new(vec![Plugin::new("Now.esp", true)]);
    assert_eq!(
        o.form_of_identifier(&ident("Now.esp|000B1A"), Some(0x05000B1A)),
        FormRef::new("Now.esp", 0xB1A)
    );
}

#[test]
fn masks_sharing_a_tini_are_matched_by_place() {
    let mut w = world();
    w.races.get_mut(&100).unwrap().sexes[1].head.tints = vec![
        mask(0x18, Some(SKIN), 6),
        mask(0x20, Some("A.dds"), 7),
        mask(0x20, Some("B.dds"), 7),
    ];
    let mut d = parse(
        br#"{ "tintInfo" : [ { "color" : 4278190081, "index" : 1, "texture" : "A.dds" }, { "color" : 4278190082, "index" : 2, "texture" : "B.dds" } ] }"#,
    )
    .unwrap();
    let (mut look, report) = look_of_preset(&d, &race(), true, &w, &order());
    assert_eq!(
        report
            .notes
            .iter()
            .filter(|n| **n == Note::RepeatedMask(0x20))
            .count(),
        1
    );
    assert_eq!(look.tints.len(), 2);
    let bytes = text(&d);
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(text(&d), bytes);
    // The second layer is the second mask's.
    look.tints[1].colour = [9, 9, 9, 255];
    apply_look(&mut d, &look, &w, &order()).unwrap();
    let l = layers(&d);
    assert_eq!((l[0].1, l[1].1), (0xFF000001, 0xFF090909));
}

#[test]
fn with_no_race_record_all_but_the_layers_is_written() {
    let w = world();
    let nowhere = FormRef::new("Test.esm", 7);
    let mut d = doc();
    let (mut look, report) = look_of_preset(&d, &nowhere, true, &w, &order());
    assert!(report.notes.contains(&Note::NoRace(nowhere.clone())));
    look.weight = 60.0;
    let saved = apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(Preset::of(&d).actor.unwrap().weight, Ok(60.0));
    assert!(saved.notes.contains(&SaveNote::NotWritten {
        what: "the tint layers".into(),
        why: "the race has no record"
    }));
    // Layers to write with no race to place them by are refused.
    look.tints.push(lips());
    assert_eq!(
        apply_look(&mut d, &look, &w, &order()),
        Err(PresetError::MissingRace("Test.esm|000007".into()))
    );
    look.tints.clear();
    look.skin_tone = Some([0.5; 3]);
    assert_eq!(
        apply_look(&mut d, &look, &w, &order()),
        Err(PresetError::MissingRace("Test.esm|000007".into()))
    );
}

#[test]
fn lists_past_the_limits_are_read_in_part_and_left_alone() {
    let w = world();
    let part = r#"{ "formId" : 33637610, "formIdentifier" : "Race.esp|0144EA", "type" : 0 }"#;
    let slider = r#"{ "name" : "S", "value" : 0.5 }"#;
    let text_of_doc = format!(
        r#"{{ "actor" : {{ "weight" : 1 }}, "headParts" : [ {} ], "morphs" : {{ "custom" : [ {} ] }} }}"#,
        vec![part; MAX_PARTS + 3].join(", "),
        vec![slider; MAX_CUSTOM + 2].join(", ")
    );
    let mut d = parse(text_of_doc.as_bytes()).unwrap();
    let (mut look, report) = look_of_preset(&d, &race(), true, &w, &order());
    assert_eq!(look.head_parts.len(), MAX_PARTS);
    assert_eq!(look.custom_morphs.len(), MAX_CUSTOM);
    assert!(report.notes.contains(&Note::TooMany {
        what: "headParts",
        limit: MAX_PARTS
    }));
    assert!(report.notes.contains(&Note::TooMany {
        what: "morphs.custom",
        limit: MAX_CUSTOM
    }));
    let bytes = text(&d);
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(text(&d), bytes);
    // Dropping the first part leaves those past the limit where they are.
    look.head_parts.remove(0);
    look.custom_morphs[0].1 = 1.0;
    apply_look(&mut d, &look, &w, &order()).unwrap();
    let p = Preset::of(&d);
    assert_eq!(p.head_parts.unwrap().len(), MAX_PARTS + 2);
    let c = p.custom_morphs.unwrap();
    assert_eq!(c.len(), MAX_CUSTOM + 2);
    assert_eq!(c[0].clone().unwrap().value, Ok(1.0));
}

#[test]
fn an_unedited_look_makes_no_section_and_reports_nothing_changed() {
    let w = world();
    let mut d = parse(b"{}").unwrap();
    let (look, _) = look_of_preset(&d, &race(), true, &w, &order());
    let saved = apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(text(&d), "{}\n");
    assert!(saved
        .notes
        .iter()
        .all(|n| !matches!(n, SaveNote::Changed(_) | SaveNote::Added { .. })));
}

#[test]
fn notes_are_capped_however_many_problems_there_are() {
    let w = world();
    let n = 100_000;
    let text_of_doc = format!(r#"{{ "modNames" : [ {} ] }}"#, vec!["1"; n].join(", "));
    let mut d = parse(text_of_doc.as_bytes()).unwrap();
    let (look, report) = look_of_preset(&d, &race(), true, &w, &order());
    assert_eq!(report.notes.len(), MAX_NOTES + 1);
    // The problems not listed are counted, with the other sections missing
    // (version, actor, headParts, mods, morphs, tintInfo) among them.
    let listed = MAX_NOTES;
    assert_eq!(report.notes[MAX_NOTES], Note::More(n + 6 - listed));
    let saved = apply_look(&mut d, &look, &w, &order()).unwrap();
    assert!(saved.notes.len() <= MAX_NOTES + 1);

    // A save with more notes than the cap: unreadable sliders, each kept.
    let text_of_doc = format!(
        r#"{{ "morphs" : {{ "default" : {{ "morphs" : [ {} ] }} }} }}"#,
        vec!["\"x\""; 1000].join(", ")
    );
    let mut d = parse(text_of_doc.as_bytes()).unwrap();
    let (look, _) = look_of_preset(&d, &race(), true, &w, &order());
    let saved = apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(saved.notes.len(), MAX_NOTES + 1);
    assert_eq!(saved.notes[MAX_NOTES], SaveNote::More(1000 - MAX_NOTES));
}

fn not_written(saved: &SaveReport) -> Vec<String> {
    saved
        .notes
        .iter()
        .filter_map(|n| match n {
            SaveNote::NotWritten { what, .. } => Some(what.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn edits_that_cannot_be_saved_are_noted() {
    let w = world();
    // Unreadable values left at their placeholders are kept, and said so.
    let mut d = doc_with(|o| {
        let actor = o.get_mut("actor").unwrap().as_object_mut().unwrap();
        actor.insert("weight", Value::String(apoc_jslot::Str::new("x")));
        let m = o.get_mut("morphs").unwrap().as_object_mut().unwrap();
        let def = m.get_mut("default").unwrap().as_object_mut().unwrap();
        def.get_mut("morphs").unwrap().as_array_mut().unwrap()[1] =
            Value::String(apoc_jslot::Str::new("x"));
        def.get_mut("presets").unwrap().as_array_mut().unwrap()[2] = parse(b"2.0").unwrap().root;
    });
    let before = text(&d);
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    assert_eq!(
        (look.weight, look.face_morphs[1], look.face_parts[2]),
        (0.0, 0.0, -1)
    );
    // A named slider taken out, and a CLFM set under a direct colour.
    look.custom_morphs.retain(|(n, _)| n != "B");
    look.hair_colour = Some(FormRef::new("Test.esm", 500));
    let saved = apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(text(&d), before);
    assert_eq!(
        not_written(&saved),
        [
            "actor.weight",
            "the hair colour record",
            "morphs.default.morphs[1]",
            "morphs.default.presets[2]",
            "the removal of morphs.custom[0] (B)",
        ]
    );
    for n in &saved.notes {
        assert!(n.to_string().ends_with('.'), "{n}");
    }
}

#[test]
fn mods_follow_the_load_order_when_parts_are_written() {
    let mut w = world();
    w.parts.insert(0xA01, part("OldHair", HeadPartKind::Hair));
    w.parts.insert(0xA02, part("NewHair", HeadPartKind::Hair));
    // Saved with Old.esp at index 5.
    let mut d = parse(
        br#"{ "headParts" : [ { "formId" : 83888641, "formIdentifier" : "Old.esp|000A01", "type" : 3 } ], "modNames" : [ "Old.esp" ], "mods" : [ { "index" : 5, "name" : "Old.esp" }, { "index" : 7, "name" : "Gone.esp" } ] }"#,
    )
    .unwrap();
    // Now New.esp is 5, Gone.esp's 7 is Seven.esp's, and Old.esp is 9.
    let names = [
        "A",
        "B",
        "C",
        "D",
        "E",
        "New.esp",
        "F",
        "Seven.esp",
        "G",
        "Old.esp",
    ];
    let order = LoadOrder::new(names.iter().map(|n| Plugin::new(n, false)).collect());
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order);
    assert_eq!(look.head_parts, vec![FormRef::new("Old.esp", 0xA01)]);
    look.head_parts = vec![FormRef::new("New.esp", 0xA02)];
    let saved = apply_look(&mut d, &look, &w, &order).unwrap();
    let p = Preset::of(&d);
    let mods: Vec<(u32, String)> = p
        .mods
        .unwrap()
        .into_iter()
        .map(|m| {
            let m = m.unwrap();
            (m.index.unwrap(), m.name.unwrap())
        })
        .collect();
    assert_eq!(
        mods,
        // Kept entries keep their places; the new one goes before the first
        // of a higher index.
        vec![
            (5, "New.esp".into()),
            (9, "Old.esp".into()),
            (7, "Gone.esp".into())
        ]
    );
    let part = p.head_parts.unwrap()[0].clone().unwrap();
    assert_eq!(part.form_id, Ok(0x0500_0A02));
    assert_eq!(part.form_identifier.unwrap().to_string(), "New.esp|000A02");
    // Paths are where the entries are once New.esp is in.
    assert!(saved
        .notes
        .contains(&SaveNote::Changed("mods[1].index".into())));
    assert!(saved.notes.contains(&SaveNote::NotWritten {
        what: "mods[2] (Gone.esp)".into(),
        why: "its plugin isn't loaded and its index is now another plugin's; kept as read",
    }));
}

#[test]
fn tint_paths_are_where_the_entries_end_up() {
    let w = world();
    let mut d = without_lips();
    let (mut look, _) = look_of_preset(&d, &race(), true, &w, &order());
    look.tints.push(lips());
    look.tints
        .iter_mut()
        .find(|t| t.mask == 0x1B)
        .unwrap()
        .colour = [7, 7, 7, 0x60];
    let saved = apply_look(&mut d, &look, &w, &order()).unwrap();
    // The cheeks were tintInfo[1] before the lips went in before them.
    assert!(saved
        .notes
        .contains(&SaveNote::Changed("tintInfo[2].color".into())));
    assert_eq!(layers(&d)[2], (2, 0x60070707, "Masks\\Cheeks.dds".into()));
}
