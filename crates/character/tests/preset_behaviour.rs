//! What `apoc_character::preset` promises, proved through its public API
//! only: presets written here by hand (no byte of any third-party preset),
//! records from [`World`], a `Records` made here, and generated inputs from
//! a seeded generator, so a failure names the seed that made it.

use std::collections::HashMap;

use apoc_character::preset::{
    apply_look, look_of_preset, new_document, LoadOrder, Note, Place, Plugin, PresetError,
    PresetReport,
};
use apoc_character::{plan, Look, Records, Tint};
use apoc_jslot::{parse, write, Document, Number, Preset, Value};
use apoc_plugin::records::{
    Armor, ArmorAddon, Colour, HeadPart, HeadPartKind, Npc, Race, TextureSet, TintMask,
};
use apoc_plugin::FormRef;

// ---------------------------------------------------------------------------
// A seeded generator (splitmix64).

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03))
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// 0 to `n`, both included.
    fn upto(&mut self, n: usize) -> usize {
        self.below(n as u64 + 1) as usize
    }
    fn one_in(&mut self, n: u64) -> bool {
        self.below(n) == 0
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
    fn u32(&mut self) -> u32 {
        self.next() as u32
    }
    fn u8(&mut self) -> u8 {
        self.next() as u8
    }
    /// Any finite float, from its bits.
    fn finite_f32(&mut self) -> f32 {
        loop {
            let v = f32::from_bits(self.u32());
            if v.is_finite() {
                return v;
            }
        }
    }
    fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            xs.swap(i, j);
        }
    }
}

fn recase(rng: &mut Rng, s: &str) -> String {
    s.chars()
        .map(|c| {
            if rng.one_in(2) {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Records made by hand, looked up by plugin and id (plugin without case, as
// `FormRef` compares).

#[derive(Default, Clone)]
struct World {
    races: HashMap<FormRef, Race>,
    parts: HashMap<FormRef, HeadPart>,
    sets: HashMap<FormRef, TextureSet>,
    colours: HashMap<FormRef, Colour>,
}

impl Records for World {
    fn kind(&self, form: &FormRef) -> Option<[u8; 4]> {
        if self.races.contains_key(form) {
            Some(*b"RACE")
        } else if self.parts.contains_key(form) {
            Some(*b"HDPT")
        } else if self.sets.contains_key(form) {
            Some(*b"TXST")
        } else if self.colours.contains_key(form) {
            Some(*b"CLFM")
        } else {
            None
        }
    }
    fn npc(&self, _: &FormRef) -> Option<Npc> {
        None
    }
    fn race(&self, form: &FormRef) -> Option<Race> {
        self.races.get(form).cloned()
    }
    fn head_part(&self, form: &FormRef) -> Option<HeadPart> {
        self.parts.get(form).cloned()
    }
    fn texture_set(&self, form: &FormRef) -> Option<TextureSet> {
        self.sets.get(form).cloned()
    }
    fn armor(&self, _: &FormRef) -> Option<Armor> {
        None
    }
    fn armor_addon(&self, _: &FormRef) -> Option<ArmorAddon> {
        None
    }
    fn colour(&self, form: &FormRef) -> Option<Colour> {
        self.colours.get(form).cloned()
    }
}

const SKIN: &str = "Actors\\Character\\Character Assets\\TintMasks\\SkinTone.dds";

fn race() -> FormRef {
    FormRef::new("Race.esp", 0x000D62)
}
fn face() -> FormRef {
    FormRef::new("Race.esp", 0x0144EA)
}
fn mouth() -> FormRef {
    FormRef::new("Race.esp", 0x019426)
}
fn eyes() -> FormRef {
    FormRef::new("Race.esp", 0x02A000)
}
fn hair_a() -> FormRef {
    FormRef::new("Hair.esl", 0xB1A)
}
fn hair_c() -> FormRef {
    FormRef::new("Hair.esl", 0xB1B)
}
fn brows_l() -> FormRef {
    FormRef::new("Hair.esl", 0xC01)
}
fn hair_b() -> FormRef {
    FormRef::new("Hair2.esp", 0x801)
}
fn brows() -> FormRef {
    FormRef::new("Hair2.esp", 0x802)
}
fn scar() -> FormRef {
    FormRef::new("Hair2.esp", 0x803)
}
fn hair_x() -> FormRef {
    FormRef::new("Extra.esm", 0x900)
}
/// A head part whose id is too large for its light plugin.
fn hair_too_big() -> FormRef {
    FormRef::new("Hair.esl", 0x1000)
}
fn texture() -> FormRef {
    FormRef::new("Race.esp", 0x011D49)
}
fn light_texture() -> FormRef {
    FormRef::new("Hair.esl", 0xD01)
}
fn clfm() -> FormRef {
    FormRef::new("Extra.esm", 0x500)
}

fn loaded_parts() -> Vec<FormRef> {
    vec![
        face(),
        mouth(),
        eyes(),
        hair_a(),
        hair_c(),
        brows_l(),
        hair_b(),
        brows(),
        scar(),
        hair_x(),
    ]
}

fn mask(index: u16, path: Option<&str>, kind: u16) -> TintMask {
    TintMask {
        index,
        mask: path.map(str::to_string),
        kind: Some(kind),
        ..TintMask::default()
    }
}

/// The women's masks: places 0 to 5, place 3 with no texture.
fn female_masks() -> Vec<TintMask> {
    vec![
        mask(0x18, Some(SKIN), 6),
        mask(0x1A, Some("Masks\\Lips.dds"), 1),
        mask(0x1B, Some("Masks\\Cheeks.dds"), 2),
        mask(0x1C, None, 7),
        mask(0x1D, Some("Masks/Brow.dds"), 3),
        mask(0x1E, Some("Masks\\Paint.dds"), 7),
    ]
}

fn male_masks() -> Vec<TintMask> {
    vec![
        mask(0x30, Some(SKIN), 6),
        mask(0x31, Some("Masks\\Beard.dds"), 7),
    ]
}

fn head_part(name: &str, kind: HeadPartKind) -> HeadPart {
    HeadPart {
        editor_id: Some(name.into()),
        kind: Some(kind),
        model: Some(format!("Actors\\Character\\{name}.nif")),
        ..HeadPart::default()
    }
}

fn world() -> World {
    let mut w = World::default();
    let mut r = Race {
        editor_id: Some("TestRace".into()),
        ..Race::default()
    };
    r.sexes[0].head.tints = male_masks();
    r.sexes[1].head.tints = female_masks();
    w.races.insert(race(), r);
    for (f, name, kind) in [
        (face(), "Face", HeadPartKind::Face),
        (mouth(), "Mouth", HeadPartKind::Misc),
        (eyes(), "Eyes", HeadPartKind::Eyes),
        (hair_a(), "HairA", HeadPartKind::Hair),
        (hair_c(), "HairC", HeadPartKind::Hair),
        (brows_l(), "BrowsL", HeadPartKind::Eyebrows),
        (hair_b(), "HairB", HeadPartKind::Hair),
        (brows(), "Brows", HeadPartKind::Eyebrows),
        (scar(), "Scar", HeadPartKind::Scar),
        (hair_x(), "HairX", HeadPartKind::Hair),
        (hair_too_big(), "HairBig", HeadPartKind::Hair),
        // A record of a plugin that is not in the load order.
        (
            FormRef::new("Gone.esp", 0x802),
            "HairGone",
            HeadPartKind::Hair,
        ),
    ] {
        w.parts.insert(f, head_part(name, kind));
    }
    w.sets.insert(texture(), TextureSet::default());
    w.sets.insert(light_texture(), TextureSet::default());
    w.colours.insert(
        clfm(),
        Colour {
            rgba: [57, 55, 40, 0],
            ..Colour::default()
        },
    );
    w
}

/// Skyrim.esm 0, Update.esm 1, Race.esp 2, Hair2.esp 3, Extra.esm 4;
/// Filler.esl light slot 0, Hair.esl light slot 1.
fn order() -> LoadOrder {
    LoadOrder::new(vec![
        Plugin::new("Skyrim.esm", false),
        Plugin::new("Update.esm", false),
        Plugin::new("Filler.esl", true),
        Plugin::new("Race.esp", false),
        Plugin::new("Hair.esl", true),
        Plugin::new("Hair2.esp", false),
        Plugin::new("Extra.esm", false),
    ])
}

/// A full preset in RaceMenu's layout, made up here. Its parts: the face,
/// the mouth (identifier in lower case), a light hair, a scar, a part of
/// `Gone.esp` (not loaded, index 0x50, with an extra key) and one of
/// `Lost.esl` (not loaded, light). Its tint layers: the skin tone, the lips
/// unused, the cheeks, a layer not of the race, the paint.
fn fixture() -> String {
    r#"{
   "actor" : {
      "hairColor" : 5196362,
      "headTexture" : "Race.esp|011D49",
      "weight" : 21
   },
   "bodyMorphs" : [
      {
         "keys" : [ { "key" : "Body.esp", "value" : 0.30000001192092896 } ],
         "name" : "Kept"
      }
   ],
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
         "formId" : 50333699,
         "formIdentifier" : "Hair2.esp|000803",
         "type" : 2
      },
      {
         "extra" : true,
         "formId" : 1342179329,
         "formIdentifier" : "Gone.esp|000801",
         "type" : 9
      },
      {
         "formId" : 4261912771,
         "formIdentifier" : "Lost.esl|07A0C3",
         "type" : 10
      }
   ],
   "modNames" : [ "Gone.esp", "Hair.esl", "Hair2.esp", "Lost.esl", "Race.esp" ],
   "mods" : [
      {
         "index" : 2,
         "name" : "Race.esp"
      },
      {
         "index" : 3,
         "name" : "Hair2.esp"
      },
      {
         "index" : 80,
         "name" : "Gone.esp"
      }
   ],
   "morphs" : {
      "custom" : [
         {
            "name" : "AB_Lip_Width",
            "value" : 0.3100000023841858
         },
         {
            "name" : "Brow_Up",
            "value" : -1
         },
         {
            "name" : "Tiny",
            "value" : 1e-05
         },
         {
            "name" : "Keep",
            "value" : 0.50,
            "x" : 1
         }
      ],
      "default" : {
         "morphs" : [
            0,
            0.3100000023841858,
            -1,
            1e-05,
            0.50,
            1.049999952316284,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            3.402823466385289e+38
         ],
         "presets" : [ 0, 4294967295, 3, 0 ]
      },
      "sculpt" : [],
      "sculptDivisor" : 10000
   },
   "overrides" : {
      "Face [Ovl0]" : [ 1, 2, 3 ]
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
      },
      {
         "color" : 4279246896,
         "index" : 5,
         "texture" : "Masks\\Paint.dds"
      }
   ],
   "version" : {
      "formatVersion" : 3,
      "runtimeVersion" : 1,
      "signature" : 2,
      "skseVersion" : 3
   }
}
"#
    .to_string()
}

fn fixture_doc() -> Document {
    parse(fixture().as_bytes()).unwrap()
}

fn text(d: &Document) -> String {
    String::from_utf8(write(d).unwrap()).unwrap()
}

fn top<'a>(d: &'a Document, key: &str) -> Option<&'a Value> {
    d.root.as_object().unwrap().get(key)
}

