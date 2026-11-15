//! A RaceMenu preset (`.jslot`) turned into the [`Look`] a build is given,
//! and a look written back into a preset.
//!
//! [`look_of_preset`] reads what a look holds from a preset and says, in a
//! [`PresetReport`], everything it could not use. [`apply_look`] writes a
//! look back into a preset document, touching only what differs from the
//! look the document reads as: what the look leaves as read (a value that
//! could not be read, a missing section, a key it does not own) is left
//! alone, so a look read from a preset and written straight back changes no
//! byte of it, and it says in a [`SaveReport`] what it changed.
//! [`new_document`] starts a preset for a race with no base preset.
//!
//! A preset holds no race or sex: the player picks them in game and then
//! loads the preset. Both are given here, as the race and sex to preview
//! the preset as.
//!
//! The rules, from the first sample preset (a UBE preset, 108 tint layers,
//! seven head parts from five plugins) and the plugins it names:
//!
//! - A head part is named twice: `formId`, the form's id in the load order
//!   the preset was saved with, and `formIdentifier`, `Plugin|ID`. For a
//!   regular plugin the id is the form's own (`0x1A0144EA`,
//!   `UBE_AllRace.esp|0144EA`: the plugin was 26th). For a light plugin the
//!   `formId` is `0xFE`, the plugin's light slot in 12 bits, then the form's
//!   own 12-bit id (`0xFE14DB1A`), and the identifier's id is the `formId`'s
//!   low 24 bits, slot and all (`Tullius Hair 3 SMP2.esp|14DB1A`). Both are
//!   written the same way here. [`LoadOrder`] does the arithmetic.
//! - A head part's `type` is not the record's `PNAM`: the sample's UBE face
//!   (`PNAM` 1) is type 0, its mouth (`PNAM` 0) type 1 and its "no gash"
//!   scar (`PNAM` 5) type 2, and other parts are 3, 9, 10 and 15. What it is
//!   is unknown, so it is kept as read (see [`apply_look`]).
//! - `mods` pairs each regular plugin a head part's `formId` uses with its
//!   load index; light plugins are not listed. `modNames` lists every plugin
//!   the head parts and the head texture name, each once, in byte order.
//! - `actor.hairColor` is a colour, `0x00RRGGBB` (`0x4F4A4A` in the
//!   sample), not a `CLFM`. `actor.headTexture` is a `TXST` as `Plugin|ID`.
//! - A tint layer's `color` is `0xAARRGGBB`, the alpha how strongly the
//!   layer is laid on; 98 of the 108 have alpha 0, 95 of them with colour
//!   `0xFFFFFF`, and are unused. Its `texture` is the race's tint mask
//!   (`TINT`), and its `index` is the mask's place in the race's list for
//!   the sex, not its `TINI`: of the sample's 108 layers, the 99 whose
//!   texture is one of `UBE_NordRace`'s 108 masks are each at that mask's
//!   place (`SkinTone.dds` is index 0 and `TINI` 0x18). Layers are matched to
//!   masks by texture, since the place depends on the race the preset was
//!   made with; paths compare without case, `/` and `\` alike.
//! - The skin tone is the layer whose mask is the race's skin tone mask
//!   (`TINP` 6).
//! - `morphs.default.morphs` are the game's 19 face sliders; the 19th is
//!   3.402823466385289e+38 in the sample, as an `NPC_`'s `NAM9` ends in
//!   `FF FF 7F 7F` (the largest float), and is kept as read.
//!   `morphs.default.presets` are four whole numbers, 4294967295 for none,
//!   which a look holds as -1.
//!
//! Written from preset files and the plugins they name. Not derived from
//! RaceMenu's source.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use apoc_jslot::preset::{Missing, Problem};
use apoc_jslot::{Document, Number, Object, Preset, Str, Value};
use apoc_plugin::records::{HeadPartKind, TintMask};
use apoc_plugin::FormRef;

use crate::look::{Look, Tint};
use crate::parts::MAX_PARTS;
use crate::records::Records;

/// A plugin in the load order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    /// The file's name, `UBE_AllRace.esp`.
    pub name: String,
    /// Whether it is light (an `.esl`, or flagged light): its forms share
    /// the `0xFE` index.
    pub light: bool,
}

impl Plugin {
    pub fn new(name: &str, light: bool) -> Plugin {
        Plugin {
            name: name.to_string(),
            light,
        }
    }
}

/// Where a plugin's forms are in the load order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A regular plugin's index, the top byte of its forms' ids: 0 to 0xFD.
    Full(u8),
    /// A light plugin's slot under `0xFE`: 0 to 0xFFF.
    Light(u16),
}

/// The most regular plugins: indexes 0 to 0xFD, since 0xFE is the light
/// plugins' and 0xFF the game's own made forms'.
const MAX_FULL: usize = 0xFE;
/// The most light plugins: their slot is 12 bits.
const MAX_LIGHT: usize = 0x1000;
/// A form of a light plugin has a 12-bit id.
const LIGHT_ID: u32 = 0xFFF;
const LIGHT_INDEX: u32 = 0xFE;

/// The active plugins in the order the game loads them, which is what a
/// preset's form ids are numbered by. Plugin names compare without case.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadOrder {
    plugins: Vec<Plugin>,
}

