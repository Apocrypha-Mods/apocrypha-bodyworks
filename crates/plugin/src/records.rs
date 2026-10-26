//! The records a character is made from, read as far as a character needs.
//!
//! Layouts are from UESP's record pages and were checked against Skyrim.esm,
//! its DLCs and USSEP (`--example records`, `--example dump`). Where the
//! files say something UESP does not, the field's comment says so.
//!
//! Every formID is resolved against the plugin that holds the record
//! ([`Masters`]), so a record read from any plugin names the same forms.

use crate::container::{Masters, Record, Subrecord};
use crate::io::{zstring, Reader};
use crate::{FormRef, PluginError};

/// A record type this module reads.
pub trait Typed: Sized {
    const KIND: [u8; 4];
    fn read(record: &Record, names: &Masters) -> Result<Self, PluginError>;
}

/// A name (`FULL`): in a localized plugin, the id of a string in its string
/// tables, which are not read; otherwise the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Name {
    Id(u32),
    Text(String),
}

fn name(s: &Subrecord, names: &Masters) -> Result<Name, PluginError> {
    if names.localized {
        Ok(Name::Id(u32_of(s)?))
    } else {
        Ok(Name::Text(zstring(s.data)))
    }
}

fn wrong(s: &Subrecord, want: &str) -> PluginError {
    PluginError::Corrupt(format!(
        "{} of {} bytes where {want} were expected",
        String::from_utf8_lossy(&s.kind),
        s.data.len()
    ))
}

fn exact<'a>(s: &Subrecord<'a>, n: usize) -> Result<Reader<'a>, PluginError> {
    if s.data.len() != n {
        return Err(wrong(s, &n.to_string()));
    }
    Ok(Reader::new(s.data))
}

fn u32_of(s: &Subrecord) -> Result<u32, PluginError> {
    exact(s, 4)?.u32()
}

fn u16_of(s: &Subrecord) -> Result<u16, PluginError> {
    exact(s, 2)?.u16()
}

fn f32_of(s: &Subrecord) -> Result<f32, PluginError> {
    exact(s, 4)?.f32()
}

/// A formID field; `None` for 0.
fn form(s: &Subrecord, names: &Masters) -> Result<Option<FormRef>, PluginError> {
    Ok(names.form(u32_of(s)?))
}

/// A formID field that must name something; a 0 is skipped.
fn push_form(list: &mut Vec<FormRef>, s: &Subrecord, names: &Masters) -> Result<(), PluginError> {
    if let Some(f) = form(s, names)? {
        list.push(f);
    }
    Ok(())
}

/// The slots of a body template: `BOD2` (eight bytes, slots then armour
/// type) or the older `BODT` (twelve: slots, a flag byte, three unused, armour
/// type). Both hold the slots first; Skyrim.esm still has `BODT` in 1,7xx
/// ARMA records and both in none.
fn slots(s: &Subrecord) -> Result<u32, PluginError> {
    if s.data.len() < 4 {
        return Err(wrong(s, "at least 4"));
    }
    Reader::new(s.data).u32()
}

/// `RACE`: what a race gives a character of each sex.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Race {
    pub editor_id: Option<String>,
    pub name: Option<Name>,
    /// `WNAM`: the skin, an `ARMO` whose addons are the naked body.
    pub skin: Option<FormRef>,
    /// `HCLF`: the default hair colour (`CLFM`) for male and female.
    /// UESP's RACE page leaves this out; it is eight bytes, two formIDs, in
    /// every playable race.
    pub default_hair_colour: [Option<FormRef>; 2],
    /// `RNAM`: the race whose head morphs this race uses, when it has one
    /// (the vampire races name their living race).
    pub morph_race: Option<FormRef>,
    /// Male, then female.
    pub sexes: [RaceSex; 2],
}

/// What a race has for one sex.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RaceSex {
    /// `ANAM` in the first `MNAM`/`FNAM` section: the skeleton.
    pub skeleton: Option<String>,
    /// `MODL` after `NAM1`: the body's lighting model (`.egt`).
    pub body_lighting: Option<String>,
    /// `MODL` after `NAM3`: the body's physics (`.hkx`).
    pub body_physics: Option<String>,
    pub head: HeadData,
}