/// Lines removed from `a` and added in `b`, trimmed, by a longest common
/// subsequence of lines.
fn diff(a: &str, b: &str) -> (Vec<String>, Vec<String>) {
    let a: Vec<&str> = a.lines().collect();
    let b: Vec<&str> = b.lines().collect();
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            added.push(b[j].trim().to_string());
            j += 1;
        } else {
            removed.push(a[i].trim().to_string());
            i += 1;
        }
    }
    (removed, added)
}

fn lines(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------------------
// 1. LoadOrder, against a model of it.

/// The place of each plugin of a list with no name twice: regular plugins
/// take 0 to 0xFD in their order, light ones 0 to 0xFFF in theirs.
fn model_places(plugins: &[Plugin]) -> Vec<Option<Place>> {
    let (mut full, mut light) = (0usize, 0usize);
    plugins
        .iter()
        .map(|p| {
            if p.light {
                light += 1;
                (light - 1 <= 0xFFF).then(|| Place::Light((light - 1) as u16))
            } else {
                full += 1;
                (full - 1 <= 0xFD).then(|| Place::Full((full - 1) as u8))
            }
        })
        .collect()
}

fn model_form_id(place: Place, id: u32) -> u32 {
    match place {
        Place::Full(i) => (u32::from(i) << 24) | id,
        Place::Light(s) => (0xFE << 24) | (u32::from(s) << 12) | id,
    }
}

/// Every promise of a load order for one plugin at its place.
fn check_plugin(o: &LoadOrder, p: &Plugin, place: Option<Place>, rng: &mut Rng, seed: u64) {
    let ctx = format!("seed {seed}, {} ({place:?})", p.name);
    let asked = recase(rng, &p.name);
    assert_eq!(o.place(&asked), place, "{ctx}: place of {asked}");
    let Some(place) = place else {
        let form = FormRef::new(&asked, 1);
        assert_eq!(o.form_id(&form), None, "{ctx}");
        assert_eq!(o.identifier(&form), None, "{ctx}");
        return;
    };
    assert_eq!(
        o.at(place).map(|q| q.name.as_str()),
        Some(p.name.as_str()),
        "{ctx}"
    );
    let id = match place {
        Place::Full(_) => rng.u32() & 0xFF_FFFF,
        Place::Light(_) => rng.u32() & 0xFFF,
    };
    let form = FormRef::new(&asked, id);
    let fid = model_form_id(place, id);
    assert_eq!(o.form_id(&form), Some(fid), "{ctx}: form id of {id:06X}");
    // Back from the id, spelled as the load order spells it.
    let back = o
        .form_of_id(fid)
        .unwrap_or_else(|| panic!("{ctx}: {fid:08X}"));
    assert_eq!(
        (back.plugin.as_str(), back.id),
        (p.name.as_str(), id),
        "{ctx}"
    );
    // The identifier: the low 24 bits of the load-order id.
    let ident = o.identifier(&form).unwrap();
    assert_eq!(ident.plugin, p.name, "{ctx}");
    assert_eq!(ident.id, fid & 0xFF_FFFF, "{ctx}");
    for written_id in [Some(fid), None] {
        let again = o.form_of_identifier(&ident, written_id);
        assert_eq!(again, form, "{ctx}");
        assert_eq!(again.plugin, p.name, "{ctx}");
    }
    // The identifier as a preset may spell its plugin.
    let spelled = apoc_jslot::FormRef {
        plugin: recase(rng, &p.name),
        id: ident.id,
    };
    assert_eq!(o.form_of_identifier(&spelled, None).plugin, p.name, "{ctx}");
    if let Place::Light(_) = place {
        // Saved with the plugin in another slot: the slot is not the id.
        let other = rng.u32() & 0xFFF;
        let saved = apoc_jslot::FormRef {
            plugin: asked.clone(),
            id: (other << 12) | id,
        };
        let saved_id = (0xFE << 24) | (other << 12) | id;
        assert_eq!(o.form_of_identifier(&saved, None), form, "{ctx}");
        assert_eq!(o.form_of_identifier(&saved, Some(saved_id)), form, "{ctx}");
        // A form whose id is past 12 bits has no id in a light plugin.
        let big = FormRef::new(&asked, 0x1000 | (rng.u32() & 0xFF_FFFF));
        assert_eq!(o.form_id(&big), None, "{ctx}");
        assert_eq!(o.identifier(&big), None, "{ctx}");
    }
}

/// Load-order ids: those that name something come back to themselves, and
/// those that name nothing are the ones whose index or slot is empty.
fn check_ids(o: &LoadOrder, places: &[Option<Place>], rng: &mut Rng, seed: u64) {
    for _ in 0..200 {
        let top = match rng.below(4) {
            0 => 0xFE,
            1 => 0xFF,
            2 => rng.below(8) as u32,
            _ => rng.u32() >> 24,
        };
        let x = (top << 24) | (rng.u32() & 0xFF_FFFF);
        let occupied = match top {
            0xFF => false,
            0xFE => places.contains(&Some(Place::Light(((x >> 12) & 0xFFF) as u16))),
            i => places.contains(&Some(Place::Full(i as u8))),
        };
        match o.form_of_id(x) {
            Some(f) => {
                assert!(occupied, "seed {seed}: {x:08X} named {f:?}");
                assert_eq!(o.form_id(&f), Some(x), "seed {seed}: {x:08X}");
            }
            None => assert!(!occupied, "seed {seed}: {x:08X} named nothing"),
        }
    }
}

fn random_plugins(rng: &mut Rng, full: usize, light: usize) -> Vec<Plugin> {
    let exts_full = ["esp", "esm", "ESP"];
    let exts_light = ["esl", "esp", "ESL"];
    let mut out: Vec<Plugin> = Vec::new();
    for i in 0..full {
        out.push(Plugin::new(
            &format!("Mod {i}.{}", rng.pick(&exts_full)),
            false,
        ));
    }
    for i in 0..light {
        out.push(Plugin::new(
            &format!("Small {i}.{}", rng.pick(&exts_light)),
            true,
        ));
    }
    rng.shuffle(&mut out);
    out
}

#[test]
fn load_order_round_trips_over_random_orders() {
    for seed in 0..400 {
        let mut rng = Rng::new(seed);
        let (full, light) = (rng.upto(40), rng.upto(40));
        let plugins = random_plugins(&mut rng, full, light);
        let places = model_places(&plugins);
        let o = LoadOrder::new(plugins.clone());
        for (p, place) in plugins.iter().zip(&places) {
            check_plugin(&o, p, *place, &mut rng, seed);
        }
        check_ids(&o, &places, &mut rng, seed);
        assert_eq!(o.place("Not There.esp"), None);
    }
}

#[test]
fn load_order_limits_are_0xfd_for_regular_plugins_and_0xfff_for_light_ones() {
    for seed in 0..3 {
        let mut rng = Rng::new(1000 + seed);
        // Past both limits, interleaved.
        let plugins = random_plugins(&mut rng, 0xFE + 2 + seed as usize, 0x1000 + 3);
        let places = model_places(&plugins);
        assert!(places.contains(&Some(Place::Full(0xFD))));
        assert!(places.contains(&Some(Place::Light(0xFFF))));
        let o = LoadOrder::new(plugins.clone());
        // Every plugin near a limit, and a sample of the rest.
        let (mut full, mut light) = (0usize, 0usize);
        for (p, place) in plugins.iter().zip(&places) {
            let n = if p.light {
                light += 1;
                light - 1
            } else {
                full += 1;
                full - 1
            };
            let near = if p.light {
                n + 4 >= 0x1000
            } else {
                n + 4 >= 0xFE
            };
            if near || rng.one_in(40) {
                check_plugin(&o, p, *place, &mut rng, seed);
            }
            if !p.light && n >= 0xFE {
                assert_eq!(*place, None);
                assert_eq!(o.place(&p.name), None, "{} is regular number {n}", p.name);
            }
            if p.light && n >= 0x1000 {
                assert_eq!(o.place(&p.name), None, "{} is light number {n}", p.name);
            }
        }
        check_ids(&o, &places, &mut rng, seed);
        // 0xFE and 0xFF are never a regular plugin's index.
        assert_eq!(o.at(Place::Light(0xFFF)).map(|p| p.light), Some(true));
        assert!(o.form_of_id(0xFF00_0001).is_none());
    }
}

#[test]
fn names_that_differ_only_in_case_are_one_plugin() {
    let o = LoadOrder::new(vec![
        Plugin::new("Skyrim.esm", false),
        Plugin::new("UBE_AllRace.esp", false),
        Plugin::new("ube_allrace.ESP", false),
        Plugin::new("Hair.esl", true),
        Plugin::new("HAIR.ESL", true),
    ]);
    for asked in ["UBE_AllRace.esp", "ube_allrace.esp", "UBE_ALLRACE.ESP"] {
        assert_eq!(o.place(asked), Some(Place::Full(1)), "{asked}");
        let f = o.form_of_id(o.form_id(&FormRef::new(asked, 0x0144EA)).unwrap());
        // Spelled as listed first.
        assert_eq!(f.unwrap().plugin, "UBE_AllRace.esp");
    }
    assert_eq!(o.place("hair.esl"), Some(Place::Light(0)));
    assert_eq!(
        o.identifier(&FormRef::new("hAiR.eSl", 0xB1A))
            .unwrap()
            .to_string(),
        "Hair.esl|000B1A"
    );
}

#[test]
fn a_plugin_listed_twice_has_its_first_place_only() {
    for seed in 0..300 {
        let mut rng = Rng::new(5000 + seed);
        let (full, light) = (rng.upto(30), rng.upto(30));
        let firsts = random_plugins(&mut rng, full, light);
        if firsts.is_empty() {
            continue;
        }
        // Copies, spelled another way and maybe of the other kind, each
        // after the first listing.
        let mut plugins = firsts.clone();
        for _ in 0..rng.upto(6) {
            let i = rng.below(plugins.len() as u64) as usize;
            let name = plugins[i].name.clone();
            let first = plugins
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&name))
                .unwrap();
            let at = first + 1 + rng.upto(plugins.len() - first - 1);
            let light = if rng.one_in(3) {
                !plugins[first].light
            } else {
                plugins[first].light
            };
            plugins.insert(at, Plugin::new(&recase(&mut rng, &name), light));
        }
        let o = LoadOrder::new(plugins.clone());
        let mut seen: Vec<Place> = Vec::new();
        for p in &firsts {
            let place = o.place(&p.name);
            let Some(place) = place else {
                continue;
            };
            assert!(!seen.contains(&place), "seed {seed}: {place:?} twice");
            seen.push(place);
            // The place is that of the first listing: its kind, and it
            // names the first listing's spelling.
            assert_eq!(matches!(place, Place::Light(_)), p.light, "seed {seed}");
            check_plugin(&o, p, Some(place), &mut rng, seed);
        }
        // Whatever names a place is a first listing.
        for s in 0..0x40u16 {
            for place in [Place::Full(s as u8), Place::Light(s)] {
                if let Some(q) = o.at(place) {
                    assert!(
                        firsts.iter().any(|f| f.name == q.name),
                        "seed {seed}: {place:?} names {}, a repeat",
                        q.name
                    );
                    assert_eq!(o.place(&q.name), Some(place), "seed {seed}");
                }
            }
        }
        // Ids still come back to themselves.
        for _ in 0..200 {
            let x = if rng.one_in(2) {
                (0xFE << 24) | (rng.u32() & 0x3F_FFFF)
            } else {
                ((rng.below(0x40) as u32) << 24) | (rng.u32() & 0xFF_FFFF)
            };
            if let Some(f) = o.form_of_id(x) {
                assert_eq!(o.form_id(&f), Some(x), "seed {seed}: {x:08X}");
            }
        }
    }
}