impl LoadOrder {
    /// `plugins` in load order. Regular plugins take indexes in their order
    /// and light ones slots in theirs; a plugin past the last index or slot,
    /// or named a second time, has no place.
    pub fn new(plugins: Vec<Plugin>) -> LoadOrder {
        LoadOrder { plugins }
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    /// Where the plugin named `name` is, if it is loaded.
    pub fn place(&self, name: &str) -> Option<Place> {
        let (mut full, mut light) = (0usize, 0usize);
        for p in &self.plugins {
            let here = if p.light {
                light += 1;
                (light <= MAX_LIGHT).then(|| Place::Light((light - 1) as u16))
            } else {
                full += 1;
                (full <= MAX_FULL).then(|| Place::Full((full - 1) as u8))
            };
            if p.name.eq_ignore_ascii_case(name) {
                return here;
            }
        }
        None
    }

    /// The plugin at a place.
    pub fn at(&self, place: Place) -> Option<&Plugin> {
        let (light, n) = match place {
            Place::Full(i) => (false, i as usize),
            Place::Light(s) => (true, s as usize),
        };
        let p = self.plugins.iter().filter(|p| p.light == light).nth(n)?;
        // A name loaded twice has its first place only.
        (self.place(&p.name) == Some(place)).then_some(p)
    }

    /// The form's id in this load order: the index and the form's own 24
    /// bits, or `0xFE`, the slot and the form's own 12 bits. None when the
    /// plugin is not loaded, or the form's id does not fit a light plugin.
    pub fn form_id(&self, form: &FormRef) -> Option<u32> {
        match self.place(&form.plugin)? {
            Place::Full(i) => (form.id <= 0xFF_FFFF).then(|| (u32::from(i) << 24) | form.id),
            Place::Light(s) => {
                (form.id <= LIGHT_ID).then(|| (LIGHT_INDEX << 24) | (u32::from(s) << 12) | form.id)
            }
        }
    }

    /// The form a load-order id names, with the plugin's name as this load
    /// order spells it. None when nothing is loaded at its index or slot.
    pub fn form_of_id(&self, form_id: u32) -> Option<FormRef> {
        let index = form_id >> 24;
        let (place, id) = match index {
            LIGHT_INDEX => (
                Place::Light(((form_id >> 12) & LIGHT_ID) as u16),
                form_id & LIGHT_ID,
            ),
            0xFF => return None,
            i => (Place::Full(i as u8), form_id & 0xFF_FFFF),
        };
        Some(FormRef::new(&self.at(place)?.name, id))
    }

    /// The form as a preset's `Plugin|ID`: the id is the low 24 bits of its
    /// load-order id, so a light plugin's carries its slot. None as for
    /// [`LoadOrder::form_id`].
    pub fn identifier(&self, form: &FormRef) -> Option<apoc_jslot::FormRef> {
        let id = self.form_id(form)?;
        let place = self.place(&form.plugin)?;
        Some(apoc_jslot::FormRef {
            plugin: self.at(place)?.name.clone(),
            id: id & 0xFF_FFFF,
        })
    }

    /// The form a preset's `Plugin|ID` names. For a light plugin the id is
    /// its low 12 bits, the rest being the slot it had when the preset was
    /// saved. A plugin is taken as light when it is light now, or when
    /// `form_id`, the load-order id written beside the identifier, says it
    /// was light when the preset was saved: a plugin flagged light since
    /// still has its 12-bit ids, and one that has lost the flag kept them
    /// (its identifier still carries the old slot above them).
    pub fn form_of_identifier(
        &self,
        identifier: &apoc_jslot::FormRef,
        form_id: Option<u32>,
    ) -> FormRef {
        let light = matches!(self.place(&identifier.plugin), Some(Place::Light(_)))
            || form_id.is_some_and(|i| i >> 24 == LIGHT_INDEX);
        let plugin = match self.place(&identifier.plugin).and_then(|p| self.at(p)) {
            Some(p) => p.name.as_str(),
            None => identifier.plugin.as_str(),
        };
        let id = if light {
            identifier.id & LIGHT_ID
        } else {
            identifier.id
        };
        FormRef::new(plugin, id)
    }
}

/// Something in a preset that [`look_of_preset`] could not use.
#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    /// A field that is absent or not what a preset has there
    /// ([`Preset::problems`]).
    Malformed(Missing),
    /// `headParts[at]`: its plugin is not loaded. The part is kept in the
    /// look, so it is written back as it was read, but it is not drawn.
    PartNotLoaded { at: usize, form: FormRef },
    /// `headParts[at]`: its plugin is loaded but holds no head part there.
    /// Kept and not drawn, as above.
    NotAHeadPart { at: usize, form: FormRef },
    /// `headParts[at]`: neither its identifier nor its form id names a form
    /// in this load order. Not in the look; left in the preset as read.
    PartUnknown { at: usize },
    /// `tintInfo[at]`: its texture is not one of the race's masks for the
    /// sex, or another layer already took that mask. Not in the look; left
    /// in the preset as read. `alpha` 0 is a layer not in use.
    TintNotOfRace {
        at: usize,
        texture: String,
        alpha: u8,
    },
    /// The race to preview as has no record, so no tint layer could be
    /// matched to a mask.
    NoRace(FormRef),
    /// `actor.headTexture`'s plugin is not loaded. Kept in the look, not
    /// drawn.
    HeadTextureNotLoaded(FormRef),
    /// `actor.headTexture` is not a texture set in the loaded plugins. Kept
    /// in the look, not drawn.
    NotATextureSet(FormRef),
    /// More entries in a list (`headParts`, `morphs.custom`) than are read:
    /// the first `limit` are, and the rest are left in the preset as read.
    TooMany { what: &'static str, limit: usize },
    /// Layers in use of race masks that share this `TINI`. A look names a
    /// layer's mask by its `TINI`, so it cannot say which of them a layer
    /// is: they are matched to the masks in the order of the race's list
    /// when written.
    RepeatedMask(u16),
    /// More notes than a report lists ([`MAX_NOTES`]): how many more.
    More(usize),
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Note::Malformed(m) => write!(f, "{m}."),
            Note::PartNotLoaded { at, form } => write!(
                f,
                "Head part {} (headParts[{at}]) is from {}, which isn't loaded.",
                id_text(form),
                form.plugin
            ),
            Note::NotAHeadPart { at, form } => write!(
                f,
                "{}|{} (headParts[{at}]) isn't a head part in the loaded plugins.",
                form.plugin,
                id_text(form)
            ),
            Note::PartUnknown { at } => {
                write!(f, "headParts[{at}] names no plugin in the load order.")
            }
            Note::TintNotOfRace { at, texture, .. } => write!(
                f,
                "Tint layer {texture} (tintInfo[{at}]) isn't one of this race's masks."
            ),
            Note::NoRace(r) => write!(
                f,
                "There's no race {}|{}, so no tint layer could be used.",
                r.plugin,
                id_text(r)
            ),
            Note::HeadTextureNotLoaded(t) => write!(
                f,
                "The head texture {}|{} is from a plugin that isn't loaded.",
                t.plugin,
                id_text(t)
            ),
            Note::NotATextureSet(t) => write!(
                f,
                "The head texture {}|{} isn't a texture set in the loaded plugins.",
                t.plugin,
                id_text(t)
            ),
            Note::TooMany { what, limit } => write!(
                f,
                "Only the first {limit} entries of {what} are read; the rest are left as they are."
            ),
            Note::RepeatedMask(tini) => write!(
                f,
                "The race has more than one tint mask numbered {tini:X}; their layers are matched to them in order."
            ),
            Note::More(n) => write!(f, "And {n} more problems."),
        }
    }
}

/// A form's id as a preset writes it, six upper-case hex digits.
fn id_text(f: &FormRef) -> String {
    format!("{:06X}", f.id)
}

/// What [`look_of_preset`] could not use, in the order it was found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PresetReport {
    pub notes: Vec<Note>,
}

/// One thing [`apply_look`] did to a preset, or did not do.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveNote {
    /// A value or list changed, by its path (`actor.weight`,
    /// `tintInfo[2].color`, `headParts`).
    Changed(String),
    /// An entry added: the list and what it is (`modNames`, `Hair.esp`).
    Added { list: &'static str, what: String },
    /// Something of the look's that was not written, and why.
    NotWritten { what: String, why: &'static str },
    /// More notes than a report lists ([`MAX_NOTES`]): how many more.
    More(usize),
}

impl fmt::Display for SaveNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveNote::Changed(path) => write!(f, "Changed {path}."),
            SaveNote::Added { list, what } => write!(f, "Added {what} to {list}."),
            SaveNote::NotWritten { what, why } => write!(f, "Didn't write {what}: {why}."),
            SaveNote::More(n) => write!(f, "And {n} more."),
        }
    }
}

