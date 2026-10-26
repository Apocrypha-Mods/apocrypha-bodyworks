//! A save's character, read in steps: the save, then the pieces of the
//! plugins its records are in, then the files its parts are made of. Each
//! step's answer says what the next needs, for the interface to fetch from
//! the mod manager: the plugins are far too large to hand over whole
//! (Skyrim.esm is 250 MB), and only the groups of the records a character
//! is made from are read, about 2% of it.

use apoc_character::{Files, Look, Tint};
use apoc_plugin::records::HeadPartKind;
use apoc_plugin::walk::Walk;
use apoc_plugin::{FormRef, LoadOrder, Plugin};
use serde::Deserialize;
use serde_json::{json, Value};

/// The kinds of record a character is made from.
pub const WANTED: &[[u8; 4]] = &[
    *b"RACE", *b"HDPT", *b"TXST", *b"ARMO", *b"ARMA", *b"CLFM", *b"NPC_",
];

/// The first piece of every plugin: its header, which says how long the
/// plugin's own header record is. As much as the walk asks for first.
const FIRST: u64 = 24;

/// How one plugin of the save's stands.
#[derive(Debug)]
enum Reading {
    /// Its first piece has been asked for; until it comes, its size is not known.
    Asked,
    Walking(Walk),
    Read(Plugin),
    /// The mod manager has not got it, or it could not be read: why.
    Missing(String),
    /// Read, and handed on to the load order.
    Done,
}

/// A save's character being read.
pub struct Making {
    look: apoc_save::Look,
    /// The character's name, as the save gives it.
    pub name: String,
    /// The save's plugins in load order: its full plugins, then its light
    /// ones. A save does not say where among the full ones each light one
    /// loaded, so a light plugin's records are taken over a full one's.
    order: Vec<String>,
    readings: Vec<Reading>,
    /// The plugins read, in load order, once every one is read or given up on.
    records: Option<LoadOrder>,
    /// The files the character's parts are made of, as they come.
    pub files: Files,
}

/// A character built: its mesh's bytes, the files that were not there,
/// and what else was left out and why.
pub struct Made {
    pub mesh: Vec<u8>,
    pub missing: Vec<String>,
    pub notes: Vec<String>,
    /// What the interface lays the face's tint from ([`FaceTint`]); none
    /// when the face was left out or has no layers to lay.
    pub face: Option<FaceTint>,
}

/// The face's tint as the interface lays it: the race's masks by their
/// number and the save's layers in its order, laid over the skin tone.
///
/// The rule, found from the game's own baked tint textures (all 2,292
/// Skyrim.esm NPCs with tint layers, `--example tints` and a script
/// comparing with each `FaceTint` texture): start with the skin tone (an
/// NPC's `QNAM`) and lay each layer over it in its record's order, a lerp
/// toward its colour by the mask's red times its strength, with no gamma.
/// The skin tone layer (`TINP` 6) is not laid: an NPC's `QNAM` already is
/// it, mid grey (127) lerped toward its colour by its strength (1,736 of
/// the 1,737 NPCs with one). A save keeps that layer's colour as its skin
/// tone, so the start is made from the layer the same way; a save without
/// one starts with its skin tone. A colour's alpha is left out: no layer
/// laid on by any of those NPCs has one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FaceTint {
    /// The face's shape in the built mesh.
    pub shape: String,
    /// What the layers are laid over: red, green, blue, 0 to 1.
    pub base: [f32; 3],
    /// The race's mask textures for the sex, by `TINI`, as the race names them.
    pub masks: Vec<(u16, String)>,
    /// The layers to lay, with no strength left out.
    pub layers: Vec<Tint>,
}

impl FaceTint {
    pub fn json(&self) -> Value {
        json!({
            "shape": self.shape,
            "base": self.base,
            "masks": self.masks.iter().map(|(i, p)| (i.to_string(), json!(p))).collect::<serde_json::Map<_, _>>(),
            "layers": self.layers.iter().map(|t| {
                let colour = [0, 1, 2].map(|c| f32::from(t.colour[c]) / 255.0);
                json!({ "mask": t.mask, "colour": colour, "strength": t.strength })
            }).collect::<Vec<_>>(),
        })
    }
}

/// `TINP` for the skin tone layer.
const SKIN_TONE_LAYER: u16 = 6;

/// A piece of a plugin, as the interface hands it back: `length` is how
/// many of the request's bytes are it, or none when the plugin could not
/// be had; `size` is the whole file's.
#[derive(Deserialize)]
pub struct Piece {
    plugin: String,
    at: u64,
    length: Option<usize>,
    #[serde(default)]
    size: Option<u64>,
}

impl Making {
    /// A save's bytes, read as far as its character.
    pub fn new(save: &[u8]) -> Result<Self, String> {
        let save =
            apoc_save::Save::read(save).map_err(|e| format!("The save could not be read: {e}"))?;
        let look = save
            .player()
            .map_err(|e| format!("The save's character could not be read: {e}"))?;
        let order: Vec<String> = save
            .plugins()
            .iter()
            .chain(save.light_plugins())
            .cloned()
            .collect();
        Ok(Making {
            look,
            name: save.header().name.clone(),
            readings: order.iter().map(|_| Reading::Asked).collect(),
            order,
            records: None,
            files: Files::new(),
        })
    }

