//! Plugins (`.esm`, `.esp`, `.esl`): the records a character is made from.
//!
//! A plugin is a header record (`TES4`) and then a run of top-level groups,
//! one for each kind of record. A save names the player's race, head parts,
//! skin and hair colour by formID; this crate finds those records, reads the
//! parts of them a character needs ([`records`]), and picks the one that wins
//! across a load order ([`load`]).
//!
//! The add-on is given files over IPC, and Skyrim.esm is 250 MB, so a plugin
//! is read by ranges ([`walk`]): its header, the header of every top-level
//! group, and then only the groups asked for.
//!
//! Written from the files themselves and from published descriptions of the
//! format (UESP's "Mod File Format" pages). Not derived from any other
//! program's source.

pub mod container;
pub mod io;
pub mod load;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod records;
pub mod walk;

pub use container::{FormRef, Header, Masters, Plugin, Record, Subrecord};

pub use load::LoadOrder;
pub use walk::{Stats, Walk};

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum PluginError {
    #[error("not a plugin file")]
    NotPlugin,
    #[error("the file ends before its contents do")]
    Truncated,
    #[error("the file disagrees with itself ({0})")]
    Corrupt(String),
    #[error("more than this will hold ({0})")]
    TooLarge(String),
    #[error("a compressed record would not inflate ({0})")]
    Inflate(String),
    #[error("bytes were fed that were not asked for ({0})")]
    Unasked(String),
}

#[cfg(test)]
mod tests;