#[test]
fn a_plugin_not_loaded_keeps_its_spelling_and_is_light_only_when_its_form_id_says_so() {
    let o = order();
    let ident = |t: &str| apoc_jslot::FormRef::parse(t).unwrap();
    let mut rng = Rng::new(77);
    for _ in 0..500 {
        let id = rng.u32() & 0xFF_FFFF;
        let i = apoc_jslot::FormRef {
            plugin: "Gone Mod.esp".into(),
            id,
        };
        let light = o.form_of_identifier(&i, Some((0xFE << 24) | id));
        assert_eq!(
            (light.plugin.as_str(), light.id),
            ("Gone Mod.esp", id & 0xFFF)
        );
        let top = rng.below(0xFE) as u32;
        let full = o.form_of_identifier(&i, Some((top << 24) | id));
        assert_eq!((full.plugin.as_str(), full.id), ("Gone Mod.esp", id));
        assert_eq!(o.form_of_identifier(&i, None).id, id);
    }
    // A plugin light when the preset was saved (its form id under 0xFE)
    // keeps its 12-bit ids though it is regular now; one light now is light
    // whatever the form id says.
    assert_eq!(
        o.form_of_identifier(&ident("Race.esp|14DB1A"), Some(0xFE14_DB1A)),
        FormRef::new("Race.esp", 0xB1A)
    );
    assert_eq!(
        o.form_of_identifier(&ident("Race.esp|14DB1A"), Some(0x0214_DB1A)),
        FormRef::new("Race.esp", 0x14DB1A)
    );
    assert_eq!(
        o.form_of_identifier(&ident("Hair.esl|14DB1A"), Some(0x0314_DB1A)),
        FormRef::new("Hair.esl", 0xB1A)
    );
}

// ---------------------------------------------------------------------------
// 2. A look read from a preset and written straight back changes no byte.

const FLOATS: &[&str] = &[
    "0",
    "-0",
    "1",
    "-1",
    "21",
    "100",
    "0.5",
    "0.50",
    "0.3100000023841858",
    "1e-05",
    "1E-5",
    "1.049999952316284",
    "3.402823466385289e+38",
    "-3.402823466385289e+38",
    "1e2",
    "0.1",
    "12.75",
    "2.5e-3",
    "7",
];

const WHOLES: &[&str] = &["0", "4294967295", "3", "2147483648", "19", "1"];

fn jstr(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\"))
}

fn jobj(members: &[(&str, String)]) -> String {
    let inner: Vec<String> = members
        .iter()
        .map(|(k, v)| format!("{} : {v}", jstr(k)))
        .collect();
    format!("{{ {} }}", inner.join(", "))
}

fn jarr(items: &[String]) -> String {
    format!("[ {} ]", items.join(", "))
}

/// A texture path spelled another way: case, and `/` for `\`.
fn respell(rng: &mut Rng, path: &str) -> String {
    let p = if rng.one_in(2) {
        recase(rng, path)
    } else {
        path.to_string()
    };
    if rng.one_in(3) {
        p.replace('\\', "/")
    } else {
        p
    }
}

/// A generated tint entry: its JSON, the mask (`TINI`) and colour when it
/// is the race's, the texture and alpha when it is not.
type Layer = (String, Option<(u16, [u8; 4])>, Option<(String, u8)>);

struct Generated {
    text: String,
    order: LoadOrder,
    female: bool,
    parts: Vec<FormRef>,
    not_loaded: Vec<usize>,
    not_parts: Vec<usize>,
    foreign: Vec<(usize, String, u8)>,
    tints: Vec<Tint>,
    texture_note: Option<Note>,
}

