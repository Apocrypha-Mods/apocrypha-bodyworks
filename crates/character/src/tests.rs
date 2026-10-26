//! The rules on records made by hand.

use std::collections::HashMap;

use apoc_plugin::records::{
    Armor, ArmorAddon, Colour, HeadPart, HeadPartKind, Npc, PartFile, PartFileKind, Race,
    TextureSet, EXTRA_PART, USE_TRAITS,
};
use apoc_plugin::FormRef;

use crate::records::{RaceExtra, Records};
use crate::{build, files::Files, look::face_source, parts::head_parts, plan, CharacterError};
use crate::{Look, Options};

#[derive(Default)]
struct Fake {
    kinds: HashMap<u32, [u8; 4]>,
    npcs: HashMap<u32, Npc>,
    races: HashMap<u32, Race>,
    parts: HashMap<u32, HeadPart>,
    sets: HashMap<u32, TextureSet>,
    armors: HashMap<u32, Armor>,
    addons: HashMap<u32, ArmorAddon>,
    colours: HashMap<u32, Colour>,
    extras: HashMap<u32, RaceExtra>,
}

fn f(id: u32) -> FormRef {
    FormRef::new("Test.esm", id)
}

impl Records for Fake {
    fn kind(&self, form: &FormRef) -> Option<[u8; 4]> {
        self.kinds.get(&form.id).copied()
    }
    fn npc(&self, form: &FormRef) -> Option<Npc> {
        self.npcs.get(&form.id).cloned()
    }
    fn race(&self, form: &FormRef) -> Option<Race> {
        self.races.get(&form.id).cloned()
    }
    fn head_part(&self, form: &FormRef) -> Option<HeadPart> {
        self.parts.get(&form.id).cloned()
    }
    fn texture_set(&self, form: &FormRef) -> Option<TextureSet> {
        self.sets.get(&form.id).cloned()
    }
    fn armor(&self, form: &FormRef) -> Option<Armor> {
        self.armors.get(&form.id).cloned()
    }
    fn armor_addon(&self, form: &FormRef) -> Option<ArmorAddon> {
        self.addons.get(&form.id).cloned()
    }
    fn colour(&self, form: &FormRef) -> Option<Colour> {
        self.colours.get(&form.id).cloned()
    }
    fn race_extra(&self, race: &FormRef) -> RaceExtra {
        self.extras.get(&race.id).cloned().unwrap_or_default()
    }
}

fn part(name: &str, kind: HeadPartKind, model: bool) -> HeadPart {
    HeadPart {
        editor_id: Some(name.into()),
        kind: Some(kind),
        model: model.then(|| format!("Actors\\{name}.nif")),
        ..HeadPart::default()
    }
}

/// A race whose men default to a face (1), eyes (2) and a model-less scar
/// (3); with parts 10 (hair, bringing extra 11, which brings 12, which
/// brings 11 again), 20 and 21 (two eyes), 30 and 31 (two scars).
fn world() -> Fake {
    let mut w = Fake::default();
    w.parts.insert(1, part("Face", HeadPartKind::Face, true));
    w.parts
        .insert(2, part("EyesDefault", HeadPartKind::Eyes, true));
    w.parts.insert(3, part("NoScar", HeadPartKind::Scar, false));
    let mut hair = part("Hair", HeadPartKind::Hair, true);
    hair.extra_parts = vec![f(11)];
    w.parts.insert(10, hair);
    let mut line = part("Hairline", HeadPartKind::Misc, true);
    line.flags = EXTRA_PART;
    line.extra_parts = vec![f(12)];
    w.parts.insert(11, line);
    let mut more = part("More", HeadPartKind::Misc, true);
    more.flags = EXTRA_PART;
    more.extra_parts = vec![f(11)];
    w.parts.insert(12, more);
    w.parts.insert(20, part("EyesA", HeadPartKind::Eyes, true));
    w.parts.insert(21, part("EyesB", HeadPartKind::Eyes, true));
    w.parts.insert(30, part("ScarL", HeadPartKind::Scar, true));
    w.parts.insert(31, part("ScarR", HeadPartKind::Scar, true));
    let mut race = Race {
        editor_id: Some("TestRace".into()),
        ..Race::default()
    };
    race.sexes[0].head.parts = vec![(0, f(1)), (1, f(2)), (2, f(3))];
    w.races.insert(100, race);
    w.kinds.insert(100, *b"RACE");
    w
}