/// The head data of one sex: the part of a `RACE` after `NAM0`, `MNAM` (or
/// `FNAM`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeadData {
    /// `INDX`/`HEAD`: the default head parts, each with its number in the
    /// list (0, 1, 2… in every file read: the number is a position, not the
    /// part's type, which is the `HDPT`'s own `PNAM`).
    pub parts: Vec<(u32, FormRef)>,
    /// `MPAI`/`MPAV`: which of the chargen tri's preset morphs the menu
    /// offers, for nose (0), brows (1), eyes (2) and lips (3).
    pub morphs: Vec<MorphPresets>,
    /// `RPRM`/`RPRF`: the preset faces, as `NPC_` records.
    pub presets: Vec<FormRef>,
    /// `AHCM`/`AHCF`: the hair colours (`CLFM`) offered.
    pub hair_colours: Vec<FormRef>,
    /// `FTSM`/`FTSF`: the face texture sets (`TXST`) offered.
    pub face_textures: Vec<FormRef>,
    /// `DFTM`/`DFTF`.
    pub default_face_texture: Option<FormRef>,
    pub tints: Vec<TintMask>,
}

/// `MPAI` and the `MPAV` after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorphPresets {
    /// Which feature: 0 nose, 1 brows, 2 eyes, 3 lips (UESP's order, and the
    /// order of `MPAI` 0..3 in every file).
    pub index: u32,
    /// 32 bytes. The first u32 is a mask over the feature's preset morphs
    /// (Nord male nose `0x001FFF3E`: presets 1–5 and 8–20 offered); the other
    /// seven are `0xFFFFFFFF` in every vanilla race, as if unused.
    pub flags: [u32; 8],
}

/// `TINI` and what follows it, up to the next `TINI`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TintMask {
    /// `TINI`: the layer's number, which an `NPC_`'s `TINI` refers to.
    pub index: u16,
    /// `TINT`: the mask texture.
    pub mask: Option<String>,
    /// `TINP`: the kind of mask (1 lips, 2 cheeks, … 6 skin tone…).
    pub kind: Option<u16>,
    /// `TIND`: the default colour (`CLFM`).
    pub default: Option<FormRef>,
    pub presets: Vec<TintPreset>,
}

/// `TINC`, `TINV`, `TIRS`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TintPreset {
    pub colour: Option<FormRef>,
    /// `TINV`, a float here (in an `NPC_` it is an integer, a percentage).
    pub value: f32,
    /// `TIRS`: a u16, unique across the race. UESP calls it the tint's
    /// number; it is what an `NPC_`'s `TIAS` names (Ulfric's skin tone layer
    /// has `TIAS` 0x4B, the Nord male skin tone's first preset's `TIRS`).
    pub number: u16,
}

impl Typed for Race {
    const KIND: [u8; 4] = *b"RACE";

