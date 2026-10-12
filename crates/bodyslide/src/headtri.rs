//! The morph file a part of a head comes with (`FRTRI003`): the part's own
//! vertices, triangles and places in the texture, then each expression or
//! each of the face's sliders as what it does to every vertex.
//!
//! Worked out from 87 such files of a real body mod
//! (`--example headtri`). Little-endian: the eight letters, then ten
//! counts and sixteen bytes of nothing (how many vertices, triangles,
//! four-cornered faces, two things never there, places in the texture, a
//! one, morphs, modifiers and vertices of modifiers); three floats a
//! vertex; three numbers a triangle; two floats a place in the texture;
//! three numbers a triangle again, into those; and for each morph a name
//! (its length first, with a nought after it), a scale, and three signed
//! 16-bit numbers a vertex, each times the scale. Every one of the 87 has
//! as many places in the texture as vertices, its triangles twice the
//! same, and none of the rest.

use crate::Error;

const MAGIC: &[u8; 8] = b"FRTRI003";

#[derive(Debug, Clone, PartialEq)]
pub struct HeadMorph {
    pub name: String,
    pub scale: f32,
    /// For each vertex, what it moves by in units of the scale.
    pub offsets: Vec<[i16; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadTri {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub morphs: Vec<HeadMorph>,
}

fn bad<T>(why: &str) -> Result<T, Error> {
    Err(Error::Invalid(format!("The head's morph file {why}.")))
}

/// Whether a file is one of these, by how it starts.
pub fn is(file: &[u8]) -> bool {
    file.get(..8) == Some(MAGIC)
}

pub fn read(file: &[u8]) -> Result<HeadTri, Error> {
    if !is(file) {
        return bad("is not one");
    }
    let word = |at: usize| -> Result<usize, Error> {
        match file.get(at..at.saturating_add(4)) {
            Some(b) => Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize),
            None => bad("ends before its contents do"),
        }
    };
    let counts: Vec<usize> = (0..10).map(|i| word(8 + i * 4)).collect::<Result<_, _>>()?;
    let (vertices, triangles, uvs, morphs) = (counts[0], counts[1], counts[5], counts[7]);
    // Only what every file measured has: no four-cornered faces, no
    // modifiers, a place in the texture for each vertex.
    if counts[2] != 0
        || counts[3] != 0
        || counts[4] != 0
        || counts[6] != 1
        || counts[8] != 0
        || counts[9] != 0
    {
        return bad("has parts this does not read");
    }
    // Sizes against the bytes there are, before room is made for them.
    let fixed = vertices
        .checked_mul(12)
        .and_then(|n| n.checked_add(triangles.checked_mul(24)?))
        .and_then(|n| n.checked_add(uvs.checked_mul(8)?))
        .and_then(|n| n.checked_add(64));
    if fixed.is_none_or(|n| n > file.len()) || vertices > u16::MAX as usize + 1 {
        return bad("ends before its contents do");
    }
    let float =
        |at: usize| f32::from_le_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]);
    let mut at = 64;
    let mut out = HeadTri {
        vertices: Vec::with_capacity(vertices),
        triangles: Vec::with_capacity(triangles),
        uvs: Vec::with_capacity(uvs),
        morphs: Vec::new(),
    };
    for _ in 0..vertices {
        out.vertices.push([float(at), float(at + 4), float(at + 8)]);
        at += 12;
    }
    for _ in 0..triangles {
        out.triangles
            .push([word(at)? as u32, word(at + 4)? as u32, word(at + 8)? as u32]);
        at += 12;
    }
    for _ in 0..uvs {
        out.uvs.push([float(at), float(at + 4)]);
        at += 8;
    }
    // The triangles again, into the places in the texture.
    at += triangles * 12;
    for _ in 0..morphs {
        let length = word(at)?;
        let Some(name) = file.get(at + 4..(at + 4).saturating_add(length)) else {
            return bad("ends before its contents do");
        };
        let name = String::from_utf8_lossy(name)
            .trim_end_matches('\0')
            .to_string();
        at += 4 + length;
        let Some(body) = file.get(at..at.saturating_add(4 + vertices * 6)) else {
            return bad("ends before its contents do");
        };
        let scale = f32::from_le_bytes([body[0], body[1], body[2], body[3]]);
        let offsets = body[4..]
            .as_chunks::<6>()
            .0
            .iter()
            .map(|c| [0, 2, 4].map(|k| i16::from_le_bytes([c[k], c[k + 1]])))
            .collect();
        at += 4 + vertices * 6;
        out.morphs.push(HeadMorph {
            name,
            scale,
            offsets,
        });
    }
    if at != file.len() {
        return bad("has more in it than it says");
    }
    Ok(out)
}

pub fn write(tri: &HeadTri) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    let counts = [
        tri.vertices.len(),
        tri.triangles.len(),
        0,
        0,
        0,
        tri.uvs.len(),
        1,
        tri.morphs.len(),
        0,
        0,
    ];
    for c in counts {
        out.extend((c as u32).to_le_bytes());
    }
    out.extend([0u8; 16]);
    for c in tri.vertices.iter().flatten() {
        out.extend(c.to_le_bytes());
    }
    for c in tri.triangles.iter().flatten() {
        out.extend(c.to_le_bytes());
    }
    for c in tri.uvs.iter().flatten() {
        out.extend(c.to_le_bytes());
    }
    for c in tri.triangles.iter().flatten() {
        out.extend(c.to_le_bytes());
    }
    for m in &tri.morphs {
        out.extend((m.name.len() as u32 + 1).to_le_bytes());
        out.extend(m.name.as_bytes());
        out.push(0);
        out.extend(m.scale.to_le_bytes());
        for c in m.offsets.iter().flatten() {
            out.extend(c.to_le_bytes());
        }
    }
    out
}

impl HeadMorph {
    /// A morph from what it does to each vertex as numbers: the scale
    /// chosen so the largest of them is the largest a 16-bit number holds.
    pub fn of(name: &str, offsets: &[[f32; 3]]) -> HeadMorph {
        let most = offsets.iter().flatten().fold(0.0f32, |m, c| m.max(c.abs()));
        let scale = if most > 0.0 { most / 32767.0 } else { 1.0 };
        HeadMorph {
            name: name.to_string(),
            scale,
            offsets: offsets
                .iter()
                .map(|o| o.map(|c| (c / scale).round().clamp(-32767.0, 32767.0) as i16))
                .collect(),
        }
    }
}
