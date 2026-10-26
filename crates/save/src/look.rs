//! How the player looks: the base NPC change form for the player (form
//! 0x000007) and a block near the end of the player's placed-actor change
//! form (0x000014).
//!
//! The NPC change form is laid out as UESP documents: the parts the change
//! flags say are there, in a fixed order, the face among them. Its face is
//! hair colour, a skin tone, skin, head parts, then the morphs and presets.
//!
//! Face tints are not there and UESP does not document where they are. In
//! all 88 saves here (14 characters) they are in the player's placed-actor
//! change form, a few hundred bytes from its end:
//!
//! ```text
//! refId  race the player record starts with (Skyrim.esm's NordRace)
//! refId  race now
//! refId  race now, again
//! u32    sex: 0 male, 1 female
//! u32    tint count
//!        { u16 tint mask index, u8[4] colour RGBA, f32 strength } [count]
//! u32    0 in every save here
//! refId  face texture set, the same as the NPC face's
//! ```
//!
//! Found from the files: the sex agrees with the header's in every save;
//! the race agrees with the header's race editor id (through Skyrim.esm's
//! race records, `--example saves` checks it); the texture set agrees with
//! the NPC face's; one tint's colour is the face's skin tone; every mask index
//! is one of the race's tint masks in Skyrim.esm; strengths are 0 to 1;
//! and the layers agree across all of one character's saves. What comes
//! before the block (the inventory and other data UESP marks incomplete) is
//! not read, so the block is found by that shape rather than walked to: a
//! count that, with its entries, ends exactly where the texture set refId is, with
//! the race refIds and sex before it.
//!
//! The refId UESP calls the face's "skin" lands on a `TXST` record in
//! Skyrim.esm in every save here: it is the face texture set.
//!
//! The body weight and height are not in the save, as far as 88 saves of
//! 14 characters show. The player has no change forms but these two. No
//! field of the NPC change form is left unexplained: UESP's layout accounts
//! for every byte of all 88. In the placed-actor change form, no 4-byte
//! value at a fixed distance from its start, its end or the tint block
//! (600 bytes either side) is a 0 to 100 float that holds steady across a
//! character's saves and differs between characters. Neither is one in
//! global data tables 1 to 3, by any fixed offset from an entry's start or
//! end, or as a value found anywhere in an entry, Papyrus (type 1001)
//! aside. The near misses were global variables (type 3) that mods set.
//! A second pass dropped the rule that the value differs between
//! characters and looked for Skyrim.esm's own default instead (the Player
//! record's `NAM7`, 100.0) at fixed places. It is not:
//! - in the face, whose 19th morph is the float maximum and whose two
//!   unknown bytes are 1 in all 88;
//! - in the placed-actor change form, at any fixed distance from its start
//!   or end, the race refIds or the end of the tints (1,500 bytes each
//!   way);
//! - in global data, where the only fixed places that hold 100.0 or 1.0 are
//!   type 3's global variables (refId and value pairs).
//!
//! The weight is the NPC_ record's `NAM7`, then.

use crate::io::Reader;
use crate::{Form, RefId, Save, SaveError, KIND_ACHR, KIND_NPC};

/// One face tint layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    /// Which of the race's tint masks (its `TINI` numbers) this paints.
    pub mask: u16,
    /// RGBA. The alpha is 0 or 255 in the saves here, so it may not be
    /// alpha at all.
    pub colour: [u8; 4],
    /// 0 to 1.
    pub strength: f32,
}

/// The NPC change form's face (UESP's "NPC face").
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Face {
    /// UESP's unknown first byte; 1 in every save here, and nothing
    /// follows when it is 0.
    pub present: bool,
    pub hair_colour: Option<RefId>,
    /// RGB and a fourth byte (0 in every save here).
    pub skin_tone: [u8; 4],
    /// UESP's "skin": a face texture set (`TXST`) in every save here.
    pub skin: Option<RefId>,
    pub head_parts: Vec<RefId>,
    /// The morph values (NPC_ `NAM9`; 19 in every save here).
    pub morphs: Vec<f32>,
    /// The face part presets (NPC_ `NAMA`; 4 in every save here).
    pub presets: Vec<i32>,
}

/// What the player's NPC change form says, by its flags.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NpcChange {
    pub race: Option<(RefId, RefId)>,
    pub face: Option<Face>,
    pub sex: Option<u8>,
}

