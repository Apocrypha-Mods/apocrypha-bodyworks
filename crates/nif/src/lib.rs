//! NIF mesh files: the container, and the blocks this project works on.
//!
//! A NIF is a header, a run of blocks, and a short footer. From Fallout 3 on
//! the header says how long every block is, which is what makes this crate
//! safe to use on files it only partly understands: a block is kept as its
//! bytes, and only the kinds something here needs are read into structures
//! ([`geometry`], [`skin`], [`node`], [`shader`]). Written back, everything
//! that was not changed on purpose is byte for byte what was read.
//!
//! Written from the files themselves and from published descriptions of the
//! format. Not derived from any other program's source.

pub mod geometry;
pub mod io;
pub mod mesh;
pub mod node;
pub mod shader;
pub mod skin;

use io::{Reader, Writer};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NifError {
    #[error("not a NIF file")]
    NotNif,
    #[error("the file ends before its contents do")]
    Truncated,
    #[error("a NIF of a kind this cannot read ({0})")]
    Unsupported(String),
    #[error("the file disagrees with itself ({0})")]
    Corrupt(String),
}

/// The one container version every game here uses.
pub const VERSION_20_2_0_7: u32 = 0x1402_0007;

/// Which game a file was written for, by the version in its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    /// Fallout 3 and New Vegas (stream version 34).
    Fallout3,
    /// The original Skyrim (83).
    Skyrim,
    /// Skyrim Special Edition (100).
    SkyrimSe,
    /// Fallout 4 (130).
    Fallout4,
    /// Fallout 76 (155).
    Fallout76,
    /// Starfield (172 and up), or anything else.
    Other,
}

impl Game {
    pub fn of(stream_version: u32) -> Game {
        match stream_version {
            0..=34 => Game::Fallout3,
            35..=83 => Game::Skyrim,
            84..=100 => Game::SkyrimSe,
            101..=139 => Game::Fallout4,
            140..=169 => Game::Fallout76,
            _ => Game::Other,
        }
    }
}

/// The part of the header Bethesda's games add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamHeader {
    /// 83 Skyrim, 100 Special Edition, 130 Fallout 4.
    pub version: u32,
    pub author: Vec<u8>,
    /// Present above version 130.
    pub unknown: Option<u32>,
    /// Present below version 131.
    pub process_script: Option<Vec<u8>>,
    pub export_script: Vec<u8>,
    /// Present from version 103.
    pub max_filepath: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The first line, without its newline: "Gamebryo File Format, Version …".
    pub line: Vec<u8>,
    pub version: u32,
    pub endian: u8,
    pub user_version: u32,
    pub stream: StreamHeader,
    /// The names of the kinds of block in the file.
    pub block_types: Vec<Vec<u8>>,
    pub strings: Vec<Vec<u8>>,
    /// As the file states it. Some files state it wrongly; it is kept, and
    /// only raised when a longer string is added.
    pub max_string_len: u32,
    pub groups: Vec<u32>,
}

/// One block: what kind it is, and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Index into [`Header::block_types`].
    pub kind: u16,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nif {
    pub header: Header,
    pub blocks: Vec<Block>,
    /// The blocks at the top of the scene, by index; -1 for none.
    pub roots: Vec<i32>,
}

