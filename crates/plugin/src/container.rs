//! The container: record and group headers, subrecords, compressed records,
//! the `TES4` header, and what a formID in a plugin refers to.
//!
//! Skyrim's headers are 24 bytes (UESP, "Mod File Format"):
//!
//! - a record is its type, the size of its data (not counting the header),
//!   flags, formID, a version-control stamp, a form version and two unknown
//!   bytes;
//! - a group is `GRUP`, its size *including* the header, a label (the record
//!   type for a top-level group), a group type (0 for top level), a stamp
//!   and four more bytes.
//!
//! Every Skyrim SE plugin read here (Skyrim.esm, its DLCs, the Creation Club
//! files, USSEP and the mods in the test install) has header records of form
//! version 43 or 44 and all of its top-level groups of type 0.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::io::{zstring, Reader};
use crate::PluginError;

pub const HEADER_LEN: usize = 24;

/// A record's flag: its data is a u32 inflated size and then zlib.
pub const COMPRESSED: u32 = 0x0004_0000;
/// A record's flag: it was deleted by the plugin that holds it.
pub const DELETED: u32 = 0x0000_0020;
/// `TES4` flags.
pub const ESM: u32 = 0x1;
pub const LOCALIZED: u32 = 0x80;
pub const LIGHT: u32 = 0x200;

/// The most a compressed record may inflate to. The largest record in
/// Skyrim.esm inflates to well under a megabyte (navmeshes and landscape);
/// a mod's claim of more than this is refused rather than allocated.
pub const MAX_INFLATED: usize = 16 << 20;

/// The 24 bytes before a record's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    pub kind: [u8; 4],
    pub size: u32,
    pub flags: u32,
    pub id: u32,
    pub version: u16,
}

impl RecordHeader {
    pub fn read(b: &[u8]) -> Result<Self, PluginError> {
        let mut r = Reader::new(b);
        let kind = r.tag()?;
        let size = r.u32()?;
        let flags = r.u32()?;
        let id = r.u32()?;
        r.u32()?; // version-control stamp
        let version = r.u16()?;
        r.u16()?;
        Ok(RecordHeader {
            kind,
            size,
            flags,
            id,
            version,
        })
    }
}

/// The 24 bytes that begin a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupHeader {
    /// Including these 24 bytes.
    pub size: u32,
    pub label: [u8; 4],
    pub kind: i32,
}

impl GroupHeader {
    pub fn read(b: &[u8]) -> Result<Self, PluginError> {
        let mut r = Reader::new(b);
        if &r.tag()? != b"GRUP" {
            return Err(PluginError::Corrupt("a group without GRUP".into()));
        }
        let size = r.u32()?;
        let label = r.tag()?;
        let kind = r.i32()?;
        r.take(8)?;
        if (size as usize) < HEADER_LEN {
            return Err(PluginError::Corrupt(format!("a group of {size} bytes")));
        }
        Ok(GroupHeader { size, label, kind })
    }
}

/// One field of a record: four letters and its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subrecord<'a> {
    pub kind: [u8; 4],
    pub data: &'a [u8],
}

/// The subrecords of a record's (inflated) data, in order.
///
/// A subrecord's size is two bytes. One larger than that is written as an
/// `XXXX` subrecord of four bytes holding the real size, and then the
/// subrecord itself with a size field that is not used (0 in the files).
pub fn subrecords(data: &[u8]) -> impl Iterator<Item = Result<Subrecord<'_>, PluginError>> {
    let mut r = Reader::new(data);
    let mut failed = false;
    std::iter::from_fn(move || {
        if failed || r.left() == 0 {
            return None;
        }
        let next = (|| {
            let mut kind = r.tag()?;
            let mut size = r.u16()? as usize;
            if &kind == b"XXXX" {
                if size != 4 {
                    return Err(PluginError::Corrupt(format!("XXXX of {size} bytes")));
                }
                size = r.u32()? as usize;
                kind = r.tag()?;
                r.u16()?;
            }
            Ok(Subrecord {
                kind,
                data: r.take(size)?,
            })
        })();
        failed = next.is_err();
        Some(next)
    })
}

/// A record's data with its compression undone.
pub fn inflate(flags: u32, data: &[u8]) -> Result<Vec<u8>, PluginError> {
    if flags & COMPRESSED == 0 {
        return Ok(data.to_vec());
    }
    let mut r = Reader::new(data);
    let size = r.u32()? as usize;
    if size > MAX_INFLATED {
        return Err(PluginError::TooLarge(format!(
            "a record that inflates to {size} bytes"
        )));
    }
    let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data[4..], size)
        .map_err(|e| PluginError::Inflate(format!("{:?}", e.status)))?;
    if out.len() != size {
        return Err(PluginError::Inflate(format!(
            "{} bytes where {size} were stated",
            out.len()
        )));
    }
    Ok(out)
}

/// A record, its data inflated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub kind: [u8; 4],
    pub flags: u32,
    /// As written in the plugin: the top byte indexes its masters.
    pub id: u32,
    pub version: u16,
    pub data: Vec<u8>,
}

impl Record {
    pub fn subrecords(&self) -> impl Iterator<Item = Result<Subrecord<'_>, PluginError>> {
        subrecords(&self.data)
    }

    pub fn deleted(&self) -> bool {
        self.flags & DELETED != 0
    }
}

/// A form wherever it is referred to from: the plugin that made it (by file
/// name) and its id within that plugin (the low 24 bits). File names are
/// compared without regard to case, as the game does.
#[derive(Debug, Clone, Eq)]
pub struct FormRef {
    pub plugin: String,
    pub id: u32,
}

impl FormRef {
    pub fn new(plugin: &str, id: u32) -> Self {
        FormRef {
            plugin: plugin.to_string(),
            id: id & 0x00FF_FFFF,
        }
    }
}