/// A well-formed preset: every section there, parts loaded, light, not
/// loaded and not head parts, tint layers of the race (unused, in use,
/// textures spelled another way), not of it, and twice, odd number texts,
/// and keys a look does not own.
fn generate(rng: &mut Rng) -> Generated {
    let female = rng.one_in(2);
    let mut plugins = vec![
        Plugin::new("Race.esp", false),
        Plugin::new("Hair.esl", true),
        Plugin::new("Hair2.esp", false),
        Plugin::new("Extra.esm", false),
    ];
    for i in 0..rng.upto(40) {
        let light = rng.one_in(3);
        let ext = if light { "esl" } else { "esp" };
        plugins.push(Plugin::new(&format!("Filler {i}.{ext}"), light));
    }
    rng.shuffle(&mut plugins);
    let places = model_places(&plugins);
    let place_of = |name: &str| {
        let i = plugins
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
            .unwrap();
        places[i].unwrap()
    };

    let mut named: Vec<String> = Vec::new();
    // The head texture.
    let mut texture_note = None;
    let head_texture = match rng.below(5) {
        0 => None,
        1 => Some(format!("{}|{:06X}", "Race.esp", texture().id)),
        2 => {
            let id = model_form_id(place_of("Hair.esl"), light_texture().id) & 0xFF_FFFF;
            Some(format!("Hair.esl|{id:06x}"))
        }
        3 => {
            let f = FormRef::new("Gone 9.esp", 0x42);
            texture_note = Some(Note::HeadTextureNotLoaded(f));
            Some("Gone 9.esp|000042".to_string())
        }
        _ => {
            let f = FormRef::new("Extra.esm", 0x123);
            texture_note = Some(Note::NotATextureSet(f));
            Some("Extra.esm|123".to_string())
        }
    };
    let head_texture = head_texture.map(|t| {
        let (p, id) = t.rsplit_once('|').unwrap();
        let p = if rng.one_in(3) {
            recase(rng, p)
        } else {
            p.to_string()
        };
        named.push(p.clone());
        format!("{p}|{id}")
    });

    // Head parts.
    let mut entries: Vec<String> = Vec::new();
    let (mut parts, mut not_loaded, mut not_parts) = (Vec::new(), Vec::new(), Vec::new());
    let mut mods: Vec<(u32, String)> = Vec::new();
    for at in 0..rng.upto(8) {
        let (fid, plugin, ident_id, form) = match rng.below(10) {
            0..=6 => {
                let is_part = rng.below(7) != 0;
                let f = if is_part {
                    rng.pick(&loaded_parts()).clone()
                } else {
                    not_parts.push(at);
                    rng.pick(&[
                        FormRef::new("Extra.esm", 0x123),
                        FormRef::new("Race.esp", 0x999),
                    ])
                    .clone()
                };
                let fid = model_form_id(place_of(&f.plugin), f.id);
                let plugin = if rng.one_in(3) {
                    recase(rng, &f.plugin)
                } else {
                    f.plugin.clone()
                };
                (fid, plugin, fid & 0xFF_FFFF, f)
            }
            7 | 8 => {
                not_loaded.push(at);
                let index = rng.below(0xFE) as u32;
                let id = rng.u32() & 0xFF_FFFF;
                let name = format!("Gone {}.esp", rng.below(4));
                (
                    (index << 24) | id,
                    name.clone(),
                    id,
                    FormRef::new(&name, id),
                )
            }
            _ => {
                not_loaded.push(at);
                let slot = rng.u32() & 0xFFF;
                let id = rng.u32() & 0xFFF;
                let name = format!("Lost {}.esl", rng.below(4));
                (
                    (0xFE << 24) | (slot << 12) | id,
                    name.clone(),
                    (slot << 12) | id,
                    FormRef::new(&name, id),
                )
            }
        };
        parts.push(form);
        let index = fid >> 24;
        if index < 0xFE
            && !mods
                .iter()
                .any(|(i, n)| *i == index && n.eq_ignore_ascii_case(&plugin))
        {
            mods.push((index, plugin.clone()));
        }
        named.push(plugin.clone());
        let hex = if rng.one_in(4) {
            format!("{ident_id:06x}")
        } else {
            format!("{ident_id:06X}")
        };
        let mut members = vec![
            ("formId", fid.to_string()),
            ("formIdentifier", jstr(&format!("{plugin}|{hex}"))),
            ("type", rng.below(20).to_string()),
        ];
        if rng.one_in(4) {
            members.insert(0, ("extra", "true".to_string()));
        }
        if rng.one_in(6) {
            members.push(("zz", "[ 1, 2 ]".to_string()));
        }
        entries.push(jobj(&members));
    }
    mods.sort();
    let mut names: Vec<String> = Vec::new();
    for n in named {
        if !names.iter().any(|o| o.eq_ignore_ascii_case(&n)) {
            names.push(n);
        }
    }
    names.sort();
    let names: Vec<String> = names
        .iter()
        .map(|n| {
            if rng.one_in(4) {
                jstr(&recase(rng, n))
            } else {
                jstr(n)
            }
        })
        .collect();
    let mods: Vec<String> = mods
        .iter()
        .map(|(i, n)| jobj(&[("index", i.to_string()), ("name", jstr(n))]))
        .collect();

    // Tint layers.
    let masks = if female { female_masks() } else { male_masks() };
    let mut layers: Vec<Layer> = Vec::new();
    for (place, m) in masks.iter().enumerate() {
        let Some(path) = m.mask.as_deref() else {
            continue;
        };
        if rng.one_in(3) {
            continue;
        }
        let any = rng.u8();
        let a = *rng.pick(&[0u8, 0, 255, 255, 1, any]);
        let rgb = if a == 0 && rng.one_in(2) {
            [255, 255, 255]
        } else {
            [rng.u8(), rng.u8(), rng.u8()]
        };
        let colour = u32::from_be_bytes([a, rgb[0], rgb[1], rgb[2]]);
        let texture = respell(rng, path);
        layers.push((
            jobj(&[
                ("color", colour.to_string()),
                ("index", place.to_string()),
                ("texture", jstr(&texture)),
            ]),
            Some((m.index, [rgb[0], rgb[1], rgb[2], a])),
            None,
        ));
        // The same mask again, at no place: not taken.
        if rng.one_in(6) {
            let a = rng.u8();
            let texture = respell(rng, path);
            let dup = (
                jobj(&[
                    ("color", u32::from_be_bytes([a, 1, 2, 3]).to_string()),
                    ("index", "99".to_string()),
                    ("texture", jstr(&texture)),
                ]),
                None,
                Some((texture, a)),
            );
            let at = rng.upto(layers.len());
            layers.insert(at, dup);
        }
    }
    for k in 0..rng.upto(3) {
        let any = rng.u8();
        let a = *rng.pick(&[0u8, 255, any]);
        let texture = format!("Other\\Foreign {k}.dds");
        let entry = (
            jobj(&[
                ("color", u32::from_be_bytes([a, 7, 8, 9]).to_string()),
                ("index", (40 + k).to_string()),
                ("texture", jstr(&texture)),
            ]),
            None,
            Some((texture, a)),
        );
        let at = rng.upto(layers.len());
        layers.insert(at, entry);
    }
    let mut tints = Vec::new();
    let mut foreign = Vec::new();
    for (at, (_, ours, other)) in layers.iter().enumerate() {
        if let Some((mask, [r, g, b, a])) = ours {
            if *a > 0 {
                tints.push(Tint {
                    mask: *mask,
                    colour: [*r, *g, *b, *a],
                    strength: f32::from(*a) / 255.0,
                });
            }
        }
        if let Some((texture, a)) = other {
            foreign.push((at, texture.clone(), *a));
        }
    }
    let layers: Vec<String> = layers.into_iter().map(|l| l.0).collect();

    // Sliders.
    let sliders: Vec<String> = (0..rng.upto(22))
        .map(|_| rng.pick(FLOATS).to_string())
        .collect();
    let presets: Vec<String> = (0..rng.upto(5))
        .map(|_| rng.pick(WHOLES).to_string())
        .collect();
    let pool = ["AB_Lip_Width", "Brow_Up", "Nose_Tip", "Tiny", "Cheek"];
    let custom: Vec<String> = (0..rng.upto(8))
        .map(|_| {
            let mut m = vec![
                ("name", jstr(rng.pick(&pool))),
                ("value", rng.pick(FLOATS).to_string()),
            ];
            if rng.one_in(5) {
                m.push(("x", "1".to_string()));
            }
            jobj(&m)
        })
        .collect();

    let mut actor = vec![("hairColor", rng.u32().to_string())];
    if let Some(t) = head_texture {
        actor.push(("headTexture", jstr(&t)));
    }
    actor.push(("weight", rng.pick(FLOATS).to_string()));
    let morphs = jobj(&[
        ("custom", jarr(&custom)),
        (
            "default",
            jobj(&[("morphs", jarr(&sliders)), ("presets", jarr(&presets))]),
        ),
        ("sculpt", "[]".to_string()),
        ("sculptDivisor", "10000".to_string()),
    ]);
    let text = jobj(&[
        ("actor", jobj(&actor)),
        (
            "bodyMorphs",
            "[ { \"name\" : \"Kept\", \"value\" : 1.5 } ]".to_string(),
        ),
        ("headParts", jarr(&entries)),
        ("modNames", jarr(&names)),
        ("mods", jarr(&mods)),
        ("morphs", morphs),
        ("overrides", "{ \"Face [Ovl0]\" : [] }".to_string()),
        ("tintInfo", jarr(&layers)),
        (
            "version",
            jobj(&[
                ("formatVersion", "3".into()),
                ("runtimeVersion", "1".into()),
                ("signature", "7".into()),
                ("skseVersion", "2".into()),
            ]),
        ),
        (
            "zUnknown",
            "{ \"a\" : null, \"b\" : [ true, false ] }".to_string(),
        ),
    ]);
    Generated {
        text,
        order: LoadOrder::new(plugins),
        female,
        parts,
        not_loaded,
        not_parts,
        foreign,
        tints,
        texture_note,
    }
}

#[test]
fn any_look_read_from_a_preset_and_written_straight_back_changes_no_byte() {
    let w = world();
    for seed in 0..3000 {
        let mut rng = Rng::new(seed);
        let g = generate(&mut rng);
        let doc = parse(g.text.as_bytes()).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let before = write(&doc).unwrap();
        let (look, report) = look_of_preset(&doc, &race(), g.female, &w, &g.order);

        // What was read.
        assert_eq!(look.head_parts, g.parts, "seed {seed}");
        assert_eq!(look.tints, g.tints, "seed {seed}");
        let at = |f: fn(&Note) -> Option<usize>| -> Vec<usize> {
            report.notes.iter().filter_map(f).collect()
        };
        assert_eq!(
            at(|n| match n {
                Note::PartNotLoaded { at, .. } => Some(*at),
                _ => None,
            }),
            g.not_loaded,
            "seed {seed}"
        );
        assert_eq!(
            at(|n| match n {
                Note::NotAHeadPart { at, .. } => Some(*at),
                _ => None,
            }),
            g.not_parts,
            "seed {seed}"
        );
        let foreign: Vec<(usize, String, u8)> = report
            .notes
            .iter()
            .filter_map(|n| match n {
                Note::TintNotOfRace { at, texture, alpha } => Some((*at, texture.clone(), *alpha)),
                _ => None,
            })
            .collect();
        assert_eq!(foreign, g.foreign, "seed {seed}");
        let texture_notes: Vec<&Note> = report
            .notes
            .iter()
            .filter(|n| matches!(n, Note::HeadTextureNotLoaded(_) | Note::NotATextureSet(_)))
            .collect();
        assert_eq!(
            texture_notes,
            g.texture_note.iter().collect::<Vec<_>>(),
            "seed {seed}"
        );
        // Nothing else, but for a head texture left out.
        assert!(
            report.notes.iter().all(|n| match n {
                Note::Malformed(m) => m.path == "actor.headTexture" && look.face_texture.is_none(),
                Note::PartUnknown { .. } | Note::NoRace(_) => false,
                _ => true,
            }),
            "seed {seed}: {:?}",
            report.notes
        );

        // Written straight back.
        let mut out = doc.clone();
        apply_look(&mut out, &look, &w, &g.order).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let after = write(&out).unwrap();
        if after != before {
            let (r, a) = diff(
                &String::from_utf8_lossy(&before),
                &String::from_utf8_lossy(&after),
            );
            panic!("seed {seed} changed: -{r:?} +{a:?}\n{}", g.text);
        }
    }
}

