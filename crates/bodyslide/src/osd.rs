//! Slider data (`.osd`): for each named list, the vertices a slider moves
//! and how far each goes when the slider is full.

use crate::Error;

/// One vertex of a list: which, and its offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diff {
    pub vertex: u16,
    pub offset: [f32; 3],
}

/// A file of named lists, in the file's order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SliderData {
    pub version: u32,
    pub lists: Vec<(String, Vec<Diff>)>,
}

const MAGIC: &[u8; 4] = b"\0DSO";

impl SliderData {
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.get(..4) != Some(MAGIC) {
            return Err(Error::NotThatKind);
        }
        let u32_at = |at: usize| -> Result<u32, Error> {
            bytes
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or(Error::Truncated)
        };
        let version = u32_at(4)?;
        let count = u32_at(8)? as usize;
        let mut at = 12usize;
        // Each list is at least a length byte and a count.
        if count.checked_mul(3).is_none_or(|n| n > bytes.len()) {
            return Err(Error::Truncated);
        }
        let mut lists = Vec::with_capacity(count);
        for _ in 0..count {
            let len = *bytes.get(at).ok_or(Error::Truncated)? as usize;
            let name = bytes.get(at + 1..at + 1 + len).ok_or(Error::Truncated)?;
            at += 1 + len;
            let n = bytes
                .get(at..at + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
                .ok_or(Error::Truncated)?;
            at += 2;
            let data = bytes.get(at..at + n * 14).ok_or(Error::Truncated)?;
            at += n * 14;
            let diffs = data
                .as_chunks::<14>()
                .0
                .iter()
                .map(|d| Diff {
                    vertex: u16::from_le_bytes([d[0], d[1]]),
                    offset: [
                        f32::from_le_bytes([d[2], d[3], d[4], d[5]]),
                        f32::from_le_bytes([d[6], d[7], d[8], d[9]]),
                        f32::from_le_bytes([d[10], d[11], d[12], d[13]]),
                    ],
                })
                .collect();
            lists.push((crate::xml::decoded(name), diffs));
        }
        if at != bytes.len() {
            return Err(Error::Invalid(format!(
                "{} bytes after the last list",
                bytes.len() - at
            )));
        }
        Ok(SliderData { version, lists })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend(self.version.to_le_bytes());
        out.extend((self.lists.len() as u32).to_le_bytes());
        for (name, diffs) in &self.lists {
            let name = name.as_bytes();
            let name = &name[..name.len().min(255)];
            out.push(name.len() as u8);
            out.extend(name);
            let diffs = &diffs[..diffs.len().min(u16::MAX as usize)];
            out.extend((diffs.len() as u16).to_le_bytes());
            for d in diffs {
                out.extend(d.vertex.to_le_bytes());
                for c in d.offset {
                    out.extend(c.to_le_bytes());
                }
            }
        }
        out
    }

    pub fn list(&self, name: &str) -> Option<&[Diff]> {
        self.lists
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.as_slice())
    }
}
