//! Reading a plugin by ranges, for when the file is not in memory.
//!
//! A [`Walk`] says which bytes it needs next, is fed them, and repeats:
//!
//! 1. the `TES4` header record, and the header of the first group after it;
//! 2. for each top-level group, its header only, and the group is stepped
//!    over by its size;
//! 3. for a group of a kind asked for, the whole of it (and the next group's
//!    header in the same read).
//!
//! Each read also takes the 24 bytes of the next group's header, so a plugin
//! of *n* top-level groups takes about *n* + 2 reads, and the bytes read are
//! the groups asked for and 24 bytes for each other.
//!
//! ```no_run
//! # fn read(at: u64, len: usize) -> Vec<u8> { vec![0; len] }
//! # let file_len = 0;
//! let mut walk = apoc_plugin::Walk::new("Skyrim.esm", file_len, &[*b"RACE", *b"HDPT"]);
//! while let Some((at, len)) = walk.need() {
//!     walk.feed(at, &read(at, len)).unwrap();
//! }
//! let plugin = walk.finish().unwrap();
//! ```

use crate::container::{GroupHeader, Header, Plugin, RecordHeader, HEADER_LEN};
use crate::PluginError;

/// The largest group that will be read into memory. Skyrim.esm's largest
/// group of a kind a character needs (`NPC_`) is a few tens of megabytes.
pub const MAX_GROUP: u64 = 512 << 20;

/// How much a walk read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub reads: u32,
    pub bytes: u64,
    pub groups: u32,
    pub groups_read: u32,
}

#[derive(Debug)]
enum State {
    /// The first record's header.
    Start,
    /// The `TES4` data and the first group header after it.
    Tes4(RecordHeader),
    /// A group header at this offset.
    Group(u64),
    /// The data of the group whose header starts at this offset, then the
    /// next group's header.
    Body(u64, GroupHeader),
    Done,
    Failed,
}

/// A pull-style reader of one plugin.
#[derive(Debug)]
pub struct Walk {
    name: String,
    len: u64,
    wanted: Vec<[u8; 4]>,
    state: State,
    plugin: Option<Plugin>,
    stats: Stats,
}

impl Walk {
    /// `name` is the plugin's file name (what formIDs from other plugins call
    /// it), `len` the file's size, `wanted` the record types to read.
    pub fn new(name: &str, len: u64, wanted: &[[u8; 4]]) -> Self {
        Walk {
            name: name.to_string(),
            len,
            wanted: wanted.to_vec(),
            state: State::Start,
            plugin: None,
            stats: Stats::default(),
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// The range wanted next, as (offset, length); `None` once done or
    /// failed.
    pub fn need(&self) -> Option<(u64, usize)> {
        let (at, len) = match &self.state {
            State::Start => (0, HEADER_LEN as u64),
            State::Tes4(h) => (HEADER_LEN as u64, h.size as u64 + HEADER_LEN as u64),
            State::Group(at) => (*at, HEADER_LEN as u64),
            State::Body(at, g) => (*at + HEADER_LEN as u64, g.size as u64),
            State::Done | State::Failed => return None,
        };
        // The read-ahead header is cut off at the end of the file.
        let len = len.min(self.len.saturating_sub(at));
        Some((at, len as usize))
    }

    /// Gives the walk the range it asked for. An error ends the walk.
    pub fn feed(&mut self, at: u64, bytes: &[u8]) -> Result<(), PluginError> {
        let r = self.step(at, bytes);
        if r.is_err() {
            self.state = State::Failed;
        }
        r
    }

    fn step(&mut self, at: u64, bytes: &[u8]) -> Result<(), PluginError> {
        let Some(want) = self.need() else {
            return Err(PluginError::Unasked("the walk is over".into()));
        };
        if want != (at, bytes.len()) {
            return Err(PluginError::Unasked(format!(
                "{} bytes at {at} where {} at {} were asked for",
                bytes.len(),
                want.1,
                want.0
            )));
        }
        self.stats.reads += 1;
        self.stats.bytes += bytes.len() as u64;
        match std::mem::replace(&mut self.state, State::Failed) {
            State::Start => {
                if bytes.len() < HEADER_LEN {
                    return Err(PluginError::NotPlugin);
                }
                let h = RecordHeader::read(bytes)?;
                if &h.kind != b"TES4" {
                    return Err(PluginError::NotPlugin);
                }
                if h.size as u64 > self.len - HEADER_LEN as u64 {
                    return Err(PluginError::Truncated);
                }
                self.state = State::Tes4(h);
            }
            State::Tes4(h) => {
                let size = h.size as usize;
                let header = Header::read(&h, &bytes[..size])?;
                self.plugin = Some(Plugin::new(&self.name, header));
                let next = (HEADER_LEN + size) as u64;
                self.after(next, &bytes[size..])?;
            }
            State::Group(at) => self.group(at, bytes)?,
            State::Body(at, g) => {
                let size = g.size as usize - HEADER_LEN;
                self.stats.groups_read += 1;
                self.plugin
                    .as_mut()
                    .ok_or(PluginError::NotPlugin)?
                    .read_group(&bytes[..size])?;
                self.after(at + g.size as u64, &bytes[size..])?;
            }
            State::Done | State::Failed => unreachable!("need() was None"),
        }
        Ok(())
    }

    /// Moves on to the group at `next`, whose header may be in `ahead`.
    fn after(&mut self, next: u64, ahead: &[u8]) -> Result<(), PluginError> {
        if next >= self.len {
            self.state = State::Done;
            return Ok(());
        }
        if ahead.len() == HEADER_LEN {
            self.group(next, ahead)
        } else {
            // Fewer than 24 bytes are left in the file.
            Err(PluginError::Truncated)
        }
    }

    /// A top-level group's header, at `at`.
    fn group(&mut self, at: u64, bytes: &[u8]) -> Result<(), PluginError> {
        if bytes.len() < HEADER_LEN {
            return Err(PluginError::Truncated);
        }
        let g = GroupHeader::read(bytes)?;
        if g.kind != 0 {
            return Err(PluginError::Corrupt(format!(
                "a top-level group of type {}",
                g.kind
            )));
        }
        if g.size as u64 > self.len - at {
            return Err(PluginError::Truncated);
        }
        self.stats.groups += 1;
        if self.wanted.contains(&g.label) {
            if g.size as u64 > MAX_GROUP {
                return Err(PluginError::TooLarge(format!(
                    "a group of {} bytes",
                    g.size
                )));
            }
            self.state = State::Body(at, g);
        } else {
            let next = at + g.size as u64;
            if next >= self.len {
                self.state = State::Done;
            } else {
                self.state = State::Group(next);
            }
        }
        Ok(())
    }

    /// The plugin read, once [`Walk::need`] has given `None`.
    pub fn finish(self) -> Result<Plugin, PluginError> {
        match self.state {
            State::Done => self.plugin.ok_or(PluginError::NotPlugin),
            State::Failed => Err(PluginError::Corrupt("the walk failed".into())),
            _ => Err(PluginError::Truncated),
        }
    }
}

/// Reads a plugin that is all in memory, the same way.
pub fn read(name: &str, bytes: &[u8], wanted: &[[u8; 4]]) -> Result<(Plugin, Stats), PluginError> {
    let mut walk = Walk::new(name, bytes.len() as u64, wanted);
    while let Some((at, len)) = walk.need() {
        let at_us = at as usize;
        walk.feed(at, &bytes[at_us..at_us + len])?;
    }
    let stats = walk.stats();
    Ok((walk.finish()?, stats))
}
