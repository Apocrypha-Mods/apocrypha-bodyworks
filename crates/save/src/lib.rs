//! Skyrim save games (`.ess`): who the character is, the plugins it was
//! played with, and how the character looks.
//!
//! A save is a magic word, a header, a screenshot, and a body. In Special
//! Edition the body is usually compressed as one LZ4 block ([`lz4`]); the
//! original game wrote it as it is. The body holds the plugin lists, a table
//! of where its parts are, three tables of global data, the change forms
//! (what the game changed about every form it touched, the player among
//! them) and an array of full form ids that most references point into
//! ([`RefId`]).
//!
//! Only what this project needs is read into structures: the header, the
//! plugins, the change form index (each form's kind, flags and where its
//! bytes are, nothing parsed until asked) and the player's look
//! ([`look`]). Reading is bounded throughout: every length is checked
//! against what is left, and nothing allocates more than the input could
//! hold or [`MAX_BODY`].
//!
//! Light plugins: a full form id of `0xFE` names one by bits 12 to 23 and
//! its form by the low 12. Across the 88 saves here, 76,498 such ids all
//! index within the save's light plugin list (up to 140 long) and their
//! local ids run 0x019 to 0xFFF. A kind-1 refId never goes past 0x10FF99,
//! so it is Skyrim.esm's alone; the other masters' forms go through the
//! form id array.
//!
//! Worn items and inventory are not read. They are in the player's
//! placed-actor change form (0x000014), whose flags in every save here
//! include `CHANGE_REFR_INVENTORY` (0x20): from near its start it is a run
//! of refId, count and extra data entries as UESP describes, the extra data
//! being where an item is marked worn.
//!
//! Written from the files themselves and from published descriptions of the
//! format (UESP's "Save File Format" pages). Not derived from any other
//! program's source.

pub mod io;
pub mod look;
pub mod lz4;

use io::Reader;
pub use look::{Look, Tint};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SaveError {
    #[error("not a Skyrim save")]
    NotSave,
    #[error("the file ends before its contents do")]
    Truncated,
    #[error("a save of a kind this cannot read ({0})")]
    Unsupported(String),
    #[error("the file disagrees with itself ({0})")]
    Corrupt(String),
}

/// What every save starts with.
pub const MAGIC: &[u8; 13] = b"TESV_SAVEGAME";

/// The largest body this will decompress, 256 MiB. The 88 real Special
/// Edition saves here come out at 9 to 31 MiB, heavily modded games among
/// them; the cap leaves room eight times over while keeping one hostile
/// file from asking a WebAssembly add-on for gigabytes. An LZ4 block can
/// also grow at most 255 times, which bounds smaller files far more tightly.
pub const MAX_BODY: usize = 256 << 20;

/// How the body after the screenshot is stored (Special Edition's header
/// says; the original game's never compresses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Zlib,
    Lz4,
}

/// The save's header: enough to list saves without reading their bodies.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// 9 for the original game (7 and 8 older), 12 for Special Edition.
    pub version: u32,
    pub save_number: u32,
    pub name: String,
    pub level: u32,
    pub location: String,
    /// In-game time played, as the game writes it.
    pub game_date: String,
    /// The player race's editor id.
    pub race_editor_id: String,
    /// 0 male, 1 female.
    pub sex: u16,
    pub current_xp: f32,
    pub level_up_xp: f32,
    /// When it was saved: a Windows FILETIME (100 ns ticks since 1601).
    pub file_time: u64,
    pub shot_width: u32,
    pub shot_height: u32,
    pub compression: Compression,
}

impl Header {
    /// Bytes a screenshot pixel takes: RGB in the original game, RGBA in
    /// Special Edition.
    pub fn bytes_per_pixel(&self) -> usize {
        if self.version >= 12 {
            4
        } else {
            3
        }
    }
}

/// The screenshot, its pixels row by row from the top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screenshot<'a> {
    pub width: u32,
    pub height: u32,
    /// 3 (RGB) or 4 (RGBA).
    pub bytes_per_pixel: usize,
    pub pixels: &'a [u8],
}

/// A save read only as far as its header and screenshot: nothing is
/// decompressed, so it costs little more than finding the bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing<'a> {
    pub header: Header,
    pub screenshot: Screenshot<'a>,
    /// Where the body starts in the file. The location table's offsets
    /// count from the start of the file as if the body sat here
    /// uncompressed: the eight bytes of compression lengths are not
    /// counted (found from all 88 Special Edition saves, where the first
    /// global table's offset is exactly this plus the bytes before it).
    body_start: usize,
}

/// Reads the header and finds the screenshot, decompressing nothing.
pub fn header_only(bytes: &[u8]) -> Result<Listing<'_>, SaveError> {
    let mut r = Reader::new(bytes);
    if r.take(MAGIC.len()).map_err(|_| SaveError::NotSave)? != MAGIC {
        return Err(SaveError::NotSave);
    }
    let size = r.u32()? as usize;
    let start = r.pos();
    let end = start.checked_add(size).ok_or(SaveError::Truncated)?;
    let mut h = Reader::new(bytes.get(..end).ok_or(SaveError::Truncated)?);
    h.take(start)?;
    let version = h.u32()?;
    if !(7..=12).contains(&version) || version == 10 || version == 11 {
        return Err(SaveError::Unsupported(format!("header version {version}")));
    }
    let save_number = h.u32()?;
    let name = h.wstring()?;
    let level = h.u32()?;
    let location = h.wstring()?;
    let game_date = h.wstring()?;
    let race_editor_id = h.wstring()?;
    let sex = h.u16()?;
    let current_xp = h.f32()?;
    let level_up_xp = h.f32()?;
    let file_time = h.u64()?;
    let shot_width = h.u32()?;
    let shot_height = h.u32()?;
    let compression = if version >= 12 {
        match h.u16()? {
            0 => Compression::None,
            1 => Compression::Zlib,
            2 => Compression::Lz4,
            c => return Err(SaveError::Unsupported(format!("compression type {c}"))),
        }
    } else {
        Compression::None
    };
    let header = Header {
        version,
        save_number,
        name,
        level,
        location,
        game_date,
        race_editor_id,
        sex,
        current_xp,
        level_up_xp,
        file_time,
        shot_width,
        shot_height,
        compression,
    };
    let mut r = Reader::at(bytes, end)?;
    let bpp = header.bytes_per_pixel();
    let n = (shot_width as usize)
        .checked_mul(shot_height as usize)
        .and_then(|n| n.checked_mul(bpp))
        .ok_or(SaveError::Truncated)?;
    let pixels = r.take(n)?;
    Ok(Listing {
        screenshot: Screenshot {
            width: shot_width,
            height: shot_height,
            bytes_per_pixel: bpp,
            pixels,
        },
        header,
        body_start: r.pos(),
    })
}

