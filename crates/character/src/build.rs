//! Building a character: every part's mesh read, shaped, textured and
//! coloured, and brought into one NIF.

use apoc_nif::copy::copy_shape;
use apoc_nif::geometry::TriShape;
use apoc_nif::shader::LightingShader;
use apoc_nif::{mesh, Nif};
use apoc_plugin::records::HeadPartKind;

use crate::files::Source;
use crate::plan::Plan;
use crate::CharacterError;

/// How a build is told what the records cannot say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// The face's tint texture, put in the face's tint slot (7th). The
    /// game's pre-built faces name their baked one there
    /// (`data\Textures\Actors\Character\FaceGenData\FaceTint\Skyrim.esm\<id>.dds`);
    /// a character without one has none.
    pub face_tint: Option<String>,
}

/// One shape of a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub name: String,
    /// Its block in [`Built::nif`].
    pub block: usize,
    pub head: bool,
}

/// A built character.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub nif: Nif,
    pub shapes: Vec<Shape>,
    /// Files that were not there, by key; their parts are left out (a tri
    /// that is missing leaves its part unshaped by it).
    pub missing: Vec<String>,
    /// Each a sentence: what else was left out and why.
    pub notes: Vec<String>,
}

/// Where a lighting shader of the tinted kinds keeps its colour: three
/// floats after the 56 bytes every kind keeps (`apoc_nif::mesh`'s
/// measure). Kind 5 is tinted by the skin tone and kind 6 by the hair
/// colour, as the game's pre-built faces have them (`--example facegen`).
const TINT_AT: usize = 56;
pub const SKIN_TINT: u32 = 5;
pub const HAIR_TINT: u32 = 6;

/// A shape's lighting kind and tint colour, if it has one.
pub fn tint(nif: &Nif, block: usize) -> Option<(u32, [f32; 3])> {
    let (_, s) = shader(nif, block)?;
    let kind = s.shader_type?;
    if kind != SKIN_TINT && kind != HAIR_TINT {
        return None;
    }
    let b = s.rest.get(TINT_AT..TINT_AT + 12)?;
    let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    Some((kind, [f(0), f(4), f(8)]))
}

fn shader(nif: &Nif, block: usize) -> Option<(usize, LightingShader)> {
    let stream = nif.header.stream.version;
    let link = TriShape::read(nif.blocks.get(block)?.data.as_slice(), stream)
        .ok()?
        .shader;
    let at = usize::try_from(link)
        .ok()
        .filter(|a| *a < nif.blocks.len())?;
    if nif.kind_of(at) != "BSLightingShaderProperty" {
        return None;
    }
    Some((at, LightingShader::read(&nif.blocks[at].data, stream).ok()?))
}

/// Tint the shape at `block` with `colour` if its lighting is of `kind`.
fn set_tint(nif: &mut Nif, block: usize, kind: u32, colour: [f32; 3]) -> bool {
    let Some((at, mut s)) = shader(nif, block) else {
        return false;
    };
    if s.shader_type != Some(kind) {
        return false;
    }
    let Some(b) = s.rest.get_mut(TINT_AT..TINT_AT + 12) else {
        return false;
    };
    for (i, c) in colour.iter().enumerate() {
        b[i * 4..i * 4 + 4].copy_from_slice(&c.to_le_bytes());
    }
    // The shader may be shared with another shape of the file it came
    // from; a copied shape brings its own.
    nif.blocks[at].data = s.write();
    true
}

fn read_nif(key: &str, files: &dyn Source, built: &mut Built) -> Option<Nif> {
    let Some(bytes) = files.read(key) else {
        if !built.missing.iter().any(|m| m == key) {
            built.missing.push(key.to_string());
        }
        return None;
    };
    match Nif::read(bytes) {
        Ok(n) => Some(n),
        Err(e) => {
            built.notes.push(format!("{key}: {e}."));
            None
        }
    }
}

/// Bring every shape of `from` into the build; where they now are.
fn bring(out: &mut Option<Nif>, from: Nif, key: &str, notes: &mut Vec<String>) -> Vec<usize> {
    match out {
        None => {
            let shapes = mesh::shapes(&from);
            *out = Some(from);
            shapes
        }
        Some(into) => {
            let mut at = Vec::new();
            for b in mesh::shapes(&from) {
                // The copy is made on a clone, so a copy that fails part way
                // leaves nothing behind.
                let mut trial = into.clone();
                match copy_shape(&mut trial, &from, b) {
                    Ok(c) => {
                        *into = trial;
                        at.push(c);
                    }
                    Err(e) => notes.push(format!("{key}: a shape was left out: {e}.")),
                }
            }
            at
        }
    }
}

fn textured(
    nif: &mut Nif,
    block: usize,
    slots: &[Option<String>; 8],
    extra: Option<(usize, &str)>,
) {
    let Ok(m) = mesh::read(nif, block) else {
        return;
    };
    if m.textures.is_empty() {
        return;
    }
    let mut t = m.textures.clone();
    for (i, s) in slots.iter().enumerate() {
        if let (Some(s), Some(have)) = (s, t.get_mut(i)) {
            *have = s.clone();
        }
    }
    if let Some((i, s)) = extra {
        if let Some(have) = t.get_mut(i) {
            *have = s.to_string();
        }
    }
    if t != m.textures {
        let _ = mesh::set_textures(nif, block, &t);
    }
}