fn names(w: &Fake, parts: &[FormRef]) -> Vec<String> {
    let mut n: Vec<String> = parts
        .iter()
        .map(|p| w.parts[&p.id].editor_id.clone().unwrap())
        .collect();
    n.sort();
    n
}

#[test]
fn the_first_of_a_type_and_the_race_defaults() {
    let w = world();
    let race = &w.races[&100];
    let got = head_parts(&w, race, false, &[f(20), f(21)]);
    assert_eq!(names(&w, &got), ["EyesA", "Face", "NoScar"]);
    // Nothing listed: the defaults.
    let got = head_parts(&w, race, false, &[]);
    assert_eq!(names(&w, &got), ["EyesDefault", "Face", "NoScar"]);
    // A woman of a race with no defaults for women gets only what she lists.
    let got = head_parts(&w, race, true, &[f(20)]);
    assert_eq!(names(&w, &got), ["EyesA"]);
}

#[test]
fn scars_are_kept_together_and_extras_followed_once() {
    let w = world();
    let race = &w.races[&100];
    let got = head_parts(&w, race, false, &[f(30), f(31), f(10)]);
    assert_eq!(
        names(&w, &got),
        [
            "EyesDefault",
            "Face",
            "Hair",
            "Hairline",
            "More",
            "NoScar",
            "ScarL",
            "ScarR"
        ]
    );
    // An unknown part listed is kept (and drawn as nothing).
    let got = head_parts(&w, race, false, &[f(999)]);
    assert_eq!(got.len(), 4);
}

#[test]
fn faces_come_from_templates_while_the_flags_say() {
    let mut w = world();
    let npc = |template: Option<u32>, flags: u16, race: u32| Npc {
        template: template.map(f),
        template_flags: flags,
        race: Some(f(race)),
        ..Npc::default()
    };
    w.npcs.insert(1000, npc(Some(1001), USE_TRAITS, 100));
    w.npcs.insert(1001, npc(Some(1002), 0, 101));
    w.npcs.insert(1002, npc(None, 0, 102));
    for id in [1000, 1001, 1002] {
        w.kinds.insert(id, *b"NPC_");
    }
    assert_eq!(face_source(&w, &f(1000)).unwrap().race, Some(f(101)));
    // A leveled template has no one face.
    w.npcs.insert(1003, npc(Some(1004), USE_TRAITS, 100));
    w.kinds.insert(1003, *b"NPC_");
    w.kinds.insert(1004, *b"LVLN");
    assert_eq!(
        face_source(&w, &f(1003)),
        Err(CharacterError::LeveledTemplate(1004))
    );
    // Two that name each other.
    w.npcs.insert(1005, npc(Some(1006), USE_TRAITS, 100));
    w.npcs.insert(1006, npc(Some(1005), USE_TRAITS, 100));
    w.kinds.insert(1005, *b"NPC_");
    w.kinds.insert(1006, *b"NPC_");
    assert_eq!(
        face_source(&w, &f(1005)),
        Err(CharacterError::TemplateLoop(1005))
    );
    // A template that is not there.
    w.npcs.insert(1007, npc(Some(4242), USE_TRAITS, 100));
    w.kinds.insert(1007, *b"NPC_");
    assert!(matches!(
        face_source(&w, &f(1007)),
        Err(CharacterError::MissingRecord(_, 4242))
    ));
}