impl PartialEq for FormRef {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.plugin.eq_ignore_ascii_case(&o.plugin)
    }
}

impl Hash for FormRef {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.id.hash(h);
        for c in self.plugin.bytes() {
            h.write_u8(c.to_ascii_lowercase());
        }
    }
}

impl std::fmt::Display for FormRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:06X}@{}", self.id, self.plugin)
    }
}

/// What a plugin's formIDs can refer to: its masters, in order, and then
/// itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Masters {
    pub own: String,
    pub masters: Vec<String>,
    /// The plugin's names are string-table ids.
    pub localized: bool,
}

impl Masters {
    /// A formID as written in this plugin. The top byte indexes the masters;
    /// past them it is the plugin itself (in the files the plugin's own forms
    /// use exactly the index one past the last master, but any higher one is
    /// taken as its own too, as the game is described to do).
    pub fn resolve(&self, raw: u32) -> FormRef {
        let index = (raw >> 24) as usize;
        let plugin = self.masters.get(index).unwrap_or(&self.own);
        FormRef::new(plugin, raw)
    }

    /// `None` for the null formID 0, which records use for "none".
    pub fn form(&self, raw: u32) -> Option<FormRef> {
        (raw != 0).then(|| self.resolve(raw))
    }
}

/// The `TES4` header record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Header {
    pub flags: u32,
    pub version: f32,
    pub records: u32,
    pub next_id: u32,
    pub author: Option<String>,
    pub masters: Vec<String>,
}

impl Header {
    pub fn esm(&self) -> bool {
        self.flags & ESM != 0
    }
    pub fn light(&self) -> bool {
        self.flags & LIGHT != 0
    }
    /// Its names are string-table ids, not text.
    pub fn localized(&self) -> bool {
        self.flags & LOCALIZED != 0
    }

    /// From the 24-byte header and the data after it.
    pub fn read(head: &RecordHeader, data: &[u8]) -> Result<Self, PluginError> {
        if &head.kind != b"TES4" {
            return Err(PluginError::NotPlugin);
        }
        let data = inflate(head.flags, data)?;
        let mut h = Header {
            flags: head.flags,
            ..Header::default()
        };
        for s in subrecords(&data) {
            let s = s?;
            match &s.kind {
                b"HEDR" => {
                    let mut r = Reader::new(s.data);
                    h.version = r.f32()?;
                    h.records = r.u32()?;
                    h.next_id = r.u32()?;
                }
                b"CNAM" => h.author = Some(zstring(s.data)),
                b"MAST" => h.masters.push(zstring(s.data)),
                _ => {}
            }
        }
        Ok(h)
    }
}

/// What was read of a plugin: its header, and the records of the groups
/// that were asked for.
#[derive(Debug, Clone, Default)]
pub struct Plugin {
    pub header: Header,
    pub names: Masters,
    pub records: Vec<Record>,
    /// Records that could not be read (a compressed one that would not
    /// inflate), by formID as written, with why. The rest of the plugin is
    /// still read.
    pub failures: Vec<(u32, PluginError)>,
    by_form: HashMap<FormRef, usize>,
}

impl Plugin {
    pub fn new(name: &str, header: Header) -> Self {
        Plugin {
            names: Masters {
                own: name.to_string(),
                masters: header.masters.clone(),
                localized: header.localized(),
            },
            header,
            ..Plugin::default()
        }
    }

    pub fn name(&self) -> &str {
        &self.names.own
    }

    pub fn push(&mut self, record: Record) {
        let form = self.names.resolve(record.id);
        self.by_form.insert(form, self.records.len());
        self.records.push(record);
    }

    /// This plugin's copy of a form, if it has one.
    pub fn get(&self, form: &FormRef) -> Option<&Record> {
        self.by_form.get(form).map(|i| &self.records[*i])
    }

    /// This plugin's copy of a form, read as `T`. `None` when it has none
    /// or has a record of another type there.
    pub fn typed<T: crate::records::Typed>(
        &self,
        form: &FormRef,
    ) -> Option<Result<T, PluginError>> {
        let r = self.get(form)?;
        (r.kind == T::KIND).then(|| T::read(r, &self.names))
    }

    /// Every record this plugin holds, with the form each is.
    pub fn forms(&self) -> impl Iterator<Item = (FormRef, &Record)> {
        self.records.iter().map(|r| (self.names.resolve(r.id), r))
    }

    /// The records of one group's data (the bytes after its 24-byte header).
    /// Groups nested inside are stepped into; records of kinds not asked for
    /// there are kept too, since a top-level group holds one kind.
    pub fn read_group(&mut self, mut body: &[u8]) -> Result<(), PluginError> {
        while !body.is_empty() {
            if body.len() < HEADER_LEN {
                return Err(PluginError::Truncated);
            }
            if &body[..4] == b"GRUP" {
                let g = GroupHeader::read(&body[..HEADER_LEN])?;
                if g.size as usize > body.len() {
                    return Err(PluginError::Truncated);
                }
                body = &body[HEADER_LEN..];
                continue;
            }
            let h = RecordHeader::read(&body[..HEADER_LEN])?;
            let end = HEADER_LEN
                .checked_add(h.size as usize)
                .ok_or(PluginError::Truncated)?;
            let data = body.get(HEADER_LEN..end).ok_or(PluginError::Truncated)?;
            match inflate(h.flags, data) {
                Ok(data) => self.push(Record {
                    kind: h.kind,
                    flags: h.flags & !COMPRESSED,
                    id: h.id,
                    version: h.version,
                    data,
                }),
                Err(e) => self.failures.push((h.id, e)),
            }
            body = &body[end..];
        }
        Ok(())
    }
}
