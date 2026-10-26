//! What a look is made of, worked out from the records before any file is
//! read: each head part and body part, the files each needs, and the
//! textures and colours each is drawn with.

use apoc_plugin::records::{HeadPartKind, PartFileKind, TextureSet};
use apoc_plugin::FormRef;

use crate::files::{under, weighted};
use crate::look::Look;
use crate::morph::Weights;
use crate::morph::VAMPIRE;
use crate::parts::{has_model, head_parts};
use crate::records::{vampire_keyword, Records};
use crate::CharacterError;

/// One head part to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadPiece {
    /// The part's editor id: the name its shape is given, as the game's
    /// pre-built faces name theirs.
    pub name: String,
    pub kind: Option<HeadPartKind>,
    /// Under `meshes`, as a [`crate::files::key`].
    pub model: String,
    /// Its tris, by kind, under `meshes`.
    pub tris: Vec<(PartFileKind, String)>,
    /// The texture paths the shape is given, by slot, where something says
    /// one: its own `TNAM` set, and for the face the face texture set over
    /// that.
    pub textures: [Option<String>; 8],
}

/// One body part to draw: an armour addon of the skin.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyPiece {
    pub name: String,
    /// Under `meshes`: the model the record names.
    pub model: String,
    /// The lightest and heaviest models when the record's names one of a
    /// pair ([`weighted`]); the build reads these in place of `model`.
    pub weights: Option<(String, String)>,
    /// The addon's skin texture set for the sex.
    pub textures: [Option<String>; 8],
}

/// Everything a build needs from the records.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub head: Vec<HeadPiece>,
    pub body: Vec<BodyPiece>,
    pub weights: Weights,
    /// 0 to 1: how far from the lightest body to the heaviest.
    pub weight: f32,
    /// The hair colour, red, green and blue from 0 to 1.
    pub hair_colour: Option<[f32; 3]>,
    pub skin_tone: Option<[f32; 3]>,
    /// What was left out, each a sentence: a record that is not there.
    pub notes: Vec<String>,
}

impl Plan {
    /// Every file a build of this plan reads, by key, each once.
    pub fn files(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |k: &String| {
            if !out.contains(k) {
                out.push(k.clone());
            }
        };
        for b in &self.body {
            match &b.weights {
                Some((light, heavy)) => {
                    add(light);
                    add(heavy);
                }
                None => add(&b.model),
            }
        }
        for h in &self.head {
            add(&h.model);
            for (_, t) in &h.tris {
                add(t);
            }
        }
        out
    }
}

/// Where in a shape's texture slots each of a `TXST`'s textures goes. Found
/// from the files: an eye's texture set has its environment mask in `TX02`
/// and its cube map in `TX05`, and the pre-built faces' eyes have them in
/// slots 5 and 4; a face's has its subsurface map in `TX03` and its detail
/// map in `TX04`, and the faces have them in slots 2 and 3.
pub const SLOT_OF: [usize; 8] = [0, 1, 5, 2, 3, 4, 6, 7];

/// A texture set's paths, by the shape's slot.
fn textures(set: Option<TextureSet>) -> [Option<String>; 8] {
    let mut out: [Option<String>; 8] = Default::default();
    if let Some(s) = set {
        for (i, t) in s.textures.into_iter().enumerate() {
            if let Some(t) = t.filter(|t| !t.is_empty()) {
                out[SLOT_OF[i]] = Some(t);
            }
        }
    }
    out
}

/// The shape's slots a face takes from its texture sets: colour, normal,
/// subsurface and detail.
const FACE_SLOTS: [usize; 4] = [0, 1, 2, 3];

/// `TXST` `DNAM` flag: its normal map is in model space (UESP).
const MODEL_SPACE: u16 = 0x4;

/// The texture paths a head part is given, by slot. Found from the files,
/// by which paths of a pre-built face are a texture set's (written as the
/// record writes them) and which its model's (written under `textures\`):
///
/// - A face takes colour, normal, subsurface and detail maps and keeps its
///   model's specular map: from its `TNAM`, the race's default face set over
///   that but for the colour (the afflicted heads are `MaleHeadAfflicted.dds`
///   from their `TNAM` with the Breton default's `BretonMale\MaleHead_msn`),
///   and the look's own face set over all four.
/// - Any other part takes its colour map from its `TNAM`, and its normal map
///   unless the set's is in model space: Argonian eyes' sets name
///   `EyeArgonian_msn` and their faces keep the model's `EyeWerewolfBeast_n`;
///   scars' name tangent-space maps and their faces have them.
fn part_textures(
    records: &dyn Records,
    look: &Look,
    head: &apoc_plugin::records::HeadData,
    part: &apoc_plugin::records::HeadPart,
) -> [Option<String>; 8] {
    let set = |f: Option<&FormRef>| f.and_then(|t| records.texture_set(t));
    let mut out: [Option<String>; 8] = Default::default();
    if part.kind == Some(HeadPartKind::Face) {
        // Later layers win; the colour map is the part's own where it has
        // one.
        let layers = [
            set(look.face_texture.as_ref()),
            set(head.default_face_texture.as_ref()),
            set(part.texture_set.as_ref()),
        ];
        let own = textures(layers[2].clone());
        for layer in [&layers[2], &layers[1], &layers[0]] {
            let t = textures(layer.clone());
            for i in FACE_SLOTS {
                if t[i].is_some() {
                    out[i] = t[i].clone();
                }
            }
        }
        if own[0].is_some() {
            out[0] = own[0].clone();
        }
    } else if let Some(s) = set(part.texture_set.as_ref()) {
        let model_space = s.flags & MODEL_SPACE != 0;
        let t = textures(Some(s));
        out[0] = t[0].clone();
        if !model_space {
            out[1] = t[1].clone();
        }
    }
    out
}