/// Keys a preset may lack, each taken out of the fixture on its own. The
/// fixture's head texture plugin is also a part's, so `modNames` does not
/// depend on it.
const OPTIONAL: &[&[&str]] = &[
    &["version"],
    &["actor", "hairColor"],
    &["actor", "headTexture"],
    &["bodyMorphs"],
    &["overrides"],
    &["morphs", "sculpt"],
];

/// Sections a look is written to, which `apply_look` makes only when it
/// has something to write into them.
const OWNED: &[&[&str]] = &[
    &["actor"],
    &["actor", "weight"],
    &["headParts"],
    &["modNames"],
    &["mods"],
    &["morphs"],
    &["morphs", "default"],
    &["morphs", "default", "morphs"],
    &["morphs", "default", "presets"],
    &["morphs", "custom"],
    &["tintInfo"],
];

fn without(path: &[&str]) -> Document {
    let mut d = fixture_doc();
    let mut o = d.root.as_object_mut().unwrap();
    for k in &path[..path.len() - 1] {
        o = o.get_mut(k).unwrap().as_object_mut().unwrap();
    }
    assert!(o.remove(path[path.len() - 1]).is_some(), "{path:?}");
    d
}

/// Paths whose removal does not survive a straight write-back.
fn changed_without(paths: &[&[&str]]) -> Vec<String> {
    let w = world();
    let mut bad = Vec::new();
    for path in paths {
        let doc = without(path);
        let before = text(&doc);
        let (look, _) = look_of_preset(&doc, &race(), true, &w, &order());
        let mut out = doc.clone();
        match apply_look(&mut out, &look, &w, &order()) {
            Ok(_) if text(&out) == before => {}
            Ok(_) => {
                let (r, a) = diff(&before, &text(&out));
                bad.push(format!("{}: -{r:?} +{a:?}", path.join(".")));
            }
            Err(e) => bad.push(format!("{}: {e}", path.join("."))),
        }
    }
    bad
}

#[test]
fn a_preset_lacking_a_key_it_need_not_have_is_written_back_unchanged() {
    // The fixture itself first.
    let w = world();
    let doc = fixture_doc();
    let (look, report) = look_of_preset(&doc, &race(), true, &w, &order());
    assert_eq!(report.notes.len(), 3, "{:?}", report.notes);
    let mut out = doc.clone();
    apply_look(&mut out, &look, &w, &order()).unwrap();
    assert_eq!(text(&out), text(&doc));
    assert_eq!(changed_without(OPTIONAL), Vec::<String>::new());
}

#[test]
fn a_preset_lacking_a_section_a_look_is_written_to_is_written_back_unchanged() {
    assert_eq!(changed_without(OWNED), Vec::<String>::new());
}

#[test]
fn an_entry_naming_no_form_keeps_its_place_when_written_back() {
    let w = world();
    // An entry with no identifier whose form id names nothing loaded, then
    // the face.
    let doc = parse(
        br#"{ "actor" : { "weight" : 0 }, "headParts" : [ { "formId" : 117440513, "type" : 0 }, { "formId" : 33637610, "formIdentifier" : "Race.esp|0144EA", "type" : 0 } ], "modNames" : [ "Race.esp" ], "mods" : [ { "index" : 2, "name" : "Race.esp" } ], "morphs" : { "custom" : [], "default" : { "morphs" : [], "presets" : [] } }, "tintInfo" : [] }"#,
    )
    .unwrap();
    let before = text(&doc);
    let (look, report) = look_of_preset(&doc, &race(), true, &w, &order());
    assert!(report.notes.contains(&Note::PartUnknown { at: 0 }));
    let mut out = doc.clone();
    apply_look(&mut out, &look, &w, &order()).unwrap();
    assert_eq!(text(&out), before);
}

/// The fixture with the value at `path` (an index into a list as its last
/// step, when a number) set to `json`.
fn with_value(path: &[&str], json: &str) -> Document {
    let mut d = fixture_doc();
    let mut v = &mut d.root;
    for k in path {
        v = match k.parse::<usize>() {
            Ok(i) => &mut v.as_array_mut().unwrap()[i],
            Err(_) => v.as_object_mut().unwrap().get_mut(k).unwrap(),
        };
    }
    *v = parse(json.as_bytes()).unwrap().root;
    d
}

#[test]
fn a_value_the_reader_cannot_use_is_written_back_as_read() {
    let w = world();
    let mut bad = Vec::new();
    for (path, json) in [
        (vec!["actor", "weight"], "1e39"),
        (vec!["actor", "weight"], "\"heavy\""),
        (vec!["morphs", "default", "morphs", "1"], "1e39"),
        (vec!["morphs", "default", "morphs", "1"], "\"x\""),
        (vec!["morphs", "default", "presets", "2"], "1.5"),
        (vec!["morphs", "custom", "1", "value"], "\"x\""),
        (vec!["morphs", "custom", "1", "value"], "-1e39"),
    ] {
        let doc = with_value(&path, json);
        let before = text(&doc);
        let (look, report) = look_of_preset(&doc, &race(), true, &w, &order());
        assert!(
            report.notes.iter().any(|n| matches!(n, Note::Malformed(_))),
            "{path:?} = {json} is not noted"
        );
        let mut out = doc.clone();
        match apply_look(&mut out, &look, &w, &order()) {
            Ok(_) if text(&out) == before => {}
            Ok(_) => {
                let (r, a) = diff(&before, &text(&out));
                bad.push(format!("{} = {json}: -{r:?} +{a:?}", path.join(".")));
            }
            Err(e) => bad.push(format!("{} = {json}: {e}", path.join("."))),
        }
    }
    assert_eq!(bad, Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// 3. An edit changes only its own lines.

/// The fixture's look, the look with `edit` made, and the lines that
/// changed when it was written.
fn edited(edit: impl FnOnce(&mut Look)) -> (Document, Vec<String>, Vec<String>) {
    let w = world();
    let doc = fixture_doc();
    let before = text(&doc);
    let (mut look, _) = look_of_preset(&doc, &race(), true, &w, &order());
    let mut out = doc.clone();
    edit(&mut look);
    apply_look(&mut out, &look, &w, &order()).unwrap();
    let (removed, added) = diff(&before, &text(&out));
    (out, removed, added)
}

#[test]
fn the_fixture_reads_as_expected() {
    let w = world();
    let (look, _) = look_of_preset(&fixture_doc(), &race(), true, &w, &order());
    assert_eq!(
        look.head_parts,
        vec![
            face(),
            mouth(),
            hair_a(),
            scar(),
            FormRef::new("Gone.esp", 0x801),
            FormRef::new("Lost.esl", 0x0C3)
        ]
    );
    assert_eq!(
        look.tints.iter().map(|t| t.mask).collect::<Vec<_>>(),
        vec![0x18, 0x1B, 0x1E]
    );
    assert_eq!(look.custom_morphs.len(), 4);
    assert_eq!(look.face_morphs.len(), 19);
}

#[test]
fn one_face_slider_changes_its_line_only() {
    let (_, removed, added) = edited(|l| l.face_morphs[1] = 0.25);
    assert_eq!(removed, lines(&["0.3100000023841858,"]));
    assert_eq!(added, lines(&["0.25,"]));
}

#[test]
fn one_named_slider_changes_its_line_only() {
    let (_, removed, added) = edited(|l| {
        let m = l
            .custom_morphs
            .iter_mut()
            .find(|m| m.0 == "Brow_Up")
            .unwrap();
        m.1 = 0.75;
    });
    assert_eq!(removed, lines(&["\"value\" : -1"]));
    assert_eq!(added, lines(&["\"value\" : 0.75"]));
}

#[test]
fn one_tint_colour_changes_its_line_only() {
    let (_, removed, added) = edited(|l| {
        let t = l.tints.iter_mut().find(|t| t.mask == 0x1B).unwrap();
        t.colour[0] = 1;
        t.colour[1] = 2;
        t.colour[2] = 3;
    });
    assert_eq!(removed, lines(&["\"color\" : 1621709906,"]));
    let want = u32::from_be_bytes([0x60, 1, 2, 3]);
    assert_eq!(added, vec![format!("\"color\" : {want},")]);
}

#[test]
fn one_tint_strength_changes_its_alpha_only() {
    let (_, removed, added) = edited(|l| {
        let t = l.tints.iter_mut().find(|t| t.mask == 0x1E).unwrap();
        t.strength = 0.5;
    });
    assert_eq!(removed, lines(&["\"color\" : 4279246896,"]));
    // 0.5 is 127.5 of 255, rounded to 128.
    let want = u32::from_be_bytes([0x80, 0x10, 0x20, 0x30]);
    assert_eq!(added, vec![format!("\"color\" : {want},")]);
}

#[test]
fn the_weight_changes_its_line_only() {
    let (_, removed, added) = edited(|l| l.weight = 50.5);
    assert_eq!(removed, lines(&["\"weight\" : 21"]));
    assert_eq!(added, lines(&["\"weight\" : 50.5"]));
}

#[test]
fn the_hair_colour_changes_its_line_only() {
    let (_, removed, added) = edited(|l| l.hair_rgb = Some([1.0, 0.0, 0.5]));
    assert_eq!(removed, lines(&["\"hairColor\" : 5196362,"]));
    assert_eq!(added, vec![format!("\"hairColor\" : {},", 0xFF0080)]);
}

#[test]
fn one_face_preset_changes_its_line_only() {
    let (_, removed, added) = edited(|l| l.face_parts[2] = 5);
    assert_eq!(removed, lines(&["\"presets\" : [ 0, 4294967295, 3, 0 ]"]));
    assert_eq!(added, lines(&["\"presets\" : [ 0, 4294967295, 5, 0 ]"]));
}

#[test]
fn the_skin_tone_alone_is_written_into_its_layer_at_full_strength() {
    let (_, removed, added) = edited(|l| l.skin_tone = Some([0.1, 0.2, 0.3]));
    assert_eq!(removed.len(), 1, "{removed:?}");
    assert!(removed[0].starts_with("\"color\" : "), "{removed:?}");
    let want = u32::from_be_bytes([0xFF, 0x1A, 0x33, 0x4D]);
    assert_eq!(added, vec![format!("\"color\" : {want},")]);
}

#[test]
fn a_part_swapped_for_another_of_its_light_plugin_changes_its_two_ids_only() {
    let (_, removed, added) = edited(|l| l.head_parts[2] = hair_c());
    assert_eq!(
        removed,
        lines(&[
            "\"formId\" : 4261419802,",
            "\"formIdentifier\" : \"Hair.esl|001B1A\","
        ])
    );
    assert_eq!(
        added,
        vec![
            format!("\"formId\" : {},", 0xFE00_1B1Bu32),
            "\"formIdentifier\" : \"Hair.esl|001B1B\",".to_string()
        ]
    );
}

#[test]
fn a_part_swapped_for_one_of_a_named_plugin_changes_its_ids_and_the_plugin_names() {
    // Hair.esl's hair for Hair2.esp's: Hair2.esp is named already, with
    // its index in `mods`, and Hair.esl stays named, as plugin names are
    // only added to.
    let (out, removed, added) = edited(|l| l.head_parts[2] = hair_b());
    assert_eq!(
        removed,
        lines(&[
            "\"formId\" : 4261419802,",
            "\"formIdentifier\" : \"Hair.esl|001B1A\","
        ])
    );
    assert_eq!(
        added,
        vec![
            format!("\"formId\" : {},", 0x0300_0801u32),
            "\"formIdentifier\" : \"Hair2.esp|000801\",".to_string()
        ]
    );
    // The new part takes the type of the hair it replaces.
    let p = Preset::of(&out);
    assert_eq!(p.head_parts.unwrap()[2].clone().unwrap().kind, Ok(3));
}

#[test]
fn a_part_swapped_for_one_of_a_new_plugin_adds_it_to_mods_and_mod_names() {
    let (out, _, _) = edited(|l| l.head_parts[2] = hair_x());
    let p = Preset::of(&out);
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
        vec![
            (2, "Race.esp".into()),
            (3, "Hair2.esp".into()),
            (4, "Extra.esm".into()),
            (80, "Gone.esp".into())
        ]
    );
    let names: Vec<String> = p
        .mod_names
        .unwrap()
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        names,
        [
            "Extra.esm",
            "Gone.esp",
            "Hair.esl",
            "Hair2.esp",
            "Lost.esl",
            "Race.esp"
        ]
    );
    // With the plugin lists put back, only the part's two ids differ.
    let mut back = out.clone();
    let o = back.root.as_object_mut().unwrap();
    for k in ["modNames", "mods"] {
        o.insert(k, top(&fixture_doc(), k).unwrap().clone());
    }
    let (removed, added) = diff(&text(&fixture_doc()), &text(&back));
    assert_eq!(
        removed,
        lines(&[
            "\"formId\" : 4261419802,",
            "\"formIdentifier\" : \"Hair.esl|001B1A\","
        ])
    );
    assert_eq!(
        added,
        vec![
            format!("\"formId\" : {},", 0x0400_0900u32),
            "\"formIdentifier\" : \"Extra.esm|000900\",".to_string()
        ]
    );
}