/// Where a form came from, as the save names it: the plugin's file name as
/// the save lists it, and the id local to that plugin (the low 24 bits, or
/// the low 12 for a light plugin).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormRef {
    pub plugin: String,
    pub id: u32,
}

impl std::fmt::Display for FormRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:06X}", self.plugin, self.id)
    }
}

/// What a reference resolves to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Form {
    /// No form (a reference of all zeros).
    None,
    /// A form from a plugin.
    Plugin(FormRef),
    /// A form the game made while playing (full id `0xFF......`).
    Created(u32),
}

/// A three-byte reference to a form, as change forms and their data write
/// one: big-endian, the top two bits its kind, the low 22 its value (UESP;
/// its example, `41 C0 F2` for Skyrim.esm's 0x01C0F2, fixes the byte order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefId(pub u32);

impl RefId {
    pub fn from_bytes(b: [u8; 3]) -> Self {
        RefId(u32::from_be_bytes([0, b[0], b[1], b[2]]))
    }

    /// 0 an index into the form id array (counting from 1, 0 meaning no
    /// form), 1 a form of the first plugin, 2 a form made in game, 3 not
    /// known.
    pub fn kind(self) -> u8 {
        (self.0 >> 22) as u8
    }

    pub fn value(self) -> u32 {
        self.0 & 0x3F_FFFF
    }
}

/// One change form, indexed: where its data is, not yet read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeForm {
    pub ref_id: RefId,
    /// Which parts of the form were changed, and so are in the data, in the
    /// order of their bits (UESP's ChangeFlags).
    pub flags: u32,
    /// The form's kind, the low six bits of its type byte (UESP's change
    /// form types: 0 a placed object, 1 a placed actor, 9 an NPC, ...).
    pub kind: u8,
    pub version: u8,
    /// Where its data starts in the body, and how many bytes it is there.
    pub offset: usize,
    pub length: usize,
    /// When not zero, the data is zlib-compressed and this is its length
    /// uncompressed.
    pub raw_length: usize,
}

/// Change form kinds used here.
pub const KIND_ACHR: u8 = 1;
pub const KIND_NPC: u8 = 9;

/// A whole save, read.
pub struct Save {
    header: Header,
    shot: (u32, u32, usize, Vec<u8>),
    body: Vec<u8>,
    form_version: u8,
    plugins: Vec<String>,
    light_plugins: Vec<String>,
    change_forms: Vec<ChangeForm>,
    form_ids: Vec<u32>,
}

impl Save {
    /// Reads the header and finds the screenshot only (see [`header_only`]).
    pub fn header_only(bytes: &[u8]) -> Result<Listing<'_>, SaveError> {
        header_only(bytes)
    }

    /// Reads a save: decompresses the body, reads its lists and indexes the
    /// change forms.
    pub fn read(bytes: &[u8]) -> Result<Save, SaveError> {
        let listing = header_only(bytes)?;
        let base = listing.body_start;
        let mut r = Reader::at(bytes, base)?;
        let body = match listing.header.compression {
            Compression::None => r.take(r.left())?.to_vec(),
            c => {
                let raw = r.u32()? as usize;
                let packed = r.u32()? as usize;
                let input = r.take(packed)?;
                if raw > MAX_BODY {
                    return Err(SaveError::Unsupported(format!(
                        "a body of {raw} bytes, over the {MAX_BODY} this reads"
                    )));
                }
                if c == Compression::Lz4 {
                    if raw / 255 > packed {
                        return Err(SaveError::Corrupt(
                            "more than LZ4 could ever expand to".into(),
                        ));
                    }
                    lz4::decode_block(input, raw)?
                } else {
                    let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(input, raw)
                        .map_err(|_| SaveError::Corrupt("zlib body".into()))?;
                    if out.len() != raw {
                        return Err(SaveError::Corrupt("zlib body length".into()));
                    }
                    out
                }
            }
        };
        let s = listing.screenshot;
        let shot = (s.width, s.height, s.bytes_per_pixel, s.pixels.to_vec());
        Self::from_body(listing.header, shot, body, base)
    }

    fn from_body(
        header: Header,
        shot: (u32, u32, usize, Vec<u8>),
        body: Vec<u8>,
        base: usize,
    ) -> Result<Save, SaveError> {
        let mut r = Reader::new(&body);
        let form_version = r.u8()?;
        let plugin_size = r.u32()? as usize;
        let plugin_end = r
            .pos()
            .checked_add(plugin_size)
            .ok_or(SaveError::Truncated)?;
        let mut p = Reader::new(body.get(..plugin_end).ok_or(SaveError::Truncated)?);
        p.take(r.pos())?;
        let n = p.u8()? as usize;
        let mut plugins = Vec::with_capacity(n);
        for _ in 0..n {
            plugins.push(p.wstring()?);
        }
        // The light plugin list follows inside the same sized block, from
        // form version 78 (every Special Edition save here from 1.5.97 on,
        // all 78; a block that ends right after the full list has none).
        let mut light_plugins = Vec::new();
        if form_version >= 78 && p.left() >= 2 {
            let n = p.u16()? as usize;
            p.check_count(n, 2)?;
            light_plugins.reserve(n);
            for _ in 0..n {
                light_plugins.push(p.wstring()?);
            }
        }
        if p.left() != 0 {
            return Err(SaveError::Corrupt(format!(
                "{} bytes left in the plugin block",
                p.left()
            )));
        }
        let mut r = Reader::at(&body, plugin_end)?;
        let form_ids_at = r.u32()? as usize;
        let _unknown3_at = r.u32()?;
        let _global1_at = r.u32()?;
        let _global2_at = r.u32()?;
        let change_forms_at = r.u32()? as usize;
        let _global3_at = r.u32()?;
        let _global1_count = r.u32()?;
        let _global2_count = r.u32()?;
        let _global3_count = r.u32()?;
        let change_form_count = r.u32()? as usize;
        let to_body = |at: usize| {
            at.checked_sub(base)
                .filter(|&o| o <= body.len())
                .ok_or_else(|| SaveError::Corrupt(format!("an offset {at} outside the body")))
        };

        let mut f = Reader::at(&body, to_body(form_ids_at)?)?;
        let n = f.u32()? as usize;
        f.check_count(n, 4)?;
        let mut form_ids = Vec::with_capacity(n);
        for _ in 0..n {
            form_ids.push(f.u32()?);
        }

        let mut c = Reader::at(&body, to_body(change_forms_at)?)?;
        // 11 bytes is the smallest a change form can be.
        c.check_count(change_form_count, 11)?;
        let mut change_forms = Vec::with_capacity(change_form_count);
        for _ in 0..change_form_count {
            change_forms.push(read_change_form(&mut c)?);
        }

        Ok(Save {
            header,
            shot,
            body,
            form_version,
            plugins,
            light_plugins,
            change_forms,
            form_ids,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn screenshot(&self) -> Screenshot<'_> {
        Screenshot {
            width: self.shot.0,
            height: self.shot.1,
            bytes_per_pixel: self.shot.2,
            pixels: &self.shot.3,
        }
    }

    /// The version of the forms' data layout (74 in late original-game and
    /// early Special Edition saves, 78 from Special Edition 1.5.97's light
    /// plugins on).
    pub fn form_version(&self) -> u8 {
        self.form_version
    }

    /// The full plugins, in load order: a form id's top byte indexes this.
    pub fn plugins(&self) -> &[String] {
        &self.plugins
    }

    /// The light plugins, in load order: a `0xFE` form id's bits 12 to 23
    /// index this.
    pub fn light_plugins(&self) -> &[String] {
        &self.light_plugins
    }

    /// The body, decompressed: from the form version on. Change form
    /// offsets index this.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn change_forms(&self) -> &[ChangeForm] {
        &self.change_forms
    }

    pub fn form_ids(&self) -> &[u32] {
        &self.form_ids
    }

    /// The change form for a form, by kind.
    pub fn change_form(&self, of: &Form, kind: u8) -> Option<&ChangeForm> {
        self.change_forms
            .iter()
            .find(|c| c.kind == kind && self.resolve(c.ref_id).ok().as_ref() == Some(of))
    }

    /// A change form's data, inflated when it is compressed.
    pub fn data<'a>(&'a self, c: &ChangeForm) -> Result<std::borrow::Cow<'a, [u8]>, SaveError> {
        let end = c.offset.checked_add(c.length).ok_or(SaveError::Truncated)?;
        let raw = self.body.get(c.offset..end).ok_or(SaveError::Truncated)?;
        if c.raw_length == 0 {
            return Ok(std::borrow::Cow::Borrowed(raw));
        }
        if c.raw_length > MAX_BODY {
            return Err(SaveError::Corrupt(
                "a change form larger than a body".into(),
            ));
        }
        let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(raw, c.raw_length)
            .map_err(|_| SaveError::Corrupt("a change form's zlib data".into()))?;
        if out.len() != c.raw_length {
            return Err(SaveError::Corrupt("a change form's length".into()));
        }
        Ok(std::borrow::Cow::Owned(out))
    }

    /// What a reference names.
    pub fn resolve(&self, r: RefId) -> Result<Form, SaveError> {
        resolve(r, &self.form_ids, &self.plugins, &self.light_plugins)
    }

    /// The form a full form id names, by load order.
    pub fn form_of(&self, full: u32) -> Result<Form, SaveError> {
        form_of(full, &self.plugins, &self.light_plugins)
    }

    /// How the player looks (see [`look`]).
    pub fn player(&self) -> Result<Look, SaveError> {
        look::player(self)
    }
}

