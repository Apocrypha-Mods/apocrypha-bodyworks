//! Bodies and outfits built from slider sets and presets.
//!
//! A slider set (`.osp`) names a mesh and its sliders. Each slider has, for
//! each shape it moves, a list of vertices and how far each goes when the
//! slider is full (`.osd`, or a file for each list in the older `.bsd`). A
//! preset gives every slider two values, one for the lightest body and one
//! for the heaviest, and a build moves the mesh's vertices by each slider's
//! list times its value and writes the result.
//!
//! Written from real projects' files and from published descriptions of the
//! formats. Not derived from any other program's source.

pub mod bsd;
pub mod build;
pub mod category;
pub mod conform;
pub mod group;
pub mod headtri;
pub mod morph;
pub mod osd;
pub mod pack;
pub mod preset;
pub mod project;
pub mod set;
pub mod tri;
pub mod xml;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("not a file of that kind")]
    NotThatKind,
    #[error("the file ends before its contents do")]
    Truncated,
    #[error("the file could not be read as XML: {0}")]
    Xml(String),
    #[error("{0}")]
    Invalid(String),
}