/// What [`apply_look`] did, in the order it did it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SaveReport {
    pub notes: Vec<SaveNote>,
}

/// The most notes a report lists. A hostile preset can hold millions of
/// unreadable values; past this they are only counted, and one last note
/// says how many more there were.
pub const MAX_NOTES: usize = 200;

/// Notes up to [`MAX_NOTES`], and a count of the rest. A note is only made
/// while there is room for it.
struct Capped<T> {
    list: Vec<T>,
    more: usize,
}

impl<T> Capped<T> {
    fn new() -> Capped<T> {
        Capped {
            list: Vec::new(),
            more: 0,
        }
    }

    fn push(&mut self, note: impl FnOnce() -> T) {
        if self.list.len() < MAX_NOTES {
            self.list.push(note());
        } else {
            self.more += 1;
        }
    }

    fn done(mut self, more: impl FnOnce(usize) -> T) -> Vec<T> {
        if self.more > 0 {
            self.list.push(more(self.more));
        }
        self.list
    }
}

impl Capped<SaveNote> {
    fn changed(&mut self, path: impl Into<String>) {
        self.push(|| SaveNote::Changed(path.into()));
    }
    fn added(&mut self, list: &'static str, what: impl Into<String>) {
        self.push(|| SaveNote::Added {
            list,
            what: what.into(),
        });
    }
    fn not_written(&mut self, what: impl Into<String>, why: &'static str) {
        self.push(|| SaveNote::NotWritten {
            what: what.into(),
            why,
        });
    }
}

/// Why something read could not be kept when unreadable.
const UNREADABLE: &str = "it couldn't be read, so it is kept as read";

/// Why a look could not be written into a preset. The document is left as
/// it was.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum PresetError {
    #[error("the preset is not a JSON object")]
    NotAnObject,
    /// A key a look is written to holds something else: what it should be.
    #[error("{0} in the preset is not {1}")]
    WrongKind(&'static str, &'static str),
    /// The look has tint layers or a skin tone to write but names no race.
    #[error("the look names no race")]
    NoRace,
    /// The look has tint layers or a skin tone to write, and its race has no
    /// record.
    #[error("there's no race {0}")]
    MissingRace(String),
    /// A form to write whose plugin is not loaded, and which the preset does
    /// not already hold as it is.
    #[error("{0} is from a plugin that isn't loaded")]
    NotLoaded(String),
    /// A form whose id does not fit its light plugin (over 0xFFF).
    #[error("{0} has an id too large for a light plugin")]
    NoFormId(String),
    #[error("{0} isn't a head part")]
    NotAHeadPart(String),
    /// A tint layer of a mask the race does not have, or more layers of a
    /// `TINI` than the race has masks of it.
    #[error("the race has no tint mask {0}")]
    NoSuchMask(u16),
    /// A tint layer of a race mask with no texture, which a preset, naming
    /// layers by texture, cannot hold.
    #[error("the race's tint mask {0} has no texture")]
    MaskHasNoTexture(u16),
    /// A value that is not a number (NaN or infinite).
    #[error("{0} is not a number")]
    NotANumber(&'static str),
}

/// The most named sliders read from a preset: the sample has 59.
pub const MAX_CUSTOM: usize = 4096;

/// A look as read from a preset, with where in the preset each part of it
/// came from, so [`apply_look`] can write back only what differs.
struct Read {
    look: Look,
    notes: Vec<Note>,
    /// `actor.weight` is there but could not be read (the look has 0).
    bad_weight: bool,
    /// Face sliders and presets that are there but could not be read (the
    /// look has 0 and -1 in their places).
    bad_morphs: Vec<usize>,
    bad_presets: Vec<usize>,
    /// For each of the look's head parts, its entry's place in `headParts`.
    part_at: Vec<usize>,
    /// For each of the look's named sliders, its entry's place.
    custom_at: Vec<usize>,
    /// For each of the look's tint layers, the place of its race mask.
    tint_place: Vec<usize>,
}

/// The look a preset gives a character of `race` and sex, and what of the
/// preset could not be used.
///
/// - Head parts come from their `Plugin|ID`, or their form id when that is
///   unreadable. Parts whose plugin is not loaded, or that are not head
///   parts, are kept in the look so they are written back, and noted. The
///   first [`crate::parts::MAX_PARTS`] are read.
/// - The hair colour is [`Look::hair_rgb`]; there is no `CLFM`.
/// - Tint layers whose mask is the race's and which are in use (alpha over
///   0) are [`Look::tints`], the alpha as the strength; the skin tone layer
///   also gives [`Look::skin_tone`], mid grey laid toward its colour by its
///   strength, as a save's skin tone is made from it.
/// - A face slider or preset that cannot be read is 0 or -1 in its place,
///   noted. The first [`MAX_CUSTOM`] named sliders are read.
pub fn look_of_preset(
    doc: &Document,
    race: &FormRef,
    female: bool,
    records: &dyn Records,
    order: &LoadOrder,
) -> (Look, PresetReport) {
    let r = read(doc, Some(race), female, records, order);
    (r.look, PresetReport { notes: r.notes })
}

