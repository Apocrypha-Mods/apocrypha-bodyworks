//! A RaceMenu preset (`.jslot`) opened as a character: read as a look under
//! a race and sex to preview it as, edited, built with the save's plugins,
//! and written back.
//!
//! A preset holds no load order and no race, and the mod manager does not
//! say which plugins are active. Both come from a save the person picks:
//! its plugins, in its order, are the load order the preset's forms are
//! numbered by ([`Making::load_order`]), its records are what the preset is
//! read and built with, and its player gives the race and sex to preview as
//! until another is picked. The order is the save's, as it was when the
//! game saved it.

use std::collections::HashSet;

use apoc_character::files::{key, Source};
use apoc_character::preset::{self, PresetReport};
use apoc_character::racemenu::{RaceMenuMorphs, SliderKind};
use apoc_character::{Look, Records, Tint};
use apoc_jslot::{Document, Value as Jv};
use apoc_plugin::records::{HeadPartKind, EXTRA_PART};
use apoc_plugin::FormRef;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::character::Making;

/// The sections a look does not edit, which are written back as they were
/// read: their keys in the preset, by where they are.
pub const KEPT: [(&str, &str); 5] = [
    ("morphs", "sculpt"),
    ("", "overrides"),
    ("", "transforms"),
    ("", "faceTextures"),
    ("", "bodyMorphs"),
];

/// The head part kinds (`PNAM`) a look's parts are picked from.
const PICKED: [u32; 5] = [3, 2, 6, 4, 5];

/// Head part flags (`DATA`): playable, male, female.
const PLAYABLE: u8 = 0x1;
const MALE: u8 = 0x2;
const FEMALE: u8 = 0x4;

/// A preset being worked on.
pub struct Opened {
    /// The file's name, without `.jslot`.
    pub name: String,
    doc: Document,
    /// The race and sex it is previewed as, once a save's plugins are read.
    race: Option<FormRef>,
    female: bool,
    /// The look the preset reads as under that race, and what it couldn't use.
    read: Option<(Look, PresetReport)>,
    /// Every file asked of the interface for this preset, by key, so none is
    /// asked twice: one the mods have not got is not asked again.
    asked: HashSet<String>,
    /// RaceMenu's slider files, read once every one was asked for.
    racemenu: Option<RaceMenuMorphs>,
}

/// A form as the interface names it.
#[derive(Deserialize, Clone)]
pub struct FormIn {
    plugin: String,
    id: u32,
}

impl From<&FormIn> for FormRef {
    fn from(f: &FormIn) -> Self {
        FormRef::new(&f.plugin, f.id)
    }
}

#[derive(Deserialize)]
pub struct TintIn {
    mask: u16,
    colour: [u8; 4],
    strength: f32,
}

/// The parts of a look the screen edits. A value given as nothing is the
/// one the preset reads as (a number JSON cannot hold, read as nothing).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookIn {
    head_parts: Vec<FormIn>,
    face_morphs: Vec<Option<f32>>,
    face_parts: Vec<i32>,
    hair_rgb: Option<[f32; 3]>,
    skin_tone: Option<[f32; 3]>,
    tints: Vec<TintIn>,
    weight: f32,
    custom: Vec<(String, Option<f32>)>,
}

fn form_json(f: &FormRef) -> Value {
    json!({ "plugin": f.plugin, "id": f.id })
}

/// A number JSON can hold, or nothing.
fn finite(v: f32) -> Value {
    if v.is_finite() {
        json!(v)
    } else {
        Value::Null
    }
}

impl Opened {
    /// A preset's bytes, read.
    pub fn new(name: &str, bytes: &[u8]) -> Result<Opened, String> {
        let doc = apoc_jslot::parse(bytes).map_err(|e| {
            let e = e.to_string();
            let mut c = e.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or(e)
                + "."
        })?;
        if doc.root.as_object().is_none() {
            return Err("This isn't a RaceMenu preset.".into());
        }
        Ok(Opened {
            name: name.to_string(),
            doc,
            race: None,
            female: false,
            read: None,
            asked: HashSet::new(),
            racemenu: None,
        })
    }