fn body_world() -> Fake {
    let mut w = world();
    let mut race = w.races[&100].clone();
    race.skin = Some(f(200));
    w.races.insert(100, race);
    w.armors.insert(
        200,
        Armor {
            addons: vec![f(201), f(202), f(203), f(204)],
            ..Armor::default()
        },
    );
    let addon = |slots: u32, race: u32, model: &str| ArmorAddon {
        editor_id: Some(model.into()),
        slots,
        race: Some(f(race)),
        models: [Some(format!("Actors\\{model}")), None, None, None],
        skin_textures: [Some(f(300)), None, None, None],
        ..ArmorAddon::default()
    };
    w.addons.insert(201, addon(0x4, 100, "Body_1.nif"));
    // Another race's.
    w.addons.insert(202, addon(0x8, 101, "Hands_1.nif"));
    // Of a slot already taken.
    w.addons.insert(203, addon(0x4, 100, "Other.nif"));
    w.addons.insert(204, addon(0x10, 100, "Tail.nif"));
    w.sets.insert(
        300,
        TextureSet {
            textures: [
                Some("Body.dds".into()),
                Some("Body_msn.dds".into()),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            ..TextureSet::default()
        },
    );
    let mut face = w.parts[&1].clone();
    face.files = vec![
        PartFile {
            kind: PartFileKind::RaceMorph,
            path: "Actors\\FaceRaces.tri".into(),
        },
        PartFile {
            kind: PartFileKind::ChargenMorph,
            path: "Actors\\FaceChargen.tri".into(),
        },
    ];
    w.parts.insert(1, face);
    w
}

#[test]
fn a_plan_names_every_file_once() {
    let w = body_world();
    let look = Look {
        race: Some(f(100)),
        weight: 50.0,
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    assert_eq!(p.body.len(), 2);
    assert_eq!(p.body[0].textures[0].as_deref(), Some("Body.dds"));
    assert_eq!(p.body[0].textures[1].as_deref(), Some("Body_msn.dds"));
    assert_eq!(p.weight, 0.5);
    assert_eq!(
        p.files(),
        [
            "meshes/actors/body_0.nif",
            "meshes/actors/body_1.nif",
            "meshes/actors/tail.nif",
            "meshes/actors/eyesdefault.nif",
            "meshes/actors/face.nif",
            "meshes/actors/faceraces.tri",
            "meshes/actors/facechargen.tri",
        ]
    );
    assert_eq!(crate::needed(&look, &w).unwrap(), p.files());
    // No race, no plan.
    assert_eq!(plan(&Look::default(), &w), Err(CharacterError::NoRace));
}

#[test]
fn vampires_take_their_morph_race_and_the_vampire_morph() {
    let mut w = world();
    w.races.insert(
        101,
        Race {
            editor_id: Some("TestRaceVampire".into()),
            ..w.races[&100].clone()
        },
    );
    w.extras.insert(
        101,
        RaceExtra {
            morph_race: Some(f(100)),
            keywords: vec![crate::records::vampire_keyword()],
        },
    );
    let look = Look {
        race: Some(f(101)),
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    assert_eq!(p.weights.race.get("testrace"), Some(&1.0));
    assert_eq!(p.weights.chargen.get("vampiremorph"), Some(&1.0));
}

#[test]
fn a_build_without_its_files_names_them_and_does_not_panic() {
    let w = body_world();
    let look = Look {
        race: Some(f(100)),
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    assert_eq!(
        build(&p, &Files::new(), &Options::default()),
        Err(CharacterError::Nothing)
    );
    // Every file there, and none of them a mesh.
    let mut files = Files::new();
    for k in p.files() {
        files.insert(
            &k,
            b"Gamebryo File Format, Version 20.2.0.7\nrubbish".to_vec(),
        );
    }
    assert_eq!(
        build(&p, &files, &Options::default()),
        Err(CharacterError::Nothing)
    );
}