    /// The pieces still wanted, as `{ plugin, at, length }`.
    pub fn pieces(&self) -> Vec<Value> {
        self.order
            .iter()
            .zip(&self.readings)
            .filter_map(|(plugin, r)| match r {
                Reading::Asked => Some(json!({ "plugin": plugin, "at": 0, "length": FIRST })),
                Reading::Walking(walk) => walk
                    .need()
                    .map(|(at, length)| json!({ "plugin": plugin, "at": at, "length": length })),
                _ => None,
            })
            .collect()
    }

    /// The pieces asked for, their bytes one after another in `data`.
    pub fn feed(&mut self, pieces: &[Piece], data: &[u8]) -> Result<(), String> {
        if self.records.is_some() {
            return Err("The save's plugins have been read already.".into());
        }
        // Every piece is checked before any is taken, so a batch that is
        // wrong leaves every plugin where it was.
        let mut checked = Vec::with_capacity(pieces.len());
        let mut from = 0usize;
        for piece in pieces {
            let Some(at) = self
                .order
                .iter()
                .position(|p| p.eq_ignore_ascii_case(&piece.plugin))
            else {
                return Err(format!(
                    "{} is not one of the save's plugins.",
                    piece.plugin
                ));
            };
            if checked.iter().any(|(a, _)| *a == at) {
                return Err(format!("{} came twice in one batch.", piece.plugin));
            }
            let bytes = match piece.length {
                None => None,
                Some(length) => {
                    let to = from
                        .checked_add(length)
                        .filter(|to| *to <= data.len())
                        .ok_or("The pieces are longer than what came with them.")?;
                    let bytes = &data[from..to];
                    from = to;
                    Some(bytes)
                }
            };
            checked.push((at, bytes));
        }
        for ((at, bytes), piece) in checked.into_iter().zip(pieces) {
            let Some(bytes) = bytes else {
                self.readings[at] = Reading::Missing("not installed".into());
                continue;
            };
            let reading = std::mem::replace(&mut self.readings[at], Reading::Asked);
            self.readings[at] = match reading {
                Reading::Asked => match piece.size {
                    Some(size) => step(Walk::new(&self.order[at], size, WANTED), piece.at, bytes),
                    None => Reading::Missing("its size was not given".into()),
                },
                Reading::Walking(walk) => step(walk, piece.at, bytes),
                done => done,
            };
        }
        Ok(())
    }