// ---------------------------------------------------------------------------
// 4. Errors, and notes.

#[test]
fn a_refused_look_leaves_the_document_byte_for_byte() {
    let w = world();
    let doc = fixture_doc();
    let before = text(&doc);
    let (look, _) = look_of_preset(&doc, &race(), true, &w, &order());
    let with = |f: &dyn Fn(&mut Look)| {
        let mut l = look.clone();
        f(&mut l);
        l
    };
    let cases: Vec<(Look, PresetError)> = vec![
        (
            with(&|l| l.head_parts.push(FormRef::new("Gone.esp", 0x802))),
            PresetError::NotLoaded("Gone.esp|000802".into()),
        ),
        (
            with(&|l| l.head_parts.push(FormRef::new("Extra.esm", 0x123))),
            PresetError::NotAHeadPart("Extra.esm|000123".into()),
        ),
        (
            with(&|l| l.head_parts.push(hair_too_big())),
            PresetError::NoFormId("Hair.esl|001000".into()),
        ),
        (
            with(&|l| l.face_texture = Some(FormRef::new("Gone.esp", 0x1))),
            PresetError::NotLoaded("Gone.esp|000001".into()),
        ),
        (
            with(&|l| l.hair_rgb = Some([0.5, f32::NAN, 0.5])),
            PresetError::NotANumber("the hair colour"),
        ),
        (
            with(&|l| l.hair_rgb = Some([f32::INFINITY, 0.0, 0.0])),
            PresetError::NotANumber("the hair colour"),
        ),
        (
            with(&|l| l.custom_morphs[0].1 = f32::NAN),
            PresetError::NotANumber("a named slider"),
        ),
        (
            with(&|l| l.custom_morphs.push(("New".into(), f32::NEG_INFINITY))),
            PresetError::NotANumber("a named slider"),
        ),
        (
            with(&|l| l.tints[0].strength = f32::INFINITY),
            PresetError::NotANumber("a tint layer's strength"),
        ),
        (
            with(&|l| l.race = Some(FormRef::new("Race.esp", 0x777))),
            PresetError::MissingRace("Race.esp|000777".into()),
        ),
        (
            with(&|l| {
                l.tints.push(Tint {
                    mask: 0x30,
                    colour: [0; 4],
                    strength: 1.0,
                })
            }),
            PresetError::NoSuchMask(0x30),
        ),
    ];
    for (bad, want) in cases {
        let mut d = doc.clone();
        assert_eq!(apply_look(&mut d, &bad, &w, &order()), Err(want.clone()));
        assert_eq!(text(&d), before, "{want}");
        assert!(!want.to_string().is_empty());
    }
    // Sections that are not what a preset has there.
    for (path, value, want) in [
        (
            vec!["actor"],
            "1",
            PresetError::WrongKind("actor", "an object"),
        ),
        (vec!["mods"], "1", PresetError::WrongKind("mods", "a list")),
        (
            vec!["tintInfo"],
            "{}",
            PresetError::WrongKind("tintInfo", "a list"),
        ),
        (
            vec!["morphs"],
            "[]",
            PresetError::WrongKind("morphs", "an object"),
        ),
        (
            vec!["morphs", "custom"],
            "{}",
            PresetError::WrongKind("custom", "a list"),
        ),
        (
            vec!["morphs", "default", "presets"],
            "\"x\"",
            PresetError::WrongKind("presets", "a list"),
        ),
    ] {
        let mut d = fixture_doc();
        let mut o = d.root.as_object_mut().unwrap();
        for k in &path[..path.len() - 1] {
            o = o.get_mut(k).unwrap().as_object_mut().unwrap();
        }
        o.insert(path[path.len() - 1], parse(value.as_bytes()).unwrap().root);
        let before = text(&d);
        let r = apply_look(&mut d, &look, &w, &order());
        // `mods` is not part of a look: the look's parts are as read, so
        // nothing is written to it and it is left as it is.
        if path == ["mods"] {
            assert!(r.is_ok(), "{r:?}");
            assert_eq!(text(&d), before);
            continue;
        }
        assert_eq!(r, Err(want), "{path:?}");
        assert_eq!(text(&d), before, "{path:?}");
    }
}

/// What a note must name to be of use: its place in the preset and the
/// thing itself. Exhaustive, so a new kind of note must be added here.
fn named_in(n: &Note) -> Vec<String> {
    match n {
        Note::Malformed(m) => vec![m.path.clone()],
        Note::PartNotLoaded { at, form } | Note::NotAHeadPart { at, form } => vec![
            format!("headParts[{at}]"),
            form.plugin.clone(),
            format!("{:06X}", form.id),
        ],
        Note::PartUnknown { at } => vec![format!("headParts[{at}]")],
        Note::TintNotOfRace { at, texture, .. } => {
            vec![format!("tintInfo[{at}]"), texture.clone()]
        }
        Note::NoRace(f) | Note::HeadTextureNotLoaded(f) | Note::NotATextureSet(f) => {
            vec![f.plugin.clone(), format!("{:06X}", f.id)]
        }
        Note::TooMany { what, limit } => vec![what.to_string(), limit.to_string()],
        Note::RepeatedMask(tini) => vec![format!("{tini:X}")],
        Note::More(n) => vec![n.to_string()],
    }
}

fn check_sentences(report: &PresetReport) {
    for n in &report.notes {
        let s = n.to_string();
        assert!(s.len() > 10, "{n:?}: {s:?}");
        assert!(s.ends_with('.'), "{s}");
        assert!(s.chars().next().unwrap().is_alphanumeric(), "{s}");
        for thing in named_in(n) {
            assert!(!thing.is_empty(), "{n:?}");
            assert!(s.contains(&thing), "{s:?} does not name {thing:?}");
        }
    }
}

