//! RaceMenu presets (`.jslot`): read one, change it, and write it back the
//! way RaceMenu writes its own.
//!
//! A preset is a JSON document. [`parse`] reads it into a [`Document`] that
//! keeps everything a JSON reader usually throws away: the order of every
//! object's keys, each number's text as it was written, each string's
//! escapes, and a byte order mark if the file starts with one. [`write`]
//! lays a document out again, so a preset RaceMenu wrote comes back byte for
//! byte. [`Preset::of`] is a typed, read-only view of the parts a character
//! editor needs; everything else stays in the document as it was read.
//!
//! The layout, from the presets themselves (the first, a 309 KB UBE preset
//! of 8,752 lines, is the evidence quoted below; it matches what jsoncpp's
//! `StyledWriter`, which is MIT licensed, is documented to produce):
//!
//! - UTF-8 with no byte order mark, LF line ends and one LF after the last
//!   `}`. No tabs anywhere.
//! - Every line is indented by a multiple of three spaces, one step per
//!   level of nesting.
//! - An object member is `"key" : value`, one per line, with `,` straight
//!   after every value but the last. Each object's keys are in byte order
//!   (true of every object in the sample, `formId` before `formIdentifier`,
//!   `sculpt` before `sculptDivisor`); a reader does not rely on it, and
//!   [`Object::insert`] keeps it.
//! - An object or array that is not empty opens on the line of its key, or
//!   on a line of its own inside an array, and closes on a line of its own
//!   at its opening line's indent.
//! - An array is written on one line, `[ 0, 4294967295, 0, 0 ]`, when it
//!   holds only scalars (or empty containers) and `[ `, the items, `, `
//!   between them and ` ]` come to fewer than 74 bytes, counted as written
//!   (escapes in full, a non-ASCII character once per byte of its UTF-8)
//!   and without the indent or the key. The sample has 7,007 arrays of
//!   scalars from 16 to 25 bytes long, every one on one line, and two that
//!   are not: the face sliders (80 bytes) and the plugin names (133). The
//!   exact limit, 74, is jsoncpp's documented right margin, and jsoncpp
//!   counts bytes; the sample only shows the limit lies between 26 and 80,
//!   and has no non-ASCII text. Any array holding a non-empty array or
//!   object is one item per line, as every one in the sample is.
//! - Empty arrays and objects are `[]` and `{}` (jsoncpp's documented form;
//!   the sample has none).
//! - Numbers RaceMenu stores as floats are C's `%.16g` of the float widened
//!   to a double: `0.3100000023841858`, `1.049999952316284`,
//!   `3.402823466385289e+38`, and `0` or `-1` for whole values. All 64
//!   fractional numbers in the sample are exactly that. Whole numbers are
//!   plain decimals, up to `4294967295`. [`Number`] keeps a read number's
//!   text, so none of this matters for numbers that were read.
//! - Strings escape `\` as `\\` (275 times in the sample, in texture paths)
//!   and `/` is left alone. The other escapes ([`Str::new`]) follow jsoncpp's
//!   documented writer and are not yet seen in a preset.
//!
//! The reader is stricter than RaceMenu may be: comments, a key named
//! twice in one object, and numbers JSON does not allow (`01`, `+1`, `.5`)
//! are refused, though a lenient JSON reader, as RaceMenu's may be, could
//! load a hand-edited preset that has them. RaceMenu's own writer produces
//! none of them.
//!
//! Written from preset files and from jsoncpp's published behaviour. Not
//! derived from RaceMenu's source.

mod parse;
pub mod preset;
mod value;
mod write;

pub use parse::parse;
pub use preset::{FormRef, Preset};
pub use value::{Document, Number, Object, Str, Value};
pub use write::{write, MAX_OUTPUT};

/// Why a preset could not be read or written.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum JslotError {
    /// The file is not JSON this reader accepts. Lines and columns count
    /// from 1; columns are bytes, counted from after a byte order mark.
    #[error("can't read this preset: {reason} (line {line}, column {column})")]
    Malformed {
        line: usize,
        column: usize,
        reason: &'static str,
    },
    /// The file to read is longer than [`MAX_INPUT`], or what would be
    /// written longer than [`MAX_OUTPUT`].
    #[error("the preset is larger than {limit} bytes")]
    TooLarge { limit: usize },
    /// A document built in code nests arrays and objects deeper than
    /// [`MAX_DEPTH`]. A file that does is [`JslotError::Malformed`], with
    /// where it happens.
    #[error("the preset nests arrays and objects more than {limit} deep")]
    TooDeep { limit: usize },
}

/// How deeply arrays and objects may nest, the top one counting as 1. A
/// preset nests 7 deep (the sample's transforms); the limit keeps a hostile
/// file from exhausting the stack of a WebAssembly add-on and bounds how
/// much indent one item can cost when written.
pub const MAX_DEPTH: usize = 32;

/// The longest file [`parse`] reads, 4 MiB. The sample, with 59 named
/// sliders and its sculpting, is 309 KB. A hostile file at the limit costs
/// a few hundred MB to read as a character (measured at about 550 MB for
/// 8 MiB of bad entries), so the limit is kept well under what a
/// WebAssembly add-on can allocate.
pub const MAX_INPUT: usize = 4 * 1024 * 1024;