    fn read(record: &Record, names: &Masters) -> Result<Self, PluginError> {
        #[derive(PartialEq)]
        enum Part {
            Top,
            Skeleton,
            Lighting,
            Physics,
            Head,
        }
        let mut race = Race::default();
        let mut part = Part::Top;
        let mut sex = 0usize;
        let mut morph: Option<u32> = None;
        let mut index: Option<u32> = None;
        for s in record.subrecords() {
            let s = s?;
            match (&s.kind, &part) {
                (b"NAM1", _) => part = Part::Lighting,
                (b"NAM3", _) => part = Part::Physics,
                (b"NAM0", _) => part = Part::Head,
                (b"MNAM", Part::Top) | (b"FNAM", Part::Top) => {
                    part = Part::Skeleton;
                    sex = (&s.kind == b"FNAM") as usize;
                }
                (b"MNAM", _) => sex = 0,
                (b"FNAM", _) => sex = 1,
                (b"EDID", Part::Top) => race.editor_id = Some(zstring(s.data)),
                (b"FULL", Part::Top) => race.name = Some(name(&s, names)?),
                (b"WNAM", _) => race.skin = form(&s, names)?,
                (b"RNAM", _) => race.morph_race = form(&s, names)?,
                (b"HCLF", _) => {
                    let mut r = exact(&s, 8)?;
                    race.default_hair_colour = [names.form(r.u32()?), names.form(r.u32()?)];
                }
                (b"ANAM", Part::Skeleton) => race.sexes[sex].skeleton = Some(zstring(s.data)),
                (b"MODL", Part::Lighting) => race.sexes[sex].body_lighting = Some(zstring(s.data)),
                (b"MODL", Part::Physics) => race.sexes[sex].body_physics = Some(zstring(s.data)),
                (_, Part::Head) => {
                    let head = &mut race.sexes[sex].head;
                    match &s.kind {
                        b"INDX" => index = Some(u32_of(&s)?),
                        b"HEAD" => {
                            let i = index.take().unwrap_or(head.parts.len() as u32);
                            if let Some(f) = form(&s, names)? {
                                head.parts.push((i, f));
                            }
                        }
                        b"MPAI" => morph = Some(u32_of(&s)?),
                        b"MPAV" => {
                            let mut r = exact(&s, 32)?;
                            let mut flags = [0; 8];
                            for f in &mut flags {
                                *f = r.u32()?;
                            }
                            head.morphs.push(MorphPresets {
                                index: morph.take().unwrap_or(u32::MAX),
                                flags,
                            });
                        }
                        b"RPRM" | b"RPRF" => push_form(&mut head.presets, &s, names)?,
                        b"AHCM" | b"AHCF" => push_form(&mut head.hair_colours, &s, names)?,
                        b"FTSM" | b"FTSF" => push_form(&mut head.face_textures, &s, names)?,
                        b"DFTM" | b"DFTF" => head.default_face_texture = form(&s, names)?,
                        b"TINI" => head.tints.push(TintMask {
                            index: u16_of(&s)?,
                            ..TintMask::default()
                        }),
                        b"TINT" | b"TINP" | b"TIND" | b"TINC" | b"TINV" | b"TIRS" => {
                            let tint = head
                                .tints
                                .last_mut()
                                .ok_or_else(|| wrong(&s, "a TINI before"))?;
                            match &s.kind {
                                b"TINT" => tint.mask = Some(zstring(s.data)),
                                b"TINP" => tint.kind = Some(u16_of(&s)?),
                                b"TIND" => tint.default = form(&s, names)?,
                                b"TINC" => tint.presets.push(TintPreset {
                                    colour: form(&s, names)?,
                                    ..TintPreset::default()
                                }),
                                b"TINV" => {
                                    let v = f32_of(&s)?;
                                    if let Some(p) = tint.presets.last_mut() {
                                        p.value = v;
                                    }
                                }
                                _ => {
                                    let n = u16_of(&s)?;
                                    if let Some(p) = tint.presets.last_mut() {
                                        p.number = n;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Ok(race)
    }
}

/// `HDPT`: a head part.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeadPart {
    pub editor_id: Option<String>,
    pub name: Option<Name>,
    pub model: Option<String>,
    /// `DATA`, one byte: 0x1 playable, 0x2 male, 0x4 female, 0x8 an extra
    /// part, 0x10 use solid tint.
    pub flags: u8,
    /// `PNAM`.
    pub kind: Option<HeadPartKind>,
    /// `HNAM`: parts that come with this one (a hairline with a hair).
    pub extra_parts: Vec<FormRef>,
    /// `NAM0`/`NAM1` pairs.
    pub files: Vec<PartFile>,
    /// `TNAM`.
    pub texture_set: Option<FormRef>,
    /// `CNAM`: a colour (`CLFM`).
    pub colour: Option<FormRef>,
    /// `RNAM`: the races it is for, a form list (`FLST`).
    pub valid_races: Option<FormRef>,
}

pub const EXTRA_PART: u8 = 0x8;

/// `PNAM` of a head part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeadPartKind {
    Misc,
    Face,
    Eyes,
    Hair,
    FacialHair,
    Scar,
    Eyebrows,
    Other(u32),
}

impl HeadPartKind {
    pub fn of(v: u32) -> Self {
        match v {
            0 => Self::Misc,
            1 => Self::Face,
            2 => Self::Eyes,
            3 => Self::Hair,
            4 => Self::FacialHair,
            5 => Self::Scar,
            6 => Self::Eyebrows,
            v => Self::Other(v),
        }
    }
}

/// A file a head part's shape is morphed by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartFile {
    pub kind: PartFileKind,
    /// `NAM1`: a `.tri`.
    pub path: String,
}

/// `NAM0`, which says what the next `NAM1` is. UESP gives 0 as "generic
/// default?", 1 "default", 2 "chargen"; the files say what each is: on
/// `MaleHeadNord` 0 is `MaleHeadRaces.tri`, 1 `MaleHead.tri` and 2
/// `MaleHeadCustomizations.tri`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartFileKind {
    /// 0: the morphs between races.
    RaceMorph,
    /// 1: the part's own tri (expressions and phonemes).
    Tri,
    /// 2: the morphs chargen's sliders drive.
    ChargenMorph,
    Other(u32),
    /// A `NAM1` with no `NAM0` before it.
    Unstated,
}

impl Typed for HeadPart {
    const KIND: [u8; 4] = *b"HDPT";

    fn read(record: &Record, names: &Masters) -> Result<Self, PluginError> {
        let mut h = HeadPart::default();
        let mut next: Option<PartFileKind> = None;
        for s in record.subrecords() {
            let s = s?;
            match &s.kind {
                b"EDID" => h.editor_id = Some(zstring(s.data)),
                b"FULL" => h.name = Some(name(&s, names)?),
                b"MODL" => h.model = Some(zstring(s.data)),
                b"DATA" => h.flags = exact(&s, 1)?.u8()?,
                b"PNAM" => h.kind = Some(HeadPartKind::of(u32_of(&s)?)),
                b"HNAM" => push_form(&mut h.extra_parts, &s, names)?,
                b"NAM0" => {
                    next = Some(match u32_of(&s)? {
                        0 => PartFileKind::RaceMorph,
                        1 => PartFileKind::Tri,
                        2 => PartFileKind::ChargenMorph,
                        v => PartFileKind::Other(v),
                    })
                }
                b"NAM1" => h.files.push(PartFile {
                    kind: next.take().unwrap_or(PartFileKind::Unstated),
                    path: zstring(s.data),
                }),
                b"TNAM" => h.texture_set = form(&s, names)?,
                b"CNAM" => h.colour = form(&s, names)?,
                b"RNAM" => h.valid_races = form(&s, names)?,
                _ => {}
            }
        }
        Ok(h)
    }
}

/// `TXST`: a texture set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextureSet {
    pub editor_id: Option<String>,
    /// `TX00`–`TX07`: diffuse, normal, environment mask (or skin's subsurface
    /// tint), glow (or detail), height, environment, multilayer, specular
    /// (or backlight).
    pub textures: [Option<String>; 8],
    /// `DNAM`, two bytes.
    pub flags: u16,
}

impl Typed for TextureSet {
    const KIND: [u8; 4] = *b"TXST";

    fn read(record: &Record, _: &Masters) -> Result<Self, PluginError> {
        let mut t = TextureSet::default();
        for s in record.subrecords() {
            let s = s?;
            match &s.kind {
                b"EDID" => t.editor_id = Some(zstring(s.data)),
                [b'T', b'X', b'0', d @ b'0'..=b'7'] => {
                    t.textures[(d - b'0') as usize] = Some(zstring(s.data))
                }
                b"DNAM" => t.flags = u16_of(&s)?,
                _ => {}
            }
        }
        Ok(t)
    }
}

/// `ARMO`: armour, and the skins races wear.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Armor {
    pub editor_id: Option<String>,
    pub name: Option<Name>,
    /// `BOD2` (or `BODT`): the biped slots, bit *n* for slot 30 + *n*.
    pub slots: u32,
    /// `MODL` formIDs: the armour addons (`ARMA`), in order.
    pub addons: Vec<FormRef>,
    /// `RNAM`.
    pub race: Option<FormRef>,
}

impl Typed for Armor {
    const KIND: [u8; 4] = *b"ARMO";

    fn read(record: &Record, names: &Masters) -> Result<Self, PluginError> {
        let mut a = Armor::default();
        for s in record.subrecords() {
            let s = s?;
            match &s.kind {
                b"EDID" => a.editor_id = Some(zstring(s.data)),
                b"FULL" => a.name = Some(name(&s, names)?),
                b"BOD2" | b"BODT" => a.slots = slots(&s)?,
                b"MODL" => push_form(&mut a.addons, &s, names)?,
                b"RNAM" => a.race = form(&s, names)?,
                _ => {}
            }
        }
        Ok(a)
    }
}

/// `ARMA`: an armour addon, the models one piece has for some races.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArmorAddon {
    pub editor_id: Option<String>,
    pub slots: u32,
    /// `RNAM`.
    pub race: Option<FormRef>,
    /// `MODL` formIDs: the other races it is for.
    pub additional_races: Vec<FormRef>,
    /// `DNAM`'s first two bytes: male and female priority.
    pub priority: [u8; 2],
    /// `MOD2` male, `MOD3` female, `MOD4` male first person, `MOD5` female
    /// first person. Without `MOD3`, `MOD2` is for both (UESP).
    pub models: [Option<String>; 4],
    /// `NAM0`–`NAM3`: the skin texture sets in the same order.
    pub skin_textures: [Option<FormRef>; 4],
}

impl ArmorAddon {
    /// The third-person model for one sex (0 male, 1 female).
    pub fn model(&self, female: bool) -> Option<&str> {
        let own = if female {
            &self.models[1]
        } else {
            &self.models[0]
        };
        own.as_deref().or(self.models[0].as_deref())
    }
}

impl Typed for ArmorAddon {
    const KIND: [u8; 4] = *b"ARMA";

