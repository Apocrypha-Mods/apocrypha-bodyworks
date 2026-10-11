//! The morph file RaceMenu reads beside a built mesh (`.tri`): for each
//! shape, each slider's offsets, so the game can move the body the player
//! shapes in its menu and the outfit over it.
//!
//! Worked out from files the original program wrote. Little-endian:
//! `"PIRT"`, a count of shapes; for each a name (one byte of length) and a
//! count of morphs (counts are two bytes); for each morph a name, a multiplier, a count of
//! vertices (two bytes), and for each vertex its index and three signed
//! 16-bit numbers. An offset is those numbers times the multiplier, which is
//! chosen so the largest of them is 32767. A second list of the same shape
//! ends the file: morphs of texture coordinates, two numbers a vertex.

use crate::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Morph {
    pub name: String,
    pub multiplier: f32,
    /// Vertex, and its offset in units of the multiplier.
    pub offsets: Vec<(u16, [i16; 3])>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeMorphs {
    pub name: String,
    pub morphs: Vec<Morph>,
}

/// A move smaller than this along an axis is written as none; a vertex
/// that moves less than this along every axis is left out. Found from the
/// original's files: 0.0000995 is dropped and 0.000105 kept.
pub const LEAST: f32 = 0.0001;

impl Morph {
    /// A morph from offsets as numbers. `None` when nothing moves.
    pub fn of(name: &str, offsets: &[(u16, [f32; 3])]) -> Option<Morph> {
        let mut moved: Vec<(u16, [f32; 3])> = offsets
            .iter()
            .map(|(v, o)| (*v, o.map(|c| if c.abs() < LEAST { 0.0 } else { c })))
            .filter(|(_, o)| *o != [0.0; 3])
            .collect();
        let largest = moved
            .iter()
            .flat_map(|(_, o)| o.iter())
            .fold(0.0f32, |m, c| m.max(c.abs()));
        if largest == 0.0 {
            return None;
        }
        // In the order of the shape's vertices, as the original writes them.
        moved.sort_by_key(|o| o.0);
        let multiplier = largest / 32767.0;
        Some(Morph {
            name: name.to_string(),
            multiplier,
            offsets: moved
                .iter()
                .map(|(v, o)| (*v, o.map(|c| (c / multiplier) as i16)))
                .collect(),
        })
    }
}

const MAGIC: &[u8; 4] = b"PIRT";

/// A whole morph file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Morphs {
    pub shapes: Vec<ShapeMorphs>,
    /// Morphs of texture coordinates: the same, with two numbers a vertex
    /// (the third of each offset here is unused).
    pub uv: Vec<ShapeMorphs>,
}

pub fn read(bytes: &[u8]) -> Result<Morphs, Error> {
    let short = || Error::Invalid("a morph file that ends early".into());
    let mut at = 0usize;
    let mut take = |n: usize| -> Result<&[u8], Error> {
        let part = bytes.get(at..at + n).ok_or_else(short)?;
        at += n;
        Ok(part)
    };
    if take(4)? != MAGIC {
        return Err(Error::Invalid("not a morph file".into()));
    }
    let u32_of = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let u16_of = |b: &[u8]| u16::from_le_bytes([b[0], b[1]]);
    let mut lists = [Vec::new(), Vec::new()];
    for (which, out) in lists.iter_mut().enumerate() {
        // Three numbers a vertex in the first list, two in the second. A
        // file may end before the second.
        let (row, parts) = if which == 0 { (8, 3) } else { (6, 2) };
        let Ok(count) = take(2) else { break };
        for _ in 0..u16_of(count) {
            let len = take(1)?[0] as usize;
            let name = String::from_utf8_lossy(take(len)?).into_owned();
            let count = u16_of(take(2)?) as usize;
            let mut morphs = Vec::new();
            for _ in 0..count {
                let len = take(1)?[0] as usize;
                let name = String::from_utf8_lossy(take(len)?).into_owned();
                let multiplier = f32::from_bits(u32_of(take(4)?));
                let n = u16_of(take(2)?) as usize;
                let rows = take(n * row)?;
                let offsets = rows
                    .chunks_exact(row)
                    .map(|r| {
                        let mut o = [0i16; 3];
                        for (k, c) in o.iter_mut().enumerate().take(parts) {
                            *c = i16::from_le_bytes([r[2 + k * 2], r[3 + k * 2]]);
                        }
                        (u16_of(r), o)
                    })
                    .collect();
                morphs.push(Morph {
                    name,
                    multiplier,
                    offsets,
                });
            }
            out.push(ShapeMorphs { name, morphs });
        }
    }
    if at != bytes.len() {
        return Err(Error::Invalid("bytes after the end of a morph file".into()));
    }
    let [shapes, uv] = lists;
    Ok(Morphs { shapes, uv })
}

pub fn write(morphs: &Morphs) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    let name = |out: &mut Vec<u8>, s: &str| {
        let bytes = &s.as_bytes()[..s.len().min(255)];
        out.push(bytes.len() as u8);
        out.extend(bytes);
    };
    for (shapes, parts) in [(&morphs.shapes, 3), (&morphs.uv, 2)] {
        out.extend((shapes.len() as u16).to_le_bytes());
        for shape in shapes {
            name(&mut out, &shape.name);
            out.extend((shape.morphs.len() as u16).to_le_bytes());
            for m in &shape.morphs {
                name(&mut out, &m.name);
                out.extend(m.multiplier.to_le_bytes());
                out.extend((m.offsets.len() as u16).to_le_bytes());
                for (v, o) in &m.offsets {
                    out.extend(v.to_le_bytes());
                    for c in &o[..parts] {
                        out.extend(c.to_le_bytes());
                    }
                }
            }
        }
    }
    out
}
