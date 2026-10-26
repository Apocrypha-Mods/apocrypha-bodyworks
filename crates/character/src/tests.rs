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

#[derive(Default, Clone)]
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

#[test]
fn texture_paths_are_under_textures() {
    use crate::build::rooted;
    assert_eq!(
        rooted("Actors\\Character\\Female\\FemaleBody_1.dds"),
        "textures\\Actors\\Character\\Female\\FemaleBody_1.dds"
    );
    assert_eq!(rooted("\\actors\\a.dds"), "textures\\actors\\a.dds");
    for kept in [
        "textures\\a.dds",
        "Textures/a.dds",
        "TEXTURES\\a.dds",
        "data\\Textures\\a.dds",
        "",
    ] {
        assert_eq!(rooted(kept), kept);
    }
}

#[test]
fn the_skeleton_is_among_the_files() {
    let mut w = body_world();
    let mut race = w.races[&100].clone();
    race.sexes[0].skeleton = Some("Actors\\Character\\Character Assets\\skeleton.nif".into());
    w.races.insert(100, race);
    let look = Look {
        race: Some(f(100)),
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    assert_eq!(
        p.files()[0],
        "meshes/actors/character/character assets/skeleton.nif"
    );
}

#[test]
fn an_extra_listed_is_kept_beside_the_race_default_of_its_type() {
    let mut w = world();
    w.parts.insert(4, part("Mouth", HeadPartKind::Misc, true));
    let mut race = w.races[&100].clone();
    race.sexes[0].head.parts.push((3, f(4)));
    w.races.insert(100, race);
    let mut bit = part("Beard_1bit", HeadPartKind::Misc, true);
    bit.flags = EXTRA_PART;
    w.parts.insert(40, bit);
    let race = &w.races[&100];
    let got = head_parts(&w, race, false, &[f(40)]);
    assert_eq!(
        names(&w, &got),
        ["Beard_1bit", "EyesDefault", "Face", "Mouth", "NoScar"]
    );
    // Listed as a part of its own (no extra flag) it stands for the type.
    w.parts.get_mut(&40).unwrap().flags = 0;
    let race = &w.races[&100];
    let got = head_parts(&w, race, false, &[f(40)]);
    assert_eq!(
        names(&w, &got),
        ["Beard_1bit", "EyesDefault", "Face", "NoScar"]
    );
}

#[test]
fn a_part_without_a_model_still_stands_for_its_type() {
    let mut w = world();
    w.parts
        .insert(50, part("NoHair", HeadPartKind::Hair, false));
    let race = &w.races[&100];
    // Listed first, it keeps the hair after it (and its hairline) off.
    let got = head_parts(&w, race, false, &[f(50), f(10)]);
    assert_eq!(names(&w, &got), ["EyesDefault", "Face", "NoHair", "NoScar"]);
    let got = head_parts(&w, race, false, &[f(10), f(50)]);
    assert_eq!(
        names(&w, &got),
        ["EyesDefault", "Face", "Hair", "Hairline", "More", "NoScar"]
    );
    // A listed face stands in for the race's.
    w.parts
        .insert(60, part("OtherFace", HeadPartKind::Face, true));
    let race = &w.races[&100];
    let got = head_parts(&w, race, false, &[f(60)]);
    assert_eq!(names(&w, &got), ["EyesDefault", "NoScar", "OtherFace"]);
    // The plan draws only those with a model.
    let look = Look {
        race: Some(f(100)),
        head_parts: vec![f(50)],
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    let drawn: Vec<&str> = p.head.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(drawn, ["EyesDefault", "Face"]);
}

#[test]
fn a_look_follows_its_template_for_its_face() {
    let mut w = world();
    w.npcs.insert(
        1000,
        Npc {
            template: Some(f(1001)),
            template_flags: USE_TRAITS,
            race: Some(f(101)),
            weight: Some(10.0),
            ..Npc::default()
        },
    );
    w.npcs.insert(
        1001,
        Npc {
            race: Some(f(100)),
            head_parts: vec![f(20)],
            weight: Some(70.0),
            ..Npc::default()
        },
    );
    w.kinds.insert(1000, *b"NPC_");
    w.kinds.insert(1001, *b"NPC_");
    let l = Look::of_form(&w, &f(1000)).unwrap();
    assert_eq!(l.race, Some(f(100)));
    assert_eq!(l.head_parts, [f(20)]);
    assert_eq!(l.weight, 70.0);
    // Without the flag, its own.
    w.npcs.get_mut(&1000).unwrap().template_flags = 0;
    let l = Look::of_form(&w, &f(1000)).unwrap();
    assert_eq!((l.race, l.weight), (Some(f(101)), 10.0));
}

fn set(textures: [Option<&str>; 8], flags: u16) -> TextureSet {
    TextureSet {
        textures: textures.map(|t| t.map(str::to_string)),
        flags,
        ..TextureSet::default()
    }
}

#[test]
fn a_faces_textures_are_the_looks_over_the_races_over_its_own_but_its_colour() {
    let mut w = world();
    let mut face = w.parts[&1].clone();
    face.texture_set = Some(f(400));
    w.parts.insert(1, face);
    // TX02 goes to slot 5, TX03 to 2, TX04 to 3.
    w.sets.insert(
        400,
        set(
            [
                Some("PartFace.dds"),
                Some("Part_msn.dds"),
                Some("PartEnv.dds"),
                Some("PartSk.dds"),
                Some("PartDetail.dds"),
                None,
                None,
                None,
            ],
            0,
        ),
    );
    w.sets.insert(
        401,
        set(
            [
                Some("RaceFace.dds"),
                Some("Race_msn.dds"),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            0,
        ),
    );
    w.sets.insert(
        402,
        set(
            [
                Some("LookFace.dds"),
                Some("Look_msn.dds"),
                None,
                Some("LookSk.dds"),
                None,
                None,
                None,
                None,
            ],
            0,
        ),
    );
    let mut race = w.races[&100].clone();
    race.sexes[0].head.default_face_texture = Some(f(401));
    w.races.insert(100, race);
    let mut look = Look {
        race: Some(f(100)),
        face_texture: Some(f(402)),
        ..Look::default()
    };
    let textures = |w: &Fake, look: &Look| {
        let p = plan(look, w).unwrap();
        p.head
            .iter()
            .find(|h| h.name == "Face")
            .unwrap()
            .textures
            .clone()
    };
    let s = |t: &str| Some(t.to_string());
    assert_eq!(
        textures(&w, &look),
        [
            s("PartFace.dds"),
            s("Look_msn.dds"),
            s("LookSk.dds"),
            s("PartDetail.dds"),
            None,
            None,
            None,
            None
        ]
    );
    // Without the look's, the race's normal map over the part's.
    look.face_texture = None;
    assert_eq!(
        textures(&w, &look),
        [
            s("PartFace.dds"),
            s("Race_msn.dds"),
            s("PartSk.dds"),
            s("PartDetail.dds"),
            None,
            None,
            None,
            None
        ]
    );
    // A part with no colour of its own takes the look's, then the race's.
    w.sets.get_mut(&400).unwrap().textures[0] = None;
    assert_eq!(textures(&w, &look)[0], s("RaceFace.dds"));
    look.face_texture = Some(f(402));
    assert_eq!(textures(&w, &look)[0], s("LookFace.dds"));
}

#[test]
fn other_parts_take_colour_and_a_normal_map_not_in_model_space() {
    let mut w = world();
    let mut eyes = w.parts[&20].clone();
    eyes.texture_set = Some(f(410));
    w.parts.insert(20, eyes);
    let mut scar = w.parts[&30].clone();
    scar.texture_set = Some(f(411));
    w.parts.insert(30, scar);
    let both = [
        Some("A.dds"),
        Some("A_n.dds"),
        Some("A_env.dds"),
        Some("A_sk.dds"),
        None,
        None,
        None,
        None,
    ];
    w.sets.insert(410, set(both, 0x4));
    w.sets.insert(411, set(both, 0x1));
    let look = Look {
        race: Some(f(100)),
        head_parts: vec![f(20), f(30)],
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    let of = |n: &str| {
        p.head
            .iter()
            .find(|h| h.name == n)
            .unwrap()
            .textures
            .clone()
    };
    let s = |t: &str| Some(t.to_string());
    assert_eq!(
        of("EyesA"),
        [s("A.dds"), None, None, None, None, None, None, None]
    );
    assert_eq!(
        of("ScarL"),
        [s("A.dds"), s("A_n.dds"), None, None, None, None, None, None]
    );
    // A part with no set of its own says nothing.
    assert_eq!(of("Face"), <[Option<String>; 8]>::default());
}

#[test]
fn the_hair_colour_is_twice_its_colour_and_else_the_races_and_else_black() {
    let mut w = world();
    let colour = |rgba: [u8; 4]| Colour {
        rgba,
        ..Colour::default()
    };
    w.colours.insert(500, colour([57, 55, 40, 0]));
    w.colours.insert(501, colour([10, 20, 30, 0]));
    w.colours.insert(502, colour([200, 0, 100, 0]));
    let mut race = w.races[&100].clone();
    race.default_hair_colour = [Some(f(501)), Some(f(502))];
    w.races.insert(100, race);
    let tint = |w: &Fake, look: &Look| plan(look, w).unwrap().hair_colour.unwrap();
    let mut look = Look {
        race: Some(f(100)),
        hair_colour: Some(f(500)),
        ..Look::default()
    };
    let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|c| (a[c] - b[c]).abs() < 1e-6);
    assert!(near(
        tint(&w, &look),
        [114.0 / 255.0, 110.0 / 255.0, 80.0 / 255.0]
    ));
    look.hair_colour = None;
    assert!(near(
        tint(&w, &look),
        [20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0]
    ));
    look.female = true;
    assert!(near(tint(&w, &look), [400.0 / 255.0, 0.0, 200.0 / 255.0]));
    w.races.get_mut(&100).unwrap().default_hair_colour = [None, None];
    assert_eq!(tint(&w, &look), [0.0; 3]);
    // A skin tone that is not numbers is none.
    look.skin_tone = Some([0.5, f32::NAN, 0.5]);
    assert_eq!(plan(&look, &w).unwrap().skin_tone, None);
    look.skin_tone = Some([0.5, 0.25, 1.0]);
    assert_eq!(plan(&look, &w).unwrap().skin_tone, Some([0.5, 0.25, 1.0]));
}

#[test]
fn a_womans_body_takes_her_models_and_else_the_mans_and_records_not_there_are_noted() {
    let mut w = body_world();
    let mut addon = w.addons[&201].clone();
    addon.models[1] = Some("Actors\\FemaleBody_1.nif".into());
    addon.skin_textures[1] = Some(f(301));
    // Another race's, worn by this one too.
    addon.additional_races = vec![f(101)];
    w.addons.insert(201, addon);
    w.sets.insert(
        301,
        set(
            [Some("Her.dds"), None, None, None, None, None, None, None],
            0,
        ),
    );
    let look = Look {
        race: Some(f(100)),
        female: true,
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    let body = &p.body[0];
    assert_eq!(body.model, "meshes/actors/femalebody_1.nif");
    assert_eq!(
        body.weights,
        Some((
            "meshes/actors/femalebody_0.nif".into(),
            "meshes/actors/femalebody_1.nif".into()
        ))
    );
    assert_eq!(body.textures[0].as_deref(), Some("Her.dds"));
    // The tail has no woman's model or textures: the man's.
    let tail = &p.body[1];
    assert_eq!(tail.model, "meshes/actors/tail.nif");
    assert_eq!(tail.weights, None);
    assert_eq!(tail.textures[0].as_deref(), Some("Body.dds"));
    // A man never takes a woman's textures.
    let mut w2 = w.clone();
    w2.addons.get_mut(&201).unwrap().skin_textures[0] = None;
    let man = Look {
        race: Some(f(100)),
        ..Look::default()
    };
    assert_eq!(plan(&man, &w2).unwrap().body[0].textures[0], None);

    // The other race wears the first addon, which names it as well as its own.
    let mut w3 = w.clone();
    w3.races.insert(101, w3.races[&100].clone());
    let other = Look {
        race: Some(f(101)),
        ..Look::default()
    };
    let names: Vec<String> = plan(&other, &w3)
        .unwrap()
        .body
        .iter()
        .map(|b| b.name.clone())
        .collect();
    assert_eq!(names, ["Body_1.nif", "Hands_1.nif"]);

    // A skin, an addon and a head part that are not there are each noted.
    let mut w4 = w.clone();
    w4.addons.remove(&204);
    let mut look = look.clone();
    look.head_parts = vec![f(777)];
    let p = plan(&look, &w4).unwrap();
    assert!(
        p.notes.contains(&"No armour addon 000000CC.".to_string()),
        "{:?}",
        p.notes
    );
    assert!(
        p.notes.contains(&"No head part 00000309.".to_string()),
        "{:?}",
        p.notes
    );
    look.skin = Some(f(888));
    let p = plan(&look, &w4).unwrap();
    assert!(p.body.is_empty());
    assert!(
        p.notes.contains(&"No skin 00000378.".to_string()),
        "{:?}",
        p.notes
    );
}

#[test]
fn files_shared_by_two_parts_are_named_once_and_a_body_of_one_model_by_it() {
    let mut w = body_world();
    let mut eyes = w.parts[&2].clone();
    eyes.files = vec![PartFile {
        kind: PartFileKind::RaceMorph,
        path: "actors\\FACERACES.tri".into(),
    }];
    w.parts.insert(2, eyes);
    let look = Look {
        race: Some(f(100)),
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    let files = p.files();
    assert_eq!(
        files
            .iter()
            .filter(|k| *k == "meshes/actors/faceraces.tri")
            .count(),
        1
    );
    assert!(files.contains(&"meshes/actors/tail.nif".to_string()));
    assert!(!files.iter().any(|k| k.contains("tail_")));
}
