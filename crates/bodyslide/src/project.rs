//! A set being worked on: its mesh and what each slider does to each of its
//! own shapes, held together so they can be changed and saved as a project.

use crate::build::{Files, Output};
use crate::osd::{Diff, SliderData};
use crate::set::{DataRef, Slider, SliderSet};
use crate::Error;
use apoc_nif::{edit, mesh, Nif};
use std::collections::HashMap;

/// What one slider does to one of the project's own shapes.
#[derive(Debug, Clone, PartialEq)]
struct Own {
    target: String,
    diffs: Vec<Diff>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// The set. Each slider's `data` holds only what is read from another
    /// project's folder (a reference body's); the rest is in `own`.
    pub set: SliderSet,
    pub nif: Nif,
    /// For each slider, in the set's order.
    own: Vec<Vec<Own>>,
    /// Each a sentence: something the set named that was not found.
    pub notes: Vec<String>,
}

/// A slider's settings, all of them, as they are changed together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub small: f32,
    pub big: f32,
    pub invert: bool,
    pub hidden: bool,
    pub zap: bool,
    pub uv: bool,
    pub clamp: bool,
}

fn invalid<T>(why: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(why.into()))
}

/// A name that can be a file's on any system.
pub fn file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    cleaned.trim().trim_end_matches('.').to_string()
}

