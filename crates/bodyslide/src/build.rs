//! Building a set: its mesh, with every slider applied at a preset's values,
//! written as the lightest and the heaviest body (or as one mesh).

use crate::morph;
use crate::osd::SliderData;
use crate::preset::Preset;
use crate::set::{Shape, SliderSet};
use crate::Error;
use apoc_nif::{edit, mesh, Nif};
use std::collections::HashMap;

/// Where a build reads its files from: the BodySlide folder, by a path under
/// it with forward slashes (`ShapeData/CBBE/CBBE Body.nif`). Letter-case is
/// the reader's to match as Windows would.
pub trait Files {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

impl<F: Fn(&str) -> Option<Vec<u8>>> Files for F {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self(path)
    }
}

/// One built mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Under the game's data folder, with forward slashes.
    pub path: String,
    pub bytes: Vec<u8>,
}

/// What a build made, and what it could not do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Built {
    pub outputs: Vec<Output>,
    /// Each a sentence: a slider that was left out and why.
    pub notes: Vec<String>,
}

fn join(parts: &[&str]) -> String {
    parts
        .iter()
        .flat_map(|p| p.split(['\\', '/']))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// The folder a shape's slider data is in: its own when it names one, the
/// set's otherwise.
fn data_folder<'a>(set: &'a SliderSet, shape: &'a Shape) -> &'a str {
    shape.data_folder.as_deref().unwrap_or(&set.data_folder)
}

/// Build `set` at `preset`'s values (the set's own defaults where the preset
/// says nothing, or throughout with no preset).
pub fn build(set: &SliderSet, preset: Option<&Preset>, files: &dyn Files) -> Result<Built, Error> {
    build_at(
        set,
        &morph::values(set, preset, false),
        &morph::values(set, preset, true),
        files,
    )
}

/// Build `set` with each slider at the given value, in percent, on the
/// lightest (`small`) and the heaviest (`big`) body: one value a slider, in
/// the set's order.
pub fn build_at(
    set: &SliderSet,
    small: &[f32],
    big: &[f32],
    files: &dyn Files,
) -> Result<Built, Error> {
    if small.len() != set.sliders.len() || big.len() != set.sliders.len() {
        return Err(Error::Invalid(format!(
            "{} sliders, and {} and {} values",
            set.sliders.len(),
            small.len(),
            big.len()
        )));
    }
    let source_path = join(&["ShapeData", &set.data_folder, &set.source_file]);
    let source = files
        .read(&source_path)
        .ok_or_else(|| Error::Invalid(format!("{source_path} is missing")))?;
    let base = Nif::read(&source).map_err(|e| Error::Invalid(format!("{source_path}: {e}")))?;

    // Each shape of the set, with the block it is in the mesh.
    let blocks = mesh::shapes(&base);
    let mut shapes = Vec::new();
    let mut built = Built::default();
    for shape in &set.shapes {
        let found = blocks
            .iter()
            .find_map(|at| mesh::read(&base, *at).ok().filter(|m| m.name == shape.name));
        match found {
            Some(m) => shapes.push((shape, m)),
            None => built
                .notes
                .push(format!("The mesh has no shape named {}.", shape.name)),
        }
    }

    // Slider data files, each read once.
    let mut data: HashMap<String, Option<SliderData>> = HashMap::new();
    let mut list = |folder: &str, file: &str, name: &str| -> Option<Vec<crate::osd::Diff>> {
        let path = join(&["ShapeData", folder, file]);
        data.entry(path.clone())
            .or_insert_with(|| files.read(&path).and_then(|b| SliderData::read(&b).ok()))
            .as_ref()
            .and_then(|d| d.list(name).map(<[_]>::to_vec))
    };

    let weights: &[(bool, &str)] = if set.gen_weights {
        &[(false, "_0"), (true, "_1")]
    } else {
        &[(true, "")]
    };
    for (heavy, suffix) in weights {
        let values = if *heavy { big } else { small };
        let mut nif = base.clone();
        // Shapes left with nothing to draw, to take out once the rest is done.
        let mut emptied = Vec::new();
        for (shape, m) in &shapes {
            let mut positions = m.positions.clone();
            let mut uvs = m.uvs.clone();
            let mut gone = vec![false; positions.len()];
            for (slider, value) in set.sliders.iter().zip(values) {
                let amount = morph::applied(slider, *value);
                if amount == 0.0 {
                    continue;
                }
                let mine = slider.data.iter().filter(|d| d.target == shape.target);
                for d in mine {
                    let folder = if d.local {
                        set.data_folder.as_str()
                    } else {
                        data_folder(set, shape)
                    };
                    let Some(diffs) = list(folder, &d.file, &d.list) else {
                        let note =
                            format!("{}: its data for {} is missing.", slider.name, shape.name);
                        if !built.notes.contains(&note) {
                            built.notes.push(note);
                        }
                        continue;
                    };
                    if slider.zap {
                        // Not a move: the vertices it marks are taken out. A
                        // list can name every vertex and mark only some, the
                        // rest with an offset of nothing.
                        for diff in diffs.iter().filter(|d| morph::marks(d)) {
                            if let Some(g) = gone.get_mut(diff.vertex as usize) {
                                *g = true;
                            }
                        }
                    } else if slider.uv {
                        morph::add_uv(&mut uvs, &diffs, amount);
                    } else {
                        morph::add(&mut positions, &diffs, amount);
                    }
                }
            }
            let fail = |e: apoc_nif::NifError| Error::Invalid(format!("{}: {e}", shape.name));
            let moved = positions != m.positions;
            let zapped = gone.contains(&true);
            if uvs != m.uvs && uvs.len() == positions.len() {
                mesh::set_uvs(&mut nif, m.block, &uvs).map_err(fail)?;
            }
            if zapped {
                let left = edit::remove_vertices(&mut nif, m.block, &gone).map_err(fail)?;
                let keep = |i: &usize| !gone[*i];
                positions = (0..positions.len())
                    .filter(keep)
                    .map(|i| positions[i])
                    .collect();
                if left == 0 {
                    emptied.push(m.block);
                    continue;
                }
            }
            mesh::set_positions(&mut nif, m.block, &positions).map_err(fail)?;
            // Moved, it has to be lit as the shape it now is. A shape that
            // stores no normals (a body, lit from a normal map) has none to
            // work out.
            if (moved || zapped) && m.normals.len() == m.positions.len() {
                let now = mesh::read(&nif, m.block).map_err(fail)?;
                let normals = apoc_nif::shading::smooth_normals(&now.positions, &now.triangles);
                let (tangents, bitangents) = apoc_nif::shading::tangent_space(
                    &now.positions,
                    &now.uvs,
                    &normals,
                    &now.triangles,
                );
                mesh::set_shading(&mut nif, m.block, &normals, &tangents, &bitangents)
                    .map_err(fail)?;
            }
        }
        if let Err(e) = edit::remove_shapes(&mut nif, &emptied) {
            // Left in, with nothing in it: drawn as nothing.
            built
                .notes
                .push(format!("A shape with nothing left was kept: {e}."));
        }
        built.outputs.push(Output {
            path: format!(
                "{}{suffix}.nif",
                join(&[&set.output_path, &set.output_file])
            ),
            bytes: nif.write(),
        });
    }
    Ok(built)
}