fn read(
    doc: &Document,
    race: Option<&FormRef>,
    female: bool,
    records: &dyn Records,
    order: &LoadOrder,
) -> Read {
    let preset = Preset::of(doc);
    let mut notes: Capped<Note> = Capped::new();
    let (problems, all) = preset.problems_up_to(MAX_NOTES);
    for m in &problems {
        notes.push(|| Note::Malformed((*m).clone()));
    }
    notes.more += all - problems.len();
    let mut out = Read {
        look: Look {
            race: race.cloned(),
            female,
            ..Look::default()
        },
        notes: Vec::new(),
        bad_weight: false,
        bad_morphs: Vec::new(),
        bad_presets: Vec::new(),
        part_at: Vec::new(),
        custom_at: Vec::new(),
        tint_place: Vec::new(),
    };
    let look = &mut out.look;
    let top = doc.root.as_object();

    if let Ok(actor) = &preset.actor {
        match &actor.weight {
            Ok(w) => look.weight = *w as f32,
            Err(m) => out.bad_weight = m.problem != Problem::Absent,
        }
        if let Ok(c) = actor.hair_color {
            look.hair_rgb = Some(rgb_of(c));
        }
        if let Ok(t) = &actor.head_texture {
            let form = order.form_of_identifier(t, None);
            if order.place(&form.plugin).is_none() {
                notes.push(|| Note::HeadTextureNotLoaded(form.clone()));
            } else if records.texture_set(&form).is_none() {
                notes.push(|| Note::NotATextureSet(form.clone()));
            }
            look.face_texture = Some(form);
        }
    }

    let parts = top
        .and_then(|o| o.get("headParts"))
        .and_then(Value::as_array)
        .unwrap_or_default();
    for (at, entry) in parts.iter().enumerate() {
        if look.head_parts.len() == MAX_PARTS {
            notes.push(|| Note::TooMany {
                what: "headParts",
                limit: MAX_PARTS,
            });
            break;
        }
        let Some(form) = part_form(entry, order) else {
            if entry.as_object().is_some() {
                notes.push(|| Note::PartUnknown { at });
            }
            continue;
        };
        if order.place(&form.plugin).is_none() {
            notes.push(|| Note::PartNotLoaded {
                at,
                form: form.clone(),
            });
        } else if records.head_part(&form).is_none() {
            notes.push(|| Note::NotAHeadPart {
                at,
                form: form.clone(),
            });
        }
        look.head_parts.push(form);
        out.part_at.push(at);
    }

    if let Ok(d) = &preset.default_morphs {
        if let Ok(m) = &d.morphs {
            look.face_morphs = m
                .iter()
                .map(|v| *v.as_ref().unwrap_or(&0.0) as f32)
                .collect();
            out.bad_morphs = (0..m.len()).filter(|&i| m[i].is_err()).collect();
        }
        if let Ok(p) = &d.presets {
            look.face_parts = p
                .iter()
                .map(|v| v.as_ref().map_or(-1, |&n| n as i32))
                .collect();
            out.bad_presets = (0..p.len()).filter(|&i| p[i].is_err()).collect();
        }
    }
    if let Ok(c) = &preset.custom_morphs {
        for (at, m) in c.iter().enumerate() {
            if look.custom_morphs.len() == MAX_CUSTOM {
                notes.push(|| Note::TooMany {
                    what: "morphs.custom",
                    limit: MAX_CUSTOM,
                });
                break;
            }
            if let Ok(m) = m {
                if let (Ok(name), Ok(value)) = (&m.name, &m.value) {
                    look.custom_morphs.push((name.clone(), *value as f32));
                    out.custom_at.push(at);
                }
            }
        }
    }

    let tints = top
        .and_then(|o| o.get("tintInfo"))
        .and_then(Value::as_array)
        .unwrap_or_default();
    match race.and_then(|r| records.race(r)) {
        None => {
            if let (Some(race), false) = (race, tints.is_empty()) {
                notes.push(|| Note::NoRace(race.clone()));
            }
        }
        Some(r) => {
            let masks = &r.sexes[female as usize].head.tints;
            let claimed = claim(tints, masks);
            let mut repeated: Vec<u16> = Vec::new();
            for (at, entry) in tints.iter().enumerate() {
                let o = entry.as_object();
                let colour = o.and_then(|o| whole_of(o.get("color")));
                let Some(m) = claimed[at] else {
                    if let Some(texture) = o.and_then(|o| text_of(o.get("texture"))) {
                        notes.push(|| Note::TintNotOfRace {
                            at,
                            texture,
                            alpha: colour.map_or(0, |c| (c >> 24) as u8),
                        });
                    }
                    continue;
                };
                let Some(c) = colour else { continue };
                let tint = tint_of(masks[m].index, c);
                if tint.strength <= 0.0 {
                    continue;
                }
                if masks[m].kind == Some(SKIN_TONE) {
                    look.skin_tone = Some(tone_of(&tint));
                }
                let tini = masks[m].index;
                if masks.iter().filter(|k| k.index == tini).count() > 1 && !repeated.contains(&tini)
                {
                    repeated.push(tini);
                    notes.push(|| Note::RepeatedMask(tini));
                }
                look.tints.push(tint);
                out.tint_place.push(m);
            }
        }
    }
    out.notes = notes.done(Note::More);
    out
}

/// `TINP` of the skin tone mask.
const SKIN_TONE: u16 = 6;

/// The skin tone a skin tone layer gives: mid grey (127) laid toward its
/// colour by its strength, the rule the interface's face tint uses (an
/// `NPC_`'s `QNAM` is its skin tone layer made so, in 1,736 of 1,737).
fn tone_of(t: &Tint) -> [f32; 3] {
    let grey = 127.0 / 255.0;
    [0, 1, 2].map(|i| grey + (f32::from(t.colour[i]) / 255.0 - grey) * t.strength)
}