    /// The plugins the character's own forms are in (race, head parts,
    /// colours, textures) that could not be read, each with why. A save
    /// lists every plugin it was played with, and most have no part in
    /// how the character looks.
    pub fn missing_plugins(&self) -> Vec<(String, String)> {
        let l = &self.look;
        let theirs: Vec<&str> = l
            .race
            .iter()
            .chain([&l.hair_colour, &l.face_texture_set])
            .chain(&l.head_parts)
            .filter_map(|f| match f {
                apoc_save::Form::Plugin(r) => Some(r.plugin.as_str()),
                _ => None,
            })
            .collect();
        self.order
            .iter()
            .zip(&self.readings)
            .filter_map(|(name, r)| match r {
                Reading::Missing(why) if theirs.iter().any(|t| t.eq_ignore_ascii_case(name)) => {
                    Some((name.clone(), why.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Once every plugin is read or given up on, the files the character's
    /// parts are made of, by path under the data folder; before, nothing.
    pub fn files_needed(&mut self) -> Result<Vec<String>, String> {
        if self.records.is_none() {
            let settled = self
                .readings
                .iter()
                .all(|r| matches!(r, Reading::Read(_) | Reading::Missing(_)));
            if !settled {
                return Ok(Vec::new());
            }
            // The plugins read go to the load order; each leaves word
            // that it was read, and those given up on keep why.
            let mut plugins = Vec::new();
            for r in &mut self.readings {
                if matches!(r, Reading::Read(_)) {
                    if let Reading::Read(p) = std::mem::replace(r, Reading::Done) {
                        plugins.push(p);
                    }
                }
            }
            self.records = Some(LoadOrder::new(plugins));
        }
        let records = self.records.as_ref().expect("made just now");
        apoc_character::needed(&self.look(records, None), records).map_err(|e| e.to_string())
    }

    /// The character's skin tone, red, green and blue from 0 to 1.
    pub fn skin_tone(&self) -> [f32; 3] {
        [0, 1, 2].map(|c| f32::from(self.look.skin_tone[c]) / 255.0)
    }

    /// The character, built from the files given.
    pub fn build(&self, weight: Option<f32>) -> Result<Made, String> {
        let records = self
            .records
            .as_ref()
            .ok_or("The save's plugins have not been read.")?;
        let look = self.look(records, weight);
        let plan = apoc_character::plan(&look, records).map_err(|e| e.to_string())?;
        let built = apoc_character::build(&plan, &self.files, &apoc_character::Options::default())
            .map_err(|e| e.to_string())?;
        let face = plan
            .head
            .iter()
            .filter(|p| p.kind == Some(HeadPartKind::Face))
            .find_map(|p| built.shapes.iter().find(|s| s.piece == p.name))
            .and_then(|s| face_tint(&look, records, &s.name));
        Ok(Made {
            mesh: built.nif.write(),
            missing: built.missing,
            notes: built.notes,
            face,
        })
    }

    /// The save's look as a build takes it. A save keeps no weight (see
    /// `apoc_save::look`): `weight` when given, else the player record's.
    fn look(&self, records: &LoadOrder, weight: Option<f32>) -> Look {
        let l = &self.look;
        // The player's own record is Skyrim.esm's, as the save spells it.
        let game = self
            .order
            .iter()
            .find(|p| p.eq_ignore_ascii_case("Skyrim.esm"))
            .map_or("Skyrim.esm", String::as_str);
        let player = apoc_character::Records::npc(records, &FormRef::new(game, 7));
        Look {
            race: l
                .race
                .as_ref()
                .and_then(form)
                .or_else(|| player.as_ref().and_then(|p| p.race.clone())),
            female: l.sex == 1,
            head_parts: l.head_parts.iter().filter_map(form).collect(),
            face_morphs: l.morphs.clone(),
            face_parts: l.presets.clone(),
            // A colour from a plugin that is not there is the race's own for the sex.
            hair_colour: form(&l.hair_colour)
                .filter(|c| apoc_character::Records::colour(records, c).is_some())
                .or_else(|| {
                    let race = l.race.as_ref().and_then(form)?;
                    apoc_character::Records::race(records, &race)?.default_hair_colour
                        [usize::from(l.sex == 1)]
                    .clone()
                }),
            face_texture: form(&l.face_texture_set),
            skin: player.as_ref().and_then(|p| p.skin.clone()),
            skin_tone: Some([0, 1, 2].map(|c| f32::from(l.skin_tone[c]) / 255.0)),
            tints: l
                .tints
                .iter()
                .map(|t| Tint {
                    mask: t.mask,
                    colour: t.colour,
                    strength: t.strength,
                })
                .collect(),
            weight: weight
                .or(l.weight)
                .or(player.and_then(|p| p.weight))
                .unwrap_or(100.0)
                .clamp(0.0, 100.0),
        }
    }
}

/// The tint of the face shape `shape`: the race's masks for the look's sex
/// and the layers of the look's that one of them paints, the skin tone
/// layer and those of no strength left out. None when no layer is left.
fn face_tint(look: &Look, records: &LoadOrder, shape: &str) -> Option<FaceTint> {
    let race = apoc_character::Records::race(records, look.race.as_ref()?)?;
    let head = &race.sexes[usize::from(look.female)].head;
    let mask = |i: u16| head.tints.iter().find(|m| m.index == i);
    let layers: Vec<Tint> = look
        .tints
        .iter()
        .filter(|t| t.strength > 0.0)
        .filter(|t| {
            mask(t.mask).is_some_and(|m| m.mask.is_some() && m.kind != Some(SKIN_TONE_LAYER))
        })
        .copied()
        .collect();
    if layers.is_empty() {
        return None;
    }
    let base = match look
        .tints
        .iter()
        .find(|t| mask(t.mask).is_some_and(|m| m.kind == Some(SKIN_TONE_LAYER)))
    {
        Some(t) => [0, 1, 2].map(|c| {
            let grey = 127.0 / 255.0;
            grey + (f32::from(t.colour[c]) / 255.0 - grey) * t.strength.clamp(0.0, 1.0)
        }),
        None => look.skin_tone.unwrap_or([1.0; 3]),
    };
    let masks = head
        .tints
        .iter()
        .filter(|m| layers.iter().any(|t| t.mask == m.index))
        .filter_map(|m| Some((m.index, m.mask.clone()?)))
        .collect();
    Some(FaceTint {
        shape: shape.to_string(),
        base,
        masks,
        layers,
    })
}

/// A form of the save's as the plugins name it; one made in the game is in no plugin.
fn form(f: &apoc_save::Form) -> Option<FormRef> {
    match f {
        apoc_save::Form::Plugin(r) => Some(FormRef::new(&r.plugin, r.id)),
        _ => None,
    }
}

/// A walk given one piece: walking on, finished, or given up with why.
fn step(mut walk: Walk, at: u64, bytes: &[u8]) -> Reading {
    if let Err(e) = walk.feed(at, bytes) {
        return Reading::Missing(e.to_string());
    }
    if walk.need().is_some() {
        return Reading::Walking(walk);
    }
    match walk.finish() {
        Ok(plugin) => Reading::Read(plugin),
        Err(e) => Reading::Missing(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    //! A save and its plugins made by hand, read as the interface reads
    //! them: a piece at a time.

    use super::*;
    use crate::state::{handle, State};

    fn sub(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = kind.to_vec();
        out.extend((data.len() as u16).to_le_bytes());
        out.extend(data);
        out
    }

    fn zs(s: &str) -> Vec<u8> {
        format!("{s}\0").into_bytes()
    }

    fn rec(kind: &[u8; 4], id: u32, subs: &[Vec<u8>]) -> Vec<u8> {
        let data: Vec<u8> = subs.concat();
        let mut out = kind.to_vec();
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(id.to_le_bytes());
        out.extend([0; 4]);
        out.extend(44u16.to_le_bytes());
        out.extend([0; 2]);
        out.extend(data);
        out
    }

    fn grup(label: &[u8; 4], records: &[Vec<u8>]) -> Vec<u8> {
        let body = records.concat();
        let mut out = b"GRUP".to_vec();
        out.extend((body.len() as u32 + 24).to_le_bytes());
        out.extend(label);
        out.extend(0i32.to_le_bytes());
        out.extend([0; 8]);
        out.extend(body);
        out
    }

    fn plugin(masters: &[&str], groups: &[Vec<u8>]) -> Vec<u8> {
        let mut head = vec![sub(
            b"HEDR",
            &[0x9a, 0x99, 0xd9, 0x3f, 1, 0, 0, 0, 0, 8, 0, 0],
        )];
        for m in masters {
            head.push(sub(b"MAST", &zs(m)));
            head.push(sub(b"DATA", &[0; 8]));
        }
        let mut out = rec(b"TES4", 0, &head);
        out.extend(groups.concat());
        out
    }

    fn u32s(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// The face's model, which the samples hold.
    const FACE_MODEL: &str = "Actors\\Character\\Character Assets\\MaleHead.nif";
    const FACE_KEY: &str = "meshes/actors/character/character assets/malehead.nif";

    /// Skyrim.esm as far as this character goes: the player, a race with
    /// default hair colours for each sex and tint masks (1 and 3 painted,
    /// 2 the skin tone; a woman's only 1), its colours, a face and a face
    /// texture set; and a group of a kind no character needs.
    fn skyrim() -> Vec<u8> {
        let player = rec(
            b"NPC_",
            7,
            &[
                sub(b"EDID", &zs("Player")),
                sub(b"RNAM", &u32s(&[0x13746])),
                sub(b"NAM7", &40f32.to_le_bytes()),
            ],
        );
        let race = rec(
            b"RACE",
            0x13746,
            &[
                sub(b"EDID", &zs("TestRace")),
                sub(b"HCLF", &u32s(&[0xA00, 0xA01])),
                sub(b"NAM0", &[]),
                sub(b"MNAM", &[]),
                sub(b"INDX", &u32s(&[0])),
                sub(b"HEAD", &u32s(&[0xB00])),
                sub(b"TINI", &1u16.to_le_bytes()),
                sub(b"TINT", &zs("Actors\\Mask1.dds")),
                sub(b"TINP", &1u16.to_le_bytes()),
                sub(b"TINI", &2u16.to_le_bytes()),
                sub(b"TINT", &zs("Actors\\SkinTone.dds")),
                sub(b"TINP", &6u16.to_le_bytes()),
                sub(b"TINI", &3u16.to_le_bytes()),
                sub(b"TINT", &zs("Actors\\Mask3.dds")),
                sub(b"TINP", &2u16.to_le_bytes()),
                // A mask with no texture.
                sub(b"TINI", &4u16.to_le_bytes()),
                sub(b"TINP", &3u16.to_le_bytes()),
                sub(b"FNAM", &[]),
                sub(b"TINI", &1u16.to_le_bytes()),
                sub(b"TINT", &zs("Actors\\FemaleMask1.dds")),
                sub(b"TINP", &1u16.to_le_bytes()),
            ],
        );
        let colour =
            |id: u32, rgb: [u8; 3]| rec(b"CLFM", id, &[sub(b"CNAM", &[rgb[0], rgb[1], rgb[2], 0])]);
        let face = rec(
            b"HDPT",
            0xB00,
            &[
                sub(b"EDID", &zs("TestFace")),
                sub(b"MODL", &zs(FACE_MODEL)),
                sub(b"PNAM", &u32s(&[1])),
            ],
        );
        plugin(
            &[],
            &[
                grup(b"WEAP", &[rec(b"WEAP", 0x99, &[sub(b"EDID", &zs("Axe"))])]),
                grup(b"RACE", &[race]),
                grup(b"HDPT", &[face]),
                grup(
                    b"CLFM",
                    &[colour(0xA00, [10, 20, 30]), colour(0xA01, [40, 50, 60])],
                ),
                grup(b"NPC_", &[player]),
                grup(
                    b"TXST",
                    &[rec(b"TXST", 0xC00, &[sub(b"EDID", &zs("Skin"))])],
                ),
            ],
        )
    }

    /// Mod.esp: the hair colour and the hair the save names.
    fn mod_esp() -> Vec<u8> {
        plugin(
            &["Skyrim.esm"],
            &[
                grup(
                    b"CLFM",
                    &[rec(
                        b"CLFM",
                        0x0100_0800,
                        &[sub(b"CNAM", &[100, 110, 120, 0])],
                    )],
                ),
                grup(
                    b"HDPT",
                    &[rec(
                        b"HDPT",
                        0x0100_0801,
                        &[
                            sub(b"EDID", &zs("ModHair")),
                            sub(b"MODL", &zs("Actors\\ModHair.nif")),
                            sub(b"PNAM", &u32s(&[3])),
                        ],
                    )],
                ),
            ],
        )
    }

    /// A plugin the save lists that the character has nothing from.
    fn extra_esp() -> Vec<u8> {
        plugin(
            &["Skyrim.esm"],
            &[grup(
                b"HDPT",
                &[rec(b"HDPT", 0x0100_0900, &[sub(b"EDID", &zs("Other"))])],
            )],
        )
    }

    fn wstr(out: &mut Vec<u8>, s: &str) {
        out.extend((s.len() as u16).to_le_bytes());
        out.extend(s.as_bytes());
    }

    /// A Special Edition save, uncompressed: its plugins, its form id
    /// array and its change forms (refId, flags, type, data).
    fn save(
        name: &str,
        plugins: &[&str],
        form_ids: &[u32],
        forms: &[([u8; 3], u32, u8, Vec<u8>)],
    ) -> Vec<u8> {
        let mut head = Vec::new();
        head.extend(12u32.to_le_bytes());
        head.extend(7u32.to_le_bytes());
        wstr(&mut head, name);
        head.extend(3u32.to_le_bytes());
        wstr(&mut head, "Somewhere");
        wstr(&mut head, "000.01.02");
        wstr(&mut head, "TestRace");
        head.extend(0u16.to_le_bytes());
        head.extend(0f32.to_le_bytes());
        head.extend(100f32.to_le_bytes());
        head.extend(0u64.to_le_bytes());
        head.extend(2u32.to_le_bytes());
        head.extend(1u32.to_le_bytes());
        head.extend(0u16.to_le_bytes());
        let base = apoc_save::MAGIC.len() + 4 + head.len() + 8;
        let mut body = vec![78u8];
        let mut pl = vec![plugins.len() as u8];
        for p in plugins {
            wstr(&mut pl, p);
        }
        pl.extend(0u16.to_le_bytes());
        body.extend((pl.len() as u32).to_le_bytes());
        body.extend(pl);
        let table = body.len();
        body.extend([0u8; 100]);
        let forms_at = body.len();
        for (r, flags, kind, data) in forms {
            body.extend(r);
            body.extend(flags.to_le_bytes());
            body.push(0x40 | kind);
            body.push(78);
            body.extend((data.len() as u16).to_le_bytes());
            body.extend(0u16.to_le_bytes());
            body.extend(data);
        }
        let ids_at = body.len();
        body.extend((form_ids.len() as u32).to_le_bytes());
        body.extend(u32s(form_ids));
        let mut put = |i: usize, v: u32| {
            body[table + 4 * i..table + 4 * i + 4].copy_from_slice(&v.to_le_bytes())
        };
        put(0, (base + ids_at) as u32);
        put(4, (base + forms_at) as u32);
        put(9, forms.len() as u32);
        let mut out = apoc_save::MAGIC.to_vec();
        out.extend((head.len() as u32).to_le_bytes());
        out.extend(head);
        out.extend([0u8; 8]);
        out.extend(body);
        out
    }

    const RACE_FLAG: u32 = 1 << 25;
    const FACE_FLAG: u32 = 1 << 11;
    const SKIN_REF: [u8; 3] = [0x40, 0x0C, 0x00];

    /// The player's look: TestRace (Skyrim.esm's, by its own refId), Mod.esp's
    /// hair colour and hair (through the form id array), skin tone 51, 102,
    /// 153, and tint layers: the skin tone layer (2), mask 1 at 0.5, mask 3 at
    /// nothing, mask 4 (no texture) and mask 9 (not the race's).
    fn player_save() -> Vec<u8> {
        let mut npc = vec![0x41, 0x37, 0x46, 0x41, 0x37, 0x46];
        npc.push(1);
        npc.extend([0, 0, 1]);
        npc.extend([51, 102, 153, 0]);
        npc.extend(SKIN_REF);
        npc.push(1 << 2);
        npc.extend([0, 0, 2]);
        npc.push(1);
        npc.extend(2u32.to_le_bytes());
        npc.extend(0.5f32.to_le_bytes());
        npc.extend((-0.25f32).to_le_bytes());
        npc.extend(4u32.to_le_bytes());
        for p in [1i32, -1, 2, 3] {
            npc.extend(p.to_le_bytes());
        }
        let tints: [(u16, [u8; 4], f32); 5] = [
            (2, [255, 0, 0, 255], 0.5),
            (1, [0, 255, 0, 255], 0.5),
            (3, [0, 0, 255, 255], 0.0),
            (4, [9, 9, 9, 255], 1.0),
            (9, [9, 9, 9, 255], 1.0),
        ];
        let mut achr = vec![0xAA; 40];
        achr.extend([0x41, 0x37, 0x46, 0x41, 0x37, 0x46, 0x41, 0x37, 0x46]);
        achr.extend(0u32.to_le_bytes());
        achr.extend((tints.len() as u32).to_le_bytes());
        for (m, c, s) in tints {
            achr.extend(m.to_le_bytes());
            achr.extend(c);
            achr.extend(s.to_le_bytes());
        }
        achr.extend(0u32.to_le_bytes());
        achr.extend(SKIN_REF);
        achr.extend([0; 20]);
        save(
            "  Someone  ",
            &["Skyrim.esm", "Mod.esp", "Extra.esp"],
            &[0x0100_0800, 0x0100_0801],
            &[
                (
                    [0x40, 0, 7],
                    RACE_FLAG | FACE_FLAG,
                    apoc_save::KIND_NPC,
                    npc,
                ),
                ([0x40, 0, 0x14], 0, apoc_save::KIND_ACHR, achr),
            ],
        )
    }

    fn plugins() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("Skyrim.esm", skyrim()),
            ("Mod.esp", mod_esp()),
            ("Extra.esp", extra_esp()),
        ]
    }

    /// The pieces `wanted` asks for, as the interface answers them: from the
    /// plugins `have` holds, and as not had for any other.
    fn answer(wanted: &[Value], have: &[(&str, Vec<u8>)]) -> (Vec<Piece>, Vec<u8>) {
        let mut data = Vec::new();
        let pieces = wanted
            .iter()
            .map(|w| {
                let name = w["plugin"].as_str().unwrap().to_string();
                let at = w["at"].as_u64().unwrap();
                let length = w["length"].as_u64().unwrap() as usize;
                match have.iter().find(|(n, _)| n.eq_ignore_ascii_case(&name)) {
                    Some((_, bytes)) => {
                        let end = (at as usize + length).min(bytes.len());
                        data.extend(&bytes[at as usize..end]);
                        Piece {
                            plugin: name,
                            at,
                            length: Some(end - at as usize),
                            size: Some(bytes.len() as u64),
                        }
                    }
                    None => Piece {
                        plugin: name,
                        at,
                        length: None,
                        size: None,
                    },
                }
            })
            .collect();
        (pieces, data)
    }

    /// Every plugin `have` holds walked to its end.
    fn read_all(m: &mut Making, have: &[(&str, Vec<u8>)]) {
        for _ in 0..64 {
            let wanted = m.pieces();
            if wanted.is_empty() {
                return;
            }
            let (pieces, data) = answer(&wanted, have);
            m.feed(&pieces, &data).unwrap();
        }
        panic!("the walk never ended");
    }

    fn piece(plugin: &str, at: u64, length: Option<usize>, size: Option<u64>) -> Piece {
        Piece {
            plugin: plugin.into(),
            at,
            length,
            size,
        }
    }

    #[test]
    fn a_batch_is_checked_whole_before_any_of_it_is_taken() {
        let mut m = Making::new(&player_save()).unwrap();
        let first = m.pieces();
        assert_eq!(
            first,
            ["Skyrim.esm", "Mod.esp", "Extra.esp"]
                .map(|p| json!({ "plugin": p, "at": 0, "length": 24 }))
        );
        let sky = skyrim();
        let head = &sky[..24];
        let size = Some(sky.len() as u64);
        // Each batch's first piece is good; what follows it is not.
        let bad: [(Vec<Piece>, Vec<u8>, &str); 3] = [
            (
                vec![
                    piece("Skyrim.esm", 0, Some(24), size),
                    piece("Gone.esp", 0, None, None),
                ],
                head.to_vec(),
                "Gone.esp is not one of the save's plugins.",
            ),
            (
                vec![
                    piece("Skyrim.esm", 0, Some(24), size),
                    piece("SKYRIM.ESM", 0, Some(24), size),
                ],
                [head, head].concat(),
                "SKYRIM.ESM came twice in one batch.",
            ),
            (
                vec![
                    piece("Skyrim.esm", 0, Some(24), size),
                    piece("Mod.esp", 0, Some(24), Some(100)),
                ],
                [head, &head[..10]].concat(),
                "The pieces are longer than what came with them.",
            ),
        ];
        for (pieces, data, why) in bad {
            assert_eq!(m.feed(&pieces, &data).err().as_deref(), Some(why));
            // Nothing was taken: every plugin still wants its first piece.
            assert_eq!(m.pieces(), first, "{why}");
        }
        // The same first piece alone is taken, under any case of its name.
        m.feed(&[piece("skyrim.ESM", 0, Some(24), size)], head)
            .unwrap();
        let now = m.pieces();
        assert_eq!(now.len(), 3);
        assert_eq!(now[0]["plugin"], "Skyrim.esm");
        assert_eq!(now[0]["at"], 24);
        assert_eq!(&now[1..], &first[1..]);
    }

    #[test]
    fn missing_plugins_are_only_those_the_looks_forms_are_in() {
        let all = plugins();
        // Mod.esp and Extra.esp are not had.
        let mut m = Making::new(&player_save()).unwrap();
        read_all(&mut m, &all[..1]);
        assert_eq!(
            m.missing_plugins(),
            [("Mod.esp".to_string(), "not installed".to_string())]
        );
        // Only Extra.esp not had: nothing the look needs is missing.
        let mut m = Making::new(&player_save()).unwrap();
        read_all(&mut m, &all[..2]);
        assert!(m.missing_plugins().is_empty());
        // A first piece without the plugin's size is a plugin given up on.
        let mut m = Making::new(&player_save()).unwrap();
        let mod_bytes = mod_esp();
        m.feed(&[piece("Mod.esp", 0, Some(24), None)], &mod_bytes[..24])
            .unwrap();
        assert_eq!(
            m.missing_plugins(),
            [("Mod.esp".to_string(), "its size was not given".to_string())]
        );
        // Every one had: none missing, and the files are named.
        let mut m = Making::new(&player_save()).unwrap();
        read_all(&mut m, &all);
        assert!(m.missing_plugins().is_empty());
        let files = m.files_needed().unwrap();
        assert_eq!(files, ["meshes/actors/modhair.nif", FACE_KEY]);
        // Read once, the plugins take no more.
        assert_eq!(
            m.feed(&[], &[]).err().as_deref(),
            Some("The save's plugins have been read already.")
        );
    }

    #[test]
    fn no_files_are_named_until_every_plugin_is_read_or_given_up() {
        let mut m = Making::new(&player_save()).unwrap();
        let all = plugins();
        let (pieces, data) = answer(&m.pieces(), &all);
        m.feed(&pieces, &data).unwrap();
        assert_eq!(m.files_needed().unwrap(), Vec::<String>::new());
        assert!(m.records.is_none());
        read_all(&mut m, &all);
        assert!(!m.files_needed().unwrap().is_empty());
    }

    #[test]
    fn a_hair_colour_from_a_plugin_not_there_is_the_races_for_the_sex() {
        let all = plugins();
        let mut m = Making::new(&player_save()).unwrap();
        read_all(&mut m, &all);
        m.files_needed().unwrap();
        let records = m.records.take().unwrap();
        let look = m.look(&records, None);
        assert_eq!(look.hair_colour, Some(FormRef::new("Mod.esp", 0x800)));
        let p = apoc_character::plan(&look, &records).unwrap();
        assert_eq!(
            p.hair_colour,
            Some([200.0, 220.0, 240.0].map(|v| v / 255.0))
        );

        let mut m = Making::new(&player_save()).unwrap();
        read_all(&mut m, &[all[0].clone(), all[2].clone()]);
        m.files_needed().unwrap();
        let records = m.records.take().unwrap();
        let look = m.look(&records, None);
        assert_eq!(look.hair_colour, Some(FormRef::new("Skyrim.esm", 0xA00)));
        // A woman's is the race's woman's.
        m.look.sex = 1;
        let look = m.look(&records, None);
        assert_eq!(look.hair_colour, Some(FormRef::new("Skyrim.esm", 0xA01)));
        let p = apoc_character::plan(&look, &records).unwrap();
        assert_eq!(p.hair_colour, Some([80.0, 100.0, 120.0].map(|v| v / 255.0)));
    }

    #[test]
    fn the_look_is_the_saves_with_the_players_record_for_what_a_save_lacks() {
        let all = plugins();
        let mut m = Making::new(&player_save()).unwrap();
        read_all(&mut m, &all);
        m.files_needed().unwrap();
        let records = m.records.take().unwrap();
        let look = m.look(&records, None);
        assert_eq!(look.race, Some(FormRef::new("Skyrim.esm", 0x13746)));
        assert!(!look.female);
        assert_eq!(look.head_parts, [FormRef::new("Mod.esp", 0x801)]);
        assert_eq!(look.face_morphs, [0.5, -0.25]);
        assert_eq!(look.face_parts, [1, -1, 2, 3]);
        assert_eq!(look.face_texture, Some(FormRef::new("Skyrim.esm", 0xC00)));
        assert_eq!(look.skin_tone, Some([0.2, 0.4, 0.6]));
        assert_eq!(look.tints.len(), 5);
        // The weight: the player record's, unless one is given.
        assert_eq!(look.weight, 40.0);
        assert_eq!(m.look(&records, Some(75.0)).weight, 75.0);
        assert_eq!(m.look(&records, Some(300.0)).weight, 100.0);
        // A save whose NPC change form names no race takes the player's.
        m.look.race = None;
        assert_eq!(
            m.look(&records, None).race,
            Some(FormRef::new("Skyrim.esm", 0x13746))
        );
    }

    #[test]
    fn the_face_tint_is_the_layers_a_mask_paints_over_the_skin_tone_layer() {
        let all = plugins();
        let mut m = Making::new(&player_save()).unwrap();
        read_all(&mut m, &all);
        m.files_needed().unwrap();
        let records = m.records.take().unwrap();
        let look = m.look(&records, None);
        let t = face_tint(&look, &records, "Face").unwrap();
        assert_eq!(t.shape, "Face");
        // Mid grey halfway to the skin tone layer's red.
        let grey = 127.0 / 255.0;
        let base = [grey + (1.0 - grey) * 0.5, grey * 0.5, grey * 0.5];
        assert!(
            (0..3).all(|c| (t.base[c] - base[c]).abs() < 1e-6),
            "{:?}",
            t.base
        );
        // Only mask 1's layer: the skin tone layer, a layer of no strength,
        // a mask with no texture and one the race lacks are left out.
        assert_eq!(
            t.layers,
            [Tint {
                mask: 1,
                colour: [0, 255, 0, 255],
                strength: 0.5
            }]
        );
        assert_eq!(t.masks, [(1, "Actors\\Mask1.dds".to_string())]);
        assert_eq!(
            t.json(),
            json!({
                "shape": "Face",
                "base": t.base,
                "masks": { "1": "Actors\\Mask1.dds" },
                "layers": [{ "mask": 1, "colour": [0.0, 1.0, 0.0], "strength": 0.5 }],
            })
        );

        // Without a skin tone layer the start is the skin tone.
        let mut no_tone = look.clone();
        no_tone.tints.retain(|t| t.mask != 2);
        let t = face_tint(&no_tone, &records, "Face").unwrap();
        assert_eq!(t.base, [0.2, 0.4, 0.6]);
        // A woman's masks are her race's woman's.
        let mut her = look.clone();
        her.female = true;
        let t = face_tint(&her, &records, "Face").unwrap();
        assert_eq!(t.masks, [(1, "Actors\\FemaleMask1.dds".to_string())]);
        assert_eq!(t.base, [0.2, 0.4, 0.6]);
        // No layer left: no tint.
        let mut none = look.clone();
        none.tints.retain(|t| t.mask != 1);
        assert_eq!(face_tint(&none, &records, "Face"), None);
        none.race = None;
        assert_eq!(face_tint(&none, &records, "Face"), None);
    }

    fn ask(state: &mut State, request: Value, data: &[u8]) -> Value {
        let a = handle(state, request.to_string().as_bytes(), data);
        let v: Value = serde_json::from_str(&a.json).unwrap();
        assert!(v.get("error").is_none(), "{request}: {v}");
        v["ok"].clone()
    }

    fn refused(state: &mut State, request: Value, data: &[u8]) -> String {
        let a = handle(state, request.to_string().as_bytes(), data);
        let v: Value = serde_json::from_str(&a.json).unwrap();
        v["error"]
            .as_str()
            .unwrap_or_else(|| panic!("{v}"))
            .to_string()
    }

    /// The pieces asked for, as the request's JSON and bytes.
    fn pieces_request(wanted: &Value, have: &[(&str, Vec<u8>)]) -> (Value, Vec<u8>) {
        let (pieces, data) = answer(wanted.as_array().unwrap(), have);
        let pieces: Vec<Value> = pieces
            .iter()
            .map(|p| json!({ "plugin": p.plugin, "at": p.at, "length": p.length, "size": p.size }))
            .collect();
        (json!({ "op": "characterPieces", "pieces": pieces }), data)
    }

    #[test]
    fn the_requests_read_a_save_step_by_step_and_open_its_character() {
        let mut state = State::default();
        for op in ["characterPieces", "characterFile", "character"] {
            let request = json!({ "op": op, "pieces": [], "path": "a" });
            assert_eq!(refused(&mut state, request, &[]), "No save is being read.");
        }
        let began = ask(&mut state, json!({ "op": "characterSave" }), &player_save());
        assert_eq!(began["name"], "  Someone  ");
        assert_eq!(began["files"], json!([]));
        assert_eq!(began["pieces"].as_array().unwrap().len(), 3);

        // Mod.esp is not had.
        let have = [plugins()[0].clone(), plugins()[2].clone()];
        let mut wanted = began["pieces"].clone();
        let mut last = Value::Null;
        for _ in 0..64 {
            if wanted.as_array().unwrap().is_empty() {
                break;
            }
            let (request, data) = pieces_request(&wanted, &have);
            last = ask(&mut state, request, &data);
            wanted = last["pieces"].clone();
        }
        assert_eq!(last["pieces"], json!([]));
        // The face only: the hair is Mod.esp's.
        assert_eq!(last["files"], json!([FACE_KEY]));
        assert_eq!(
            last["missingPlugins"],
            json!([{ "plugin": "Mod.esp", "why": "not installed" }])
        );
        // Once every plugin is settled, no more pieces are taken.
        assert_eq!(
            refused(
                &mut state,
                json!({ "op": "characterPieces", "pieces": [{ "plugin": "Gone.esp", "at": 0 }] }),
                &[]
            ),
            "The save's plugins have been read already."
        );

        // Nothing to build from yet.
        assert_eq!(
            refused(&mut state, json!({ "op": "character" }), &[]),
            "none of the character's meshes could be read"
        );
        let Some(face) = sample(FACE_KEY) else { return };
        ask(
            &mut state,
            json!({ "op": "characterFile", "path": "Meshes\\Actors\\Character\\Character Assets\\MaleHead.NIF" }),
            &face,
        );
        let work = ask(&mut state, json!({ "op": "character", "weight": 20 }), &[]);
        assert_eq!(work["skinTone"], json!([0.2f32, 0.4f32, 0.6f32]));
        assert_eq!(work["changed"], true);
        assert_eq!(work["missing"], json!([]));
        assert_eq!(work["faceTint"]["shape"], "TestFace");
        assert_eq!(
            work["faceTint"]["masks"],
            json!({ "1": "Actors\\Mask1.dds" })
        );
        // The project is open and the save let go.
        assert_eq!(
            refused(&mut state, json!({ "op": "character" }), &[]),
            "No save is being read."
        );
        assert!(ask(
            &mut state,
            json!({ "op": "preview", "set": "", "work": true }),
            &[]
        )["shapes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "TestFace"));
    }

    /// A file of the unpacked game, when the samples are there.
    fn sample(key: &str) -> Option<Vec<u8>> {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/skyrim");
        std::fs::read(root.join(key)).ok()
    }

    #[test]
    fn a_save_that_is_not_one_is_refused_and_forgets_the_last() {
        let mut state = State::default();
        ask(&mut state, json!({ "op": "characterSave" }), &player_save());
        assert!(
            refused(&mut state, json!({ "op": "characterSave" }), b"not a save")
                .starts_with("The save could not be read: ")
        );
        assert_eq!(
            refused(&mut state, json!({ "op": "character" }), &[]),
            "No save is being read."
        );
    }
}
