//! Reading a head part's tri (`FRTRI003`) for its morphs, including the
//! files `apoc_bodyslide::headtri` refuses because they carry modifiers.
//!
//! The layout is `headtri`'s, with modifiers, found from the game's own
//! files (`childhead.tri` says 14 of them naming 847 vertices,
//! `eyesargonian.tri` 4 and 196): their positions, three floats each, come
//! straight after the shape's own vertices, and after the morphs each
//! modifier is a name (its length first, with a nought after it), a count
//! and that many u32 vertex numbers. That accounts for every byte of every
//! tri in the game's archives (`--example tris`). They are expressions of
//! a few vertices each (blinks on a child's eyes); a look sets none, so
//! they are checked and skipped, not kept.

use apoc_bodyslide::headtri::{HeadMorph, HeadTri};

const MAGIC: &[u8; 8] = b"FRTRI003";

fn word(file: &[u8], at: usize) -> Result<usize, String> {
    file.get(at..at.saturating_add(4))
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or_else(|| "it ends before its contents do".to_string())
}

/// The tri's vertices and morphs; no triangles or places in the texture
/// (the model's are used).
pub fn read(file: &[u8]) -> Result<HeadTri, String> {
    if file.get(..8) != Some(MAGIC) {
        return Err("it is not a tri".into());
    }
    let mut counts = [0usize; 10];
    for (i, c) in counts.iter_mut().enumerate() {
        *c = word(file, 8 + i * 4)?;
    }
    let [vertices, triangles, quads, u1, u2, uvs, one, morphs, modifiers, modified] = counts;
    if quads != 0 || u1 != 0 || u2 != 0 || one != 1 || uvs != vertices {
        return Err("it has parts this does not read".into());
    }
    let fixed = vertices
        .checked_add(modified)
        .and_then(|n| n.checked_mul(12))
        .and_then(|n| n.checked_add(triangles.checked_mul(24)?))
        .and_then(|n| n.checked_add(uvs.checked_mul(8)?))
        .and_then(|n| n.checked_add(64));
    let Some(fixed) = fixed.filter(|n| *n <= file.len()) else {
        return Err("it ends before its contents do".into());
    };
    if vertices > u16::MAX as usize + 1 {
        return Err("it has more vertices than a shape holds".into());
    }
    let float =
        |at: usize| f32::from_le_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]);
    let mut out = HeadTri {
        vertices: (0..vertices)
            .map(|v| {
                let at = 64 + v * 12;
                [float(at), float(at + 4), float(at + 8)]
            })
            .collect(),
        triangles: Vec::new(),
        uvs: Vec::new(),
        morphs: Vec::new(),
    };
    let mut at = fixed;
    let each = vertices * 6;
    for _ in 0..morphs {
        let length = word(file, at)?;
        let name = file
            .get(at + 4..(at + 4).saturating_add(length))
            .ok_or("it ends before its contents do")?;
        let name = String::from_utf8_lossy(name)
            .trim_end_matches('\0')
            .to_string();
        at += 4 + length;
        let body = file
            .get(at..at.saturating_add(4 + each))
            .ok_or("it ends before its contents do")?;
        let scale = f32::from_le_bytes([body[0], body[1], body[2], body[3]]);
        let offsets = body[4..]
            .as_chunks::<6>()
            .0
            .iter()
            .map(|c| [0, 2, 4].map(|k| i16::from_le_bytes([c[k], c[k + 1]])))
            .collect();
        at += 4 + each;
        out.morphs.push(HeadMorph {
            name,
            scale,
            offsets,
        });
    }
    let mut named = 0usize;
    for _ in 0..modifiers {
        let length = word(file, at)?;
        at = at
            .checked_add(4 + length)
            .ok_or("it ends before its contents do")?;
        let n = word(file, at)?;
        at = n
            .checked_mul(4)
            .and_then(|b| b.checked_add(at + 4))
            .filter(|a| *a <= file.len())
            .ok_or("it ends before its contents do")?;
        named = named.saturating_add(n);
    }
    if named != modified {
        return Err("its modifiers name another number of vertices than it says".into());
    }
    let end = at;
    if end != file.len() {
        return Err(if end > file.len() {
            "it ends before its contents do".into()
        } else {
            "it has more in it than it says".into()
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use apoc_bodyslide::headtri;

    fn sample() -> HeadTri {
        HeadTri {
            vertices: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
            triangles: vec![[0, 1, 1]],
            uvs: vec![[0.0, 0.0], [1.0, 1.0]],
            morphs: vec![HeadMorph {
                name: "SkinnyMorph".into(),
                scale: 0.5,
                offsets: vec![[1, -2, 3], [0, 0, 7]],
            }],
        }
    }

    fn with_modifier(mut bytes: Vec<u8>) -> Vec<u8> {
        bytes[40..44].copy_from_slice(&1u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&2u32.to_le_bytes());
        bytes.extend(6u32.to_le_bytes());
        bytes.extend(b"Blink\0");
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        // Their two positions, after the shape's two vertices.
        let at = 64 + 2 * 12;
        bytes.splice(at..at, [0u8; 24]);
        bytes
    }

    #[test]
    fn reads_what_headtri_writes() {
        let t = read(&headtri::write(&sample())).unwrap();
        assert_eq!(t.vertices, sample().vertices);
        assert_eq!(t.morphs, sample().morphs);
    }

    #[test]
    fn modifiers_are_skipped() {
        let bytes = with_modifier(headtri::write(&sample()));
        assert!(headtri::read(&bytes).is_err());
        let t = read(&bytes).unwrap();
        assert_eq!(t.morphs, sample().morphs);
    }

    /// Two morphs and two modifiers: the modifiers' positions, after the
    /// shape's own vertices, are not taken for vertices, and each morph is
    /// read from where it is.
    #[test]
    fn morphs_around_modifiers_read_whole() {
        let mut two = sample();
        two.morphs.push(HeadMorph {
            name: "Blush".into(),
            scale: 2.0,
            offsets: vec![[-5, 6, -7], [8, -9, 10]],
        });
        let mut bytes = headtri::write(&two);
        // Two modifiers naming three vertices between them.
        bytes[40..44].copy_from_slice(&2u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&3u32.to_le_bytes());
        for (name, vertices) in [
            (&b"BlinkL\0"[..], &[0u32][..]),
            (&b"BlinkR\0"[..], &[1u32, 0][..]),
        ] {
            bytes.extend((name.len() as u32).to_le_bytes());
            bytes.extend(name);
            bytes.extend((vertices.len() as u32).to_le_bytes());
            for v in vertices {
                bytes.extend(v.to_le_bytes());
            }
        }
        let at = 64 + 2 * 12;
        let positions: Vec<u8> = (0..9)
            .flat_map(|i| (100.0f32 + i as f32).to_le_bytes())
            .collect();
        bytes.splice(at..at, positions);
        let t = read(&bytes).unwrap();
        assert_eq!(t.vertices, two.vertices);
        assert_eq!(t.morphs, two.morphs);
        // Said to name four, or two, they are refused.
        for said in [4u32, 2] {
            let mut lying = bytes.clone();
            lying[44..48].copy_from_slice(&said.to_le_bytes());
            // The positions said are there, so only the count is wrong.
            if said == 4 {
                lying.splice(at..at, [0u8; 12]);
            } else {
                lying.drain(at..at + 12);
            }
            assert_eq!(
                read(&lying).err().as_deref(),
                Some("its modifiers name another number of vertices than it says"),
                "said {said}"
            );
        }
    }

    #[test]
    fn cut_or_lying_files_are_refused() {
        let bytes = with_modifier(headtri::write(&sample()));
        for cut in 0..bytes.len() {
            assert!(read(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut long = bytes.clone();
        long.push(0);
        assert!(read(&long).is_err());
        // Counts of billions in a few bytes.
        for at in [8, 36, 40, 44] {
            let mut huge = bytes.clone();
            huge[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(read(&huge).is_err(), "count at {at}");
        }
        // A modifier that names a billion vertices.
        let mut many = bytes;
        let at = many.len() - 8 - 4;
        many[at..at + 4].copy_from_slice(&0x4000_0000u32.to_le_bytes());
        assert!(read(&many).is_err());
    }
}
