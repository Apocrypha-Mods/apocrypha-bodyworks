//! Reading and writing little-endian values, with every read checked against
//! the bytes that are there: these files come from the internet.

use crate::NifError;

/// A forward reader over a byte slice.
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn left(&self) -> usize {
        self.bytes.len() - self.pos
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], NifError> {
        let end = self.pos.checked_add(n).ok_or(NifError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(NifError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> Result<u8, NifError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, NifError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32, NifError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32, NifError> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64, NifError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn f32(&mut self) -> Result<f32, NifError> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// A count that is about to be used to allocate: refused when the file
    /// could not possibly hold that many items of `each` bytes.
    pub fn count(&mut self, each: usize) -> Result<usize, NifError> {
        let n = self.u32()? as usize;
        if n.checked_mul(each.max(1))
            .is_none_or(|need| need > self.left())
        {
            return Err(NifError::Truncated);
        }
        Ok(n)
    }

    /// A string with a four-byte length before it.
    pub fn sized_string(&mut self) -> Result<Vec<u8>, NifError> {
        let n = self.count(1)?;
        Ok(self.take(n)?.to_vec())
    }

    /// A string with a one-byte length before it (the length counts its
    /// closing zero byte, which is kept).
    pub fn short_string(&mut self) -> Result<Vec<u8>, NifError> {
        let n = self.u8()? as usize;
        Ok(self.take(n)?.to_vec())
    }
}

/// Appends little-endian values to a buffer.
#[derive(Default)]
pub struct Writer {
    pub out: Vec<u8>,
}

impl Writer {
    pub fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }

    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.bytes(&v.to_bits().to_le_bytes());
    }

    pub fn sized_string(&mut self, s: &[u8]) {
        self.u32(s.len() as u32);
        self.bytes(s);
    }

    pub fn short_string(&mut self, s: &[u8]) {
        self.u8(s.len() as u8);
        self.bytes(s);
    }
}
