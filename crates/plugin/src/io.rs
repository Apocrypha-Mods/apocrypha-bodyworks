//! Reading little-endian values, with every read checked against the bytes
//! that are there: plugins come from the internet.

use crate::PluginError;

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

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], PluginError> {
        let end = self.pos.checked_add(n).ok_or(PluginError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(PluginError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    pub fn tag(&mut self) -> Result<[u8; 4], PluginError> {
        let b = self.take(4)?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    pub fn u8(&mut self) -> Result<u8, PluginError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, PluginError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16, PluginError> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32, PluginError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32, PluginError> {
        Ok(self.u32()? as i32)
    }

    pub fn f32(&mut self) -> Result<f32, PluginError> {
        Ok(f32::from_bits(self.u32()?))
    }
}

/// A zero-terminated string's text, without the zero (or all of it, when a
/// file leaves the zero out).
pub fn zstring(b: &[u8]) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    // Plugins are written in Windows-1252; the paths and editor ids read here
    // are ASCII in every real file, and anything else is kept as Latin-1.
    b[..end].iter().map(|c| *c as char).collect()
}