/// A hair colour as the tint its shapes are drawn with: twice the colour,
/// as a fraction of 255. Found from the files: Ulfric's hair colour
/// (`HairColor04HoneyBlond`) is 57, 55, 40, and every hair shape of his
/// pre-built face is tinted 114/255, 110/255, 80/255, and so for every face.
fn hair_tint(c: [u8; 4]) -> [f32; 3] {
    [c[0], c[1], c[2]].map(|v| v as f32 * 2.0 / 255.0)
}

/// Work out what `look` is made of.
pub fn plan(look: &Look, records: &dyn Records) -> Result<Plan, CharacterError> {
    let race_form = look.race.as_ref().ok_or(CharacterError::NoRace)?;
    let race = records
        .race(race_form)
        .ok_or(CharacterError::MissingRecord("RACE", race_form.id))?;
    let sex = look.female as usize;
    let mut plan = Plan::default();

    // The race whose head morphs these are, and whether it is a vampire's.
    let extra = records.race_extra(race_form);
    let morph_race = extra
        .morph_race
        .as_ref()
        .and_then(|r| records.race(r))
        .and_then(|r| r.editor_id)
        .or_else(|| race.editor_id.clone());
    plan.weights = Weights::of(look, morph_race.as_deref());
    if extra.keywords.contains(&vampire_keyword()) {
        plan.weights.chargen.insert(VAMPIRE.into(), 1.0);
    }
    plan.weight = if look.weight.is_finite() {
        look.weight.clamp(0.0, 100.0) / 100.0
    } else {
        0.0
    };
    // With no colour at all, black: the Dremora have none, and their faces'
    // hair is tinted 0, 0, 0.
    plan.hair_colour = Some(
        look.hair_colour
            .as_ref()
            .or(race.default_hair_colour[sex].as_ref())
            .and_then(|c| records.colour(c))
            .map_or([0.0; 3], |c| hair_tint(c.rgba)),
    );
    plan.skin_tone = look.skin_tone.filter(|t| t.iter().all(|v| v.is_finite()));

    for p in head_parts(records, &race, look.female, &look.head_parts) {
        let Some(h) = records.head_part(&p) else {
            plan.notes.push(format!("No head part {:08X}.", p.id));
            continue;
        };
        if !has_model(&h) {
            continue;
        }
        let tex = part_textures(records, look, &race.sexes[sex].head, &h);
        plan.head.push(HeadPiece {
            name: h
                .editor_id
                .clone()
                .unwrap_or_else(|| format!("{:08X}", p.id)),
            kind: h.kind,
            model: under("meshes", h.model.as_deref().unwrap_or_default()),
            tris: h
                .files
                .iter()
                .filter(|f| !f.path.is_empty())
                .map(|f| (f.kind, under("meshes", &f.path)))
                .collect(),
            textures: tex,
        });
    }
    // The order the game's pre-built faces keep (`--example facegen`) is
    // not one this follows; by name, so a build is the same each time.
    plan.head.sort_by(|a, b| a.name.cmp(&b.name));
    plan.body = body(
        look,
        &race_form.clone(),
        race.skin.as_ref(),
        records,
        &mut plan.notes,
    );
    Ok(plan)
}

/// The skin's addons for the race and sex.
fn body(
    look: &Look,
    race: &FormRef,
    race_skin: Option<&FormRef>,
    records: &dyn Records,
    notes: &mut Vec<String>,
) -> Vec<BodyPiece> {
    let Some(skin_form) = look.skin.as_ref().or(race_skin) else {
        return Vec::new();
    };
    let Some(skin) = records.armor(skin_form) else {
        notes.push(format!("No skin {:08X}.", skin_form.id));
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut slots = 0u32;
    for a in skin.addons.iter().take(256) {
        let Some(addon) = records.armor_addon(a) else {
            notes.push(format!("No armour addon {:08X}.", a.id));
            continue;
        };
        let for_race =
            addon.race.as_ref() == Some(race) || addon.additional_races.iter().any(|r| r == race);
        // One addon a slot: the first the skin lists.
        if !for_race || addon.slots & slots != 0 {
            continue;
        }
        let Some(model) = addon.model(look.female).filter(|m| !m.is_empty()) else {
            continue;
        };
        slots |= addon.slots;
        let model = under("meshes", model);
        let weights = weighted(&model);
        let set = addon.skin_textures[look.female as usize]
            .as_ref()
            .or(addon.skin_textures[0].as_ref().filter(|_| look.female))
            .and_then(|t| records.texture_set(t));
        out.push(BodyPiece {
            name: addon
                .editor_id
                .clone()
                .unwrap_or_else(|| format!("{:08X}", a.id)),
            model,
            weights,
            textures: textures(set),
        });
    }
    out
}