    /// What the preset holds that the screen keeps as read, each with how
    /// many entries it has; those the preset has not got are left out.
    pub fn kept(&self) -> Vec<Value> {
        let top = self.doc.root.as_object();
        KEPT.iter()
            .filter_map(|(parent, k)| {
                let o = if parent.is_empty() {
                    top
                } else {
                    top?.get(parent)?.as_object()
                };
                let v = o?.get(k)?;
                let count = match v {
                    Jv::Object(o) => o.len(),
                    Jv::Array(a) => a.len(),
                    _ => 1,
                };
                Some(json!({ "key": k, "count": count }))
            })
            .collect()
    }

    /// Forget what was read with another save's plugins.
    pub fn forget(&mut self) {
        self.race = None;
        self.read = None;
        self.asked.clear();
        self.racemenu = None;
    }

    /// Read the preset as `race` and sex (the save's player's when not
    /// given), and say what files are still wanted for its sliders and its
    /// build. The answer is the look and everything the screen picks from.
    pub fn look(
        &mut self,
        making: &Making,
        race: Option<FormRef>,
        female: Option<bool>,
    ) -> Result<Value, String> {
        let records = making
            .records()
            .ok_or("The save's plugins have not been read.")?;
        let (player_race, player_female) = making.player();
        let race = race
            .or_else(|| self.race.clone())
            .or(player_race)
            .ok_or("The save names no race for its character.")?;
        let female = female.unwrap_or(if self.race.is_some() {
            self.female
        } else {
            player_female
        });
        let race_record = records
            .race(&race)
            .ok_or_else(|| format!("There's no race {}|{:06X}.", race.plugin, race.id))?;
        let order = making.load_order();
        if self.race.as_ref() != Some(&race) || self.female != female || self.read.is_none() {
            self.read = Some(preset::look_of_preset(
                &self.doc, &race, female, records, &order,
            ));
            self.race = Some(race.clone());
            self.female = female;
        }
        let (look, report) = self.read.clone().expect("read just now");

        // RaceMenu's slider files first: what is wanted depends on what came.
        let plugins: Vec<&str> = order.plugins().iter().map(|p| p.name.as_str()).collect();
        let mut files = Vec::new();
        if self.racemenu.is_none() {
            let (wanted, _) = RaceMenuMorphs::needed(&plugins, &making.files);
            files = wanted
                .into_iter()
                .filter(|k| self.asked.insert(k.clone()))
                .collect();
            if files.is_empty() {
                self.racemenu = Some(RaceMenuMorphs::read(&plugins, &making.files));
            }
        }
        let sliders: Vec<Value> = self
            .racemenu
            .as_ref()
            .and_then(|rm| {
                let id = race_record.editor_id.as_deref()?;
                Some(
                    rm.sliders_for(id, female)
                        .into_iter()
                        .map(slider_json)
                        .collect(),
                )
            })
            .unwrap_or_default();

        let head = &race_record.sexes[usize::from(female)].head;
        let entries = self.tint_entries();
        let masks: Vec<Value> = head
            .tints
            .iter()
            .enumerate()
            .filter_map(|(place, m)| {
                let texture = m.mask.as_deref()?;
                // The colour the preset keeps for the mask, in use or not,
                // so a layer taken up starts from it.
                let same = |(t, _, _): &&(String, Option<u32>, u32)| *t == plain_path(texture);
                let stored = entries
                    .iter()
                    .filter(same)
                    .find(|(_, i, _)| *i == Some(place as u32))
                    .or_else(|| entries.iter().find(same))
                    .map(|(_, _, c)| {
                        let [_, r, g, b] = c.to_be_bytes();
                        [r, g, b]
                    });
                Some(json!({
                    "index": m.index,
                    "kind": m.kind,
                    "texture": texture,
                    "place": place,
                    "stored": stored,
                }))
            })
            .collect();
        // Which of each feature's presets the race offers: nose, the unused
        // one, eyes, lips, as a look holds them; the race's lists are nose,
        // brows, eyes, lips.
        let offered: Vec<Vec<u32>> = [Some(0u32), None, Some(2), Some(3)]
            .iter()
            .map(|which| {
                let Some(w) = which else { return Vec::new() };
                head.morphs
                    .iter()
                    .find(|m| m.index == *w)
                    .map(|m| (0..32).filter(|b| m.flags[0] >> b & 1 == 1).collect())
                    .unwrap_or_default()
            })
            .collect();
        let parts: Vec<Value> = look
            .head_parts
            .iter()
            .map(|f| {
                let h = records.head_part(f);
                json!({
                    "plugin": f.plugin,
                    "id": f.id,
                    "name": h.as_ref().and_then(|h| h.editor_id.clone()),
                    "kind": h.as_ref().and_then(|h| h.kind).map(kind_number),
                    // In the save's plugins, and whether that plugin could be read.
                    "loaded": order.place(&f.plugin).is_some(),
                    "installed": records.plugin(&f.plugin).is_some(),
                })
            })
            .collect();

        let mut answer = json!({
            "name": self.name,
            "save": making.name.trim(),
            "race": { "plugin": race.plugin, "id": race.id, "name": race_record.editor_id },
            "female": female,
            "races": races(records),
            "look": look_json(&look),
            "parts": parts,
            "choices": choices(records, female),
            "masks": masks,
            "offered": offered,
            "sliders": sliders,
            // The game's face sliders' morphs below and above nought, as a
            // build turns them on.
            "faceSliders": apoc_character::morph::SLIDERS,
            "notes": report.notes.iter().map(|n| n.to_string()).collect::<Vec<_>>(),
            "kept": self.kept(),
            "files": files,
        });
        if answer["files"].as_array().is_some_and(|f| f.is_empty()) {
            answer["files"] = json!(self.wanted(making, &look)?);
        }
        Ok(answer)
    }