/// Writes `look` into a preset, changing only what differs from the look
/// the preset reads as ([`look_of_preset`] with the look's race, sex, the
/// records and the load order). A section, entry or value the look leaves
/// as read is not touched, so a value that could not be read stays as it
/// is, a missing section is not made, and keys this does not own are left
/// alone. A value that is written but equal to the one there keeps its
/// text. On an error the document is not changed. The report lists what
/// was changed, added and not written.
///
/// - Weight, hair colour and head texture. The hair colour is
///   [`Look::hair_rgb`], else the colour of the `CLFM` [`Look::hair_colour`],
///   else left as read. A head texture taken out of the look is left as
///   read.
/// - Head parts, when they differ, are written in the look's order into the
///   places of the entries they were read from; entries that were not read
///   (naming no form, or past the limit) keep their places, and parts beyond
///   those read go after the last of them. A part the preset already holds
///   keeps its entry, `type` and other keys included, with its `formId` and
///   `formIdentifier` set from the load order; a part whose plugin is not
///   loaded keeps its entry as read. A new part takes the `type` of an
///   entry for a part of the same `PNAM` (so a hair swapped for another
///   keeps the hair's), or else its `PNAM`, which is a guess: what `type`
///   is is not known.
/// - `modNames` and `mods` are only added to: a plugin of a part or head
///   texture written that is not named is added (`modNames` in byte order,
///   `mods` before the first entry of a higher index). When head parts are
///   written, every `mods` entry of a loaded regular plugin takes its index
///   now, so it agrees with the form ids; one of a plugin not loaded is kept
///   as read, and noted when its index is now a loaded plugin's. Nothing is
///   taken out and no entry moves: the rule is from one sample.
/// - Face sliders and presets, value by value.
/// - Named sliders are set and added, never taken out: an entry the look
///   does not have is left as it is. To take a slider away, set it to 0.
/// - Tint layers are matched to the race's masks by texture as
///   [`look_of_preset`] matches them, and to the look's layers by `TINI`, in
///   the order of the race's list where masks share one. A layer of the look
///   sets its entry's colour and alpha; an entry whose layer the look has
///   left out gets alpha 0, its colour kept; a layer the preset lacks is
///   added at its mask's place, `{ color, index, texture }`. Entries not of
///   the race are left as they are.
/// - The skin tone: when [`Look::skin_tone`] differs from the one read and
///   from the one the look's own skin tone layer gives, the skin tone layer
///   is set to that colour at full strength, which gives it back exactly
///   (up to a 255th) by the same rule.
/// - With no race record, everything but the tint layers is written, and
///   the report says so; a look whose layers or skin tone differ is
///   refused.
/// - Changing the race: tint layers are named by the race's `TINI` numbers,
///   so a look's layers mean nothing under another race. To preview or save
///   a preset as another race, read it again with [`look_of_preset`] under
///   that race and make the edits on that look.
/// - Edits that are not saved are noted: a named slider taken out, a value
///   the preset has but could not be read left at its placeholder (0, or
///   -1 for a preset), and a `CLFM` set while a direct hair colour is.
///   The report lists [`MAX_NOTES`] notes at most, then how many more.
pub fn apply_look(
    doc: &mut Document,
    look: &Look,
    records: &dyn Records,
    order: &LoadOrder,
) -> Result<SaveReport, PresetError> {
    let base = read(doc, look.race.as_ref(), look.female, records, order);
    let was = &base.look;
    let mut root = doc.root.clone();
    let top = root.as_object_mut().ok_or(PresetError::NotAnObject)?;
    let mut rep: Capped<SaveNote> = Capped::new();

    // The actor.
    if !same_f32(look.weight, was.weight) {
        let actor = object_mut(top, "actor")?;
        if set_float(actor, "weight", look.weight, "the weight")? {
            rep.changed("actor.weight");
        }
    } else if base.bad_weight {
        rep.not_written("actor.weight", UNREADABLE);
    }
    if look.hair_colour != was.hair_colour && look.hair_rgb.is_some() {
        rep.not_written(
            "the hair colour record",
            "a direct hair colour is set, and it wins",
        );
    }
    if look.hair_rgb != was.hair_rgb || look.hair_colour != was.hair_colour {
        let rgb = match look.hair_rgb {
            Some(c) if c.iter().all(|v| v.is_finite()) => Some(pack_rgb(c)),
            Some(_) => return Err(PresetError::NotANumber("the hair colour")),
            None => look
                .hair_colour
                .as_ref()
                .and_then(|c| records.colour(c))
                .map(|c| u32::from_be_bytes([0, c.rgba[0], c.rgba[1], c.rgba[2]])),
        };
        match rgb {
            Some(rgb) => {
                let actor = object_mut(top, "actor")?;
                // The top byte is 0 in the sample; one that is not is kept.
                let old = whole_of(actor.get("hairColor")).unwrap_or(0);
                if set_whole(actor, "hairColor", (old & 0xFF00_0000) | rgb) {
                    rep.changed("actor.hairColor");
                }
            }
            None => rep.not_written("the hair colour", "the look has none"),
        }
    }
    // Plugins of what is written, which `modNames` and `mods` must name.
    let mut written: Vec<(u32, String)> = Vec::new();
    let mut parts_written = false;
    if look.face_texture != was.face_texture {
        match &look.face_texture {
            Some(t) => {
                let i = order.identifier(t).ok_or_else(|| not_placed(order, t))?;
                let actor = object_mut(top, "actor")?;
                if set_identifier(actor, "headTexture", &i.to_string()) {
                    rep.changed("actor.headTexture");
                }
                // Not a form id: in `modNames` only.
                written.push((u32::MAX, i.plugin));
            }
            None => rep.not_written(
                "the head texture's removal",
                "a preset's head texture is left as read",
            ),
        }
    }

    // Head parts.
    if look.head_parts != was.head_parts {
        let parts = array_mut(top, "headParts")?;
        let mut by_form: HashMap<(String, u32), VecDeque<usize>> = HashMap::new();
        let mut type_of: HashMap<u32, u32> = HashMap::new();
        for (k, &at) in base.part_at.iter().enumerate() {
            let form = &was.head_parts[k];
            by_form.entry(form_key(form)).or_default().push_back(at);
            let kind = records.head_part(form).and_then(|h| h.kind);
            let type_ = parts[at].as_object().and_then(|o| whole_of(o.get("type")));
            if let (Some(kind), Some(type_)) = (kind, type_) {
                type_of.entry(kind_number(kind)).or_insert(type_);
            }
        }
        let mut made: Vec<Value> = Vec::with_capacity(look.head_parts.len());
        for form in &look.head_parts {
            let have = by_form
                .get_mut(&form_key(form))
                .and_then(VecDeque::pop_front);
            let loaded = order.place(&form.plugin).is_some();
            let mut entry = match have {
                // Not loaded: as read.
                Some(at) if !loaded => {
                    made.push(parts[at].clone());
                    continue;
                }
                Some(at) => parts[at].clone(),
                None => {
                    if !loaded {
                        return Err(PresetError::NotLoaded(form_text(form)));
                    }
                    let kind = records
                        .head_part(form)
                        .and_then(|h| h.kind)
                        .ok_or_else(|| PresetError::NotAHeadPart(form_text(form)))?;
                    let n = kind_number(kind);
                    let mut o = Object::new();
                    let type_ = type_of.get(&n).copied().unwrap_or(n);
                    o.insert("type", Value::Number(Number::from_u64(type_.into())));
                    rep.added("headParts", form_text(form));
                    Value::Object(o)
                }
            };
            let o = entry
                .as_object_mut()
                .ok_or(PresetError::WrongKind("a head part", "an object"))?;
            let id = order.form_id(form).ok_or_else(|| not_placed(order, form))?;
            let identifier = order
                .identifier(form)
                .ok_or_else(|| not_placed(order, form))?;
            set_whole(o, "formId", id);
            set_identifier(o, "formIdentifier", &identifier.to_string());
            written.push((id, identifier.plugin));
            made.push(entry);
        }
        let read_at: HashSet<usize> = base.part_at.iter().copied().collect();
        let last = base.part_at.last().copied();
        let mut made = made.into_iter();
        let mut out: Vec<Value> = Vec::with_capacity(parts.len() + look.head_parts.len());
        for (i, e) in parts.iter().enumerate() {
            if read_at.contains(&i) {
                out.extend(made.next());
            } else {
                out.push(e.clone());
            }
            if Some(i) == last {
                out.extend(made.by_ref());
            }
        }
        out.extend(made);
        if *parts != out {
            *parts = out;
            rep.changed("headParts");
        }
        parts_written = true;
    }

    // The plugins the preset names: only added to.
    if !written.is_empty() {
        let names = array_mut(top, "modNames")?;
        let mut have: HashSet<String> = names
            .iter()
            .filter_map(|v| text_of(Some(v)))
            .map(|n| n.to_ascii_lowercase())
            .collect();
        for (_, name) in &written {
            if have.insert(name.to_ascii_lowercase()) {
                insert_name(names, name);
                rep.added("modNames", name.clone());
            }
        }
    }
    if parts_written {
        write_mods(top, &written, order, &mut rep)?;
    }

    // Face sliders and presets.
    if !same_f32s(&look.face_morphs, &was.face_morphs) {
        let morphs = object_mut(top, "morphs")?;
        let default = object_mut(morphs, "default")?;
        let sliders = array_mut(default, "morphs")?;
        let before = sliders.clone();
        for (i, &v) in look.face_morphs.iter().enumerate() {
            let same = was.face_morphs.get(i).is_some_and(|&b| same_f32(b, v));
            if same && i < sliders.len() {
                continue;
            }
            let n = float_number(v, "a face slider")?;
            match sliders.get_mut(i) {
                Some(old) => *old = n,
                None => sliders.push(n),
            }
        }
        sliders.truncate(look.face_morphs.len());
        if *sliders != before {
            rep.changed("morphs.default.morphs");
        }
    }
    // An unreadable slider or preset the look leaves at its placeholder is
    // kept: the look cannot tell an edit to it from no edit.
    for &i in &base.bad_morphs {
        if look.face_morphs.get(i).is_some_and(|&v| same_f32(v, 0.0)) {
            rep.push(|| SaveNote::NotWritten {
                what: format!("morphs.default.morphs[{i}]"),
                why: UNREADABLE,
            });
        }
    }
    for &i in &base.bad_presets {
        if look.face_parts.get(i) == Some(&-1) {
            rep.push(|| SaveNote::NotWritten {
                what: format!("morphs.default.presets[{i}]"),
                why: UNREADABLE,
            });
        }
    }
    if look.face_parts != was.face_parts {
        let morphs = object_mut(top, "morphs")?;
        let default = object_mut(morphs, "default")?;
        let presets = array_mut(default, "presets")?;
        let before = presets.clone();
        for (i, &v) in look.face_parts.iter().enumerate() {
            if was.face_parts.get(i) == Some(&v) && i < presets.len() {
                continue;
            }
            // -1 is 4294967295.
            let n = Value::Number(Number::from_u64(u64::from(v as u32)));
            match presets.get_mut(i) {
                Some(old) => *old = n,
                None => presets.push(n),
            }
        }
        presets.truncate(look.face_parts.len());
        if *presets != before {
            rep.changed("morphs.default.presets");
        }
    }

    // RaceMenu's named sliders: set and added, never taken out.
    if !same_named(&look.custom_morphs, &was.custom_morphs) {
        let morphs = object_mut(top, "morphs")?;
        let custom = array_mut(morphs, "custom")?;
        let mut wanted: HashMap<&str, VecDeque<usize>> = HashMap::new();
        for (i, (name, _)) in look.custom_morphs.iter().enumerate() {
            wanted.entry(name.as_str()).or_default().push_back(i);
        }
        let mut taken = vec![false; look.custom_morphs.len()];
        for (k, &at) in base.custom_at.iter().enumerate() {
            let (name, old) = &was.custom_morphs[k];
            let Some(i) = wanted.get_mut(name.as_str()).and_then(VecDeque::pop_front) else {
                continue;
            };
            taken[i] = true;
            let v = look.custom_morphs[i].1;
            if same_f32(v, *old) {
                continue;
            }
            if let Some(o) = custom[at].as_object_mut() {
                if set_float(o, "value", v, "a named slider")? {
                    rep.changed(format!("morphs.custom[{at}].value"));
                }
            }
        }
        for (k, &at) in base.custom_at.iter().enumerate() {
            let name = &was.custom_morphs[k].0;
            if !look.custom_morphs.iter().any(|(n, _)| n == name) {
                rep.push(|| SaveNote::NotWritten {
                    what: format!("the removal of morphs.custom[{at}] ({name})"),
                    why: "named sliders are never taken out; set it to 0 instead",
                });
            }
        }
        for (i, (name, value)) in look.custom_morphs.iter().enumerate() {
            if taken[i] {
                continue;
            }
            let mut o = Object::new();
            o.insert("name", Value::String(Str::new(name)));
            o.insert("value", float_number(*value, "a named slider")?);
            custom.push(Value::Object(o));
            rep.added("morphs.custom", name.clone());
        }
    }

    write_tints(top, look, &base, records, &mut rep)?;

    doc.root = root;
    Ok(SaveReport {
        notes: rep.done(SaveNote::More),
    })
}