impl Nif {
    /// Read a whole file.
    pub fn read(bytes: &[u8]) -> Result<Nif, NifError> {
        let newline = bytes
            .iter()
            .take(128)
            .position(|b| *b == b'\n')
            .ok_or(NifError::NotNif)?;
        let line = &bytes[..newline];
        if !line.starts_with(b"Gamebryo File Format")
            && !line.starts_with(b"NetImmerse File Format")
        {
            return Err(NifError::NotNif);
        }
        let mut r = Reader::new(&bytes[newline + 1..]);
        let version = r.u32()?;
        if version != VERSION_20_2_0_7 {
            return Err(NifError::Unsupported(format!(
                "version {}.{}.{}.{}",
                version >> 24,
                (version >> 16) & 0xff,
                (version >> 8) & 0xff,
                version & 0xff
            )));
        }
        let endian = r.u8()?;
        if endian != 1 {
            return Err(NifError::Unsupported("big-endian".into()));
        }
        let user_version = r.u32()?;
        let block_count = r.count(6)?;
        let stream_version = r.u32()?;
        let author = r.short_string()?;
        let unknown = if stream_version > 130 {
            Some(r.u32()?)
        } else {
            None
        };
        let process_script = if stream_version < 131 {
            Some(r.short_string()?)
        } else {
            None
        };
        let export_script = r.short_string()?;
        let max_filepath = if stream_version >= 103 {
            Some(r.short_string()?)
        } else {
            None
        };

        let type_count = r.u16()? as usize;
        let mut block_types = Vec::with_capacity(type_count.min(4096));
        for _ in 0..type_count {
            block_types.push(r.sized_string()?);
        }
        let mut kinds = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            let kind = r.u16()?;
            // The top bit marks a block built for PhysX; none of these games
            // set it, and an index past the table is a broken file.
            if kind as usize >= block_types.len() {
                return Err(NifError::Corrupt(format!(
                    "block kind {kind} of {type_count}"
                )));
            }
            kinds.push(kind);
        }
        let mut sizes = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            sizes.push(r.u32()? as usize);
        }
        let string_count = r.count(4)?;
        let max_string_len = r.u32()?;
        let mut strings = Vec::with_capacity(string_count);
        for _ in 0..string_count {
            strings.push(r.sized_string()?);
        }
        let group_count = r.count(4)?;
        let mut groups = Vec::with_capacity(group_count);
        for _ in 0..group_count {
            groups.push(r.u32()?);
        }

        let mut blocks = Vec::with_capacity(block_count);
        for (kind, size) in kinds.into_iter().zip(sizes) {
            blocks.push(Block {
                kind,
                data: r.take(size)?.to_vec(),
            });
        }
        let root_count = r.count(4)?;
        let mut roots = Vec::with_capacity(root_count);
        for _ in 0..root_count {
            roots.push(r.i32()?);
        }
        if r.left() != 0 {
            return Err(NifError::Corrupt(format!(
                "{} bytes after the end",
                r.left()
            )));
        }

        Ok(Nif {
            header: Header {
                line: line.to_vec(),
                version,
                endian,
                user_version,
                stream: StreamHeader {
                    version: stream_version,
                    author,
                    unknown,
                    process_script,
                    export_script,
                    max_filepath,
                },
                block_types,
                strings,
                max_string_len,
                groups,
            },
            blocks,
            roots,
        })
    }

    /// The file as bytes. Unchanged since [`Nif::read`], these are the bytes
    /// that were read.
    pub fn write(&self) -> Vec<u8> {
        let h = &self.header;
        let mut w = Writer::default();
        w.bytes(&h.line);
        w.u8(b'\n');
        w.u32(h.version);
        w.u8(h.endian);
        w.u32(h.user_version);
        w.u32(self.blocks.len() as u32);
        w.u32(h.stream.version);
        w.short_string(&h.stream.author);
        if let Some(v) = h.stream.unknown {
            w.u32(v);
        }
        if let Some(s) = &h.stream.process_script {
            w.short_string(s);
        }
        w.short_string(&h.stream.export_script);
        if let Some(s) = &h.stream.max_filepath {
            w.short_string(s);
        }
        w.u16(h.block_types.len() as u16);
        for t in &h.block_types {
            w.sized_string(t);
        }
        for b in &self.blocks {
            w.u16(b.kind);
        }
        for b in &self.blocks {
            w.u32(b.data.len() as u32);
        }
        w.u32(h.strings.len() as u32);
        w.u32(h.max_string_len);
        for s in &h.strings {
            w.sized_string(s);
        }
        w.u32(h.groups.len() as u32);
        for g in &h.groups {
            w.u32(*g);
        }
        for b in &self.blocks {
            w.bytes(&b.data);
        }
        w.u32(self.roots.len() as u32);
        for root in &self.roots {
            w.i32(*root);
        }
        w.out
    }

    pub fn game(&self) -> Game {
        Game::of(self.header.stream.version)
    }

    /// The name of a block's kind, e.g. `BSTriShape`.
    pub fn kind_of(&self, block: usize) -> &str {
        self.blocks
            .get(block)
            .and_then(|b| self.header.block_types.get(b.kind as usize))
            .and_then(|t| std::str::from_utf8(t).ok())
            .unwrap_or("")
    }

    /// A string from the header's table, by the index a block stores. -1 and
    /// anything out of range is no string.
    pub fn string(&self, index: i32) -> Option<&[u8]> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.header.strings.get(i))
            .map(Vec::as_slice)
    }

    /// The index of `s` in the header's table, adding it if it is not there.
    pub fn intern(&mut self, s: &[u8]) -> i32 {
        if let Some(at) = self.header.strings.iter().position(|have| have == s) {
            return at as i32;
        }
        self.header.strings.push(s.to_vec());
        self.header.max_string_len = self.header.max_string_len.max(s.len() as u32);
        (self.header.strings.len() - 1) as i32
    }

    /// The index of a kind of block in the header's table, adding it if it
    /// is not there.
    pub fn kind_index(&mut self, name: &str) -> u16 {
        if let Some(at) = self
            .header
            .block_types
            .iter()
            .position(|t| t == name.as_bytes())
        {
            return at as u16;
        }
        self.header.block_types.push(name.as_bytes().to_vec());
        (self.header.block_types.len() - 1) as u16
    }
}

#[cfg(test)]
mod tests;