fn read_change_form(c: &mut Reader) -> Result<ChangeForm, SaveError> {
    let ref_id = c.ref_id()?;
    let flags = c.u32()?;
    let kind_byte = c.u8()?;
    let version = c.u8()?;
    let (length, raw_length) = match kind_byte >> 6 {
        0 => (c.u8()? as usize, c.u8()? as usize),
        1 => (c.u16()? as usize, c.u16()? as usize),
        2 => (c.u32()? as usize, c.u32()? as usize),
        _ => return Err(SaveError::Corrupt("a change form length of size 3".into())),
    };
    let offset = c.pos();
    c.take(length)?;
    Ok(ChangeForm {
        ref_id,
        flags,
        kind: kind_byte & 0x3F,
        version,
        offset,
        length,
        raw_length,
    })
}

/// What a reference names, given the save's form id array and plugin lists.
pub fn resolve(
    r: RefId,
    form_ids: &[u32],
    plugins: &[String],
    light: &[String],
) -> Result<Form, SaveError> {
    match r.kind() {
        0 => {
            if r.value() == 0 {
                return Ok(Form::None);
            }
            let full = *form_ids
                .get(r.value() as usize - 1)
                .ok_or_else(|| SaveError::Corrupt(format!("form id index {}", r.value())))?;
            form_of(full, plugins, light)
        }
        1 => {
            let plugin = plugins
                .first()
                .cloned()
                .unwrap_or_else(|| "Skyrim.esm".into());
            Ok(Form::Plugin(FormRef {
                plugin,
                id: r.value(),
            }))
        }
        2 => Ok(Form::Created(0xFF00_0000 | r.value())),
        _ => Err(SaveError::Corrupt("a reference of kind 3".into())),
    }
}

