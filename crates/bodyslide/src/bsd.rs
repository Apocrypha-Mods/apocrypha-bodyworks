//! One list of slider data on its own (`.bsd`): the older form, a file for
//! each slider and shape, where an `.osd` holds them all by name.
//!
//! A count, then for each vertex its number and its offset, little-endian:
//! 4 + 16 bytes a vertex and nothing after. That holds for all 3,781 of a
//! real outfit mod's files (Devious Devices, `--example bsd`).

use crate::osd::Diff;
use crate::Error;

/// A file's vertices, in its order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bsd {
    pub diffs: Vec<Diff>,
    /// For each vertex, the two bytes its number's slot holds past the
    /// number. A vertex's number takes four bytes, but over half the real
    /// files (2,086 of 3,781) have something other than nothing in the
    /// upper two for a few vertices, while the lower two count on from the
    /// vertex before as everywhere else: a number written in two bytes into
    /// a slot of four, the rest left as it was. Kept, so a file writes back
    /// as it was; empty, they are written as nothing.
    pub unused: Vec<u16>,
}

impl Bsd {
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let count = bytes
            .get(..4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or(Error::Truncated)?;
        let body = &bytes[4..];
        let need = count.checked_mul(16).ok_or(Error::Truncated)?;
        if body.len() < need {
            return Err(Error::Truncated);
        }
        if body.len() > need {
            return Err(Error::Invalid(format!(
                "{} bytes after the last vertex",
                body.len() - need
            )));
        }
        let rows = body.as_chunks::<16>().0;
        let diffs = rows
            .iter()
            .map(|d| {
                let f = |i: usize| f32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]);
                Diff {
                    vertex: u16::from_le_bytes([d[0], d[1]]),
                    offset: [f(4), f(8), f(12)],
                }
            })
            .collect();
        let unused: Vec<u16> = rows
            .iter()
            .map(|d| u16::from_le_bytes([d[2], d[3]]))
            .collect();
        Ok(Bsd {
            diffs,
            unused: if unused.iter().all(|u| *u == 0) {
                Vec::new()
            } else {
                unused
            },
        })
    }

    /// A file of `diffs` alone.
    pub fn of(diffs: &[Diff]) -> Self {
        Bsd {
            diffs: diffs.to_vec(),
            unused: Vec::new(),
        }
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 16 * self.diffs.len());
        out.extend((self.diffs.len() as u32).to_le_bytes());
        for (i, d) in self.diffs.iter().enumerate() {
            out.extend(d.vertex.to_le_bytes());
            out.extend(self.unused.get(i).copied().unwrap_or(0).to_le_bytes());
            for c in d.offset {
                out.extend(c.to_le_bytes());
            }
        }
        out
    }
}

/// Whether a set's file of slider data is one of these, by its name.
pub fn named(file: &str) -> bool {
    file.len() > 4
        && file
            .get(file.len() - 4..)
            .is_some_and(|e| e.eq_ignore_ascii_case(".bsd"))
}
