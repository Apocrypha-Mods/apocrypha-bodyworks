//! Which head parts a face gets.
//!
//! Found from the files: for 2,547 of Skyrim.esm's 2,550 pre-built faces the
//! parts worked out here are exactly the NIF's shapes, by editor id, with
//! the records of Skyrim.esm, its DLCs and USSEP; the other three were built
//! before USSEP changed their records (`apoc-plugin`'s `--example records`).

use std::collections::HashSet;

use apoc_plugin::records::{HeadPart, HeadPartKind, Race, EXTRA_PART};
use apoc_plugin::FormRef;

use crate::records::Records;

/// The most parts a face is given, extras and all: well past any the game's
/// faces have (under ten), and a stop for a mod's parts that name each other
/// without end.
pub const MAX_PARTS: usize = 256;

/// The head parts the game gives a face of `race` and sex from the parts it
/// lists, in no particular order, each once; parts without a model (the
/// "no scar", "no brow" parts races default to) are kept here and left out
/// by the build, which has nothing to draw for them.
///
/// - A listed part that is not an extra stands for its type, and the first
///   listed of a type is kept. WEAdventurerWarriorDualKhajiitM lists the
///   model-less HairKhajiit00 before KhajiitMaleEarTufts, and its face has
///   no tufts.
/// - Scars are not one to a type: dunAlftandValie and
///   dunAlftandSullaTrebatius each list a left and a right gash, and their
///   faces have both.
/// - An extra listed is kept as well: 2 NPC_s list a beard's `_1bit` part
///   (an extra, of type misc) and still get the race's default mouth (also
///   misc).
/// - Each type with nothing listed takes the race's default for the sex.
/// - Each part's `HNAM` extras come with it, recursively.
pub fn head_parts(
    records: &dyn Records,
    race: &Race,
    female: bool,
    listed: &[FormRef],
) -> Vec<FormRef> {
    let mut by_kind: Vec<(HeadPartKind, FormRef)> = Vec::new();
    let mut parts: Vec<FormRef> = Vec::new();
    for f in listed.iter().take(MAX_PARTS) {
        match records.head_part(f) {
            Some(HeadPart {
                kind: Some(k),
                flags,
                ..
            }) if flags & EXTRA_PART == 0 && k != HeadPartKind::Scar => {
                if !by_kind.iter().any(|(have, _)| *have == k) {
                    by_kind.push((k, f.clone()));
                }
            }
            _ => parts.push(f.clone()),
        }
    }
    for (_, d) in race.sexes[female as usize]
        .head
        .parts
        .iter()
        .take(MAX_PARTS)
    {
        if let Some(k) = records.head_part(d).and_then(|h| h.kind) {
            if !by_kind.iter().any(|(have, _)| *have == k) {
                by_kind.push((k, d.clone()));
            }
        }
    }
    parts.extend(by_kind.into_iter().map(|(_, f)| f));
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while let Some(p) = parts.pop() {
        if out.len() >= MAX_PARTS {
            break;
        }
        if !seen.insert(p.clone()) {
            continue;
        }
        if let Some(h) = records.head_part(&p) {
            parts.extend(h.extra_parts.iter().take(MAX_PARTS).cloned());
        }
        out.push(p);
    }
    out
}

/// Whether a part has something to draw.
pub fn has_model(part: &HeadPart) -> bool {
    part.model.as_deref().is_some_and(|m| !m.is_empty())
}