/// The form a full form id names by load order: the top byte indexes the
/// full plugins; `0xFE` is a light plugin, indexed by bits 12 to 23, its id
/// the low 12; `0xFF` was made in game.
pub fn form_of(full: u32, plugins: &[String], light: &[String]) -> Result<Form, SaveError> {
    let top = full >> 24;
    match top {
        0xFF => Ok(Form::Created(full)),
        0xFE => {
            let i = ((full >> 12) & 0xFFF) as usize;
            let plugin = light.get(i).ok_or_else(|| {
                SaveError::Corrupt(format!("light plugin {i} of {}", light.len()))
            })?;
            Ok(Form::Plugin(FormRef {
                plugin: plugin.clone(),
                id: full & 0xFFF,
            }))
        }
        i => {
            let plugin = plugins
                .get(i as usize)
                .ok_or_else(|| SaveError::Corrupt(format!("plugin {i} of {}", plugins.len())))?;
            Ok(Form::Plugin(FormRef {
                plugin: plugin.clone(),
                id: full & 0xFF_FFFF,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// The test build's allocator, which notes the largest allocation each
    /// thread asks for, so a test can hold a read to allocating no more than
    /// its input could hold. (A count left unchecked asks for gigabytes,
    /// which Linux grants without touching, so only this sees it.)
    struct Noting;

    thread_local! {
        static LARGEST: Cell<usize> = const { Cell::new(0) };
    }

    fn note(n: usize) {
        let _ = LARGEST.try_with(|l| l.set(l.get().max(n)));
    }

    unsafe impl GlobalAlloc for Noting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            note(layout.size());
            System.alloc(layout)
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            note(layout.size());
            System.alloc_zeroed(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            note(new_size);
            System.realloc(ptr, layout, new_size)
        }
    }

    #[global_allocator]
    static NOTING: Noting = Noting;

    /// The largest single allocation `f` made on this thread.
    pub(crate) fn largest_allocation<R>(f: impl FnOnce() -> R) -> (R, usize) {
        LARGEST.with(|l| l.set(0));
        let r = f();
        (r, LARGEST.with(|l| l.get()))
    }

    fn wstr(out: &mut Vec<u8>, s: &str) {
        out.extend((s.len() as u16).to_le_bytes());
        out.extend(s.as_bytes());
    }

    /// One change form for [`Spec`]: its refId bytes, flags, type byte and
    /// data, and whether the data is stored zlib-compressed.
    #[derive(Clone)]
    pub(crate) struct FormSpec {
        pub r: [u8; 3],
        pub flags: u32,
        pub ty: u8,
        pub data: Vec<u8>,
        pub zlib: bool,
    }

    /// A save, built byte by byte: the header (version 12 unless said),
    /// a 2 by 1 screenshot, and a body of the plugin block, the location
    /// table, the change forms and the form id array, stored as the
    /// compression says. The location table's offsets count from the start
    /// of the file as if the body were stored as it is.
    #[derive(Clone)]
    pub(crate) struct Spec {
        pub version: u32,
        pub compression: u16,
        pub form_version: u8,
        pub plugins: Vec<String>,
        /// `None` writes no light plugin list at all.
        pub light: Option<Vec<String>>,
        /// Bytes after the lists, still inside the plugin block.
        pub plugin_tail: Vec<u8>,
        pub sex: u16,
        pub form_ids: Vec<u32>,
        pub forms: Vec<FormSpec>,
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec {
                version: 12,
                compression: 0,
                form_version: 78,
                plugins: vec!["Skyrim.esm".into(), "Mod.esp".into()],
                light: Some(vec!["Light.esl".into()]),
                plugin_tail: Vec::new(),
                sex: 1,
                form_ids: Vec::new(),
                forms: Vec::new(),
            }
        }
    }

    /// Literals only, as one LZ4 sequence: a valid block of any length.
    pub(crate) fn lz4_literals(data: &[u8]) -> Vec<u8> {
        let n = data.len();
        let mut out = vec![(n.min(15) as u8) << 4];
        if n >= 15 {
            let mut rest = n - 15;
            while rest >= 255 {
                out.push(255);
                rest -= 255;
            }
            out.push(rest as u8);
        }
        out.extend(data);
        out
    }

    pub(crate) fn zlib(data: &[u8]) -> Vec<u8> {
        miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
    }

    impl Spec {
        /// Where the body starts: what the location table counts from.
        pub fn base(&self) -> usize {
            self.header().len() + MAGIC.len() + 4 + self.shot_len()
        }

        fn shot_len(&self) -> usize {
            if self.version >= 12 {
                8
            } else {
                6
            }
        }

        fn header(&self) -> Vec<u8> {
            let mut head = Vec::new();
            head.extend(self.version.to_le_bytes());
            head.extend(7u32.to_le_bytes());
            wstr(&mut head, "Someone");
            head.extend(3u32.to_le_bytes());
            wstr(&mut head, "Somewhere");
            wstr(&mut head, "000.01.02");
            wstr(&mut head, "NordRace");
            head.extend(self.sex.to_le_bytes());
            head.extend(0f32.to_le_bytes());
            head.extend(100f32.to_le_bytes());
            head.extend(0u64.to_le_bytes());
            head.extend(2u32.to_le_bytes());
            head.extend(1u32.to_le_bytes());
            if self.version >= 12 {
                head.extend(self.compression.to_le_bytes());
            }
            head
        }

        /// The body as it is once decompressed, and where its location
        /// table is in it.
        pub fn body(&self) -> (Vec<u8>, usize) {
            let base = self.base();
            let mut body = vec![self.form_version];
            let mut pl = vec![self.plugins.len() as u8];
            for p in &self.plugins {
                wstr(&mut pl, p);
            }
            if let Some(light) = &self.light {
                pl.extend((light.len() as u16).to_le_bytes());
                for p in light {
                    wstr(&mut pl, p);
                }
            }
            pl.extend(&self.plugin_tail);
            body.extend((pl.len() as u32).to_le_bytes());
            body.extend(pl);
            let table_at = body.len();
            body.extend([0u8; 100]);
            let forms_at = body.len();
            for f in &self.forms {
                let stored = if f.zlib {
                    zlib(&f.data)
                } else {
                    f.data.clone()
                };
                let raw = if f.zlib { f.data.len() } else { 0 };
                body.extend(f.r);
                body.extend(f.flags.to_le_bytes());
                body.push(f.ty);
                body.push(78);
                match f.ty >> 6 {
                    0 => body.extend([stored.len() as u8, raw as u8]),
                    1 => {
                        body.extend((stored.len() as u16).to_le_bytes());
                        body.extend((raw as u16).to_le_bytes());
                    }
                    _ => {
                        body.extend((stored.len() as u32).to_le_bytes());
                        body.extend((raw as u32).to_le_bytes());
                    }
                }
                body.extend(stored);
            }
            let ids_at = body.len();
            body.extend((self.form_ids.len() as u32).to_le_bytes());
            for f in &self.form_ids {
                body.extend(f.to_le_bytes());
            }
            let put = |body: &mut Vec<u8>, i: usize, v: u32| {
                body[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes())
            };
            put(&mut body, 0, (base + ids_at) as u32);
            put(&mut body, 4, (base + forms_at) as u32);
            put(&mut body, 9, self.forms.len() as u32);
            (body, table_at)
        }

        /// The whole file, with this body stored as the compression says.
        pub fn with_body(&self, body: &[u8]) -> Vec<u8> {
            let head = self.header();
            let mut out = MAGIC.to_vec();
            out.extend((head.len() as u32).to_le_bytes());
            out.extend(head);
            out.extend(&[1, 2, 3, 4, 5, 6, 7, 8][..self.shot_len()]);
            let packed = match self.compression {
                0 => {
                    out.extend(body);
                    return out;
                }
                1 => zlib(body),
                _ => lz4_literals(body),
            };
            out.extend((body.len() as u32).to_le_bytes());
            out.extend((packed.len() as u32).to_le_bytes());
            out.extend(packed);
            out
        }

        pub fn bytes(&self) -> Vec<u8> {
            self.with_body(&self.body().0)
        }
    }

    /// A Special Edition save with no compression, two plugins, one light
    /// plugin, the given form ids and change forms (each its refId bytes,
    /// flags, type byte and data).
    pub(crate) fn build(form_ids: &[u32], forms: &[([u8; 3], u32, u8, Vec<u8>)]) -> Vec<u8> {
        Spec {
            form_ids: form_ids.to_vec(),
            forms: forms
                .iter()
                .map(|(r, flags, ty, data)| FormSpec {
                    r: *r,
                    flags: *flags,
                    ty: *ty,
                    data: data.clone(),
                    zlib: false,
                })
                .collect(),
            ..Spec::default()
        }
        .bytes()
    }

    #[test]
    fn a_built_save_reads() {
        let forms = vec![
            ([0x40, 0, 7], 0, KIND_NPC, vec![9; 5]),
            ([0, 0, 1], 2, 0x40 | KIND_ACHR, vec![1, 2, 3]),
            ([0, 0, 2], 3, 0x80, vec![]),
        ];
        let bytes = build(&[0x0100_0ABC, 0xFE00_0234], &forms);
        let l = header_only(&bytes).unwrap();
        assert_eq!(l.header.name, "Someone");
        assert_eq!(l.header.race_editor_id, "NordRace");
        assert_eq!(l.screenshot.pixels, &[1, 2, 3, 4, 5, 6, 7, 8]);
        let s = Save::read(&bytes).unwrap();
        assert_eq!(s.plugins(), ["Skyrim.esm", "Mod.esp"]);
        assert_eq!(s.light_plugins(), ["Light.esl"]);
        assert_eq!(s.form_ids(), [0x0100_0ABC, 0xFE00_0234]);
        let c = s.change_forms();
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].kind, c[0].length), (KIND_NPC, 5));
        assert_eq!((c[1].kind, c[1].length, c[1].flags), (KIND_ACHR, 3, 2));
        assert_eq!(&*s.data(&c[1]).unwrap(), &[1, 2, 3]);
        assert_eq!(c[2].length, 0);
        assert_eq!(
            s.resolve(c[1].ref_id).unwrap(),
            Form::Plugin(FormRef {
                plugin: "Mod.esp".into(),
                id: 0xABC
            })
        );
        assert_eq!(
            s.resolve(c[2].ref_id).unwrap(),
            Form::Plugin(FormRef {
                plugin: "Light.esl".into(),
                id: 0x234
            })
        );
        assert_eq!(
            s.change_form(
                &Form::Plugin(FormRef {
                    plugin: "Skyrim.esm".into(),
                    id: 7
                }),
                KIND_NPC
            ),
            Some(&c[0])
        );
    }

    #[test]
    fn every_cut_and_bad_magic_is_an_error() {
        let bytes = build(&[0x0100_0001], &[([0x40, 0, 7], 0, KIND_NPC, vec![1; 40])]);
        for n in 0..bytes.len() {
            assert!(Save::read(&bytes[..n]).is_err(), "cut at {n}");
        }
        let mut b = bytes.clone();
        b[0] = b'X';
        assert_eq!(Save::read(&b).err(), Some(SaveError::NotSave));
    }

    #[test]
    fn bad_headers_are_errors() {
        let bytes = build(&[], &[]);
        let mut b = bytes.clone();
        b[17] = 11; // header version 11
        assert!(matches!(header_only(&b), Err(SaveError::Unsupported(_))));
        let mut b = bytes.clone();
        b[13..17].copy_from_slice(&u32::MAX.to_le_bytes()); // header size
        assert!(header_only(&b).is_err());
    }

    #[test]
    fn offsets_and_counts_out_of_range_are_errors() {
        let bytes = build(&[1], &[]);
        let l = header_only(&bytes).unwrap();
        // The table sits after formVersion, the plugin block's size and
        // the block.
        let block = u32::from_le_bytes(
            bytes[l.body_start + 1..l.body_start + 5]
                .try_into()
                .unwrap(),
        );
        let table = l.body_start + 5 + block as usize;
        for (field, v) in [(0, 0u32), (0, u32::MAX), (4, u32::MAX), (9, 1_000_000)] {
            let mut b = bytes.clone();
            b[table + 4 * field..table + 4 * field + 4].copy_from_slice(&v.to_le_bytes());
            assert!(Save::read(&b).is_err(), "field {field} = {v}");
        }
    }

    #[test]
    fn lz4_bodies_are_bounded() {
        let mut bytes = build(&[], &[]);
        let l = header_only(&bytes).unwrap();
        let at = l.body_start;
        // Mark it LZ4 and claim a body far larger than 255 times its input.
        let c = at - 8 - 2;
        bytes[c..c + 2].copy_from_slice(&2u16.to_le_bytes());
        let mut b = bytes[..at].to_vec();
        b.extend(1_000_000u32.to_le_bytes());
        b.extend(2u32.to_le_bytes());
        b.extend([0x10, 0]);
        assert!(matches!(Save::read(&b), Err(SaveError::Corrupt(_))));
        let mut b = bytes[..at].to_vec();
        b.extend((MAX_BODY as u32 + 1).to_le_bytes());
        b.extend(0u32.to_le_bytes());
        assert!(matches!(Save::read(&b), Err(SaveError::Unsupported(_))));
    }

    #[test]
    fn change_form_length_sizes() {
        for (ty, len) in [
            (0x00u8, vec![3u8, 0]),
            (0x40, vec![3, 0, 0, 0]),
            (0x80, vec![3, 0, 0, 0, 0, 0, 0, 0]),
        ] {
            let mut b = vec![0x40, 0, 7, 0, 0, 0, 0, ty | 9, 78];
            b.extend(&len);
            b.extend([7, 8, 9, 10]);
            let mut r = Reader::new(&b);
            let c = read_change_form(&mut r).unwrap();
            assert_eq!(
                (c.kind, c.length, c.raw_length, c.offset),
                (9, 3, 0, 9 + len.len())
            );
            assert_eq!(r.left(), 1);
        }
        let mut r = Reader::new(&[0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0]);
        assert!(read_change_form(&mut r).is_err());
        let mut r = Reader::new(&[0, 0, 0, 0, 0, 0, 0, 0x00, 0, 9, 0]);
        assert_eq!(read_change_form(&mut r).err(), Some(SaveError::Truncated));
    }

    #[test]
    fn ref_ids() {
        let plugins = ["Skyrim.esm".to_string(), "Mod.esp".to_string()];
        let light = ["A.esl".to_string(), "B.esl".to_string()];
        let ids = [
            0x0100_0ABC,
            0xFE00_1FFF,
            0xFF00_0042,
            0x0500_0001,
            0xFE00_5001,
        ];
        let r = |b: [u8; 3]| resolve(RefId::from_bytes(b), &ids, &plugins, &light);
        // UESP's example: 41 C0 F2 is Skyrim.esm's 0x01C0F2.
        assert_eq!(RefId::from_bytes([0x41, 0xC0, 0xF2]).kind(), 1);
        assert_eq!(
            r([0x41, 0xC0, 0xF2]).unwrap(),
            Form::Plugin(FormRef {
                plugin: "Skyrim.esm".into(),
                id: 0x01C0F2
            })
        );
        assert_eq!(r([0, 0, 0]).unwrap(), Form::None);
        assert_eq!(
            r([0, 0, 1]).unwrap(),
            Form::Plugin(FormRef {
                plugin: "Mod.esp".into(),
                id: 0xABC
            })
        );
        assert_eq!(
            r([0, 0, 2]).unwrap(),
            Form::Plugin(FormRef {
                plugin: "B.esl".into(),
                id: 0xFFF
            })
        );
        assert_eq!(r([0, 0, 3]).unwrap(), Form::Created(0xFF00_0042));
        assert!(r([0, 0, 4]).is_err()); // plugin 5 of 2
        assert!(r([0, 0, 5]).is_err()); // light plugin 5 of 2
        assert!(r([0, 0, 6]).is_err()); // past the array
        assert_eq!(r([0x80, 0x12, 0x34]).unwrap(), Form::Created(0xFF00_1234));
        assert!(r([0xC0, 0, 1]).is_err());
    }

    #[test]
    fn vsval_and_wstring() {
        let mut r = Reader::new(&[0x24, 0x05, 0x01, 0x02, 0x00, 0x01, 0x03]);
        assert_eq!(r.vsval().unwrap(), 9);
        assert_eq!(r.vsval().unwrap(), 0x0105 >> 2);
        assert_eq!(r.vsval().unwrap(), 0x010002 >> 2);
        assert!(r.vsval().is_err()); // kind 3
        let mut r = Reader::new(&[2, 0, b'h', 0xE9]);
        assert_eq!(r.wstring().unwrap(), "h\u{e9}");
        assert!(Reader::new(&[5, 0, b'h']).wstring().is_err());
    }

    fn form(r: [u8; 3], ty: u8, data: Vec<u8>) -> FormSpec {
        FormSpec {
            r,
            flags: 0x20,
            ty,
            data,
            zlib: false,
        }
    }

    /// A save with something in each of its tables.
    fn full_spec() -> Spec {
        Spec {
            form_ids: vec![0x0100_0ABC, 0xFE00_0234, 0x0000_0007],
            forms: vec![
                form([0x40, 0, 7], KIND_NPC, vec![9; 5]),
                form([0, 0, 1], 0x40 | KIND_ACHR, (0..=255).collect()),
                form([0, 0, 2], 0x80, vec![]),
            ],
            ..Spec::default()
        }
    }

    fn same_save(a: &Save, b: &Save) {
        assert_eq!(a.form_version(), b.form_version());
        assert_eq!(a.plugins(), b.plugins());
        assert_eq!(a.light_plugins(), b.light_plugins());
        assert_eq!(a.form_ids(), b.form_ids());
        assert_eq!(a.change_forms(), b.change_forms());
        assert_eq!(a.body(), b.body());
        for c in a.change_forms() {
            assert_eq!(a.data(c).unwrap(), b.data(c).unwrap());
        }
    }

    #[test]
    fn zlib_and_lz4_bodies_read_as_the_body_stored_as_it_is() {
        let stored = full_spec();
        let plain = Save::read(&stored.bytes()).unwrap();
        assert_eq!(plain.form_ids(), [0x0100_0ABC, 0xFE00_0234, 7]);
        for (c, kind) in [(1, Compression::Zlib), (2, Compression::Lz4)] {
            let spec = Spec {
                compression: c,
                ..stored.clone()
            };
            let s = Save::read(&spec.bytes()).unwrap();
            assert_eq!(s.header().compression, kind);
            same_save(&s, &plain);
        }
    }

    /// The location table's offsets count from where the body starts as if
    /// it were stored as it is: in a compressed save the eight bytes of
    /// lengths before it are not counted (found from the real saves).
    #[test]
    fn compressed_offsets_do_not_count_the_eight_length_bytes() {
        for c in [1, 2] {
            let spec = Spec {
                compression: c,
                ..full_spec()
            };
            let (mut body, table) = spec.body();
            assert_eq!(
                Save::read(&spec.with_body(&body)).unwrap().form_ids(),
                [0x0100_0ABC, 0xFE00_0234, 7]
            );
            // The same body with its offsets counting those eight bytes is
            // another save: what it points at is eight bytes on.
            for field in [0, 4] {
                let at = table + 4 * field;
                let v = u32::from_le_bytes(body[at..at + 4].try_into().unwrap());
                body[at..at + 4].copy_from_slice(&(v + 8).to_le_bytes());
            }
            let shifted = Save::read(&spec.with_body(&body));
            assert!(
                shifted.as_ref().map(|s| s.form_ids().to_vec()).ok()
                    != Some(vec![0x0100_0ABC, 0xFE00_0234, 7]),
                "compression {c}"
            );
        }
    }

    #[test]
    fn a_compressed_body_must_come_out_at_its_stated_length() {
        for c in [1u16, 2] {
            let spec = Spec {
                compression: c,
                ..full_spec()
            };
            let good = spec.bytes();
            let at = header_only(&good).unwrap().body_start;
            let raw = u32::from_le_bytes(good[at..at + 4].try_into().unwrap());
            for stated in [raw - 1, raw + 1] {
                let mut b = good.clone();
                b[at..at + 4].copy_from_slice(&stated.to_le_bytes());
                assert!(
                    matches!(Save::read(&b), Err(SaveError::Corrupt(_))),
                    "compression {c}, {stated} for {raw}"
                );
            }
        }
    }

    /// The bytes of a compressed save whose body claims `raw` bytes from
    /// `packed`.
    fn claiming(compression: u16, raw: u32, packed: &[u8]) -> Vec<u8> {
        let spec = Spec {
            compression,
            ..Spec::default()
        };
        let mut b = spec.bytes();
        b.truncate(spec.base());
        b.extend(raw.to_le_bytes());
        b.extend((packed.len() as u32).to_le_bytes());
        b.extend(packed);
        b
    }

    /// An LZ4 block grows at most 255 times: a claim of more is refused
    /// before decoding, one of exactly that much is decoded (and here fails
    /// for want of bytes).
    #[test]
    fn an_lz4_body_may_claim_255_times_its_input_and_no_more() {
        for (raw, packed) in [(255, 1), (509, 1), (510, 2)] {
            let b = claiming(2, raw, &vec![0xF0; packed]);
            assert_eq!(Save::read(&b).err(), Some(SaveError::Truncated), "{raw}");
        }
        for (raw, packed) in [(510, 1), (1_000_000, 2)] {
            let b = claiming(2, raw, &vec![0xF0; packed]);
            assert!(
                matches!(Save::read(&b), Err(SaveError::Corrupt(m)) if m.contains("expand")),
                "{raw}"
            );
        }
    }

    /// 256 MiB is the most a body may claim; zlib's claims are bounded by
    /// it alone (its input is garbage here, so a claim within it fails to
    /// inflate).
    #[test]
    fn a_body_may_claim_256_mib_and_no_more() {
        let cap = 256u32 << 20;
        for c in [1, 2] {
            for raw in [cap + 1, u32::MAX] {
                assert!(
                    matches!(
                        Save::read(&claiming(c, raw, &[0; 4])),
                        Err(SaveError::Unsupported(_))
                    ),
                    "compression {c}, {raw}"
                );
            }
        }
        assert_eq!(
            Save::read(&claiming(1, cap, &[0; 4])).err(),
            Some(SaveError::Corrupt("zlib body".into()))
        );
    }

    #[test]
    fn header_versions_and_compression_types() {
        for v in [6, 10, 11, 13, 0, u32::MAX] {
            let b = Spec {
                version: v,
                ..Spec::default()
            }
            .bytes();
            assert!(
                matches!(header_only(&b), Err(SaveError::Unsupported(_))),
                "version {v}"
            );
        }
        // The original game's saves: three bytes a pixel, no compression
        // field, the body as it is.
        for v in [7, 8, 9] {
            let spec = Spec {
                version: v,
                ..full_spec()
            };
            let s = Save::read(&spec.bytes()).unwrap();
            assert_eq!(s.header().compression, Compression::None);
            assert_eq!(s.screenshot().bytes_per_pixel, 3);
            assert_eq!(s.screenshot().pixels, [1, 2, 3, 4, 5, 6]);
            assert_eq!(s.form_ids(), [0x0100_0ABC, 0xFE00_0234, 7]);
        }
        let s = Save::read(&full_spec().bytes()).unwrap();
        assert_eq!(s.screenshot().bytes_per_pixel, 4);
        assert_eq!(s.screenshot().pixels.len(), 8);
        for c in [3, 0xFFFF] {
            let b = Spec {
                compression: c,
                ..Spec::default()
            }
            .bytes();
            assert!(
                matches!(header_only(&b), Err(SaveError::Unsupported(_))),
                "{c}"
            );
        }
    }

    /// From form version 78 the light plugin list follows the full list
    /// inside the plugin block's size; a block that ends after the full list
    /// has none, and nothing may be left over.
    #[test]
    fn the_light_plugin_list_is_inside_the_plugin_block_from_form_version_78() {
        let read = |form_version, light: Option<Vec<String>>, tail: &[u8]| {
            Save::read(
                &Spec {
                    form_version,
                    light,
                    plugin_tail: tail.to_vec(),
                    ..full_spec()
                }
                .bytes(),
            )
        };
        let two = Some(vec!["A.esl".to_string(), "B.esl".to_string()]);
        let s = read(78, two.clone(), &[]).unwrap();
        assert_eq!(s.plugins(), ["Skyrim.esm", "Mod.esp"]);
        assert_eq!(s.light_plugins(), ["A.esl", "B.esl"]);
        assert_eq!(read(79, two.clone(), &[]).unwrap().light_plugins().len(), 2);
        // The light list comes in from the block, so the table after it
        // still reads.
        assert_eq!(s.form_ids().len(), 3);
        let s = read(78, None, &[]).unwrap();
        assert!(s.light_plugins().is_empty());
        assert_eq!(s.form_ids().len(), 3);
        // Before 78 there is no light list: two bytes more are left over,
        // not an empty list.
        let s = read(74, None, &[]).unwrap();
        assert!(s.light_plugins().is_empty());
        for tail in [&[0u8, 0][..], &[0], &[0, 0, 0]] {
            assert!(
                matches!(read(74, None, tail), Err(SaveError::Corrupt(_))),
                "{tail:?}"
            );
        }
        // At 78, a byte past the light list, or one byte where a count
        // would be, is left over.
        assert!(matches!(read(78, two, &[0]), Err(SaveError::Corrupt(_))));
        assert!(matches!(read(78, None, &[0]), Err(SaveError::Corrupt(_))));
        // A light list longer than its block is cut off by the block.
        let b = Spec {
            light: Some(vec!["A.esl".into()]),
            ..full_spec()
        };
        let (mut body, _) = b.body();
        let count_at = 1 + 4 + 1 + (2 + 10) + (2 + 7);
        body[count_at] = 2;
        assert_eq!(
            Save::read(&b.with_body(&body)).err(),
            Some(SaveError::Truncated)
        );
        // And one of 65,535 is refused before a place is made for them.
        body[count_at..count_at + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        let bytes = b.with_body(&body);
        let (r, largest) = largest_allocation(|| Save::read(&bytes));
        assert_eq!(r.err(), Some(SaveError::Truncated));
        assert!(largest <= 4 * bytes.len(), "{largest}");
    }

    /// An empty change form table may start where the body ends.
    #[test]
    fn a_table_may_start_at_the_very_end_of_the_body() {
        let spec = Spec {
            form_ids: vec![1, 2],
            ..Spec::default()
        };
        let (mut body, table) = spec.body();
        let end = (spec.base() + body.len()) as u32;
        body[table + 16..table + 20].copy_from_slice(&end.to_le_bytes());
        let s = Save::read(&spec.with_body(&body)).unwrap();
        assert!(s.change_forms().is_empty());
        assert_eq!(s.form_ids(), [1, 2]);
        // One past it is outside, and so is anything before the body, even
        // with nothing to read there.
        let start = spec.base() as u32;
        for at in [end + 1, start - 1, 0] {
            body[table + 16..table + 20].copy_from_slice(&at.to_le_bytes());
            assert!(
                matches!(
                    Save::read(&spec.with_body(&body)),
                    Err(SaveError::Corrupt(_))
                ),
                "{at}"
            );
        }
        body[table + 16..table + 20].copy_from_slice(&start.to_le_bytes());
        assert!(Save::read(&spec.with_body(&body)).is_ok());
    }

    /// Counts that the bytes left could never hold are refused before
    /// anything is allocated for them (a panic or an abort otherwise).
    #[test]
    fn counts_past_what_is_left_are_refused_before_allocating() {
        let spec = Spec {
            form_ids: vec![1],
            forms: vec![form([0, 0, 1], 0, vec![1, 2])],
            ..Spec::default()
        };
        let (body, table) = spec.body();
        let ids_at =
            u32::from_le_bytes(body[table..table + 4].try_into().unwrap()) as usize - spec.base();
        for n in [u32::MAX, 0x4000_0000, 2] {
            for at in [ids_at, table + 36] {
                let mut b = body.clone();
                b[at..at + 4].copy_from_slice(&n.to_le_bytes());
                let b = spec.with_body(&b);
                let (r, largest) = largest_allocation(|| Save::read(&b));
                assert_eq!(r.err(), Some(SaveError::Truncated), "{n} at {at}");
                assert!(largest <= 4 * b.len(), "{largest} bytes for {n} at {at}");
            }
        }
        // Exactly what is left is read.
        assert_eq!(Save::read(&spec.bytes()).unwrap().form_ids(), [1]);
    }

    #[test]
    fn a_change_form_is_found_by_its_kind_as_well_as_its_form() {
        let spec = Spec {
            forms: vec![
                form([0x40, 0, 0x14], KIND_NPC, vec![1]),
                form([0x40, 0, 0x14], KIND_ACHR, vec![2]),
                form([0x40, 0, 0x15], KIND_ACHR, vec![3]),
            ],
            ..Spec::default()
        };
        let s = Save::read(&spec.bytes()).unwrap();
        let at = |id| {
            Form::Plugin(FormRef {
                plugin: "Skyrim.esm".into(),
                id,
            })
        };
        let data = |id, kind| s.change_form(&at(id), kind).map(|c| s.data(c).unwrap()[0]);
        assert_eq!(data(0x14, KIND_NPC), Some(1));
        assert_eq!(data(0x14, KIND_ACHR), Some(2));
        assert_eq!(data(0x15, KIND_ACHR), Some(3));
        assert_eq!(data(0x15, KIND_NPC), None);
        assert_eq!(data(0x16, KIND_ACHR), None);
    }

    #[test]
    fn compressed_change_form_data_is_inflated_to_its_stated_length() {
        let data: Vec<u8> = (0..300u32).map(|i| (i * 7) as u8).collect();
        let mut zipped = form([0x40, 0, 7], 0x40 | KIND_NPC, data.clone());
        zipped.zlib = true;
        let spec = Spec {
            forms: vec![zipped, form([0x40, 0, 8], KIND_NPC, vec![5, 6])],
            ..Spec::default()
        };
        let s = Save::read(&spec.bytes()).unwrap();
        let c = s.change_forms()[0];
        assert_eq!(c.raw_length, 300);
        assert!(c.length < 300);
        let out = s.data(&c).unwrap();
        assert!(matches!(out, std::borrow::Cow::Owned(_)));
        assert_eq!(&*out, &data[..]);
        // Stored as it is, the data is the body's own bytes.
        let c1 = s.change_forms()[1];
        assert!(matches!(
            s.data(&c1).unwrap(),
            std::borrow::Cow::Borrowed(&[5, 6])
        ));
        // A stated length either way of what it inflates to is refused, as
        // is one over the body's cap.
        for raw in [299, 301, MAX_BODY + 1] {
            let wrong = ChangeForm {
                raw_length: raw,
                ..c
            };
            assert!(
                matches!(s.data(&wrong), Err(SaveError::Corrupt(_))),
                "{raw}"
            );
        }
        // Data that runs past the body.
        let past = ChangeForm {
            length: s.body().len(),
            ..c
        };
        assert_eq!(s.data(&past).err(), Some(SaveError::Truncated));
    }

    /// Resolution at the edges: index 0, the last index, one past it; the
    /// light plugin's twelve bits; forms made in game.
    #[test]
    fn ref_ids_at_the_edges() {
        let plugins: Vec<String> = (0..3).map(|i| format!("P{i}.esp")).collect();
        let light: Vec<String> = (0..0x1000).map(|i| format!("L{i:03X}.esl")).collect();
        let ids = [
            0x0200_0001,
            0xFEAB_C123,
            0xFEFF_FFFF,
            0xFE00_0000,
            0xFF12_3456,
            0x0300_0001,
        ];
        let r = |v: u32| resolve(RefId(v), &ids, &plugins, &light);
        let at = |p: &str, id| {
            Ok(Form::Plugin(FormRef {
                plugin: p.into(),
                id,
            }))
        };
        assert_eq!(r(0), Ok(Form::None));
        assert_eq!(r(1), at("P2.esp", 1));
        assert_eq!(r(2), at("LABC.esl", 0x123));
        assert_eq!(r(3), at("LFFF.esl", 0xFFF));
        assert_eq!(r(4), at("L000.esl", 0));
        assert_eq!(r(5), Ok(Form::Created(0xFF12_3456)));
        // The last index names a plugin past the list.
        assert!(matches!(r(6), Err(SaveError::Corrupt(_))));
        assert!(matches!(r(7), Err(SaveError::Corrupt(_))));
        assert!(matches!(r(0x3F_FFFF), Err(SaveError::Corrupt(_))));
        // With one light plugin fewer, 0xFFF is past the list.
        assert!(matches!(
            form_of(0xFEFF_F000, &plugins, &light[..0xFFF]),
            Err(SaveError::Corrupt(_))
        ));
        assert_eq!(
            form_of(0xFEFF_E000, &plugins, &light[..0xFFF]),
            at("LFFE.esl", 0)
        );
        // Kind 1 is the first plugin's, all 22 bits of it; kind 2 made in
        // game, under 0xFF.
        assert_eq!(r(0x7F_FFFF), at("P0.esp", 0x3F_FFFF));
        assert_eq!(r(0x40_0000), at("P0.esp", 0));
        assert_eq!(r(0xBF_FFFF), Ok(Form::Created(0xFF3F_FFFF)));
        assert!(r(0xC0_0000).is_err());
        assert_eq!(
            resolve(RefId(0x40_0007), &[], &[], &[]),
            at("Skyrim.esm", 7)
        );
        // An array of one: index 1 is all there is.
        assert_eq!(resolve(RefId(1), &[7], &plugins, &light), at("P0.esp", 7));
        assert!(resolve(RefId(2), &[7], &plugins, &light).is_err());
    }

    #[test]
    fn vsvals_at_the_edges_of_their_sizes() {
        let v = |b: &[u8]| {
            let mut r = Reader::new(b);
            r.vsval().map(|v| (v, r.left()))
        };
        assert_eq!(v(&[0x00]), Ok((0, 0)));
        assert_eq!(v(&[0xFC, 9]), Ok((63, 1)));
        assert_eq!(v(&[0x01, 0x01]), Ok((0x40, 0)));
        assert_eq!(v(&[0xFD, 0xFF]), Ok((0x3FFF, 0)));
        assert_eq!(v(&[0x02, 0x00, 0x01]), Ok((0x4000, 0)));
        assert_eq!(v(&[0xFE, 0xFF, 0xFF, 9]), Ok((0x3F_FFFF, 1)));
        assert!(matches!(
            v(&[0xFF, 0xFF, 0xFF, 0xFF]),
            Err(SaveError::Corrupt(_))
        ));
        assert_eq!(v(&[]), Err(SaveError::Truncated));
        assert_eq!(v(&[0x01]), Err(SaveError::Truncated));
        assert_eq!(v(&[0x02, 0]), Err(SaveError::Truncated));
    }

    #[test]
    fn a_reader_may_start_at_the_end_but_not_past_it() {
        let b = [1, 2, 3];
        assert_eq!(Reader::at(&b, 3).unwrap().left(), 0);
        assert!(Reader::at(&b, 4).is_err());
        let mut r = Reader::new(&b);
        assert!(r.take(usize::MAX).is_err());
        assert_eq!(r.check_count(3, 1), Ok(3));
        assert!(r.check_count(4, 1).is_err());
        assert!(r.check_count(usize::MAX, 2).is_err());
        assert_eq!(r.check_count(1, 3), Ok(1));
    }
}