/// `mods`, once head parts are written: every entry of a loaded regular
/// plugin takes its index now, so it agrees with the form ids just written,
/// and a plugin written that is not listed is added in order of index.
/// Entries are never taken out. One of a plugin that is not loaded is kept
/// as read, and noted when its index is now a loaded plugin's.
fn write_mods(
    top: &mut Object,
    written: &[(u32, String)],
    order: &LoadOrder,
    rep: &mut Capped<SaveNote>,
) -> Result<(), PresetError> {
    let mods = array_mut(top, "mods")?;
    // Places in `mods` to report once every entry is in, as (place,
    // note maker).
    let mut changed: Vec<usize> = Vec::new();
    let mut collide: Vec<(usize, String)> = Vec::new();
    let mut listed: HashSet<String> = HashSet::new();
    for (at, m) in mods.iter_mut().enumerate() {
        let Some(o) = m.as_object_mut() else { continue };
        let Some(name) = text_of(o.get("name")) else {
            continue;
        };
        match order.place(&name) {
            Some(Place::Full(i)) => {
                if set_whole(o, "index", i.into()) {
                    changed.push(at);
                }
            }
            Some(Place::Light(_)) => {}
            None => {
                let taken = whole_of(o.get("index"))
                    .and_then(|i| u8::try_from(i).ok())
                    .is_some_and(|i| order.at(Place::Full(i)).is_some());
                if taken {
                    collide.push((at, name.clone()));
                }
            }
        }
        listed.insert(name.to_ascii_lowercase());
    }
    let mut new: Vec<(u32, String)> = Vec::new();
    for (id, name) in written {
        if id >> 24 < LIGHT_INDEX && listed.insert(name.to_ascii_lowercase()) {
            new.push((id >> 24, name.clone()));
        }
    }
    for (index, name) in new {
        let mut o = Object::new();
        o.insert("index", Value::Number(Number::from_u64(index.into())));
        o.insert("name", Value::String(Str::new(&name)));
        let at = mods
            .iter()
            .position(|e| {
                e.as_object()
                    .and_then(|o| whole_of(o.get("index")))
                    .is_some_and(|i| i > index)
            })
            .unwrap_or(mods.len());
        mods.insert(at, Value::Object(o));
        shift(&mut changed, at);
        for (c, _) in &mut collide {
            if *c >= at {
                *c += 1;
            }
        }
        rep.added("mods", name);
    }
    for at in changed {
        rep.changed(format!("mods[{at}].index"));
    }
    for (at, name) in collide {
        rep.not_written(
            format!("mods[{at}] ({name})"),
            "its plugin isn't loaded and its index is now another plugin's; kept as read",
        );
    }
    Ok(())
}

/// Places in a list after an entry is inserted at `at`.
fn shift(places: &mut [usize], at: usize) {
    for p in places {
        if *p >= at {
            *p += 1;
        }
    }
}