/// NPC change flags (UESP's ChangeFlags) whose data comes before the face,
/// with their sizes; `None` is read by its own rule.
const FORM_FLAGS: u32 = 1 << 0;
const BASE_DATA: u32 = 1 << 1;
const AI_DATA: u32 = 1 << 3;
const SPELLS: u32 = 1 << 4;
const FULL_NAME: u32 = 1 << 5;
const FACTIONS: u32 = 1 << 6;
const SKILLS: u32 = 1 << 9;
const CLASS: u32 = 1 << 10;
const FACE: u32 = 1 << 11;
const GENDER: u32 = 1 << 24;
const RACE: u32 = 1 << 25;

/// Reads an NPC change form's data up to and including its sex.
pub fn read_npc(data: &[u8], flags: u32) -> Result<NpcChange, SaveError> {
    let mut r = Reader::new(data);
    let skip_list = |r: &mut Reader, each: usize| -> Result<(), SaveError> {
        let n = r.vsval()? as usize;
        let n = r.check_count(n, each)?;
        r.take(n * each)?;
        Ok(())
    };
    if flags & FORM_FLAGS != 0 {
        r.take(6)?;
    }
    if flags & BASE_DATA != 0 {
        r.take(24)?;
    }
    if flags & FACTIONS != 0 {
        skip_list(&mut r, 4)?;
    }
    if flags & SPELLS != 0 {
        for _ in 0..3 {
            skip_list(&mut r, 3)?;
        }
    }
    if flags & AI_DATA != 0 {
        r.take(20)?;
    }
    if flags & FULL_NAME != 0 {
        let n = r.u16()? as usize;
        r.take(n)?;
    }
    if flags & SKILLS != 0 {
        r.take(52)?;
    }
    if flags & CLASS != 0 {
        r.take(3)?;
    }
    let mut out = NpcChange::default();
    if flags & RACE != 0 {
        out.race = Some((r.ref_id()?, r.ref_id()?));
    }
    if flags & FACE != 0 {
        out.face = Some(read_face(&mut r)?);
    }
    if flags & GENDER != 0 {
        out.sex = Some(r.u8()?);
    }
    Ok(out)
}

/// The face, as UESP lays it out.
pub fn read_face(r: &mut Reader) -> Result<Face, SaveError> {
    let mut face = Face {
        present: r.u8()? != 0,
        ..Face::default()
    };
    if !face.present {
        return Ok(face);
    }
    face.hair_colour = Some(r.ref_id()?);
    let t = r.take(4)?;
    face.skin_tone = [t[0], t[1], t[2], t[3]];
    face.skin = Some(r.ref_id()?);
    let n = r.vsval()? as usize;
    let n = r.check_count(n, 3)?;
    face.head_parts.reserve(n);
    for _ in 0..n {
        face.head_parts.push(r.ref_id()?);
    }
    if r.u8()? != 0 {
        let n = r.u32()? as usize;
        let n = r.check_count(n, 4)?;
        face.morphs.reserve(n);
        for _ in 0..n {
            face.morphs.push(r.f32()?);
        }
        let n = r.u32()? as usize;
        let n = r.check_count(n, 4)?;
        face.presets.reserve(n);
        for _ in 0..n {
            face.presets.push(r.u32()? as i32);
        }
    }
    Ok(face)
}

/// The player block of the placed-actor change form (see the module).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerBlock {
    /// The race the player record starts with, the race now, and the race
    /// now again (the two differ in no save here).
    pub races: [RefId; 3],
    pub sex: u32,
    pub tints: Vec<Tint>,
    /// Where the block starts in the change form's data.
    pub offset: usize,
}

/// The most tint layers a block is looked for with. The races in
/// Skyrim.esm have at most a few dozen masks, and the saves here at most
/// 108 layers (a modded race); this bounds the search.
pub const MAX_TINTS: usize = 1024;