/// Build what `plan` says from `files`. A file that is not there leaves its
/// part out and is named in [`Built::missing`]; only a build with nothing in
/// it at all is an error.
pub fn build(plan: &Plan, files: &dyn Source, options: &Options) -> Result<Built, CharacterError> {
    let mut built = Built {
        nif: empty(),
        shapes: Vec::new(),
        missing: Vec::new(),
        notes: plan.notes.clone(),
    };
    let mut out: Option<Nif> = None;
    let mut shapes: Vec<Shape> = Vec::new();

    for piece in &plan.body {
        let (nif, light) = match &piece.weights {
            Some((l, h)) => {
                let heavy = read_nif(h, files, &mut built);
                let light = read_nif(l, files, &mut built);
                match (heavy, light) {
                    (Some(h), l) => (h, l),
                    (None, Some(l)) => (l, None),
                    (None, None) => continue,
                }
            }
            None => match read_nif(&piece.model, files, &mut built) {
                Some(n) => (n, None),
                None => continue,
            },
        };
        // Each shape's positions at the weight: the heaviest's, and the
        // lightest's of the same name and size moved towards them.
        let mut moved: Vec<Option<Vec<[f32; 3]>>> = Vec::new();
        for b in mesh::shapes(&nif) {
            let Ok(h) = mesh::read(&nif, b) else {
                moved.push(None);
                continue;
            };
            let l = light.as_ref().and_then(|l| {
                mesh::shapes(l)
                    .into_iter()
                    .filter_map(|lb| mesh::read(l, lb).ok())
                    .find(|m| m.name == h.name)
            });
            moved.push(match l {
                Some(l) if l.positions.len() == h.positions.len() => Some(
                    l.positions
                        .iter()
                        .zip(&h.positions)
                        .map(|(a, b)| [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * plan.weight))
                        .collect(),
                ),
                Some(_) => {
                    built.notes.push(format!(
                        "{}: its lightest and heaviest {} differ in size; the heaviest is used.",
                        piece.name, h.name
                    ));
                    None
                }
                None => None,
            });
        }
        let at = bring(&mut out, nif, &piece.model, &mut built.notes);
        let Some(o) = out.as_mut() else { continue };
        for (block, pos) in at.into_iter().zip(moved) {
            if let Some(p) = pos {
                let _ = mesh::move_vertices(o, block, &p, true);
            }
            textured(o, block, &piece.textures, None);
            if let Some(t) = plan.skin_tone {
                set_tint(o, block, SKIN_TINT, t);
            }
            shapes.push(Shape {
                name: mesh::name(o, block).unwrap_or_default(),
                block,
                head: false,
            });
        }
    }

    for piece in &plan.head {
        let Some(nif) = read_nif(&piece.model, files, &mut built) else {
            continue;
        };
        let mut nif = nif;
        let first = mesh::shapes(&nif);
        let Some(&main) = first.first() else {
            built
                .notes
                .push(format!("{}: its model has no shape.", piece.name));
            continue;
        };
        let base = match mesh::read(&nif, main) {
            Ok(m) => m,
            Err(e) => {
                built.notes.push(format!("{}: {e}.", piece.name));
                continue;
            }
        };
        let mut positions = base.positions.clone();
        let mut failed = None;
        for (kind, key) in &piece.tris {
            let Some(bytes) = files.read(key) else {
                if !built.missing.contains(key) {
                    built.missing.push(key.clone());
                }
                continue;
            };
            let tri = match crate::tri::read(bytes) {
                Ok(t) => t,
                Err(e) => {
                    failed = Some(format!("{key}: {e}"));
                    break;
                }
            };
            if let Err(e) = crate::morph::apply(&mut positions, &tri, *kind, &plan.weights) {
                failed = Some(format!("{key}: {e}"));
                break;
            }
        }
        if let Some(why) = failed {
            built
                .notes
                .push(format!("{} was left out: {why}.", piece.name));
            continue;
        }
        if mesh::move_vertices(&mut nif, main, &positions, true).is_err()
            || mesh::rename(&mut nif, main, &piece.name).is_err()
        {
            built
                .notes
                .push(format!("{}: its shape could not be changed.", piece.name));
            continue;
        }
        let at = bring(&mut out, nif, &piece.model, &mut built.notes);
        let Some(o) = out.as_mut() else { continue };
        let face = piece.kind == Some(HeadPartKind::Face);
        for block in at {
            let tint = options
                .face_tint
                .as_deref()
                .filter(|_| face)
                .map(|t| (6, t));
            textured(o, block, &piece.textures, tint);
            if let Some(c) = plan.hair_colour {
                set_tint(o, block, HAIR_TINT, c);
            }
            if let Some(t) = plan.skin_tone {
                set_tint(o, block, SKIN_TINT, t);
            }
            shapes.push(Shape {
                name: mesh::name(o, block).unwrap_or_default(),
                block,
                head: true,
            });
        }
    }

    built.nif = out.ok_or(CharacterError::Nothing)?;
    built.shapes = shapes;
    Ok(built)
}

/// A placeholder until the first mesh is read.
fn empty() -> Nif {
    Nif {
        header: apoc_nif::Header {
            line: Vec::new(),
            version: apoc_nif::VERSION_20_2_0_7,
            endian: 1,
            user_version: 12,
            stream: apoc_nif::StreamHeader {
                version: 100,
                author: Vec::new(),
                unknown: None,
                process_script: None,
                export_script: Vec::new(),
                max_filepath: None,
            },
            block_types: Vec::new(),
            strings: Vec::new(),
            max_string_len: 0,
            groups: Vec::new(),
        },
        blocks: Vec::new(),
        roots: Vec::new(),
        trailing: Vec::new(),
    }
}