    fn read(record: &Record, names: &Masters) -> Result<Self, PluginError> {
        let mut a = ArmorAddon::default();
        for s in record.subrecords() {
            let s = s?;
            match &s.kind {
                b"EDID" => a.editor_id = Some(zstring(s.data)),
                b"BOD2" | b"BODT" => a.slots = slots(&s)?,
                b"RNAM" => a.race = form(&s, names)?,
                b"MODL" => push_form(&mut a.additional_races, &s, names)?,
                b"DNAM" => {
                    let mut r = exact(&s, 12)?;
                    a.priority = [r.u8()?, r.u8()?];
                }
                [b'M', b'O', b'D', d @ b'2'..=b'5'] => {
                    a.models[(d - b'2') as usize] = Some(zstring(s.data))
                }
                [b'N', b'A', b'M', d @ b'0'..=b'3'] => {
                    a.skin_textures[(d - b'0') as usize] = form(&s, names)?
                }
                _ => {}
            }
        }
        Ok(a)
    }
}

/// `CLFM`: a colour.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Colour {
    pub editor_id: Option<String>,
    /// `CNAM`: red, green, blue and a fourth byte (0 in Skyrim.esm).
    pub rgba: [u8; 4],
    /// `FNAM`: 1 playable (it is offered in chargen).
    pub flags: u32,
}