/// Finds the player block, given the skin refId it ends with.
pub fn find_player_block(data: &[u8], skin: RefId) -> Option<PlayerBlock> {
    let s = skin.0.to_be_bytes();
    let skin = [s[1], s[2], s[3]];
    let u32_at = |o: usize| {
        data.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let mut p = data.len().checked_sub(3)?;
    loop {
        if data[p..p + 3] == skin {
            for n in 0..=MAX_TINTS.min(p / 10) {
                let Some(o) = p.checked_sub(8 + 10 * n) else {
                    break;
                };
                if o < 13 || u32_at(o) != Some(n as u32) {
                    continue;
                }
                let sex = u32_at(o - 4)?;
                if sex > 1 {
                    continue;
                }
                let mut r = Reader::at(data, o + 4).ok()?;
                let mut tints = Vec::with_capacity(n);
                for _ in 0..n {
                    let mask = r.u16().ok()?;
                    let c = r.take(4).ok()?;
                    let strength = r.f32().ok()?;
                    tints.push(Tint {
                        mask,
                        colour: [c[0], c[1], c[2], c[3]],
                        strength,
                    });
                }
                if !tints.iter().all(|t| (0.0..=1.0).contains(&t.strength)) {
                    continue;
                }
                let at = o - 13;
                let mut r = Reader::at(data, at).ok()?;
                let races = [r.ref_id().ok()?, r.ref_id().ok()?, r.ref_id().ok()?];
                return Some(PlayerBlock {
                    races,
                    sex,
                    tints,
                    offset: at,
                });
            }
        }
        if p == 0 {
            return None;
        }
        p -= 1;
    }
}

/// How the player looks, every reference resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// The race: the NPC change form's when it has one, else the player
    /// block's. `None` when neither was found.
    pub race: Option<Form>,
    /// 0 male, 1 female: the NPC change form's, else the player block's,
    /// else the header's.
    pub sex: u8,
    /// The face's texture set: UESP calls this refId "skin", but in every
    /// save here it lands on a `TXST` record (Skyrim.esm), never an `ARMO`.
    pub face_texture_set: Form,
    pub hair_colour: Form,
    pub skin_tone: [u8; 4],
    pub head_parts: Vec<Form>,
    pub morphs: Vec<f32>,
    pub presets: Vec<i32>,
    /// Empty when the player block was not found.
    pub tints: Vec<Tint>,
    /// Always `None`: no save here holds a weight (see the module), so it
    /// comes from the player's NPC_ record (`NAM7`) instead.
    pub weight: Option<f32>,
}

