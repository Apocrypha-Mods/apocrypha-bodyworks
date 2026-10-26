//! A save's character, read in steps: the save, then the pieces of the
//! plugins its records are in, then the files its parts are made of. Each
//! step's answer says what the next needs, for the interface to fetch from
//! the mod manager: the plugins are far too large to hand over whole
//! (Skyrim.esm is 250 MB), and only the groups of the records a character
//! is made from are read, about 2% of it.

use apoc_character::{Files, Look, Tint};
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
            let Some(length) = piece.length else {
                self.readings[at] = Reading::Missing("not installed".into());
                continue;
            };
            let bytes = from
                .checked_add(length)
                .and_then(|to| data.get(from..to))
                .ok_or("The pieces are longer than what came with them.")?;
            from += length;
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

    /// The plugins that could not be read, each with why.
    pub fn missing_plugins(&self) -> Vec<(String, String)> {
        self.order
            .iter()
            .zip(&self.readings)
            .filter_map(|(name, r)| match r {
                Reading::Missing(why) => Some((name.clone(), why.clone())),
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

    /// The character, built from the files given, as a mesh's bytes, with
    /// what was not there or left out and why.
    pub fn build(&self, weight: Option<f32>) -> Result<(Vec<u8>, Vec<String>), String> {
        let records = self
            .records
            .as_ref()
            .ok_or("The save's plugins have not been read.")?;
        let look = self.look(records, weight);
        let plan = apoc_character::plan(&look, records).map_err(|e| e.to_string())?;
        let built = apoc_character::build(&plan, &self.files, &apoc_character::Options::default())
            .map_err(|e| e.to_string())?;
        let mut left = built.missing.clone();
        left.extend(built.notes.iter().cloned());
        Ok((built.nif.write(), left))
    }

    /// The save's look as a build takes it. A save keeps no weight (see
    /// `apoc_save::look`): `weight` when given, else the player record's.
    fn look(&self, records: &LoadOrder, weight: Option<f32>) -> Look {
        let l = &self.look;
        let first = self.order.first().map_or("Skyrim.esm", String::as_str);
        let player = apoc_character::Records::npc(records, &FormRef::new(first, 7));
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
            hair_colour: form(&l.hair_colour),
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