impl Typed for Colour {
    const KIND: [u8; 4] = *b"CLFM";

    fn read(record: &Record, _: &Masters) -> Result<Self, PluginError> {
        let mut c = Colour::default();
        for s in record.subrecords() {
            let s = s?;
            match &s.kind {
                b"EDID" => c.editor_id = Some(zstring(s.data)),
                b"CNAM" => {
                    let b = exact(&s, 4)?.take(4)?;
                    c.rgba = [b[0], b[1], b[2], b[3]];
                }
                b"FNAM" => c.flags = u32_of(&s)?,
                _ => {}
            }
        }
        Ok(c)
    }
}

/// `NPC_`: a character, as far as its looks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Npc {
    pub editor_id: Option<String>,
    pub name: Option<Name>,
    /// `ACBS` flags (0x1 female, 0x4 a chargen face preset…).
    pub flags: u32,
    /// `ACBS`: which parts come from the template (0x1 traits, which takes
    /// in race, sex, height, weight and the face).
    pub template_flags: u16,
    /// `TPLT`: an `NPC_` or a leveled list (`LVLN`).
    pub template: Option<FormRef>,
    /// `RNAM`.
    pub race: Option<FormRef>,
    /// `PNAM`, each a `HDPT`.
    pub head_parts: Vec<FormRef>,
    /// `HCLF`.
    pub hair_colour: Option<FormRef>,
    /// `FTST`.
    pub face_texture: Option<FormRef>,
    /// `QNAM`: red, green, blue, 0 to 1.
    pub skin_tone: Option<[f32; 3]>,
    /// `NAM9`: 18 sliders (UESP gives their order), then four bytes.
    pub face_morphs: Option<FaceMorphs>,
    /// `NAMA`.
    pub face_parts: Option<FaceParts>,
    /// `NAM6`, `NAM7`.
    pub height: Option<f32>,
    pub weight: Option<f32>,
    /// `WNAM`: a skin worn in place of the race's.
    pub skin: Option<FormRef>,
    pub tints: Vec<NpcTint>,
}