/// The tint layers and skin tone of [`apply_look`].
fn write_tints(
    top: &mut Object,
    look: &Look,
    base: &Read,
    records: &dyn Records,
    rep: &mut Capped<SaveNote>,
) -> Result<(), PresetError> {
    let was = &base.look;
    let tone_changed = look.skin_tone.is_some() && look.skin_tone != was.skin_tone;
    let race = look.race.as_ref().and_then(|r| records.race(r));
    let Some(race) = race else {
        if look.tints != was.tints || tone_changed {
            return Err(match &look.race {
                None => PresetError::NoRace,
                Some(r) => PresetError::MissingRace(form_text(r)),
            });
        }
        rep.not_written("the tint layers", "the race has no record");
        return Ok(());
    };
    let masks = &race.sexes[look.female as usize].head.tints;

    let mut want = look.tints.clone();
    if let (true, Some(tone)) = (tone_changed, look.skin_tone) {
        if !tone.iter().all(|v| v.is_finite()) {
            return Err(PresetError::NotANumber("the skin tone"));
        }
        match masks.iter().find(|m| m.kind == Some(SKIN_TONE)) {
            None => rep.not_written("the skin tone", "the race has no skin tone mask"),
            Some(m) => {
                let own = want.iter().position(|t| t.mask == m.index);
                let gives = own.map(|i| tone_of(&want[i]));
                let near = gives.is_some_and(|g| (0..3).all(|c| (g[c] - tone[c]).abs() < 1e-6));
                if !near {
                    let [r, g, b] = tone.map(byte);
                    let layer = Tint {
                        mask: m.index,
                        colour: [r, g, b, 255],
                        strength: 1.0,
                    };
                    match own {
                        Some(i) => want[i] = layer,
                        None => want.push(layer),
                    }
                }
            }
        }
    }
    if want == was.tints {
        return Ok(());
    }

    // Each layer at a place in the race's list: by TINI, in order where
    // masks share one.
    let mut want_at: Vec<Option<&Tint>> = vec![None; masks.len()];
    for t in &want {
        let place = (0..masks.len()).find(|&p| masks[p].index == t.mask && want_at[p].is_none());
        match place {
            Some(p) => want_at[p] = Some(t),
            None => return Err(PresetError::NoSuchMask(t.mask)),
        }
    }
    let mut was_at: Vec<Option<&Tint>> = vec![None; masks.len()];
    for (k, &p) in base.tint_place.iter().enumerate() {
        was_at[p] = Some(&was.tints[k]);
    }
    let tints = array_mut(top, "tintInfo")?;
    let mut entry_of: Vec<Option<usize>> = vec![None; masks.len()];
    for (at, c) in claim(tints, masks).into_iter().enumerate() {
        if let Some(p) = c {
            entry_of[p] = Some(at);
        }
    }
    // New entries go in once every place is settled, so no place moves
    // while others are being set.
    let mut new: Vec<(usize, Value)> = Vec::new();
    let mut changed: Vec<usize> = Vec::new();
    for p in 0..masks.len() {
        if want_at[p] == was_at[p] {
            continue;
        }
        match entry_of[p] {
            Some(at) => {
                let Some(o) = tints[at].as_object_mut() else {
                    continue;
                };
                let colour = match (want_at[p], whole_of(o.get("color"))) {
                    (Some(t), _) => pack_tint(t)?,
                    (None, Some(c)) => c & 0x00FF_FFFF,
                    (None, None) => continue,
                };
                if set_whole(o, "color", colour) {
                    changed.push(at);
                }
            }
            None => {
                let Some(t) = want_at[p] else { continue };
                let Some(texture) = masks[p].mask.as_deref() else {
                    return Err(PresetError::MaskHasNoTexture(masks[p].index));
                };
                let mut o = Object::new();
                o.insert(
                    "color",
                    Value::Number(Number::from_u64(pack_tint(t)?.into())),
                );
                o.insert("index", Value::Number(Number::from_u64(p as u64)));
                o.insert("texture", Value::String(Str::new(texture)));
                new.push((p, Value::Object(o)));
            }
        }
    }
    for (p, entry) in new {
        // Before the first entry with a higher index, as the sample's are
        // in order of index.
        let at = tints
            .iter()
            .position(|e| {
                e.as_object()
                    .and_then(|o| whole_of(o.get("index")))
                    .is_some_and(|i| i as usize > p)
            })
            .unwrap_or(tints.len());
        tints.insert(at, entry);
        shift(&mut changed, at);
        rep.added("tintInfo", masks[p].mask.clone().unwrap_or_default());
    }
    // Reported at their places once the new entries are in.
    for at in changed {
        rep.changed(format!("tintInfo[{at}].color"));
    }
    Ok(())
}

/// The runtime, SKSE and signature numbers of the sample preset's
/// `version`, which a new preset takes.
const RUNTIME_VERSION: u64 = 17039392;
const SKSE_VERSION: u64 = 33685552;
const SIGNATURE: u64 = 1163086675;

/// A preset with nothing in it yet for a character of `race` and sex, to
/// [`apply_look`] a look to: format 3 with the sample's version numbers, an
/// actor of weight 0 and hair colour 0, no head parts, plugins or sliders,
/// and one unused tint layer (alpha 0, colour `0xFFFFFF`, as 95 of the
/// sample's 98 unused ones are) for each of the race's masks that has a texture, by its
/// place in the race's list.
///
/// This layout for a new preset is unverified until the owner loads one in
/// game: the sample is a preset RaceMenu wrote, and which of its keys
/// RaceMenu needs on load is not known. Sculpting, overrides, transforms,
/// face textures and body morphs are left out.
pub fn new_document(
    race: &FormRef,
    female: bool,
    records: &dyn Records,
) -> Result<Document, PresetError> {
    let r = records
        .race(race)
        .ok_or_else(|| PresetError::MissingRace(form_text(race)))?;
    let number = |n: u64| Value::Number(Number::from_u64(n));
    let mut actor = Object::new();
    actor.insert("hairColor", number(0));
    actor.insert("weight", number(0));
    let mut default = Object::new();
    default.insert("morphs", Value::Array(Vec::new()));
    default.insert("presets", Value::Array(Vec::new()));
    let mut morphs = Object::new();
    morphs.insert("custom", Value::Array(Vec::new()));
    morphs.insert("default", Value::Object(default));
    let tints = r.sexes[female as usize]
        .head
        .tints
        .iter()
        .enumerate()
        .filter_map(|(place, m)| {
            let mut o = Object::new();
            o.insert("color", number(0x00FF_FFFF));
            o.insert("index", number(place as u64));
            o.insert("texture", Value::String(Str::new(m.mask.as_deref()?)));
            Some(Value::Object(o))
        })
        .collect();
    let mut version = Object::new();
    version.insert("formatVersion", number(3));
    version.insert("runtimeVersion", number(RUNTIME_VERSION));
    version.insert("signature", number(SIGNATURE));
    version.insert("skseVersion", number(SKSE_VERSION));
    let mut top = Object::new();
    top.insert("actor", Value::Object(actor));
    top.insert("headParts", Value::Array(Vec::new()));
    top.insert("modNames", Value::Array(Vec::new()));
    top.insert("mods", Value::Array(Vec::new()));
    top.insert("morphs", Value::Object(morphs));
    top.insert("tintInfo", Value::Array(tints));
    top.insert("version", Value::Object(version));
    Ok(Document {
        root: Value::Object(top),
        bom: false,
    })
}

/// The form a head part entry names: its `Plugin|ID`, else its form id in
/// this load order.
fn part_form(entry: &Value, order: &LoadOrder) -> Option<FormRef> {
    let o = entry.as_object()?;
    let form_id = whole_of(o.get("formId"));
    let identifier = text_of(o.get("formIdentifier")).and_then(|t| apoc_jslot::FormRef::parse(&t));
    match identifier {
        Some(i) => Some(order.form_of_identifier(&i, form_id)),
        None => order.form_of_id(form_id?),
    }
}