    /// The look the screen sent, over the one the preset reads as.
    pub fn edited(&self, edits: &LookIn) -> Result<Look, String> {
        let (read, _) = self
            .read
            .as_ref()
            .ok_or("The preset has not been read as a race yet.")?;
        let mut look = read.clone();
        look.head_parts = edits.head_parts.iter().map(FormRef::from).collect();
        look.face_morphs = edits
            .face_morphs
            .iter()
            .enumerate()
            .map(|(i, v)| v.or(read.face_morphs.get(i).copied()).unwrap_or(0.0))
            .collect();
        look.face_parts = edits.face_parts.clone();
        look.hair_rgb = edits.hair_rgb.or(read.hair_rgb);
        look.skin_tone = edits.skin_tone.or(read.skin_tone);
        look.tints = edits
            .tints
            .iter()
            .map(|t| Tint {
                mask: t.mask,
                colour: t.colour,
                strength: t.strength,
            })
            .collect();
        look.weight = edits.weight;
        look.custom_morphs = edits
            .custom
            .iter()
            .map(|(name, v)| {
                let read_as = || {
                    read.custom_morphs
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, v)| *v)
                };
                (name.clone(), v.or_else(read_as).unwrap_or(0.0))
            })
            .collect();
        Ok(look)
    }

    /// The files a build of `look` reads that have not been asked for yet,
    /// each now taken as asked.
    pub fn wanted(&mut self, making: &Making, look: &Look) -> Result<Vec<String>, String> {
        let records = making
            .records()
            .ok_or("The save's plugins have not been read.")?;
        let files = apoc_character::needed_with(look, records, self.racemenu.as_ref())
            .map_err(|e| e.to_string())?;
        Ok(files
            .into_iter()
            .filter(|k| making.files.read(k).is_none() && self.asked.insert(key(k)))
            .collect())
    }

    /// The preset's tint entries as `(texture, index, color)`, the texture
    /// as [`plain_path`] has it; entries missing any of them are left out.
    fn tint_entries(&self) -> Vec<(String, Option<u32>, u32)> {
        let Some(list) = self
            .doc
            .root
            .as_object()
            .and_then(|o| o.get("tintInfo"))
            .and_then(Jv::as_array)
        else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|e| {
                let o = e.as_object()?;
                let texture = o.get("texture")?.as_str()?.text()?;
                let colour = o.get("color")?.as_number()?.as_u64()?;
                let index = o
                    .get("index")
                    .and_then(Jv::as_number)
                    .and_then(|n| n.as_u64())
                    .and_then(|n| u32::try_from(n).ok());
                Some((plain_path(&texture), index, u32::try_from(colour).ok()?))
            })
            .collect()
    }

    pub fn racemenu(&self) -> Option<&RaceMenuMorphs> {
        self.racemenu.as_ref()
    }

    /// `look` written into the preset as read, as a file's bytes, and what
    /// was changed. The preset kept here is not changed.
    pub fn write(&self, making: &Making, look: &Look) -> Result<(Vec<u8>, Vec<String>), String> {
        let records = making
            .records()
            .ok_or("The save's plugins have not been read.")?;
        let mut doc = self.doc.clone();
        let report = preset::apply_look(&mut doc, look, records, &making.load_order())
            .map_err(|e| sentence(&e.to_string()))?;
        let bytes = apoc_jslot::write(&doc).map_err(|e| sentence(&e.to_string()))?;
        Ok((bytes, report.notes.iter().map(|n| n.to_string()).collect()))
    }
}

