//! A character's look: what a build is given. Made from an `NPC_` record
//! here; a save's look is turned into one elsewhere.

use apoc_plugin::records::Npc;
use apoc_plugin::FormRef;

use crate::records::Records;
use crate::CharacterError;

/// One face tint layer: which of the race's tint masks, its colour and how
/// strongly it is laid on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    /// The race's tint mask (its `TINI` number).
    pub mask: u16,
    /// RGBA.
    pub colour: [u8; 4],
    /// 0 to 1.
    pub strength: f32,
}

/// How a character looks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Look {
    /// `RACE`.
    pub race: Option<FormRef>,
    pub female: bool,
    /// The head parts (`HDPT`) as the character lists them. The race's
    /// defaults and each part's extras are worked out by the build
    /// ([`crate::parts::head_parts`]).
    pub head_parts: Vec<FormRef>,
    /// The chargen sliders (`NAM9`): 19 values in a save; an `NPC_` holds 18
    /// and four bytes after them, read here as a 19th.
    pub face_morphs: Vec<f32>,
    /// The preset chosen for nose, an unused one, eyes and lips (`NAMA`);
    /// -1 for none.
    pub face_parts: Vec<i32>,
    /// `CLFM`.
    pub hair_colour: Option<FormRef>,
    /// `TXST`: the face's textures.
    pub face_texture: Option<FormRef>,
    /// `ARMO` worn in place of the race's skin.
    pub skin: Option<FormRef>,
    /// Red, green, blue, 0 to 1.
    pub skin_tone: Option<[f32; 3]>,
    pub tints: Vec<Tint>,
    /// 0 to 100.
    pub weight: f32,
}

/// How far templates are followed before giving up: Skyrim.esm's longest
/// chain is a few links.
const MAX_TEMPLATES: usize = 32;

/// The `NPC_` a face comes from: `form` itself, or, while its template flags
/// say its traits come from its template, that template's, followed. A
/// template that is a leveled list (`LVLN`) has no one face.
pub fn face_source(records: &dyn Records, form: &FormRef) -> Result<Npc, CharacterError> {
    let mut npc = records
        .npc(form)
        .ok_or(CharacterError::MissingRecord("NPC_", form.id))?;
    for _ in 0..MAX_TEMPLATES {
        if !npc.face_from_template() {
            return Ok(npc);
        }
        let Some(t) = npc.template.clone() else {
            return Ok(npc);
        };
        match records.kind(&t) {
            Some(k) if &k == b"NPC_" => {
                npc = records
                    .npc(&t)
                    .ok_or(CharacterError::MissingRecord("NPC_", t.id))?;
            }
            Some(_) => return Err(CharacterError::LeveledTemplate(t.id)),
            None => return Err(CharacterError::MissingRecord("NPC_", t.id)),
        }
    }
    Err(CharacterError::TemplateLoop(form.id))
}

impl Look {
    /// The look of an `NPC_` record as it stands (no templates followed).
    pub fn of_npc(npc: &Npc) -> Look {
        Look {
            race: npc.race.clone(),
            female: npc.female(),
            head_parts: npc.head_parts.clone(),
            face_morphs: npc
                .face_morphs
                .map(|m| {
                    let mut v = m.values.to_vec();
                    v.push(f32::from_bits(m.tail));
                    v
                })
                .unwrap_or_default(),
            face_parts: npc
                .face_parts
                .map(|p| vec![p.nose, p.unknown, p.eyes, p.mouth])
                .unwrap_or_default(),
            hair_colour: npc.hair_colour.clone(),
            face_texture: npc.face_texture.clone(),
            skin: npc.skin.clone(),
            skin_tone: npc.skin_tone,
            tints: npc
                .tints
                .iter()
                .map(|t| Tint {
                    mask: t.index,
                    colour: t.rgba,
                    strength: t.value as f32 / 100.0,
                })
                .collect(),
            weight: npc.weight.unwrap_or(0.0),
        }
    }

    /// The look of the `NPC_` at `form`, its face from its template where
    /// the template flags say so ([`face_source`]).
    pub fn of_form(records: &dyn Records, form: &FormRef) -> Result<Look, CharacterError> {
        Ok(Look::of_npc(&face_source(records, form)?))
    }
}

impl From<&Npc> for Look {
    fn from(npc: &Npc) -> Self {
        Look::of_npc(npc)
    }
}
