//! Building a set: its mesh, with every slider applied at a preset's values,
//! written as the lightest and the heaviest body (or as one mesh).

use crate::morph;
use crate::osd::SliderData;
use crate::preset::Preset;
use crate::set::{Shape, SliderSet};
use crate::Error;
use apoc_nif::{mesh, Nif};
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
    for (big, suffix) in weights {
        let values = morph::values(set, preset, *big);
        let mut nif = base.clone();
        for (shape, m) in &shapes {
            let mut positions = m.positions.clone();
            for (slider, value) in set.sliders.iter().zip(&values) {
                let amount = morph::applied(slider, *value);
                if amount == 0.0 {
                    continue;
                }
                let mine = slider.data.iter().filter(|d| d.target == shape.target);
                for d in mine {
                    if slider.zap || slider.uv {
                        let what = if slider.zap {
                            "removes vertices"
                        } else {
                            "moves the texture"
                        };
                        let note = format!("{} {what}, which is not built yet.", slider.name);
                        if !built.notes.contains(&note) {
                            built.notes.push(note);
                        }
                        continue;
                    }
                    let folder = if d.local {
                        set.data_folder.as_str()
                    } else {
                        data_folder(set, shape)
                    };
                    match list(folder, &d.file, &d.list) {
                        Some(diffs) => morph::add(&mut positions, &diffs, amount),
                        None => {
                            let note =
                                format!("{}: its data for {} is missing.", slider.name, shape.name);
                            if !built.notes.contains(&note) {
                                built.notes.push(note);
                            }
                        }
                    }
                }
            }
            mesh::set_positions(&mut nif, m.block, &positions)
                .map_err(|e| Error::Invalid(format!("{}: {e}", shape.name)))?;
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