/// A texture path as two are compared: lower case, either slash alike.
fn plain_path(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

/// A message as a sentence: its first letter upper case, a full stop after.
fn sentence(e: &str) -> String {
    let mut c = e.chars();
    match c.next() {
        Some(f) => format!("{}{}.", f.to_uppercase(), c.as_str()),
        None => String::new(),
    }
}

fn kind_number(k: HeadPartKind) -> u32 {
    match k {
        HeadPartKind::Misc => 0,
        HeadPartKind::Face => 1,
        HeadPartKind::Eyes => 2,
        HeadPartKind::Hair => 3,
        HeadPartKind::FacialHair => 4,
        HeadPartKind::Scar => 5,
        HeadPartKind::Eyebrows => 6,
        HeadPartKind::Other(v) => v,
    }
}

/// A look as the screen edits it.
fn look_json(look: &Look) -> Value {
    json!({
        "headParts": look.head_parts.iter().map(form_json).collect::<Vec<_>>(),
        "faceMorphs": look.face_morphs.iter().map(|v| finite(*v)).collect::<Vec<_>>(),
        "faceParts": look.face_parts,
        "hairRgb": look.hair_rgb,
        "skinTone": look.skin_tone,
        "tints": look.tints.iter().map(|t| json!({ "mask": t.mask, "colour": t.colour, "strength": t.strength })).collect::<Vec<_>>(),
        "weight": look.weight,
        "custom": look.custom_morphs.iter().map(|(n, v)| json!([n, finite(*v)])).collect::<Vec<_>>(),
    })
}

/// A slider of RaceMenu's as the screen shows it. For a `Slider`, the
/// morphs below and above nought are as [`SliderKind::morph_at`] has them.
fn slider_json(d: &apoc_character::racemenu::SliderDef) -> Value {
    match &d.kind {
        SliderKind::Slider { .. } => json!({
            "name": d.name,
            "kind": "slider",
            "below": d.kind.morph_at(-1.0).and_then(|(m, _)| m),
            "above": d.kind.morph_at(1.0).and_then(|(m, _)| m),
        }),
        SliderKind::Preset { count, .. } => {
            json!({ "name": d.name, "kind": "preset", "count": count })
        }
        SliderKind::HeadPart { .. } => json!({ "name": d.name, "kind": "headPart" }),
    }
}

/// Every race with head parts for either sex, by editor id.
fn races(records: &apoc_plugin::LoadOrder) -> Vec<Value> {
    let mut out: Vec<(String, &FormRef)> = records
        .forms()
        .filter(|f| records.kind(f) == Some(*b"RACE"))
        .filter_map(|f| {
            let r = records.race(f)?;
            let any = r.sexes.iter().any(|s| !s.head.parts.is_empty());
            any.then_some((r.editor_id?, f))
        })
        .collect();
    out.sort_by_key(|a| a.0.to_lowercase());
    out.into_iter()
        .map(|(name, f)| json!({ "plugin": f.plugin, "id": f.id, "name": name }))
        .collect()
}

/// The playable head parts for the sex that a look's parts are picked
/// from, by kind (`PNAM`), each sorted by editor id. Not narrowed to the
/// race: the races a part is for are a form list, which is not read.
fn choices(records: &apoc_plugin::LoadOrder, female: bool) -> Value {
    let sex = if female { FEMALE } else { MALE };
    let mut by: Vec<Vec<(String, &FormRef)>> = vec![Vec::new(); PICKED.len()];
    for f in records.forms() {
        if records.kind(f) != Some(*b"HDPT") {
            continue;
        }
        let Some(h) = records.head_part(f) else {
            continue;
        };
        if h.flags & PLAYABLE == 0 || h.flags & sex == 0 || h.flags & EXTRA_PART != 0 {
            continue;
        }
        let (Some(kind), Some(name)) = (h.kind.map(kind_number), h.editor_id) else {
            continue;
        };
        if let Some(i) = PICKED.iter().position(|k| *k == kind) {
            by[i].push((name, f));
        }
    }
    let mut out = serde_json::Map::new();
    for (i, mut list) in by.into_iter().enumerate() {
        list.sort_by_key(|a| a.0.to_lowercase());
        out.insert(
            PICKED[i].to_string(),
            json!(list
                .into_iter()
                .map(|(name, f)| json!({ "plugin": f.plugin, "id": f.id, "name": name }))
                .collect::<Vec<_>>()),
        );
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use apoc_character::racemenu::parse_sliders;

    /// A slider file laid out as mods ship them, with made-up names: each
    /// slider's morph below and above nought is what `morph_at` gives, the
    /// file's first morph below.
    #[test]
    fn a_sliders_ends_are_the_sign_rules() {
        let file = b"# made up\r\n[Female]\r\nMade Width = 16, Slider, MadeNarrow, MadeWide\r\nMade Up = 16, Slider, None, MadeUp\r\nMade Shape = 16, Preset, MadeShape, 4\r\nMade Part = 8, HeadPart, 3\r\n";
        let (sliders, notes) = parse_sliders(file, "made.ini");
        assert!(notes.is_empty(), "{notes:?}");
        let out: Vec<Value> = sliders.female.iter().map(slider_json).collect();
        assert_eq!(
            out,
            [
                json!({ "name": "Made Width", "kind": "slider", "below": "MadeNarrow", "above": "MadeWide" }),
                json!({ "name": "Made Up", "kind": "slider", "below": null, "above": "MadeUp" }),
                json!({ "name": "Made Shape", "kind": "preset", "count": 4 }),
                json!({ "name": "Made Part", "kind": "headPart" }),
            ]
        );
        for (d, j) in sliders.female.iter().zip(&out) {
            if j["kind"] == "slider" {
                assert_eq!(
                    j["below"].as_str(),
                    d.kind.morph_at(-1.0).and_then(|(m, _)| m)
                );
                assert_eq!(
                    j["above"].as_str(),
                    d.kind.morph_at(1.0).and_then(|(m, _)| m)
                );
            }
        }
    }
}
