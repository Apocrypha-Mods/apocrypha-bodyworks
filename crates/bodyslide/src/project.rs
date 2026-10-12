//! A set being worked on: its mesh and what each slider does to each of its
//! own shapes, held together so they can be changed and saved as a project.

use crate::build::{Files, Output};
use crate::conform;
use crate::osd::{Diff, SliderData};
use crate::set::{DataRef, Shape, Slider, SliderSet};
use crate::Error;
use apoc_nif::{copy, edit, mesh, weights, Nif};
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

/// A slider moves a vertex less than this along every axis: it does not
/// move it.
const GONE: f32 = 0.00001;

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

    /// A project begun from a mesh alone: its shapes, and no sliders yet.
    /// `at` is where the mesh is under the game's data folder, which is
    /// where a build of the project goes unless that is changed.
    pub fn from_mesh(name: &str, at: &str, mesh_file: &[u8]) -> Result<Project, Error> {
        let name = name.trim();
        if file_name(name).is_empty() {
            return invalid("Give the project a name.");
        }
        let nif = Nif::read(mesh_file).map_err(|e| Error::Invalid(e.to_string()))?;
        let mut shapes: Vec<Shape> = Vec::new();
        for block in mesh::shapes(&nif) {
            let Some(shape) = mesh::name(&nif, block) else {
                continue;
            };
            if !shape.is_empty() && shapes.iter().all(|s| s.name != shape) {
                shapes.push(Shape {
                    target: shape.clone(),
                    name: shape,
                    data_folder: None,
                });
            }
        }
        if shapes.is_empty() {
            return invalid("The mesh has no shapes this can work on.");
        }
        let at = path(&[at]);
        let (folder, file) = at.rsplit_once('/').unwrap_or(("meshes", &at));
        let stem = file
            .rsplit_once('.')
            .map_or(file, |(stem, _)| stem)
            .to_string();
        // A mesh that is one of a pair by weight builds as a pair.
        let single = stem.strip_suffix("_0").or_else(|| stem.strip_suffix("_1"));
        Ok(Project {
            set: SliderSet {
                name: name.to_string(),
                data_folder: file_name(name),
                source_file: format!("{}.nif", file_name(name)),
                output_path: folder.replace('/', "\\"),
                output_file: single.unwrap_or(&stem).to_string(),
                gen_weights: single.is_some(),
                shapes,
                sliders: Vec::new(),
            },
            nif,
            own: Vec::new(),
            notes: Vec::new(),
        })
    }

    /// The same shape again under another name, doing what the first does.
    pub fn duplicate_shape(&mut self, name: &str, to: &str) -> Result<(), Error> {
        let to = to.trim();
        let at = self.shape(name)?;
        if to.is_empty() {
            return invalid("Give the shape a name.");
        }
        if self
            .set
            .shapes
            .iter()
            .any(|s| s.name == to || s.target == to)
            || self.block(to).is_some()
        {
            return invalid(format!("There is already a shape named {to}."));
        }
        let block = self
            .block(name)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {name}.")))?;
        let from = self.nif.clone();
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let copy = copy::copy_shape(&mut self.nif, &from, block).map_err(fail)?;
        mesh::rename(&mut self.nif, copy, to).map_err(fail)?;
        let first = self.set.shapes[at].clone();
        self.set.shapes.push(Shape {
            name: to.to_string(),
            target: to.to_string(),
            data_folder: first.data_folder,
        });
        for (slider, own) in self.set.sliders.iter_mut().zip(&mut self.own) {
            let again: Vec<DataRef> = slider
                .data
                .iter()
                .filter(|d| d.target == first.target)
                .map(|d| DataRef {
                    target: to.to_string(),
                    ..d.clone()
                })
                .collect();
            slider.data.extend(again);
            let again: Vec<Own> = own
                .iter()
                .filter(|o| o.target == first.target)
                .map(|o| Own {
                    target: to.to_string(),
                    diffs: o.diffs.clone(),
                })
                .collect();
            own.extend(again);
        }
        Ok(())
    }

    /// Bring the first shape of another project in as this one's reference:
    /// the body the outfit is fitted to. It keeps its sliders, which stay
    /// that project's, and this project gains any slider of them it did not
    /// have. A reference already here is taken out first.
    pub fn add_reference(&mut self, other: &SliderSet, files: &dyn Files) -> Result<(), Error> {
        let theirs = other
            .shapes
            .first()
            .ok_or_else(|| Error::Invalid(format!("{} has no shapes.", other.name)))?;
        let source = path(&["ShapeData", &other.data_folder, &other.source_file]);
        let from = files
            .read(&source)
            .ok_or_else(|| Error::Invalid(format!("{source} is missing")))
            .and_then(|b| Nif::read(&b).map_err(|e| Error::Invalid(format!("{source}: {e}"))))?;
        let block = mesh::shapes(&from)
            .into_iter()
            .find(|at| mesh::name(&from, *at).as_deref() == Some(theirs.name.as_str()))
            .ok_or_else(|| {
                Error::Invalid(format!("{source} has no shape named {}.", theirs.name))
            })?;
        let before = self.clone();
        let done = (|| {
            while let Some(old) = self.set.shapes.iter().find(|s| s.data_folder.is_some()) {
                let old = old.name.clone();
                self.delete_shape(&old)?;
            }
            if self
                .set
                .shapes
                .iter()
                .any(|s| s.name == theirs.name || s.target == theirs.target)
                || self.block(&theirs.name).is_some()
            {
                return invalid(format!(
                    "There is already a shape named {}. Rename or delete it first.",
                    theirs.name
                ));
            }
            copy::copy_shape(&mut self.nif, &from, block)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let folder = theirs
                .data_folder
                .clone()
                .unwrap_or_else(|| other.data_folder.clone());
            self.set.shapes.push(Shape {
                name: theirs.name.clone(),
                target: theirs.target.clone(),
                data_folder: Some(folder),
            });
            for slider in &other.sliders {
                let Some(d) = slider.data.iter().find(|d| d.target == theirs.target) else {
                    continue;
                };
                let at = match self.set.sliders.iter().position(|s| s.name == slider.name) {
                    Some(at) => at,
                    None => {
                        self.set.sliders.push(Slider {
                            data: Vec::new(),
                            ..slider.clone()
                        });
                        self.own.push(Vec::new());
                        self.set.sliders.len() - 1
                    }
                };
                self.set.sliders[at].data.push(DataRef {
                    local: false,
                    ..d.clone()
                });
            }
            Ok(())
        })();
        if done.is_err() {
            *self = before;
        }
        done
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

    /// Move vertices of a shape, each by its offset. With a slider, it is
    /// what that slider does to the shape that changes by as much, and the
    /// shape itself stays; a vertex the slider is left doing nothing to is
    /// dropped from it.
    pub fn move_vertices(
        &mut self,
        shape: &str,
        slider: Option<&str>,
        moves: &[Diff],
    ) -> Result<(), Error> {
        let at = self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let read = mesh::read(&self.nif, block).map_err(|e| Error::Invalid(e.to_string()))?;
        if moves
            .iter()
            .any(|m| m.vertex as usize >= read.positions.len())
        {
            return invalid(format!("{shape} has {} vertices.", read.positions.len()));
        }
        let Some(slider) = slider else {
            let mut positions = read.positions;
            crate::morph::add(&mut positions, moves, 1.0);
            return mesh::set_positions(&mut self.nif, block, &positions)
                .map_err(|e| Error::Invalid(e.to_string()));
        };
        let index = self.slider(slider)?;
        let target = self.set.shapes[at].target.clone();
        if self.set.sliders[index]
            .data
            .iter()
            .any(|d| d.target == target)
        {
            return invalid(format!(
                "What {slider} does to {shape} is another project's, and is not changed from here."
            ));
        }
        let own = &mut self.own[index];
        let list = match own.iter().position(|o| o.target == target) {
            Some(i) => &mut own[i].diffs,
            None => {
                own.push(Own {
                    target,
                    diffs: Vec::new(),
                });
                &mut own.last_mut().expect("just pushed").diffs
            }
        };
        // Only what the stroke touched changes: the rest of the list stays
        // as it was read, in its order.
        let sorted = list.is_sorted_by_key(|d| d.vertex);
        let mut place: HashMap<u16, usize> = list
            .iter()
            .enumerate()
            .map(|(i, d)| (d.vertex, i))
            .collect();
        let mut touched = vec![false; list.len()];
        for m in moves {
            let i = *place.entry(m.vertex).or_insert_with(|| {
                list.push(Diff {
                    vertex: m.vertex,
                    offset: [0.0; 3],
                });
                touched.push(false);
                list.len() - 1
            });
            touched[i] = true;
            for (c, by) in list[i].offset.iter_mut().zip(m.offset) {
                *c += by;
            }
        }
        let mut i = 0;
        list.retain(|d| {
            i += 1;
            !touched[i - 1] || d.offset.iter().any(|c| c.abs() >= GONE)
        });
        if sorted {
            list.sort_by_key(|d| d.vertex);
        }
        Ok(())
    }

    /// What one slider does to one shape: the project's own list for it,
    /// or the list another project keeps, read from `files`.
    fn list(&self, slider: usize, shape: usize, files: &dyn Files) -> Option<Vec<Diff>> {
        let of = &self.set.shapes[shape];
        if let Some(own) = self.own[slider].iter().find(|o| o.target == of.target) {
            return Some(own.diffs.clone());
        }
        let d = self.set.sliders[slider]
            .data
            .iter()
            .find(|d| d.target == of.target)?;
        let folder = of.data_folder.as_deref().unwrap_or(&self.set.data_folder);
        let data = files.read(&path(&["ShapeData", folder, &d.file]))?;
        SliderData::read(&data)
            .ok()?
            .list(&d.list)
            .map(<[_]>::to_vec)
    }

    /// Give `shape` what every slider does to `body`, so that it follows
    /// the body: each of its vertices takes what the body's vertices
    /// nearest it do. What sliders did to the shape before is replaced,
    /// but for the vertices in `spared`, which go on as they were.
    /// Returns how many sliders now move it.
    pub fn conform(
        &mut self,
        shape: &str,
        body: &str,
        files: &dyn Files,
        options: conform::Options,
        spared: &[u16],
    ) -> Result<usize, Error> {
        let (at, from) = (self.shape(shape)?, self.shape(body)?);
        if at == from {
            return invalid("A shape is conformed to another shape.");
        }
        let target = self.set.shapes[at].target.clone();
        if self
            .set
            .sliders
            .iter()
            .any(|s| s.data.iter().any(|d| d.target == target))
        {
            return invalid(format!(
                "What sliders do to {shape} is another project's, and is not changed from here."
            ));
        }
        let read = |name: &str| {
            self.block(name)
                .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {name}.")))
                .and_then(|b| mesh::read(&self.nif, b).map_err(|e| Error::Invalid(e.to_string())))
        };
        let ties = conform::tie(&read(body)?.positions, &read(shape)?.positions, options);
        let mut scratch = Vec::new();
        let mut moving = 0;
        for slider in 0..self.set.sliders.len() {
            if self.set.sliders[slider].zap || self.set.sliders[slider].uv {
                continue;
            }
            let Some(does) = self.list(slider, from, files) else {
                continue;
            };
            let mut diffs = conform::carried(&ties, &does, crate::tri::LEAST, &mut scratch);
            let own = &mut self.own[slider];
            if !spared.is_empty() {
                // What is spared goes on doing what it did.
                diffs.retain(|d| !spared.contains(&d.vertex));
                if let Some(old) = own.iter().find(|o| o.target == target) {
                    diffs.extend(old.diffs.iter().filter(|d| spared.contains(&d.vertex)));
                }
                diffs.sort_by_key(|d| d.vertex);
            }
            own.retain(|o| o.target != target);
            if !diffs.is_empty() {
                moving += 1;
                own.push(Own {
                    target: target.clone(),
                    diffs,
                });
            }
        }
        Ok(moving)
    }

    /// Move, turn and scale a whole shape about the middle of the mesh's
    /// space: scaled first, then turned about X, Y and Z in that order (in
    /// degrees), then moved. What sliders do to the shape is scaled and
    /// turned with it, where it is the project's own.
    pub fn transform(
        &mut self,
        shape: &str,
        by: [f32; 3],
        scale: f32,
        turn: [f32; 3],
    ) -> Result<(), Error> {
        let at = self.shape(shape)?;
        if !(scale.is_finite() && scale > 0.0) || by.iter().chain(&turn).any(|c| !c.is_finite()) {
            return invalid(
                "A shape is scaled by more than nothing, and moved and turned by numbers.",
            );
        }
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let [(sx, cx), (sy, cy), (sz, cz)] = turn.map(|d| d.to_radians().sin_cos());
        let turned = |p: [f32; 3]| {
            let p = p.map(|c| c * scale);
            let p = [p[0], p[1] * cx - p[2] * sx, p[1] * sx + p[2] * cx];
            let p = [p[0] * cy + p[2] * sy, p[1], -p[0] * sy + p[2] * cy];
            [p[0] * cz - p[1] * sz, p[0] * sz + p[1] * cz, p[2]]
        };
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let mut positions = mesh::read(&self.nif, block).map_err(fail)?.positions;
        for p in &mut positions {
            let t = turned(*p);
            *p = [t[0] + by[0], t[1] + by[1], t[2] + by[2]];
        }
        mesh::set_positions(&mut self.nif, block, &positions).map_err(fail)?;
        let target = &self.set.shapes[at].target;
        for own in &mut self.own {
            for o in own.iter_mut().filter(|o| &o.target == target) {
                for d in &mut o.diffs {
                    d.offset = turned(d.offset);
                }
            }
        }
        Ok(())
    }

    /// Give `shape` the bones of `body`, so that it bends as the body does:
    /// each of its vertices follows the bones the body's vertices nearest
    /// it follow, the nearer the more, and the heaviest four of them. The
    /// shape's own weights are replaced, but for the vertices in `spared`,
    /// which keep theirs. Returns how many bones it now follows.
    pub fn copy_weights(
        &mut self,
        shape: &str,
        body: &str,
        options: conform::Options,
        spared: &[u16],
    ) -> Result<usize, Error> {
        if shape == body {
            return invalid("A shape takes its weights from another shape.");
        }
        self.shape(shape)?;
        self.shape(body)?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let block = |name: &str| {
            self.block(name)
                .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {name}.")))
        };
        let (to, from) = (block(shape)?, block(body)?);
        let theirs = weights::read(&self.nif, from).map_err(fail)?;
        let ours = mesh::read(&self.nif, to).map_err(fail)?.positions;
        let on = mesh::read(&self.nif, from).map_err(fail)?.positions;
        if on.len() != theirs.vertices.len() {
            return invalid(format!(
                "{body} has weights for another number of vertices."
            ));
        }
        let ties = conform::tie(&on, &ours, options);
        // Every bone there is to follow: the body's, then any only the
        // shape has, which a spared vertex may still follow.
        let mut names = theirs.bones.clone();
        let had = if spared.is_empty() {
            None
        } else {
            let had = weights::read(&self.nif, to).map_err(fail)?;
            for bone in &had.bones {
                if !names.contains(bone) {
                    names.push(bone.clone());
                }
            }
            Some(had)
        };
        // Those the shape ends up following, in that order.
        let mut place: Vec<Option<u16>> = vec![None; names.len()];
        let mut mixed: Vec<[(u16, f32); weights::MOST]> = Vec::with_capacity(ours.len());
        let mut all: Vec<(u16, f32)> = Vec::new();
        for v in 0..ours.len() {
            all.clear();
            match had.as_ref().filter(|_| spared.contains(&(v as u16))) {
                Some(had) => {
                    for (bone, weight) in had.vertices.get(v).copied().unwrap_or_default() {
                        if weight > 0.0 {
                            let name = &had.bones[bone as usize];
                            let at = names.iter().position(|n| n == name).unwrap_or(0);
                            all.push((at as u16, weight));
                        }
                    }
                }
                None => {
                    for (near, share) in ties.of(v) {
                        for (bone, weight) in theirs.vertices[near as usize] {
                            if weight > 0.0 {
                                all.push((bone, weight * share));
                            }
                        }
                    }
                }
            }
            let kept = weights::settled(&all);
            for (bone, weight) in kept {
                if weight > 0.0 {
                    if let Some(p) = place.get_mut(bone as usize) {
                        *p = Some(0);
                    }
                }
            }
            mixed.push(kept);
        }
        let mut bones = Vec::new();
        for (i, p) in place.iter_mut().enumerate() {
            if p.is_some() {
                *p = Some(bones.len() as u16);
                bones.push(names[i].clone());
            }
        }
        for slots in &mut mixed {
            for (bone, weight) in slots.iter_mut() {
                if *weight > 0.0 {
                    *bone = place[*bone as usize].unwrap_or(0);
                }
            }
        }
        let skin = weights::Skin {
            bones,
            vertices: mixed,
        };
        weights::write(&mut self.nif, to, &skin, Some(from)).map_err(fail)?;
        Ok(skin.bones.len())
    }

    /// How much each vertex of `shape` follows `bone`: nothing for a bone
    /// the shape does not follow at all.
    pub fn bone_weights(&self, shape: &str, bone: &str) -> Result<Vec<f32>, Error> {
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let skin = weights::read(&self.nif, block).map_err(|e| Error::Invalid(e.to_string()))?;
        let at = skin.bones.iter().position(|b| b == bone);
        Ok(skin
            .vertices
            .iter()
            .map(|slots| {
                slots
                    .iter()
                    .find(|(b, w)| Some(*b as usize) == at && *w > 0.0)
                    .map_or(0.0, |x| x.1)
            })
            .collect())
    }

    /// Set how much some vertices of `shape` follow `bone` (nothing to all
    /// of it). What each follows of its other bones is scaled to make room,
    /// keeping their shares of each other; a vertex with no other bone
    /// follows this one wholly. A bone the shape did not follow is placed
    /// as another shape of the mesh that follows it has it.
    pub fn paint_weights(
        &mut self,
        shape: &str,
        bone: &str,
        painted: &[(u16, f32)],
    ) -> Result<(), Error> {
        self.shape(shape)?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let mut skin = weights::read(&self.nif, block).map_err(fail)?;
        let (at, like) = match skin.bones.iter().position(|b| b == bone) {
            Some(at) => (at, None),
            None => {
                let like = mesh::shapes(&self.nif).into_iter().find(|other| {
                    *other != block
                        && mesh::bone_names(&self.nif, *other)
                            .iter()
                            .any(|b| b == bone)
                });
                if like.is_none() {
                    return invalid(format!("No shape here follows a bone named {bone}."));
                }
                skin.bones.push(bone.to_string());
                (skin.bones.len() - 1, like)
            }
        };
        for (vertex, weight) in painted {
            let Some(slots) = skin.vertices.get_mut(*vertex as usize) else {
                return invalid(format!("{shape} has {} vertices.", skin.vertices.len()));
            };
            if !weight.is_finite() {
                continue;
            }
            let weight = weight.clamp(0.0, 1.0);
            let mut all: Vec<(u16, f32)> = slots
                .iter()
                .copied()
                .filter(|(b, w)| *w > 0.0 && *b as usize != at)
                .collect();
            let rest: f32 = all.iter().map(|x| x.1).sum();
            if rest > 0.0 {
                for x in &mut all {
                    x.1 *= (1.0 - weight) / rest;
                }
                all.push((at as u16, weight));
            } else if weight > 0.0 {
                all.push((at as u16, 1.0));
            } else {
                // Its only bone: it has to follow something.
                continue;
            }
            *slots = weights::settled(&all);
        }
        weights::write(&mut self.nif, block, &skin, like).map_err(fail)
    }

    /// Take away what a slider does to one shape, where that is the
    /// project's own.
    pub fn clear_slider(&mut self, slider: &str, shape: &str) -> Result<(), Error> {
        let (index, at) = (self.slider(slider)?, self.shape(shape)?);
        let target = &self.set.shapes[at].target;
        if self.set.sliders[index]
            .data
            .iter()
            .any(|d| &d.target == target)
        {
            return invalid(format!(
                "What {slider} does to {shape} is another project's, and is not changed from here."
            ));
        }
        self.own[index].retain(|o| &o.target != target);
        Ok(())
    }

    /// Turn what a slider does round: every vertex it moves goes as far the
    /// other way, on every shape where that is the project's own.
    pub fn negate_slider(&mut self, slider: &str) -> Result<(), Error> {
        let index = self.slider(slider)?;
        for own in &mut self.own[index] {
            for d in &mut own.diffs {
                d.offset = d.offset.map(|c| -c);
            }
        }
        Ok(())
    }

    /// Conform every shape that is the project's own to `body` (see
    /// [`Project::conform`]). Returns how many shapes were.
    pub fn conform_all(
        &mut self,
        body: &str,
        files: &dyn Files,
        options: conform::Options,
    ) -> Result<usize, Error> {
        self.shape(body)?;
        let shapes: Vec<String> = self
            .set
            .shapes
            .iter()
            .filter(|s| s.name != body && s.data_folder.is_none())
            .map(|s| s.name.clone())
            .collect();
        for shape in &shapes {
            self.conform(shape, body, files, options, &[])?;
        }
        Ok(shapes.len())
    }

    /// Make the outfit as it stands with each slider at `values` (percent,
    /// in the set's order) the shape every slider starts from: the base
    /// shape. Only the project's own shapes move; what sliders do from
    /// there is as it was.
    pub fn set_base(&mut self, values: &[f32]) -> Result<(), Error> {
        if values.len() != self.set.sliders.len() {
            return invalid(format!(
                "{} sliders, and {} values",
                self.set.sliders.len(),
                values.len()
            ));
        }
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        for shape in self.set.shapes.clone() {
            let Some(block) = self.block(&shape.name) else {
                continue;
            };
            let mut positions = mesh::read(&self.nif, block).map_err(fail)?.positions;
            let before = positions.clone();
            for ((slider, own), value) in self.set.sliders.iter().zip(&self.own).zip(values) {
                if slider.zap || slider.uv {
                    continue;
                }
                let amount = crate::morph::applied(slider, *value);
                for o in own.iter().filter(|o| o.target == shape.target) {
                    crate::morph::add(&mut positions, &o.diffs, amount);
                }
            }
            if positions != before {
                mesh::set_positions(&mut self.nif, block, &positions).map_err(fail)?;
            }
        }
        Ok(())
    }

    /// Take vertices out of a shape, with every triangle that had one of
    /// them, and with what sliders did to them.
    pub fn delete_vertices(&mut self, shape: &str, vertices: &[u16]) -> Result<(), Error> {
        let at = self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let count = mesh::read(&self.nif, block).map_err(fail)?.positions.len();
        let mut gone = vec![false; count];
        for v in vertices {
            match gone.get_mut(*v as usize) {
                Some(g) => *g = true,
                None => return invalid(format!("{shape} has {count} vertices.")),
            }
        }
        if !gone.contains(&true) {
            return Ok(());
        }
        let target = self.set.shapes[at].target.clone();
        if self
            .set
            .sliders
            .iter()
            .any(|s| s.data.iter().any(|d| d.target == target))
        {
            return invalid(format!(
                "What sliders do to {shape} is another project's, which counts on every vertex of it."
            ));
        }
        let left = edit::remove_vertices(&mut self.nif, block, &gone).map_err(fail)?;
        if left == 0 {
            return invalid("That would leave nothing of the shape. Delete the shape instead.");
        }
        let mut next = 0u16;
        let now: Vec<Option<u16>> = gone
            .iter()
            .map(|g| {
                (!g).then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        for own in &mut self.own {
            for o in own.iter_mut().filter(|o| o.target == target) {
                o.diffs = o
                    .diffs
                    .iter()
                    .filter_map(|d| {
                        now.get(d.vertex as usize)
                            .copied()
                            .flatten()
                            .map(|vertex| Diff {
                                vertex,
                                offset: d.offset,
                            })
                    })
                    .collect();
            }
        }
        Ok(())
    }

    /// Turn a shape's texture over: left to right (`across`) or top to
    /// bottom.
    pub fn flip_uv(&mut self, shape: &str, across: bool) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let mut uvs = mesh::read(&self.nif, block).map_err(fail)?.uvs;
        if uvs.is_empty() {
            return invalid(format!("{shape} has no texture coordinates."));
        }
        for uv in &mut uvs {
            let k = if across { 0 } else { 1 };
            uv[k] = 1.0 - uv[k];
        }
        mesh::set_uvs(&mut self.nif, block, &uvs).map_err(fail)
    }

    /// A new slider that takes these vertices of a shape out when it is on.
    pub fn add_zap(&mut self, name: &str, shape: &str, vertices: &[u16]) -> Result<(), Error> {
        let at = self.shape(shape)?;
        if vertices.is_empty() {
            return invalid("Mask what the slider is to take out first.");
        }
        let before = self.clone();
        let done = (|| {
            self.add_slider(name)?;
            let slider = self.set.sliders.last_mut().expect("just added");
            slider.zap = true;
            slider.small = 100.0;
            slider.big = 100.0;
            let name = slider.name.clone();
            // A vertex is marked by a move of any size: one unit up.
            let marks: Vec<Diff> = vertices
                .iter()
                .map(|v| Diff {
                    vertex: *v,
                    offset: [0.0, 0.0, 1.0],
                })
                .collect();
            let shape = self.set.shapes[at].name.clone();
            self.move_vertices(&shape, Some(&name), &marks)
        })();
        if done.is_err() {
            *self = before;
        }
        done
    }

    /// Name the texture files a shape is drawn with, slot by slot.
    pub fn set_textures(&mut self, shape: &str, textures: &[String]) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        if textures
            .iter()
            .any(|t| t.len() > 260 || t.chars().any(char::is_control))
        {
            return invalid("A texture is named by a path, of 260 letters at most.");
        }
        mesh::set_textures(&mut self.nif, block, textures)
            .map_err(|e| Error::Invalid(e.to_string()))
    }

    /// Part some vertices of a shape off as a shape of their own, named
    /// `to`: the first keeps the rest. Both keep what sliders did to them.
    pub fn separate(&mut self, shape: &str, to: &str, vertices: &[u16]) -> Result<(), Error> {
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let count = mesh::read(&self.nif, block)
            .map_err(|e| Error::Invalid(e.to_string()))?
            .positions
            .len();
        let mut parted = vec![false; count];
        for v in vertices {
            match parted.get_mut(*v as usize) {
                Some(p) => *p = true,
                None => return invalid(format!("{shape} has {count} vertices.")),
            }
        }
        let rest: Vec<u16> = (0..count as u32)
            .filter(|v| !parted[*v as usize])
            .map(|v| v as u16)
            .collect();
        if rest.is_empty() || rest.len() == count {
            return invalid("Mask the part of the shape that is to be its own shape.");
        }
        let taken: Vec<u16> = (0..count as u32)
            .filter(|v| parted[*v as usize])
            .map(|v| v as u16)
            .collect();
        let before = self.clone();
        let done = (|| {
            self.duplicate_shape(shape, to)?;
            let to = to.trim().to_string();
            self.delete_vertices(shape, &taken)?;
            self.delete_vertices(&to, &rest)
        })();
        if done.is_err() {
            *self = before;
        }
        done
    }

    /// Stop a shape following a bone: what each vertex followed of it goes
    /// to its other bones, in their shares. A vertex that followed nothing
    /// else keeps the bone, and then so does the shape.
    pub fn drop_bone(&mut self, shape: &str, bone: &str) -> Result<(), Error> {
        self.shape(shape)?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let skin = weights::read(&self.nif, block).map_err(fail)?;
        let Some(at) = skin.bones.iter().position(|b| b == bone) else {
            return invalid(format!("{shape} does not follow a bone named {bone}."));
        };
        let follows: Vec<(u16, f32)> = skin
            .vertices
            .iter()
            .enumerate()
            .filter(|(_, slots)| slots.iter().any(|(b, w)| *b as usize == at && *w > 0.0))
            .map(|(v, _)| (v as u16, 0.0))
            .collect();
        self.paint_weights(shape, bone, &follows)?;
        // Out of the shape's list of bones, when nothing follows it now.
        let mut skin = weights::read(&self.nif, block).map_err(fail)?;
        if skin
            .vertices
            .iter()
            .flatten()
            .any(|(b, w)| *b as usize == at && *w > 0.0)
        {
            return invalid(format!(
                "Some of {shape} follows {bone} and nothing else, and has to follow something."
            ));
        }
        skin.bones.remove(at);
        for slots in &mut skin.vertices {
            for (b, w) in slots.iter_mut() {
                if *w > 0.0 && *b as usize > at {
                    *b -= 1;
                }
            }
        }
        weights::write(&mut self.nif, block, &skin, None).map_err(fail)
    }

    /// Say which part of the body the `index`th part of a shape is.
    pub fn set_part(&mut self, shape: &str, index: usize, part: u16) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        weights::set_part(&mut self.nif, block, index, part)
            .map_err(|e| Error::Invalid(e.to_string()))
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