/// For each tint entry, the place of the race's mask it is: the mask with
/// its texture, each mask taken by one entry only. An entry whose index is
/// the mask's place is taken first, then the first with the texture, so
/// entries or masks that share a texture pair up by place where they can.
fn claim(entries: &[Value], masks: &[TintMask]) -> Vec<Option<usize>> {
    let read: Vec<(Option<String>, Option<u32>)> = entries
        .iter()
        .map(|e| {
            let o = e.as_object();
            (
                o.and_then(|o| text_of(o.get("texture"))),
                o.and_then(|o| whole_of(o.get("index"))),
            )
        })
        .collect();
    let mut out: Vec<Option<usize>> = vec![None; entries.len()];
    let mut done = vec![false; masks.len()];
    for by_place in [true, false] {
        for (place, m) in masks.iter().enumerate() {
            let Some(path) = m.mask.as_deref() else {
                continue;
            };
            if done[place] {
                continue;
            }
            let at = (0..entries.len()).find(|&i| {
                out[i].is_none()
                    && read[i].0.as_deref().is_some_and(|t| same_path(t, path))
                    && (!by_place || read[i].1 == Some(place as u32))
            });
            if let Some(at) = at {
                out[at] = Some(place);
                done[place] = true;
            }
        }
    }
    out
}

/// Two texture paths are the same without case, `/` and `\` alike.
fn same_path(a: &str, b: &str) -> bool {
    let norm = |c: u8| match c {
        b'/' => b'\\',
        c => c.to_ascii_lowercase(),
    };
    a.len() == b.len() && a.bytes().zip(b.bytes()).all(|(x, y)| norm(x) == norm(y))
}

/// `0x00RRGGBB` as red, green and blue from 0 to 1.
fn rgb_of(c: u32) -> [f32; 3] {
    let [_, r, g, b] = c.to_be_bytes();
    [r, g, b].map(|v| f32::from(v) / 255.0)
}

/// Red, green and blue from 0 to 1 as `0x00RRGGBB`, each rounded to the
/// nearest of 255 steps.
fn pack_rgb(c: [f32; 3]) -> u32 {
    let [r, g, b] = c.map(byte);
    u32::from_be_bytes([0, r, g, b])
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A tint layer's `0xAARRGGBB` as a look's layer: the colour with the
/// alpha as its fourth byte, and the alpha from 0 to 1 as the strength.
fn tint_of(mask: u16, c: u32) -> Tint {
    let [a, r, g, b] = c.to_be_bytes();
    Tint {
        mask,
        colour: [r, g, b, a],
        strength: f32::from(a) / 255.0,
    }
}

/// A look's layer as `0xAARRGGBB`. The strength is the alpha; the colour's
/// fourth byte is not used.
fn pack_tint(t: &Tint) -> Result<u32, PresetError> {
    if !t.strength.is_finite() {
        return Err(PresetError::NotANumber("a tint layer's strength"));
    }
    let [r, g, b, _] = t.colour;
    Ok(u32::from_be_bytes([byte(t.strength), r, g, b]))
}

/// The head part's `PNAM` as a number.
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

/// Adds a plugin name to `modNames` before the first name that sorts after
/// it byte by byte, as the sample's are sorted; names that cannot be read
/// are passed over.
fn insert_name(names: &mut Vec<Value>, name: &str) {
    let at = names
        .iter()
        .position(|v| text_of(Some(v)).is_some_and(|n| n.as_str() > name))
        .unwrap_or(names.len());
    names.insert(at, Value::String(Str::new(name)));
}

/// A form as a key: its plugin without case, and its id.
fn form_key(f: &FormRef) -> (String, u32) {
    (f.plugin.to_ascii_lowercase(), f.id)
}

/// Two floats are the same value (or the same bits, which a NaN only is to
/// itself).
fn same_f32(a: f32, b: f32) -> bool {
    a == b || a.to_bits() == b.to_bits()
}

fn same_f32s(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_f32(*x, *y))
}

fn same_named(a: &[(String, f32)], b: &[(String, f32)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.0 == y.0 && same_f32(x.1, y.1))
}

fn form_text(f: &FormRef) -> String {
    format!("{}|{}", f.plugin, id_text(f))
}

fn not_placed(order: &LoadOrder, f: &FormRef) -> PresetError {
    if order.place(&f.plugin).is_none() {
        PresetError::NotLoaded(form_text(f))
    } else {
        PresetError::NoFormId(form_text(f))
    }
}

fn whole_of(v: Option<&Value>) -> Option<u32> {
    v?.as_number()?.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn float_of(v: Option<&Value>) -> Option<f64> {
    Some(v?.as_number()?.as_f64())
}

fn text_of(v: Option<&Value>) -> Option<String> {
    v?.as_str()?.text()
}

fn float_number(v: f32, what: &'static str) -> Result<Value, PresetError> {
    Number::from_f32(v)
        .map(Value::Number)
        .ok_or(PresetError::NotANumber(what))
}

/// Sets a whole number unless it is that already; whether it was set.
fn set_whole(o: &mut Object, key: &str, n: u32) -> bool {
    let set = whole_of(o.get(key)) != Some(n);
    if set {
        o.insert(key, Value::Number(Number::from_u64(n.into())));
    }
    set
}

/// Sets a float unless it is that already once read as a float, as
/// RaceMenu keeps it; whether it was set.
fn set_float(o: &mut Object, key: &str, v: f32, what: &'static str) -> Result<bool, PresetError> {
    let n = float_number(v, what)?;
    let set = float_of(o.get(key)).map(|d| d as f32) != Some(v);
    if set {
        o.insert(key, n);
    }
    Ok(set)
}

/// Sets a `Plugin|ID` unless it names that form already, plugin compared
/// without case; whether it was set.
fn set_identifier(o: &mut Object, key: &str, text: &str) -> bool {
    let same = |a: &str| match (
        apoc_jslot::FormRef::parse(a),
        apoc_jslot::FormRef::parse(text),
    ) {
        (Some(x), Some(y)) => x.id == y.id && x.plugin.eq_ignore_ascii_case(&y.plugin),
        _ => a == text,
    };
    let set = !text_of(o.get(key)).is_some_and(|t| same(&t));
    if set {
        o.insert(key, Value::String(Str::new(text)));
    }
    set
}

/// The object at `key`, made empty when it is not there.
fn object_mut<'a>(o: &'a mut Object, key: &'static str) -> Result<&'a mut Object, PresetError> {
    if o.get(key).is_none() {
        o.insert(key, Value::Object(Object::new()));
    }
    o.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or(PresetError::WrongKind(key, "an object"))
}

/// The list at `key`, made empty when it is not there.
fn array_mut<'a>(o: &'a mut Object, key: &'static str) -> Result<&'a mut Vec<Value>, PresetError> {
    if o.get(key).is_none() {
        o.insert(key, Value::Array(Vec::new()));
    }
    o.get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or(PresetError::WrongKind(key, "a list"))
}

#[cfg(test)]
mod tests;