pub const FEMALE: u32 = 0x1;
/// Template flag: traits, which take in the face.
pub const USE_TRAITS: u16 = 0x1;

impl Npc {
    pub fn female(&self) -> bool {
        self.flags & FEMALE != 0
    }

    /// Its race, sex and face come from its template.
    pub fn face_from_template(&self) -> bool {
        self.template.is_some() && self.template_flags & USE_TRAITS != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceMorphs {
    pub values: [f32; 18],
    /// The four bytes after them: 0 or `FF FF 7F 7F` (UESP).
    pub tail: u32,
}

/// `NAMA`: the preset morph chosen for each feature, -1 for none. UESP calls
/// the second unknown; it is -1 in every humanoid NPC_ read (brows are not
/// chosen this way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceParts {
    pub nose: i32,
    pub unknown: i32,
    pub eyes: i32,
    pub mouth: i32,
}

/// `TINI`, `TINC`, `TINV`, `TIAS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NpcTint {
    /// The race's tint layer (its `TINI`).
    pub index: u16,
    pub rgba: [u8; 4],
    /// Percent.
    pub value: i32,
    /// UESP leaves this unknown: it is the `TIRS` of the race preset the
    /// layer was set from.
    pub preset: i16,
}

impl Typed for Npc {
    const KIND: [u8; 4] = *b"NPC_";

    fn read(record: &Record, names: &Masters) -> Result<Self, PluginError> {
        let mut n = Npc::default();
        for s in record.subrecords() {
            let s = s?;
            match &s.kind {
                b"EDID" => n.editor_id = Some(zstring(s.data)),
                b"FULL" => n.name = Some(name(&s, names)?),
                b"ACBS" => {
                    let mut r = exact(&s, 24)?;
                    n.flags = r.u32()?;
                    r.take(14)?;
                    n.template_flags = r.u16()?;
                }
                b"TPLT" => n.template = form(&s, names)?,
                b"RNAM" => n.race = form(&s, names)?,
                b"PNAM" => push_form(&mut n.head_parts, &s, names)?,
                b"HCLF" => n.hair_colour = form(&s, names)?,
                b"FTST" => n.face_texture = form(&s, names)?,
                b"QNAM" => {
                    let mut r = exact(&s, 12)?;
                    n.skin_tone = Some([r.f32()?, r.f32()?, r.f32()?]);
                }
                b"NAM9" => {
                    let mut r = exact(&s, 76)?;
                    let mut values = [0.0; 18];
                    for v in &mut values {
                        *v = r.f32()?;
                    }
                    n.face_morphs = Some(FaceMorphs {
                        values,
                        tail: r.u32()?,
                    });
                }
                b"NAMA" => {
                    let mut r = exact(&s, 16)?;
                    n.face_parts = Some(FaceParts {
                        nose: r.i32()?,
                        unknown: r.i32()?,
                        eyes: r.i32()?,
                        mouth: r.i32()?,
                    });
                }
                b"NAM6" => n.height = Some(f32_of(&s)?),
                b"NAM7" => n.weight = Some(f32_of(&s)?),
                b"WNAM" => n.skin = form(&s, names)?,
                b"TINI" => n.tints.push(NpcTint {
                    index: u16_of(&s)?,
                    ..NpcTint::default()
                }),
                b"TINC" | b"TINV" | b"TIAS" => {
                    let t = n
                        .tints
                        .last_mut()
                        .ok_or_else(|| wrong(&s, "a TINI before"))?;
                    match &s.kind {
                        b"TINC" => {
                            let b = exact(&s, 4)?.take(4)?;
                            t.rgba = [b[0], b[1], b[2], b[3]];
                        }
                        b"TINV" => t.value = exact(&s, 4)?.i32()?,
                        _ => t.preset = exact(&s, 2)?.i16()?,
                    }
                }
                _ => {}
            }
        }
        Ok(n)
    }
}
