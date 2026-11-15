//! A character's look turned into meshes: the body, head, hair, eyes and
//! brows of one character in one NIF, shaped and textured as the game would.
//!
//! 1. [`Look`]: what the character is (from an `NPC_` record here; a save's
//!    look is turned into one elsewhere). A RaceMenu preset is turned into
//!    a look, and a look written back into one, by [`preset`].
//! 2. [`plan::plan`]: the parts it is made of, from the records (and
//!    [`plan::plan_with`], with RaceMenu's named sliders, [`racemenu`]), and
//!    [`plan::Plan::files`], every file a build will read, so they can be
//!    fetched first.
//! 3. [`build::build`]: the parts read from those files, shaped
//!    ([`morph`]), textured and coloured, in one [`apoc_nif::Nif`].
//!
//! Written from the files themselves and from published descriptions of the
//! formats (UESP's wiki). Not derived from any other program's source. The
//! library reads no files: it is given the records and the files' bytes.

pub mod build;
pub mod files;
pub mod look;
pub mod morph;
pub mod parts;
pub mod place;
pub mod plan;
pub mod preset;
pub mod racemenu;
pub mod records;
pub mod tri;

pub use build::{build, Built, Options};
pub use files::{Files, Source};
pub use look::{Look, Tint};
pub use plan::{plan, plan_with, Plan};
pub use racemenu::RaceMenuMorphs;
pub use records::Records;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum CharacterError {
    #[error("no {0} record {1:08X}")]
    MissingRecord(&'static str, u32),
    #[error("the face comes from a leveled list ({0:08X}), which has no one face")]
    LeveledTemplate(u32),
    #[error("the templates of {0:08X} go round in a ring")]
    TemplateLoop(u32),
    #[error("the look names no race")]
    NoRace,
    #[error("none of the character's meshes could be read")]
    Nothing,
}

/// The files a build of `look` will read, by key (lower case, `/`, under
/// the Data folder), each once.
pub fn needed(look: &Look, records: &dyn Records) -> Result<Vec<String>, CharacterError> {
    Ok(plan(look, records)?.files())
}

/// The files a build of `look` will read with RaceMenu's named sliders
/// drawn by `racemenu` ([`plan::plan_with`]), by key, each once: what
/// [`needed`] gives, and the extra tris. Without `racemenu`, the same as
/// [`needed`].
pub fn needed_with(
    look: &Look,
    records: &dyn Records,
    racemenu: Option<&RaceMenuMorphs>,
) -> Result<Vec<String>, CharacterError> {
    Ok(plan_with(look, records, racemenu)?.files())
}

#[cfg(test)]
mod tests;