#[test]
fn every_unusable_input_gets_a_note_that_names_it() {
    let w = world();
    let doc = parse(
        br#"{
        "actor" : { "hairColor" : -5, "headTexture" : "Gone.esp|000001", "weight" : "heavy" },
        "headParts" : [
            { "formId" : 117440513, "type" : 0 },
            { "formId" : 1342179329, "formIdentifier" : "Gone.esp|000801", "type" : 9 },
            { "formId" : 33556889, "formIdentifier" : "Race.esp|000999", "type" : 0 },
            5,
            { "formIdentifier" : "not an identifier", "type" : 0 }
        ],
        "modNames" : [ 7 ],
        "mods" : [ { "index" : "two" } ],
        "morphs" : {
            "custom" : [ { "name" : "NoValue" }, { "value" : 1 }, { "name" : "Bad", "value" : "x" } ],
            "default" : { "morphs" : [ 0, "x", 1 ], "presets" : [ 1.5, -1 ] }
        },
        "tintInfo" : [
            { "color" : 4278190080, "index" : 0, "texture" : "Other\\Foreign.dds" },
            { "index" : 1, "texture" : "Masks\\Lips.dds" },
            { "color" : 1, "index" : 2 },
            "not a layer"
        ],
        "version" : { "formatVersion" : "three" }
    }"#,
    )
    .unwrap();
    let (look, report) = look_of_preset(&doc, &race(), true, &w, &order());
    check_sentences(&report);
    let has = |n: &Note| report.notes.contains(n);
    assert!(has(&Note::HeadTextureNotLoaded(FormRef::new(
        "Gone.esp", 1
    ))));
    assert!(has(&Note::PartUnknown { at: 0 }));
    assert!(has(&Note::PartUnknown { at: 4 }));
    assert!(has(&Note::PartNotLoaded {
        at: 1,
        form: FormRef::new("Gone.esp", 0x801)
    }));
    assert!(has(&Note::NotAHeadPart {
        at: 2,
        form: FormRef::new("Race.esp", 0x999)
    }));
    assert!(has(&Note::TintNotOfRace {
        at: 0,
        texture: "Other\\Foreign.dds".into(),
        alpha: 0xFF
    }));
    // Each unreadable field, by its path.
    let paths: Vec<String> = report
        .notes
        .iter()
        .filter_map(|n| match n {
            Note::Malformed(m) => Some(m.path.clone()),
            _ => None,
        })
        .collect();
    for p in [
        "version.formatVersion",
        "actor.weight",
        "actor.hairColor",
        "headParts[0].formIdentifier",
        "headParts[3]",
        "modNames[0]",
        "mods[0].index",
        "mods[0].name",
        "morphs.default.morphs[1]",
        "morphs.default.presets[0]",
        "morphs.default.presets[1]",
        "morphs.custom[0].value",
        "morphs.custom[1].name",
        "morphs.custom[2].value",
        "tintInfo[1].color",
        "tintInfo[2].texture",
        "tintInfo[3]",
    ] {
        assert!(paths.iter().any(|q| q == p), "no note for {p}: {paths:?}");
    }
    // What could not be read is not in the look, or is in it as nothing.
    assert_eq!(look.weight, 0.0);
    assert_eq!(look.hair_rgb, None);
    assert_eq!(look.face_morphs, vec![0.0, 0.0, 1.0]);
    assert_eq!(look.face_parts, vec![-1, -1]);
    assert_eq!(look.custom_morphs, vec![]);
    assert_eq!(look.tints, vec![]);

    // A head texture that is not a texture set, and a race with no record.
    let doc = parse(
        br#"{ "actor" : { "hairColor" : 0, "headTexture" : "Extra.esm|000123", "weight" : 0 }, "tintInfo" : [ { "color" : 0, "index" : 0, "texture" : "x.dds" } ] }"#,
    )
    .unwrap();
    let (_, report) = look_of_preset(&doc, &race(), true, &w, &order());
    check_sentences(&report);
    assert!(report
        .notes
        .contains(&Note::NotATextureSet(FormRef::new("Extra.esm", 0x123))));
    let nowhere = FormRef::new("Gone.esp", 0xABC);
    let (_, report) = look_of_preset(&doc, &nowhere, true, &w, &order());
    check_sentences(&report);
    assert!(report.notes.contains(&Note::NoRace(nowhere)));
    // The whole preset not an object.
    let (_, report) = look_of_preset(&parse(b"[ 1 ]").unwrap(), &race(), true, &w, &order());
    assert!(!report.notes.is_empty());
    for n in &report.notes {
        assert!(!n.to_string().trim().is_empty(), "{n:?}");
    }
}

// ---------------------------------------------------------------------------
// 5. A new preset.

#[test]
fn a_new_preset_writes_reads_back_and_is_a_neutral_look() {
    let w = world();
    for female in [false, true] {
        let doc = new_document(&race(), female, &w).unwrap();
        let bytes = write(&doc).unwrap();
        let again = parse(&bytes).unwrap();
        assert_eq!(write(&again).unwrap(), bytes);
        let (look, report) = look_of_preset(&again, &race(), female, &w, &order());
        assert_eq!(
            look,
            Look {
                race: Some(race()),
                female,
                hair_rgb: Some([0.0; 3]),
                ..Look::default()
            }
        );
        // The only gap is the head texture a new preset has none of.
        let notes: Vec<String> = report.notes.iter().map(|n| n.to_string()).collect();
        assert!(
            notes.iter().all(|n| n.contains("actor.headTexture")),
            "{notes:?}"
        );
        // One unused layer for each of the race's masks with a texture.
        let masks = if female { female_masks() } else { male_masks() };
        let layers = Preset::of(&again).tints.unwrap();
        let with_texture: Vec<(usize, &TintMask)> = masks
            .iter()
            .enumerate()
            .filter(|(_, m)| m.mask.is_some())
            .collect();
        assert_eq!(layers.len(), with_texture.len());
        for (l, (place, m)) in layers.iter().zip(&with_texture) {
            let l = l.clone().unwrap();
            assert_eq!(l.index, Ok(*place as u32));
            assert_eq!(l.texture.as_deref(), Ok(m.mask.as_deref().unwrap()));
            assert_eq!(l.color.map(|c| c >> 24), Ok(0));
        }
    }
    assert_eq!(
        new_document(&FormRef::new("Race.esp", 0x777), true, &w),
        Err(PresetError::MissingRace("Race.esp|000777".into()))
    );
}

/// A look a new preset can hold: loaded parts, layers of the race's masks
/// with textures (in their order, colour's fourth byte the alpha), colours
/// and strengths of whole steps of 255, any finite numbers.
fn random_look(rng: &mut Rng, female: bool) -> Look {
    let parts = loaded_parts();
    let masks = if female { female_masks() } else { male_masks() };
    let mut tints = Vec::new();
    for m in &masks {
        if m.mask.is_none() || rng.one_in(2) {
            continue;
        }
        let a = 1 + rng.below(255) as u8;
        tints.push(Tint {
            mask: m.index,
            colour: [rng.u8(), rng.u8(), rng.u8(), a],
            strength: f32::from(a) / 255.0,
        });
    }
    let names = ["AB_Lip_Width", "Brow_Up", "Nose_Tip", "Tiny"];
    Look {
        race: Some(race()),
        female,
        head_parts: (0..rng.upto(6)).map(|_| rng.pick(&parts).clone()).collect(),
        face_morphs: (0..rng.upto(20)).map(|_| rng.finite_f32()).collect(),
        face_parts: (0..rng.upto(4)).map(|_| rng.u32() as i32).collect(),
        hair_colour: None,
        hair_rgb: Some([rng.u8(), rng.u8(), rng.u8()].map(|v| f32::from(v) / 255.0)),
        face_texture: rng
            .pick(&[None, Some(texture()), Some(light_texture())])
            .clone(),
        skin: None,
        skin_tone: None,
        tints,
        weight: rng.finite_f32(),
        custom_morphs: (0..rng.upto(6))
            .map(|_| (rng.pick(&names).to_string(), rng.finite_f32()))
            .collect(),
    }
}

#[test]
fn a_look_applied_to_a_new_preset_reads_back_as_itself() {
    let w = world();
    for seed in 0..1500 {
        let mut rng = Rng::new(9000 + seed);
        let female = rng.one_in(2);
        let mut plugins = order().plugins().to_vec();
        rng.shuffle(&mut plugins);
        let order = LoadOrder::new(plugins);
        let look = random_look(&mut rng, female);
        let mut doc = new_document(&race(), female, &w).unwrap();
        apply_look(&mut doc, &look, &w, &order).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let bytes = write(&doc).unwrap();
        let doc = parse(&bytes).unwrap();
        let (again, report) = look_of_preset(&doc, &race(), female, &w, &order);
        // The skin tone is worked out from its layer.
        let skin = look.tints.iter().find(|t| t.mask == 0x18 || t.mask == 0x30);
        match (skin, again.skin_tone) {
            (None, None) => {}
            (Some(t), Some(tone)) => {
                let grey = 127.0 / 255.0;
                for c in 0..3 {
                    let want = grey + (f32::from(t.colour[c]) / 255.0 - grey) * t.strength;
                    assert!((tone[c] - want).abs() < 1e-6, "seed {seed}: {tone:?}");
                }
            }
            other => panic!("seed {seed}: skin tone {other:?}"),
        }
        let want = Look {
            skin_tone: again.skin_tone,
            ..look.clone()
        };
        assert_eq!(again, want, "seed {seed}");
        if look.face_texture.is_some() {
            assert_eq!(report.notes, vec![], "seed {seed}");
        }
        // And once more changes nothing.
        let mut twice = doc.clone();
        apply_look(&mut twice, &again, &w, &order).unwrap();
        assert_eq!(write(&twice).unwrap(), bytes, "seed {seed}");
    }
}

// ---------------------------------------------------------------------------
// 6. Colours.

/// A new preset whose skin tone layer has `color`.
fn with_skin_colour(w: &World, color: u32) -> Document {
    let mut d = new_document(&race(), true, w).unwrap();
    let layers = d
        .root
        .as_object_mut()
        .unwrap()
        .get_mut("tintInfo")
        .unwrap()
        .as_array_mut()
        .unwrap();
    layers[0]
        .as_object_mut()
        .unwrap()
        .insert("color", Value::Number(Number::from_u64(color.into())));
    d
}