fn path(parts: &[&str]) -> String {
    parts
        .iter()
        .flat_map(|p| p.split(['\\', '/']))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

impl Project {
    /// Open `set`, reading its mesh and its own slider data from `files`.
    pub fn open(set: &SliderSet, files: &dyn Files) -> Result<Project, Error> {
        let source = path(&["ShapeData", &set.data_folder, &set.source_file]);
        let nif = files
            .read(&source)
            .ok_or_else(|| Error::Invalid(format!("{source} is missing")))
            .and_then(|b| Nif::read(&b).map_err(|e| Error::Invalid(format!("{source}: {e}"))))?;
        let mut set = set.clone();
        let mut notes = Vec::new();
        let mut read: HashMap<String, Option<SliderData>> = HashMap::new();
        let mut own = Vec::with_capacity(set.sliders.len());
        // A list is the project's own when it is in the project's folder:
        // marked so, or of a shape that names no other.
        let elsewhere: Vec<String> = set
            .shapes
            .iter()
            .filter(|s| s.data_folder.is_some())
            .map(|s| s.target.clone())
            .collect();
        let folder = set.data_folder.clone();
        for slider in &mut set.sliders {
            let mut mine = Vec::new();
            let mut kept = Vec::new();
            for d in std::mem::take(&mut slider.data) {
                if !d.local && elsewhere.contains(&d.target) {
                    kept.push(d);
                    continue;
                }
                let at = path(&["ShapeData", &folder, &d.file]);
                let diffs = read
                    .entry(at.clone())
                    .or_insert_with(|| files.read(&at).and_then(|b| SliderData::read(&b).ok()))
                    .as_ref()
                    .and_then(|data| data.list(&d.list).map(<[_]>::to_vec));
                match diffs {
                    Some(diffs) => mine.push(Own {
                        target: d.target,
                        diffs,
                    }),
                    None => notes.push(format!(
                        "{} has nothing for {} in {at}.",
                        slider.name, d.target
                    )),
                }
            }
            slider.data = kept;
            own.push(mine);
        }
        Ok(Project {
            set,
            nif,
            own,
            notes,
        })
    }

    fn slider(&self, name: &str) -> Result<usize, Error> {
        self.set
            .sliders
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| Error::Invalid(format!("There is no slider named {name}.")))
    }

    fn shape(&self, name: &str) -> Result<usize, Error> {
        self.set
            .shapes
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| Error::Invalid(format!("There is no shape named {name}.")))
    }

    fn block(&self, name: &str) -> Option<usize> {
        mesh::shapes(&self.nif)
            .into_iter()
            .find(|at| mesh::name(&self.nif, *at).as_deref() == Some(name))
    }

    /// Give a shape another name, in the mesh and in the set.
    pub fn rename_shape(&mut self, from: &str, to: &str) -> Result<(), Error> {
        let to = to.trim();
        let at = self.shape(from)?;
        if to.is_empty() {
            return invalid("Give the shape a name.");
        }
        if to == from {
            return Ok(());
        }
        if self.set.shapes.iter().any(|s| s.name == to) || self.block(to).is_some() {
            return invalid(format!("There is already a shape named {to}."));
        }
        if let Some(block) = self.block(from) {
            mesh::rename(&mut self.nif, block, to).map_err(|e| Error::Invalid(e.to_string()))?;
        }
        self.set.shapes[at].name = to.to_string();
        Ok(())
    }

    /// Take a shape out of the mesh and the set, with what sliders did to it.
    pub fn delete_shape(&mut self, name: &str) -> Result<(), Error> {
        let at = self.shape(name)?;
        if let Some(block) = self.block(name) {
            edit::remove_shapes(&mut self.nif, &[block])
                .map_err(|e| Error::Invalid(e.to_string()))?;
        }
        let target = self.set.shapes.remove(at).target;
        // Two shapes can share a target only by mistake; what the other
        // still needs stays.
        if self.set.shapes.iter().any(|s| s.target == target) {
            return Ok(());
        }
        for (slider, own) in self.set.sliders.iter_mut().zip(&mut self.own) {
            slider.data.retain(|d| d.target != target);
            own.retain(|o| o.target != target);
        }
        Ok(())
    }

    pub fn rename_slider(&mut self, from: &str, to: &str) -> Result<(), Error> {
        let to = to.trim();
        let at = self.slider(from)?;
        if to.is_empty() {
            return invalid("Give the slider a name.");
        }
        if to == from {
            return Ok(());
        }
        if self.set.sliders.iter().any(|s| s.name == to) {
            return invalid(format!("There is already a slider named {to}."));
        }
        self.set.sliders[at].name = to.to_string();
        // A switch that named it follows it.
        for slider in &mut self.set.sliders {
            for toggled in &mut slider.zap_toggles {
                if toggled == from {
                    *toggled = to.to_string();
                }
            }
        }
        Ok(())
    }

    pub fn delete_slider(&mut self, name: &str) -> Result<(), Error> {
        let at = self.slider(name)?;
        self.set.sliders.remove(at);
        self.own.remove(at);
        for slider in &mut self.set.sliders {
            slider.zap_toggles.retain(|t| t != name);
        }
        Ok(())
    }

    /// A new slider that does nothing yet, after the others.
    pub fn add_slider(&mut self, name: &str) -> Result<(), Error> {
        let name = name.trim();
        if name.is_empty() {
            return invalid("Give the slider a name.");
        }
        if self.set.sliders.iter().any(|s| s.name == name) {
            return invalid(format!("There is already a slider named {name}."));
        }
        self.set.sliders.push(Slider {
            name: name.to_string(),
            small: 0.0,
            big: 100.0,
            ..Slider::default()
        });
        self.own.push(Vec::new());
        Ok(())
    }

    /// Move a slider to be the `to`th.
    pub fn move_slider(&mut self, name: &str, to: usize) -> Result<(), Error> {
        let at = self.slider(name)?;
        let to = to.min(self.set.sliders.len() - 1);
        let slider = self.set.sliders.remove(at);
        self.set.sliders.insert(to, slider);
        let own = self.own.remove(at);
        self.own.insert(to, own);
        Ok(())
    }

    pub fn settings(&mut self, name: &str, to: Settings) -> Result<(), Error> {
        let at = self.slider(name)?;
        if to.zap && to.uv {
            return invalid("A slider takes vertices out or moves the texture, not both.");
        }
        let slider = &mut self.set.sliders[at];
        slider.small = to.small;
        slider.big = to.big;
        slider.invert = to.invert;
        slider.hidden = to.hidden;
        slider.zap = to.zap;
        slider.uv = to.uv;
        slider.clamp = to.clamp;
        if !to.zap {
            slider.zap_toggles.clear();
        }
        Ok(())
    }

    /// Where a build of the project goes, and whether as two meshes.
    pub fn output(&mut self, folder: &str, file: &str, weights: bool) -> Result<(), Error> {
        let file = file_name(file.trim().trim_end_matches(".nif"));
        if file.is_empty() {
            return invalid("Give the mesh a name.");
        }
        let folder: Vec<&str> = folder
            .split(['\\', '/'])
            .map(str::trim)
            .filter(|p| !p.is_empty() && *p != "." && *p != "..")
            .collect();
        self.set.output_path = folder.join("\\");
        self.set.output_file = file;
        self.set.gen_weights = weights;
        Ok(())
    }

    /// The project as files under the BodySlide folder, named `name`, and
    /// the set as those files say it.
    pub fn save(&self, name: &str) -> Result<(SliderSet, Vec<Output>), Error> {
        let name = name.trim();
        let file = file_name(name);
        if file.is_empty() {
            return invalid("Give the project a name.");
        }
        let mut set = self.set.clone();
        set.name = name.to_string();
        set.data_folder = file.clone();
        set.source_file = format!("{file}.nif");
        let data_file = format!("{file}.osd");
        let mut data = SliderData {
            version: 1,
            lists: Vec::new(),
        };
        for (slider, own) in set.sliders.iter_mut().zip(&self.own) {
            for o in own {
                let list = format!("{}{}", o.target, slider.name);
                slider.data.push(DataRef {
                    name: list.clone(),
                    target: o.target.clone(),
                    local: true,
                    file: data_file.clone(),
                    list: list.clone(),
                });
                data.lists.push((list, o.diffs.clone()));
            }
        }
        let files = vec![
            Output {
                path: format!("SliderSets/{file}.osp"),
                bytes: crate::set::write(std::slice::from_ref(&set)).into_bytes(),
            },
            Output {
                path: format!("ShapeData/{file}/{file}.nif"),
                bytes: self.nif.write(),
            },
            Output {
                path: format!("ShapeData/{file}/{data_file}"),
                bytes: data.write(),
            },
        ];
        Ok((set, files))
    }
}
