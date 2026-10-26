//! Reading little-endian values, with every read checked against the bytes
//! that are there: saves come from anywhere.

use crate::SaveError;

/// A forward reader over a byte slice.
#[derive(Clone)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// A reader starting at `pos`, refused when that is past the end.
    pub fn at(bytes: &'a [u8], pos: usize) -> Result<Self, SaveError> {
        if pos > bytes.len() {
            return Err(SaveError::Truncated);
        }
        Ok(Self { bytes, pos })
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn left(&self) -> usize {
        self.bytes.len() - self.pos
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], SaveError> {
        let end = self.pos.checked_add(n).ok_or(SaveError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(SaveError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8, SaveError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, SaveError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32, SaveError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&mut self) -> Result<u64, SaveError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn f32(&mut self) -> Result<f32, SaveError> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// A count about to be used to allocate: refused when what is left could
    /// not hold that many items of at least `each` bytes.
    pub fn check_count(&self, n: usize, each: usize) -> Result<usize, SaveError> {
        if n.checked_mul(each.max(1))
            .is_none_or(|need| need > self.left())
        {
            return Err(SaveError::Truncated);
        }
        Ok(n)
    }

    /// A string with a two-byte length before it and no closing zero
    /// (UESP's `wstring`). Windows-1252; bytes outside ASCII are mapped one
    /// for one through Latin-1, which differs only in 0x80..0x9F.
    pub fn wstring(&mut self) -> Result<String, SaveError> {
        let n = self.u16()? as usize;
        Ok(latin1(self.take(n)?))
    }

    /// A three-byte reference to a form (see [`crate::RefId`]).
    pub fn ref_id(&mut self) -> Result<crate::RefId, SaveError> {
        let b = self.take(3)?;
        Ok(crate::RefId::from_bytes([b[0], b[1], b[2]]))
    }

    /// A count whose own length is in its two low bits (UESP's `vsval`):
    /// 0, 1 and 2 mean one, two and three bytes, little-endian, the value
    /// above those two bits. 3 is not used and is refused.
    pub fn vsval(&mut self) -> Result<u32, SaveError> {
        let first = self.u8()?;
        let v = match first & 3 {
            0 => first as u32,
            1 => {
                let b = self.u8()?;
                u16::from_le_bytes([first, b]) as u32
            }
            2 => {
                let b = self.take(2)?;
                u32::from_le_bytes([first, b[0], b[1], 0])
            }
            _ => return Err(SaveError::Corrupt("a vsval of kind 3".into())),
        };
        Ok(v >> 2)
    }
}

pub(crate) fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}