fn skin_colour(d: &Document) -> u32 {
    Preset::of(d).tints.unwrap()[0]
        .clone()
        .unwrap()
        .color
        .unwrap()
}

#[test]
fn a_tint_colour_packs_both_ways_for_every_value_of_every_channel() {
    let w = world();
    let base = [0x40u8, 0x80, 0xC0, 0xFF];
    for channel in 0..4 {
        for v in 0..=255u8 {
            let mut argb = base;
            argb[channel] = v;
            let [a, r, g, b] = argb;
            let color = u32::from_be_bytes(argb);
            let doc = with_skin_colour(&w, color);
            let (look, _) = look_of_preset(&doc, &race(), true, &w, &order());
            if a == 0 {
                assert_eq!(look.tints, vec![], "{color:08X}");
                let mut same = doc.clone();
                apply_look(&mut same, &look, &w, &order()).unwrap();
                assert_eq!(skin_colour(&same), color);
                continue;
            }
            assert_eq!(
                look.tints,
                vec![Tint {
                    mask: 0x18,
                    colour: [r, g, b, a],
                    strength: f32::from(a) / 255.0
                }],
                "{color:08X}"
            );
            // Into a preset whose layer held another colour.
            let mut fresh = with_skin_colour(&w, 0x00FF_FFFF);
            apply_look(&mut fresh, &look, &w, &order()).unwrap();
            assert_eq!(skin_colour(&fresh), color, "{color:08X}");
        }
    }
}

#[test]
fn a_tint_strength_settles_after_one_round_trip() {
    let w = world();
    let mut rng = Rng::new(31);
    for i in 0..2000 {
        let strength = match i {
            0 => 1.5,
            1 => -0.25,
            2 => 0.5,
            3 => 1.0 / 510.0,
            _ => (rng.u32() as f32) / (u32::MAX as f32),
        };
        let mut look = Look {
            race: Some(race()),
            female: true,
            tints: vec![Tint {
                mask: 0x1D,
                colour: [rng.u8(), rng.u8(), rng.u8(), rng.u8()],
                strength,
            }],
            ..Look::default()
        };
        let mut doc = new_document(&race(), true, &w).unwrap();
        apply_look(&mut doc, &look, &w, &order()).unwrap();
        let want_alpha = (strength.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut written = Vec::new();
        for _ in 0..4 {
            let (back, _) = look_of_preset(&doc, &race(), true, &w, &order());
            let bytes = write(&doc).unwrap();
            written.push(bytes);
            let alpha = back.tints.first().map_or(0, |t| t.colour[3]);
            assert_eq!(alpha, want_alpha, "strength {strength}");
            look = back;
            apply_look(&mut doc, &look, &w, &order()).unwrap();
        }
        assert!(
            written.windows(2).all(|p| p[0] == p[1]),
            "strength {strength}"
        );
    }
}

fn with_hair(color: u32) -> Document {
    let mut d = new_document(&race(), true, &world()).unwrap();
    d.root
        .as_object_mut()
        .unwrap()
        .get_mut("actor")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("hairColor", Value::Number(Number::from_u64(color.into())));
    d
}

fn hair_of(d: &Document) -> u32 {
    Preset::of(d).actor.unwrap().hair_color.unwrap()
}

#[test]
fn a_hair_colour_packs_both_ways_for_every_value_of_every_channel() {
    let w = world();
    let base = [0x20u8, 0x90, 0xF0];
    for channel in 0..3 {
        for v in 0..=255u8 {
            let mut rgb = base;
            rgb[channel] = v;
            let color = u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]]);
            let (look, _) = look_of_preset(&with_hair(color), &race(), true, &w, &order());
            assert_eq!(
                look.hair_rgb,
                Some(rgb.map(|c| f32::from(c) / 255.0)),
                "{color:06X}"
            );
            let mut d = with_hair(0);
            apply_look(&mut d, &look, &w, &order()).unwrap();
            assert_eq!(hair_of(&d), color, "{color:06X}");
        }
    }
}

#[test]
fn a_hair_colour_settles_after_one_round_trip_and_keeps_its_top_byte() {
    let w = world();
    let mut rng = Rng::new(32);
    for i in 0..2000 {
        let rgb = match i {
            0 => [1.5, -0.2, 0.5],
            1 => [0.5 / 255.0, 1.5 / 255.0, 254.5 / 255.0],
            _ => [0; 3].map(|_| (rng.u32() as f32) / (u32::MAX as f32) * 1.4 - 0.2),
        };
        let top = if rng.one_in(2) { 0 } else { rng.u8() };
        let want = u32::from_be_bytes([
            top,
            (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        ]);
        let mut d = with_hair(u32::from(top) << 24);
        let mut look = Look {
            race: Some(race()),
            female: true,
            hair_rgb: Some(rgb),
            ..Look::default()
        };
        for _ in 0..4 {
            apply_look(&mut d, &look, &w, &order()).unwrap();
            assert_eq!(hair_of(&d), want, "{rgb:?}");
            look = look_of_preset(&d, &race(), true, &w, &order()).0;
        }
    }
    // A CLFM's colour when no colour is given directly; a direct one wins.
    let mut d = with_hair(0);
    let mut look = Look {
        race: Some(race()),
        female: true,
        hair_colour: Some(clfm()),
        ..Look::default()
    };
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(hair_of(&d), u32::from_be_bytes([0, 57, 55, 40]));
    look.hair_rgb = Some([0.0, 0.0, 1.0]);
    apply_look(&mut d, &look, &w, &order()).unwrap();
    assert_eq!(hair_of(&d), 0x0000FF);
}

// ---------------------------------------------------------------------------
// 7. The plan.

fn plan_world() -> World {
    let mut w = world();
    w.colours.insert(
        FormRef::new("Race.esp", 0x501),
        Colour {
            rgba: [10, 20, 30, 0],
            ..Colour::default()
        },
    );
    w.races.get_mut(&race()).unwrap().default_hair_colour = [
        Some(FormRef::new("Race.esp", 0x501)),
        Some(FormRef::new("Race.esp", 0x501)),
    ];
    w
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|c| (a[c] - b[c]).abs() < 1e-6)
}

#[test]
fn a_direct_hair_colour_wins_over_a_clfm_at_twice_it_clamped() {
    let w = plan_world();
    let look = Look {
        race: Some(race()),
        female: true,
        head_parts: vec![hair_a()],
        hair_colour: Some(clfm()),
        hair_rgb: Some([0.25, 1.5, -0.5]),
        ..Look::default()
    };
    let p = plan(&look, &w).unwrap();
    assert_eq!(p.hair_colour, Some([0.5, 2.0, 0.0]));
    // Nothing else is touched by it.
    let without = plan(
        &Look {
            hair_rgb: None,
            ..look.clone()
        },
        &w,
    )
    .unwrap();
    assert_eq!(
        apoc_character::Plan {
            hair_colour: without.hair_colour,
            ..p
        },
        without
    );
}

#[test]
fn a_hair_colour_that_is_not_numbers_falls_back_to_the_clfm() {
    let w = plan_world();
    let clfm_tint = [114.0 / 255.0, 110.0 / 255.0, 80.0 / 255.0];
    for bad in [
        [f32::NAN, 0.5, 0.5],
        [0.5, 0.5, f32::NAN],
        [f32::NAN; 3],
        [f32::INFINITY, 0.0, 0.0],
    ] {
        let look = Look {
            race: Some(race()),
            hair_colour: Some(clfm()),
            hair_rgb: Some(bad),
            ..Look::default()
        };
        assert!(near(
            plan(&look, &w).unwrap().hair_colour.unwrap(),
            clfm_tint
        ));
        // With no CLFM, the race's default.
        let look = Look {
            hair_colour: None,
            ..look
        };
        assert!(near(
            plan(&look, &w).unwrap().hair_colour.unwrap(),
            [20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0]
        ));
    }
}

#[test]
fn a_look_without_a_direct_hair_colour_plans_as_before() {
    let w = plan_world();
    // As an NPC_ gives it.
    let npc = Npc {
        race: Some(race()),
        hair_colour: Some(clfm()),
        head_parts: vec![hair_b(), face()],
        weight: Some(40.0),
        ..Npc::default()
    };
    let look = Look::of_npc(&npc);
    assert_eq!(look.hair_rgb, None);
    assert_eq!(look.custom_morphs, vec![]);
    let p = plan(&look, &w).unwrap();
    assert!(near(
        p.hair_colour.unwrap(),
        [114.0 / 255.0, 110.0 / 255.0, 80.0 / 255.0]
    ));
    assert_eq!(p.head.len(), 2);
    // As a save gives it: no CLFM of its own, so the race's.
    let save = Look {
        hair_colour: None,
        skin_tone: Some([0.5, 0.25, 1.0]),
        tints: vec![Tint {
            mask: 0x18,
            colour: [1, 2, 3, 4],
            strength: 0.5,
        }],
        ..look.clone()
    };
    let q = plan(&save, &w).unwrap();
    assert!(near(
        q.hair_colour.unwrap(),
        [20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0]
    ));
    assert_eq!(q.skin_tone, Some([0.5, 0.25, 1.0]));
    // Named sliders are carried, not drawn: they change no part of a plan.
    let named = Look {
        custom_morphs: vec![("Brow_Up".into(), 1.0), ("Tiny".into(), -0.5)],
        ..save.clone()
    };
    assert_eq!(plan(&named, &w).unwrap(), q);
    // With no colour anywhere, black.
    let mut bare = plan_world();
    bare.races.get_mut(&race()).unwrap().default_hair_colour = [None, None];
    assert_eq!(plan(&save, &bare).unwrap().hair_colour, Some([0.0; 3]));
}