pub(crate) fn player(save: &Save) -> Result<Look, SaveError> {
    let first = save
        .plugins()
        .first()
        .cloned()
        .ok_or_else(|| SaveError::Corrupt("no plugins".into()))?;
    let me = |id| {
        Form::Plugin(crate::FormRef {
            plugin: first.clone(),
            id,
        })
    };
    let npc = save
        .change_form(&me(0x7), KIND_NPC)
        .ok_or_else(|| SaveError::Corrupt("no change form for the player's NPC".into()))?;
    let npc_data = save.data(npc)?;
    let change = read_npc(&npc_data, npc.flags)?;
    let face = change
        .face
        .filter(|f| f.present)
        .ok_or_else(|| SaveError::Corrupt("the player's NPC has no face".into()))?;
    let skin = face.skin.unwrap_or(RefId(0));
    let block = match save.change_form(&me(0x14), KIND_ACHR) {
        Some(c) => find_player_block(&save.data(c)?, skin),
        None => None,
    };
    let race = match (change.race, &block) {
        (Some((r, _)), _) => Some(save.resolve(r)?),
        (None, Some(b)) => Some(save.resolve(b.races[1])?),
        _ => None,
    };
    let sex = change
        .sex
        .or(block.as_ref().map(|b| b.sex as u8))
        .unwrap_or(save.header().sex as u8);
    let head_parts = face
        .head_parts
        .iter()
        .map(|r| save.resolve(*r))
        .collect::<Result<_, _>>()?;
    Ok(Look {
        race,
        sex,
        face_texture_set: save.resolve(skin)?,
        hair_colour: save.resolve(face.hair_colour.unwrap_or(RefId(0)))?,
        skin_tone: face.skin_tone,
        head_parts,
        morphs: face.morphs,
        presets: face.presets,
        tints: block.map(|b| b.tints).unwrap_or_default(),
        weight: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_bytes() -> Vec<u8> {
        let mut b = vec![1, 0x00, 0x00, 0x05]; // present, hair colour
        b.extend([0x12, 0x34, 0x56, 0]); // skin tone
        b.extend([0x41, 0x22, 0x33]); // texture set
        b.push(2 << 2); // two head parts
        b.extend([0x40, 0x00, 0x10, 0x00, 0x00, 0x06]);
        b.push(1);
        b.extend(2u32.to_le_bytes());
        b.extend(0.5f32.to_le_bytes());
        b.extend((-0.25f32).to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend((-1i32).to_le_bytes());
        b
    }

    #[test]
    fn the_face_reads_as_uesp_lays_it_out() {
        let b = face_bytes();
        let mut r = Reader::new(&b);
        let f = read_face(&mut r).unwrap();
        assert_eq!(r.left(), 0);
        assert!(f.present);
        assert_eq!(f.hair_colour, Some(RefId(5)));
        assert_eq!(f.skin_tone, [0x12, 0x34, 0x56, 0]);
        assert_eq!(f.skin, Some(RefId(0x412233)));
        assert_eq!(f.head_parts, [RefId(0x400010), RefId(6)]);
        assert_eq!(f.morphs, [0.5, -0.25]);
        assert_eq!(f.presets, [-1]);
        // Without face data the morphs and presets are empty.
        let mut b2 = b[..18].to_vec();
        b2.push(0);
        let f = read_face(&mut Reader::new(&b2)).unwrap();
        assert!(f.morphs.is_empty() && f.presets.is_empty());
        // Absent face: one zero byte.
        assert!(!read_face(&mut Reader::new(&[0])).unwrap().present);
        for n in 1..b.len() {
            assert!(read_face(&mut Reader::new(&b[..n])).is_err(), "cut at {n}");
        }
        // A morph count larger than the data is refused before allocating.
        let mut b3 = b[..19].to_vec();
        b3.extend(u32::MAX.to_le_bytes());
        assert_eq!(
            read_face(&mut Reader::new(&b3)).err(),
            Some(SaveError::Truncated)
        );
    }

    #[test]
    fn npc_parts_are_skipped_by_their_flags() {
        let flags = FORM_FLAGS
            | BASE_DATA
            | FACTIONS
            | SPELLS
            | AI_DATA
            | FULL_NAME
            | SKILLS
            | CLASS
            | RACE
            | FACE
            | GENDER;
        let mut b = vec![0; 6 + 24];
        b.push(1 << 2); // one faction
        b.extend([0, 0, 1, 2]);
        b.extend([1 << 2, 0, 0, 1, 0, 2 << 2, 0, 0, 2, 0, 0, 3]); // spells, none, shouts
        b.extend([0; 20]);
        b.extend([3, 0, b'a', b'b', b'c']);
        b.extend([0; 52]);
        b.extend([0, 0, 9]);
        b.extend([0x41, 0x37, 0x42, 0x41, 0x37, 0x46]);
        b.extend(face_bytes());
        b.push(1);
        let n = read_npc(&b, flags).unwrap();
        assert_eq!(n.race, Some((RefId(0x413742), RefId(0x413746))));
        assert_eq!(n.sex, Some(1));
        assert_eq!(n.face.unwrap().presets, [-1]);
        // Without the race and gender flags, neither is read.
        let n = read_npc(&b, flags & !(GENDER)).unwrap();
        assert_eq!(n.sex, None);
        for cut in 0..b.len() {
            assert!(read_npc(&b[..cut], flags).is_err(), "cut at {cut}");
        }
    }

    fn block(tints: &[(u16, [u8; 4], f32)], sex: u32) -> Vec<u8> {
        let mut b = vec![0xAA; 40]; // whatever comes before
        b.extend([0x41, 0x37, 0x46, 0x41, 0x37, 0x42, 0x41, 0x37, 0x42]);
        b.extend(sex.to_le_bytes());
        b.extend((tints.len() as u32).to_le_bytes());
        for (m, c, s) in tints {
            b.extend(m.to_le_bytes());
            b.extend(c);
            b.extend(s.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes());
        b.extend([0x41, 0x22, 0x33]);
        b.extend([0; 30]);
        b
    }

    #[test]
    fn the_player_block_is_found_by_its_shape() {
        let t = [
            (0x18, [0x12, 0x34, 0x56, 0], 1.0),
            (0x11, [255, 255, 255, 0], 0.0),
        ];
        let b = block(&t, 1);
        let p = find_player_block(&b, RefId(0x412233)).unwrap();
        assert_eq!(p.offset, 40);
        assert_eq!(p.sex, 1);
        assert_eq!(p.races[1], RefId(0x413742));
        assert_eq!(p.tints.len(), 2);
        assert_eq!(
            p.tints[0],
            Tint {
                mask: 0x18,
                colour: [0x12, 0x34, 0x56, 0],
                strength: 1.0
            }
        );
        // No layers is a block too.
        assert_eq!(
            find_player_block(&block(&[], 0), RefId(0x412233))
                .unwrap()
                .tints,
            []
        );
        // A sex other than 0 or 1, or a strength out of 0 to 1, is not one.
        assert!(find_player_block(&block(&t, 2), RefId(0x412233)).is_none());
        let bad = [(1, [0; 4], 7.0)];
        assert!(find_player_block(&block(&bad, 0), RefId(0x412233)).is_none());
        // Another texture set, or too little data, finds nothing.
        assert!(find_player_block(&b, RefId(0x412234)).is_none());
        assert!(find_player_block(&[0x41, 0x93], RefId(0x412233)).is_none());
        assert!(find_player_block(&[], RefId(0x412233)).is_none());
    }
}
