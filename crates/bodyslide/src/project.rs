//! A set being worked on: its mesh and what each slider does to each of its
//! own shapes, held together so they can be changed and saved as a project.

use crate::build::{Files, Output};
use crate::conform;
use crate::osd::{Diff, SliderData};
use crate::set::{DataRef, Shape, Slider, SliderSet};
use crate::Error;
use apoc_nif::{copy, edit, mesh, weights, Nif};
use std::collections::HashMap;
use std::sync::Arc;

/// What a file says its mesh's vertices follow: its bones by name, for
/// each vertex four pairs of a bone's number among them and how much, and
/// where the file stands each bone (nothing for one it does not stand).
#[derive(Debug, Clone, Copy)]
pub struct Followed<'a> {
    pub bones: &'a [String],
    pub follows: &'a [[(u32, f32); 4]],
    pub stands: &'a [Option<Stand>],
}

/// Where a file stands a bone: its place, and its turn a row at a time.
pub type Stand = ([f32; 3], [f32; 9]);

/// What one slider does to one of the project's own shapes.
#[derive(Debug, Clone, PartialEq)]
struct Own {
    target: String,
    /// Shared with the copies of a project kept to step back to: a list is
    /// only copied when it is the one being changed.
    diffs: Arc<Vec<Diff>>,
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
                    .or_insert_with(|| {
                        files
                            .read(&at)
                            .and_then(|b| SliderData::read_file(&at, &b).ok())
                    })
                    .as_ref()
                    .and_then(|data| data.list(&d.list).map(<[_]>::to_vec));
                match diffs {
                    Some(diffs) => mine.push(Own {
                        target: d.target,
                        diffs: Arc::new(diffs),
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

    /// How many bytes the project's mesh is: what a copy of the project
    /// kept to step back to costs, its slider lists being shared.
    pub fn mesh_bytes(&self) -> usize {
        self.nif.blocks.iter().map(|b| b.data.len()).sum()
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
                    lock_normals: false,
                    smooth_seam_angle: None,
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
            // Lit when built as the one it is a copy of.
            lock_normals: first.lock_normals,
            smooth_seam_angle: first.smooth_seam_angle,
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
                // Lit when built as its own set says.
                lock_normals: theirs.lock_normals,
                smooth_seam_angle: theirs.smooth_seam_angle.clone(),
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

    /// Say what a slider that moves the texture does to a shape of the
    /// project's own: for each vertex given, how far its place in the
    /// texture goes, across and down. What it did to that shape before is
    /// put aside for this.
    pub fn set_uv_moves(
        &mut self,
        shape: &str,
        slider: &str,
        moves: &[(u16, [f32; 2])],
    ) -> Result<(), Error> {
        let (at, index) = (self.shape(shape)?, self.slider(slider)?);
        if !self.set.sliders[index].uv {
            return invalid(format!("{slider} does not move the texture."));
        }
        let said = self.set.shapes[at].clone();
        if said.data_folder.is_some() {
            return invalid(format!(
                "{shape} is another project's, and is not changed from here."
            ));
        }
        let count = self
            .block(shape)
            .and_then(|b| mesh::read(&self.nif, b).ok())
            .map_or(0, |m| m.uvs.len());
        if count == 0 {
            return invalid(format!("{shape} has no places in a texture to move."));
        }
        if moves
            .iter()
            .any(|(v, by)| *v as usize >= count || by.iter().any(|c| !c.is_finite()))
        {
            return invalid(format!("{shape} has {count} vertices."));
        }
        let diffs: Vec<Diff> = moves
            .iter()
            .filter(|(_, by)| *by != [0.0; 2])
            .map(|(vertex, by)| Diff {
                vertex: *vertex,
                offset: [by[0], by[1], 0.0],
            })
            .collect();
        let own = &mut self.own[index];
        own.retain(|o| o.target != said.target);
        own.push(Own {
            target: said.target,
            diffs: Arc::new(diffs),
        });
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
            Some(i) => Arc::make_mut(&mut own[i].diffs),
            None => {
                own.push(Own {
                    target,
                    diffs: Arc::default(),
                });
                Arc::make_mut(&mut own.last_mut().expect("just pushed").diffs)
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
            return Some(own.diffs.to_vec());
        }
        let d = self.set.sliders[slider]
            .data
            .iter()
            .find(|d| d.target == of.target)?;
        let folder = of.data_folder.as_deref().unwrap_or(&self.set.data_folder);
        let data = files.read(&path(&["ShapeData", folder, &d.file]))?;
        SliderData::read_file(&d.file, &data)
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
                    diffs: Arc::new(diffs),
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
        if positions.iter().flatten().any(|c| !c.is_finite()) {
            return invalid("That would put the shape further off than a mesh can say.");
        }
        mesh::set_positions(&mut self.nif, block, &positions).map_err(fail)?;
        let target = &self.set.shapes[at].target;
        if self
            .own
            .iter()
            .flatten()
            .filter(|o| &o.target == target)
            .flat_map(|o| o.diffs.iter())
            .any(|d| turned(d.offset).iter().any(|c| !c.is_finite()))
        {
            return invalid("That would scale what a slider does past what a mesh can say.");
        }
        for own in &mut self.own {
            for o in own.iter_mut().filter(|o| &o.target == target) {
                for d in Arc::make_mut(&mut o.diffs) {
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
        let mut is_spared = vec![false; ours.len()];
        for v in spared {
            if let Some(s) = is_spared.get_mut(*v as usize) {
                *s = true;
            }
        }
        // Those the shape ends up following, in that order.
        let mut place: Vec<Option<u16>> = vec![None; names.len()];
        let mut mixed: Vec<[(u16, f32); weights::MOST]> = Vec::with_capacity(ours.len());
        let mut all: Vec<(u16, f32)> = Vec::new();
        for v in 0..ours.len() {
            all.clear();
            match had
                .as_ref()
                .filter(|_| is_spared.get(v).copied().unwrap_or(false))
            {
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

    /// The bones `shape` follows, each with how it stood when the shape was
    /// fitted to it, and for each vertex the bones it follows and by how
    /// much, as numbers into that list.
    #[allow(clippy::type_complexity)]
    pub fn rig(
        &self,
        shape: &str,
    ) -> Result<
        (
            Vec<(String, apoc_nif::node::Transform)>,
            Vec<[(u16, f32); 4]>,
        ),
        Error,
    > {
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let skin = weights::read(&self.nif, block).map_err(fail)?;
        let binds = weights::bone_binds(&self.nif, block).map_err(fail)?;
        if binds.len() != skin.bones.len() {
            return invalid(format!("{shape} and its skin disagree about its bones."));
        }
        Ok((skin.bones.into_iter().zip(binds).collect(), skin.vertices))
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
                // Placed as another shape that follows it has it, or failing
                // that from where the bone's own node is in the mesh.
                let like = mesh::shapes(&self.nif).into_iter().find(|other| {
                    *other != block
                        && mesh::bone_names(&self.nif, *other)
                            .iter()
                            .any(|b| b == bone)
                });
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
    /// Turn what a slider does round one way only: every vertex it moves
    /// goes as far the other way along `axis` (0 across, 1 front to back, 2
    /// up and down), and as before along the other two.
    pub fn flip_slider(&mut self, slider: &str, axis: usize) -> Result<(), Error> {
        let index = self.slider(slider)?;
        if axis > 2 {
            return invalid("A slider is turned round across, front to back, or up and down.");
        }
        for own in &mut self.own[index] {
            for d in Arc::make_mut(&mut own.diffs) {
                d.offset[axis] = -d.offset[axis];
            }
        }
        Ok(())
    }

    pub fn negate_slider(&mut self, slider: &str) -> Result<(), Error> {
        let index = self.slider(slider)?;
        for own in &mut self.own[index] {
            for d in Arc::make_mut(&mut own.diffs) {
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
        if count > u16::MAX as usize + 1 {
            return invalid(format!("{shape} has more vertices than can be numbered."));
        }
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
                o.diffs = Arc::new(
                    o.diffs
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
                        .collect(),
                );
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

    /// Say how a shape is lit when it is built: its normals kept as the
    /// mesh has them, or worked out with a seam's two sides lit as one
    /// within `angle` degrees (nothing for the usual sixty).
    pub fn set_normals_rule(
        &mut self,
        shape: &str,
        lock: bool,
        angle: Option<f32>,
    ) -> Result<(), Error> {
        let at = self.shape(shape)?;
        if angle.is_some_and(|a| !(0.0..=180.0).contains(&a)) {
            return invalid("The angle is 0 to 180 degrees.");
        }
        let said = &mut self.set.shapes[at];
        said.lock_normals = lock;
        said.smooth_seam_angle = angle
            .filter(|a| *a != apoc_nif::shading::SMOOTH_ANGLE)
            .map(|a| format!("{a}"));
        Ok(())
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

    /// Change how a shape takes light: its glow, how solid it is, its gloss
    /// and its shine.
    pub fn set_look(&mut self, shape: &str, look: &mesh::Look) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let colours = look.glow.iter().chain(&look.shine);
        // How solid may pass all of it: files use that for how it fades at
        // its edges.
        if colours.clone().any(|c| !(0.0..=1.0).contains(c))
            || [
                look.alpha,
                look.glow_strength,
                look.gloss,
                look.shine_strength,
            ]
            .iter()
            .any(|v| !(0.0..=1000.0).contains(v))
        {
            return invalid(
                "Colours run from 0 to 1; opacity, strengths and gloss from 0 to 1000.",
            );
        }
        // Which game's rule holds is the file's to say, not the asker's.
        let smooth = mesh::look(&self.nif, block).is_some_and(|l| l.smooth);
        if smooth && look.gloss > 1.0 {
            return invalid("Smoothness runs from 0 to 1.");
        }
        mesh::set_look(&mut self.nif, block, look).map_err(|e| Error::Invalid(e.to_string()))
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

    /// Give a bone's twin across the body what the bone has, mirrored:
    /// each vertex follows the twin as much as the vertex opposite it
    /// follows the bone. Returns the twin's name.
    pub fn mirror_bone(&mut self, shape: &str, bone: &str) -> Result<String, Error> {
        self.shape(shape)?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let twin = weights::across(bone);
        if twin == bone {
            return invalid(format!(
                "{bone} is not a bone of one side with a twin on the other."
            ));
        }
        if !copy::bone_names(&self.nif).contains(&twin) {
            return invalid(format!("The mesh has no bone named {twin}."));
        }
        let has = self.bone_weights(shape, bone)?;
        if has.iter().all(|w| *w <= 0.0) {
            return invalid(format!("{shape} does not follow {bone}."));
        }
        let positions = mesh::read(&self.nif, block).map_err(fail)?.positions;
        // The vertex opposite each: the nearest to where it would be were
        // the shape turned over, left for right.
        let across: Vec<[f32; 3]> = positions.iter().map(|p| [-p[0], p[1], p[2]]).collect();
        let ties = conform::tie(
            &positions,
            &across,
            conform::Options {
                nearest: 1,
                ..conform::Options::default()
            },
        );
        let was = self.bone_weights(shape, &twin)?;
        let painted: Vec<(u16, f32)> = (0..positions.len())
            .filter_map(|v| {
                let opposite = ties.of(v).next()?.0 as usize;
                let w = has.get(opposite).copied().unwrap_or(0.0);
                // Only where it changes anything: a vertex neither follows
                // is not made to follow its other bones any differently.
                (w > 0.0 || was.get(v).is_some_and(|x| *x > 0.0)).then_some((v as u16, w))
            })
            .collect();
        self.paint_weights(shape, &twin, &painted)?;
        Ok(twin)
    }

    /// Stop every shape of the project's own following a bone, as
    /// [`Project::drop_bone`] stops one. Returns how many followed it.
    pub fn drop_bone_everywhere(&mut self, bone: &str) -> Result<usize, Error> {
        let before = self.clone();
        let mut dropped = 0;
        let names: Vec<String> = self
            .set
            .shapes
            .iter()
            .filter(|s| s.data_folder.is_none())
            .map(|s| s.name.clone())
            .collect();
        for name in names {
            let follows = self
                .block(&name)
                .is_some_and(|b| mesh::bone_names(&self.nif, b).iter().any(|n| n == bone));
            if !follows {
                continue;
            }
            if let Err(e) = self.drop_bone(&name, bone) {
                *self = before;
                return Err(e);
            }
            dropped += 1;
        }
        if dropped == 0 {
            return invalid(format!("No shape of the project's own follows {bone}."));
        }
        Ok(dropped)
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

    /// Move the triangles of a shape whose vertices are all in `vertices`
    /// into its `to`th part, or into a new part numbered `new_part`.
    /// Returns how many moved.
    pub fn move_to_part(
        &mut self,
        shape: &str,
        vertices: &[u16],
        to: Option<usize>,
        new_part: u16,
    ) -> Result<usize, Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let count = mesh::read(&self.nif, block).map_err(fail)?.positions.len();
        let mut marked = vec![false; count];
        for v in vertices {
            if let Some(m) = marked.get_mut(*v as usize) {
                *m = true;
            }
        }
        let before = self.nif.clone();
        let moved = weights::move_to_part(&mut self.nif, block, &marked, to, new_part);
        if moved.is_err() {
            self.nif = before;
        }
        moved.map_err(fail)
    }

    /// Take the `index`th part out of a shape, its triangles going to the
    /// `into`th.
    pub fn remove_part(&mut self, shape: &str, index: usize, into: usize) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let before = self.nif.clone();
        let done = weights::remove_part(&mut self.nif, block, index, into);
        if done.is_err() {
            self.nif = before;
        }
        done.map_err(|e| Error::Invalid(e.to_string()))
    }

    /// Part a shape from its bones: it follows none, and is as a shape that
    /// never did.
    pub fn remove_skinning(&mut self, shape: &str) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let before = self.nif.clone();
        let was = mesh::read(&self.nif, block).map_err(|e| Error::Invalid(e.to_string()))?;
        let done = weights::unskin(&mut self.nif, block).and_then(|()| {
            // Read back as a check: the same vertices, and no bones.
            let at = mesh::shapes(&self.nif)
                .into_iter()
                .find(|at| mesh::name(&self.nif, *at).as_deref() == Some(was.name.as_str()));
            let now = at.map(|at| mesh::read(&self.nif, at)).transpose()?;
            if now.is_some_and(|n| n.positions == was.positions && n.uvs == was.uvs && !n.skinned) {
                Ok(())
            } else {
                Err(apoc_nif::NifError::Unsupported(
                    "the shape could not be parted from its bones".into(),
                ))
            }
        });
        if done.is_err() {
            self.nif = before;
        }
        done.map_err(|e| Error::Invalid(e.to_string()))
    }

    /// The cloth data the mesh carries (Fallout 4's), as a Havok file.
    pub fn cloth(&self) -> Option<&[u8]> {
        apoc_nif::edit::cloth(&self.nif)
    }

    /// Give the mesh cloth data: a Havok file, in place of any it carried.
    pub fn set_cloth(&mut self, file: &[u8]) -> Result<(), Error> {
        let before = self.nif.clone();
        let done = apoc_nif::edit::set_cloth(&mut self.nif, file);
        if done.is_err() {
            self.nif = before;
        }
        done.map_err(|e| Error::Invalid(e.to_string()))
    }

    /// Give a shape's vertices colours to carry, all white and solid, where
    /// they carry none.
    pub fn add_colors(&mut self, shape: &str) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let before = self.nif.clone();
        let done = mesh::add_colors(&mut self.nif, block).and_then(|()| {
            // Read back as a check: as many colours as it has vertices.
            let count = mesh::read(&self.nif, block)?.positions.len();
            if mesh::colors(&self.nif, block)?.len() == count {
                Ok(())
            } else {
                Err(apoc_nif::NifError::Unsupported(
                    "this kind of shape cannot be given colours".into(),
                ))
            }
        });
        if done.is_err() {
            self.nif = before;
        }
        done.map_err(|e| Error::Invalid(e.to_string()))
    }

    /// Give vertices of a shape colours: each `(vertex, colour)`.
    pub fn paint_colors(&mut self, shape: &str, colors: &[(u16, [u8; 4])]) -> Result<(), Error> {
        self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let count = mesh::colors(&self.nif, block).map_err(fail)?.len();
        if count == 0 {
            return invalid(format!("{shape} carries no colours to paint."));
        }
        if colors.iter().any(|(v, _)| *v as usize >= count) {
            return invalid(format!("{shape} has {count} vertices."));
        }
        mesh::set_colors(&mut self.nif, block, colors).map_err(fail)
    }

    /// Bring a bone into the mesh from a skeleton (a mesh file of bones),
    /// placed as the skeleton has it, for shapes to be painted onto.
    pub fn add_bone(&mut self, bone: &str, skeleton: &[u8]) -> Result<(), Error> {
        let from = Nif::read(skeleton).map_err(|e| Error::Invalid(e.to_string()))?;
        copy::copy_bone(&mut self.nif, &from, bone)
            .map(drop)
            .map_err(|e| Error::Invalid(e.to_string()))
    }

    /// A bone made up, not one of a skeleton's: named, and standing where
    /// it is put (`turn` a row at a time), for shapes to be painted onto.
    pub fn new_bone(&mut self, bone: &str, at: [f32; 3], turn: [f32; 9]) -> Result<(), Error> {
        let bone = bone.trim();
        if bone.is_empty() || bone.len() > 255 || bone.chars().any(char::is_control) {
            return invalid("Give the bone a name, of letters a file can hold.");
        }
        if at.iter().chain(&turn).any(|c| !c.is_finite()) {
            return invalid("Where the bone stands is not numbers a mesh can hold.");
        }
        // A turn only: rows a unit long and square to each other.
        let row = |r: usize| [turn[r * 3], turn[r * 3 + 1], turn[r * 3 + 2]];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let square = (0..3).all(|r| (dot(row(r), row(r)) - 1.0).abs() < 0.01)
            && dot(row(0), row(1)).abs() < 0.01
            && dot(row(0), row(2)).abs() < 0.01
            && dot(row(1), row(2)).abs() < 0.01;
        if !square {
            return invalid("How the bone is turned is not a turn.");
        }
        let place = apoc_nif::node::Transform {
            translation: at,
            rotation: turn,
            scale: 1.0,
        };
        copy::new_bone(&mut self.nif, bone, place)
            .map(drop)
            .map_err(|e| Error::Invalid(e.to_string()))
    }

    /// Make two shapes one: `from` goes into `into`, with what sliders do
    /// to it, and is no longer a shape of its own. The one shape is drawn
    /// as `into` was: with its textures.
    pub fn merge_shapes(&mut self, into: &str, from: &str) -> Result<(), Error> {
        let (a, b) = (self.shape(into)?, self.shape(from)?);
        if a == b {
            return invalid("A shape is merged into another shape.");
        }
        let (ta, tb) = (
            self.set.shapes[a].target.clone(),
            self.set.shapes[b].target.clone(),
        );
        if self
            .set
            .sliders
            .iter()
            .any(|s| s.data.iter().any(|d| d.target == ta || d.target == tb))
        {
            return invalid(
                "What sliders do to one of them is another project's, which counts on its vertices as they are.",
            );
        }
        let block = |p: &Project, name: &str| {
            p.block(name)
                .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {name}.")))
        };
        let before = self.clone();
        let done = (|| {
            let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
            let (to, other) = (block(self, into)?, block(self, from)?);
            let first = weights::merge(&mut self.nif, to, other).map_err(fail)?;
            // The sphere round it, round all of it now.
            let positions = mesh::read(&self.nif, to).map_err(fail)?.positions;
            mesh::set_positions(&mut self.nif, to, &positions).map_err(fail)?;
            edit::remove_shapes(&mut self.nif, &[other]).map_err(fail)?;
            for own in &mut self.own {
                let Some(at) = own.iter().position(|o| o.target == tb) else {
                    continue;
                };
                let theirs = own.remove(at);
                // Past what one shape can number, a slider moves nothing.
                let moved = theirs.diffs.iter().filter_map(|d| {
                    Some(Diff {
                        vertex: u16::try_from(d.vertex as usize + first).ok()?,
                        offset: d.offset,
                    })
                });
                match own.iter_mut().find(|o| o.target == ta) {
                    Some(mine) => Arc::make_mut(&mut mine.diffs).extend(moved),
                    None => own.push(Own {
                        target: ta.clone(),
                        diffs: Arc::new(moved.collect()),
                    }),
                }
            }
            self.set.shapes.remove(b);
            Ok(())
        })();
        if done.is_err() {
            *self = before;
        }
        done
    }

    /// Refine a shape: every triangle made four, with a vertex in the
    /// middle of each edge. What sliders do to a new vertex is halfway
    /// between what they do to the two it lies between. Returns how many
    /// vertices were added.
    pub fn refine(&mut self, shape: &str, spared: &[u16]) -> Result<usize, Error> {
        let at = self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let target = self.set.shapes[at].target.clone();
        if self
            .set
            .sliders
            .iter()
            .any(|s| s.data.iter().any(|d| d.target == target))
        {
            return invalid(format!(
                "What sliders do to {shape} is another project's, which counts on its vertices as they are."
            ));
        }
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let had = mesh::read(&self.nif, block).map_err(fail)?.positions.len();
        let before = self.nif.clone();
        let between = match weights::refine(&mut self.nif, block, spared) {
            Ok(b) => b,
            Err(e) => {
                self.nif = before;
                return Err(fail(e));
            }
        };
        // The sphere round it stays right: nothing is further out than it was.
        for own in &mut self.own {
            for o in own.iter_mut().filter(|o| o.target == target) {
                // What it does to each vertex, records for one vertex added
                // up as applying them adds them.
                let mut dense: HashMap<u16, [f32; 3]> = HashMap::new();
                for d in o.diffs.iter() {
                    let sum = dense.entry(d.vertex).or_insert([0.0; 3]);
                    for (c, by) in sum.iter_mut().zip(d.offset) {
                        *c += by;
                    }
                }
                let diffs = Arc::make_mut(&mut o.diffs);
                for (i, (a, b)) in between.iter().enumerate() {
                    let (da, db) = (dense.get(a).copied(), dense.get(b).copied());
                    if da.is_none() && db.is_none() {
                        continue;
                    }
                    let (da, db) = (da.unwrap_or([0.0; 3]), db.unwrap_or([0.0; 3]));
                    let offset = [0, 1, 2].map(|k| (da[k] + db[k]) / 2.0);
                    let Ok(vertex) = u16::try_from(had + i) else {
                        break;
                    };
                    dense.insert(vertex, offset);
                    diffs.push(Diff { vertex, offset });
                }
            }
        }
        Ok(between.len())
    }

    /// A new shape made of an outside mesh (an OBJ or FBX file's): its vertices,
    /// their places in the texture, and its triangles. It is made in the
    /// likeness of `like`, a shape of the project: drawn with what that is
    /// drawn with, and following the bones its nearest vertices follow.
    pub fn shape_from(
        &mut self,
        like: &str,
        name: &str,
        positions: &[[f32; 3]],
        uvs: &[[f32; 2]],
        triangles: &[[u16; 3]],
        given: Option<Followed>,
    ) -> Result<(), Error> {
        let name = name.trim();
        if given.is_some_and(|g| g.follows.len() != positions.len()) {
            return invalid("The file says what other vertices follow than it has.");
        }
        if positions.is_empty() || triangles.is_empty() {
            return invalid("The file has no triangles to make a shape of.");
        }
        if positions.len() > u16::MAX as usize || triangles.len() > u16::MAX as usize {
            return invalid("The file has more vertices or triangles than one shape can hold.");
        }
        if positions.iter().flatten().any(|c| !c.is_finite())
            || uvs.iter().flatten().any(|c| !c.is_finite())
            || triangles
                .iter()
                .flatten()
                .any(|v| *v as usize >= positions.len())
        {
            return invalid("The file's mesh is not one a shape can be made of.");
        }
        let before = self.clone();
        let done = (|| {
            self.duplicate_shape(like, name)?;
            let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
            let block = self
                .block(name)
                .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {name}.")))?;
            let (bytes, layout, _) = weights::packed(&self.nif, block).map_err(fail)?;
            let stride = layout.stride.max(1);
            let template = mesh::read(&self.nif, block).map_err(fail)?.positions;
            let skin = weights::skin_or_none(&self.nif, block).map_err(fail)?;
            if template.is_empty()
                || template.len() != skin.vertices.len()
                || template.len() * stride != bytes.len()
            {
                return invalid(format!(
                    "{like} is not a shape another can be made in the likeness of."
                ));
            }
            // The new shape is the project's own, whatever its likeness is:
            // what sliders did to that, here or in another project, is not
            // what they do to this.
            if let Some(made) = self.set.shapes.iter_mut().find(|s| s.name == name) {
                made.data_folder = None;
            }
            for slider in &mut self.set.sliders {
                slider.data.retain(|d| d.target != name);
            }
            // Each new vertex starts as the template's nearest, for what a
            // vertex carries besides its place: colour, and its bones.
            let ties = conform::tie(
                &template,
                positions,
                conform::Options {
                    nearest: 1,
                    ..conform::Options::default()
                },
            );
            let mut data = Vec::with_capacity(positions.len() * stride);
            let mut vertices = Vec::with_capacity(positions.len());
            for v in 0..positions.len() {
                let near = ties.of(v).next().map_or(0, |x| x.0 as usize);
                data.extend_from_slice(&bytes[near * stride..(near + 1) * stride]);
                vertices.push(skin.vertices[near]);
            }
            let mut bones = skin.bones.clone();
            if let Some(Followed {
                bones: names,
                follows,
                stands,
            }) = given
            {
                // Names as the mesh will hold them: without the space round
                // them a file may carry.
                let trimmed: Vec<String> = names.iter().map(|n| n.trim().to_string()).collect();
                let names = &trimmed[..];
                // A bone of the file's is one of the mesh's by its name; one
                // the mesh has not got is left out, and a vertex left with
                // nothing keeps what its likeness follows.
                // A bone the file stands somewhere and the mesh has not got
                // is made where the file stands it, if a vertex follows it.
                let known: std::collections::HashSet<String> =
                    copy::bone_names(&self.nif).into_iter().collect();
                let mut wanted = vec![false; names.len()];
                for (bone, weight) in follows.iter().flatten() {
                    if let Some(w) = wanted.get_mut(*bone as usize) {
                        *w |= *weight > 0.0 && weight.is_finite();
                    }
                }
                for (i, bone) in names.iter().enumerate() {
                    let Some((at, turn)) = stands.get(i).copied().flatten() else {
                        continue;
                    };
                    if wanted[i] && !known.contains(bone) && !bone.trim().is_empty() {
                        self.new_bone(bone, at, turn)?;
                    }
                }
                let have: std::collections::HashSet<String> =
                    copy::bone_names(&self.nif).into_iter().collect();
                let mut at: Vec<Option<u16>> = vec![None; names.len()];
                for (v, slots) in follows.iter().enumerate() {
                    let mut all: Vec<(u16, f32)> = Vec::with_capacity(4);
                    for (bone, weight) in slots {
                        let Some(name) = names.get(*bone as usize) else {
                            continue;
                        };
                        if !(*weight > 0.0 && weight.is_finite()) || !have.contains(name) {
                            continue;
                        }
                        let slot = &mut at[*bone as usize];
                        let index = match *slot {
                            Some(i) => i,
                            None => {
                                let i = match bones.iter().position(|b| b == name) {
                                    Some(i) => i,
                                    None => {
                                        bones.push(name.clone());
                                        bones.len() - 1
                                    }
                                };
                                let Ok(i) = u16::try_from(i) else {
                                    continue;
                                };
                                *slot = Some(i);
                                i
                            }
                        };
                        // No more than all of it: weights that add up past
                        // what a number can hold would leave it nothing.
                        all.push((index, weight.min(1.0)));
                    }
                    if !all.is_empty() {
                        vertices[v] = weights::settled(&all);
                    }
                }
            }
            if given.is_some() {
                // Only the bones something follows: those of its likeness
                // that the file gave no vertex to are not the new shape's.
                let mut used = vec![false; bones.len()];
                for slot in vertices.iter().flatten().filter(|x| x.1 > 0.0) {
                    if let Some(u) = used.get_mut(slot.0 as usize) {
                        *u = true;
                    }
                }
                let mut now = vec![0u16; bones.len()];
                let mut kept = Vec::with_capacity(bones.len());
                for (i, name) in bones.iter().enumerate() {
                    if used[i] {
                        now[i] = kept.len() as u16;
                        kept.push(name.clone());
                    }
                }
                if !kept.is_empty() {
                    for slot in vertices.iter_mut().flatten() {
                        slot.0 = now.get(slot.0 as usize).copied().unwrap_or(0);
                    }
                    bones = kept;
                }
            }
            let made = weights::Skin { bones, vertices };
            weights::rebuild(
                &mut self.nif,
                block,
                data,
                vec![triangles.to_vec()],
                &made,
                None,
            )
            .map_err(fail)?;
            mesh::set_positions(&mut self.nif, block, positions).map_err(fail)?;
            if uvs.len() == positions.len() {
                mesh::set_uvs(&mut self.nif, block, uvs).map_err(fail)?;
            }
            // What sliders did to the template is not what they do to this.
            for own in &mut self.own {
                own.retain(|o| o.target != name);
            }
            Ok(())
        })();
        if done.is_err() {
            *self = before;
        }
        done
    }

    /// Take sliders from the morph file of a part of a head: each of its
    /// morphs becomes what the slider of that name does to the one shape of
    /// the project's own that has as many vertices as the file. Such a
    /// file names no shape, so it must be plain which is meant.
    fn import_head_morphs(&mut self, file: &[u8]) -> Result<usize, Error> {
        let tri = crate::headtri::read(file)?;
        let fits: Vec<Shape> = self
            .set
            .shapes
            .iter()
            .filter(|s| s.data_folder.is_none())
            .filter(|s| {
                self.block(&s.name)
                    .and_then(|b| mesh::read(&self.nif, b).ok())
                    .is_some_and(|m| m.positions.len() == tri.vertices.len())
            })
            .cloned()
            .collect();
        let [shape] = &fits[..] else {
            return invalid(format!(
                "The file is for a mesh of {} vertices, and {} of the project's shapes have that many.",
                tri.vertices.len(),
                if fits.is_empty() { "none" } else { "more than one" }
            ));
        };
        let before = self.clone();
        let mut taken = 0;
        let done = (|| {
            for morph in &tri.morphs {
                if !morph.scale.is_finite() || morph.name.trim().is_empty() {
                    continue;
                }
                // A name a project file could not hold is not a slider's.
                if morph.name.chars().any(char::is_control) {
                    return invalid("A morph in the file has a name no slider can have.");
                }
                let diffs: Vec<Diff> = morph
                    .offsets
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| **o != [0; 3])
                    .map(|(v, o)| Diff {
                        vertex: v as u16,
                        offset: o.map(|c| c as f32 * morph.scale),
                    })
                    .collect();
                if diffs
                    .iter()
                    .any(|d| d.offset.iter().any(|c| !c.is_finite()))
                {
                    return invalid(format!(
                        "{} in the file moves a vertex further than a mesh can say.",
                        morph.name
                    ));
                }
                let index = match self.set.sliders.iter().position(|s| s.name == morph.name) {
                    Some(i) => i,
                    None => {
                        self.add_slider(&morph.name)?;
                        self.set.sliders.len() - 1
                    }
                };
                if self.set.sliders[index].zap || self.set.sliders[index].uv {
                    continue;
                }
                let own = &mut self.own[index];
                own.retain(|o| o.target != shape.target);
                own.push(Own {
                    target: shape.target.clone(),
                    diffs: Arc::new(diffs),
                });
                taken += 1;
            }
            Ok(())
        })();
        if done.is_err() {
            *self = before;
        }
        done.map(|()| taken)
    }

    /// The project's sliders as the morph file of a part of a head: the
    /// shape as it is, and what each slider of the project's own does to
    /// it, under the slider's name. Sliders that take vertices out or move
    /// the texture are left out, as are those that say nothing of it.
    pub fn head_morphs(&self, shape: &str) -> Result<Vec<u8>, Error> {
        let at = self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let m = mesh::read(&self.nif, block).map_err(|e| Error::Invalid(e.to_string()))?;
        // Every such file measured has a place in the texture for each
        // vertex; one without is not written.
        if m.uvs.len() != m.positions.len() {
            return invalid(format!(
                "{shape} has no place in the texture for each vertex, as a head's morph file needs."
            ));
        }
        let target = &self.set.shapes[at].target;
        let mut morphs = Vec::new();
        for (slider, own) in self.set.sliders.iter().zip(&self.own) {
            if slider.zap || slider.uv {
                continue;
            }
            // Everything the slider does to it, where it is said in more
            // than one list, added up as applying them adds them.
            let mut lists = own.iter().filter(|o| &o.target == target).peekable();
            if lists.peek().is_none() {
                continue;
            }
            let mut offsets = vec![[0.0f32; 3]; m.positions.len()];
            for d in lists.flat_map(|o| o.diffs.iter()) {
                if let Some(slot) = offsets.get_mut(d.vertex as usize) {
                    for (c, by) in slot.iter_mut().zip(d.offset) {
                        *c += by;
                    }
                }
            }
            // One that does nothing to it is written all the same: the game
            // looks each up by its name.
            morphs.push(crate::headtri::HeadMorph::of(&slider.name, &offsets));
        }
        if morphs.is_empty() {
            return invalid(format!(
                "No slider of the project's own does anything to {shape}."
            ));
        }
        Ok(crate::headtri::write(&crate::headtri::HeadTri {
            vertices: m.positions,
            triangles: m.triangles.iter().map(|t| t.map(u32::from)).collect(),
            // Places in the texture as the mesh has them: the file's own
            // were found to be the mesh's, unturned.
            uvs: m.uvs.clone(),
            morphs,
        }))
    }

    /// Take sliders from a morph file (`.tri`): each morph it has for a
    /// shape of the project's own becomes what the slider of that name
    /// does to the shape, a slider being added where there is none.
    /// Returns how many morphs were taken.
    pub fn import_morphs(&mut self, file: &[u8]) -> Result<usize, Error> {
        if crate::headtri::is(file) {
            return self.import_head_morphs(file);
        }
        let morphs = crate::tri::read(file)?;
        let before = self.clone();
        let mut taken = 0;
        let done = (|| {
            for of_shape in &morphs.shapes {
                let Some(shape) = self
                    .set
                    .shapes
                    .iter()
                    .find(|s| s.name == of_shape.name && s.data_folder.is_none())
                    .cloned()
                else {
                    continue;
                };
                let count = self
                    .block(&shape.name)
                    .and_then(|b| mesh::read(&self.nif, b).ok())
                    .map_or(0, |m| m.positions.len());
                for morph in &of_shape.morphs {
                    if morph.offsets.iter().any(|(v, _)| *v as usize >= count) {
                        return invalid(format!(
                            "The file moves a vertex {} does not have: it is for another mesh.",
                            shape.name
                        ));
                    }
                    let index = match self.set.sliders.iter().position(|s| s.name == morph.name) {
                        Some(i) => i,
                        None => {
                            self.add_slider(&morph.name)?;
                            self.set.sliders.len() - 1
                        }
                    };
                    if self.set.sliders[index].zap || self.set.sliders[index].uv {
                        continue;
                    }
                    let diffs: Vec<Diff> = morph
                        .offsets
                        .iter()
                        .map(|(vertex, o)| Diff {
                            vertex: *vertex,
                            offset: o.map(|c| c as f32 * morph.multiplier),
                        })
                        .collect();
                    if diffs
                        .iter()
                        .any(|d| d.offset.iter().any(|c| !c.is_finite()))
                    {
                        return invalid(format!(
                            "{} in the file moves a vertex further than a mesh can say.",
                            morph.name
                        ));
                    }
                    let own = &mut self.own[index];
                    own.retain(|o| o.target != shape.target);
                    own.push(Own {
                        target: shape.target.clone(),
                        diffs: Arc::new(diffs),
                    });
                    taken += 1;
                }
            }
            if taken == 0 {
                return invalid("Nothing in the file is for a shape of this project.");
            }
            Ok(())
        })();
        if done.is_err() {
            *self = before;
        }
        done.map(|()| taken)
    }

    /// Give `shape` one bone of `body`'s: how much each of its vertices
    /// follows `bone` is what the body's vertices nearest it do, and what
    /// it follows of its other bones makes room. The vertices in `spared`
    /// are left as they are.
    pub fn copy_bone(
        &mut self,
        shape: &str,
        body: &str,
        bone: &str,
        options: conform::Options,
        spared: &[u16],
    ) -> Result<(), Error> {
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
        let theirs = self.bone_weights(body, bone)?;
        if theirs.iter().all(|w| *w == 0.0) {
            return invalid(format!("{body} does not follow {bone}."));
        }
        let ours = mesh::read(&self.nif, to).map_err(fail)?.positions;
        let on = mesh::read(&self.nif, from).map_err(fail)?.positions;
        let ties = conform::tie(&on, &ours, options);
        let mut is_spared = vec![false; ours.len()];
        for v in spared {
            if let Some(s) = is_spared.get_mut(*v as usize) {
                *s = true;
            }
        }
        let painted: Vec<(u16, f32)> = (0..ours.len())
            .filter(|v| !is_spared[*v])
            .map(|v| {
                let weight: f32 = ties
                    .of(v)
                    .map(|(near, share)| theirs.get(near as usize).copied().unwrap_or(0.0) * share)
                    .sum();
                (v as u16, weight)
            })
            .collect();
        self.paint_weights(shape, bone, &painted)
    }

    /// Make the outfit as it stands with each slider at `values` the base
    /// shape, and a new slider that takes it back to what it was: a
    /// reference for fitting what was made for the one to the other.
    pub fn conversion(&mut self, name: &str, values: &[f32]) -> Result<(), Error> {
        let before = self.clone();
        let done = (|| {
            self.combine(name, values)?;
            // The new slider is last, and at nothing while the base is set.
            let at = self.slider(name.trim())?;
            let mut with = values.to_vec();
            with.insert(at.min(with.len()), 0.0);
            self.set_base(&with)?;
            self.negate_slider(name.trim())
        })();
        if done.is_err() {
            *self = before;
        }
        done
    }

    /// A new slider that does at once what the others do together at
    /// `values` (percent, in the set's order): the outfit as it stands, as
    /// one slider. Only what the project's own shapes are moved by counts.
    pub fn combine(&mut self, name: &str, values: &[f32]) -> Result<(), Error> {
        if values.len() != self.set.sliders.len() {
            return invalid(format!(
                "{} sliders, and {} values",
                self.set.sliders.len(),
                values.len()
            ));
        }
        let mut sums: Vec<(String, HashMap<u16, [f32; 3]>)> = Vec::new();
        for ((slider, own), value) in self.set.sliders.iter().zip(&self.own).zip(values) {
            if slider.zap || slider.uv {
                continue;
            }
            let amount = crate::morph::applied(slider, *value);
            if amount == 0.0 {
                continue;
            }
            for o in own {
                let at = match sums.iter().position(|(t, _)| *t == o.target) {
                    Some(at) => at,
                    None => {
                        sums.push((o.target.clone(), HashMap::new()));
                        sums.len() - 1
                    }
                };
                for d in o.diffs.iter() {
                    let sum = sums[at].1.entry(d.vertex).or_insert([0.0; 3]);
                    for (c, by) in sum.iter_mut().zip(d.offset) {
                        *c += by * amount;
                    }
                }
            }
        }
        if sums.is_empty() {
            return invalid("Move the sliders the new one is to stand for first.");
        }
        self.add_slider(name)?;
        let made = self.own.last_mut().expect("just added");
        for (target, sum) in sums {
            let mut diffs: Vec<Diff> = sum
                .into_iter()
                .filter(|(_, o)| o.iter().any(|c| c.abs() >= GONE))
                .map(|(vertex, offset)| Diff { vertex, offset })
                .collect();
            diffs.sort_by_key(|d| d.vertex);
            made.push(Own {
                target,
                diffs: Arc::new(diffs),
            });
        }
        Ok(())
    }

    /// Weld a shape: vertices at one place, with one place in the
    /// texture, become one. What sliders did to the first of them is what
    /// they do to the one. Returns how many vertices went.
    pub fn weld(&mut self, shape: &str) -> Result<usize, Error> {
        let at = self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let target = self.set.shapes[at].target.clone();
        if self
            .set
            .sliders
            .iter()
            .any(|s| s.data.iter().any(|d| d.target == target))
        {
            return invalid(format!(
                "What sliders do to {shape} is another project's, which counts on its vertices as they are."
            ));
        }
        let before = self.nif.clone();
        let now = match weights::weld(&mut self.nif, block) {
            Ok(now) => now,
            Err(e) => {
                self.nif = before;
                return Err(Error::Invalid(e.to_string()));
            }
        };
        let left = now.iter().map(|v| *v as usize + 1).max().unwrap_or(0);
        let gone = now.len() - left;
        if gone == 0 {
            return Ok(0);
        }
        // Each vertex that is left is the first of those welded into it,
        // and does what that one did: nothing, if it did nothing.
        let mut first: Vec<Option<u16>> = vec![None; left];
        for (old, new) in now.iter().enumerate() {
            first[*new as usize].get_or_insert(old as u16);
        }
        for own in &mut self.own {
            for o in own.iter_mut().filter(|o| o.target == target) {
                let mut did: HashMap<u16, [f32; 3]> = HashMap::new();
                for d in o.diffs.iter() {
                    let sum = did.entry(d.vertex).or_insert([0.0; 3]);
                    for (c, by) in sum.iter_mut().zip(d.offset) {
                        *c += by;
                    }
                }
                let diffs: Vec<Diff> = first
                    .iter()
                    .enumerate()
                    .filter_map(|(new, old)| {
                        did.get(&(*old)?).map(|offset| Diff {
                            vertex: new as u16,
                            offset: *offset,
                        })
                    })
                    .collect();
                o.diffs = Arc::new(diffs);
            }
        }
        Ok(gone)
    }

    /// Collapse, split or flip the edge between two vertices of a shape.
    /// What sliders do follows: a vertex put in the middle of an edge, or
    /// made of its two ends, does half of what each end did.
    pub fn edge(&mut self, shape: &str, a: u16, b: u16, op: weights::EdgeOp) -> Result<(), Error> {
        let at = self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let target = self.set.shapes[at].target.clone();
        if self
            .set
            .sliders
            .iter()
            .any(|s| s.data.iter().any(|d| d.target == target))
        {
            return invalid(format!(
                "What sliders do to {shape} is another project's, which counts on its vertices as they are."
            ));
        }
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let had = mesh::read(&self.nif, block).map_err(fail)?.positions.len();
        let before = self.nif.clone();
        let done = match weights::edge(&mut self.nif, block, a, b, op) {
            Ok(done) => done,
            Err(e) => {
                self.nif = before;
                return Err(fail(e));
            }
        };
        if done == weights::EdgeDone::Same {
            return Ok(());
        }
        for own in &mut self.own {
            for o in own.iter_mut().filter(|o| o.target == target) {
                let mut did: HashMap<u16, [f32; 3]> = HashMap::new();
                for d in o.diffs.iter() {
                    let sum = did.entry(d.vertex).or_insert([0.0; 3]);
                    for (c, by) in sum.iter_mut().zip(d.offset) {
                        *c += by;
                    }
                }
                // Half of what each end of the edge did, where either did anything.
                let half = (did.contains_key(&a) || did.contains_key(&b)).then(|| {
                    let (x, y) = (
                        did.get(&a).copied().unwrap_or([0.0; 3]),
                        did.get(&b).copied().unwrap_or([0.0; 3]),
                    );
                    [0, 1, 2].map(|k| (x[k] + y[k]) / 2.0)
                });
                let mut diffs: Vec<Diff> = match &done {
                    weights::EdgeDone::Added(..) => {
                        let mut all = o.diffs.to_vec();
                        if let (Some(offset), Ok(vertex)) = (half, u16::try_from(had)) {
                            all.push(Diff { vertex, offset });
                        }
                        all
                    }
                    weights::EdgeDone::Renumbered(now) => {
                        let (keep, gone) = (a.min(b), a.max(b));
                        did.remove(&gone);
                        match half {
                            Some(offset) => did.insert(keep, offset),
                            None => did.remove(&keep),
                        };
                        did.into_iter()
                            .filter_map(|(v, offset)| {
                                now.get(v as usize).map(|vertex| Diff {
                                    vertex: *vertex,
                                    offset,
                                })
                            })
                            .collect()
                    }
                    weights::EdgeDone::Same => o.diffs.to_vec(),
                };
                diffs.sort_by_key(|d| d.vertex);
                o.diffs = Arc::new(diffs);
            }
        }
        Ok(())
    }

    /// Mirror a shape across the middle of the body: every vertex to the
    /// other side, its triangles turned to face out again, what sliders do
    /// to it mirrored too, and each bone it follows changed for its twin on
    /// the other side where the mesh has one.
    pub fn mirror_shape(&mut self, shape: &str) -> Result<(), Error> {
        let at = self.shape(shape)?;
        let block = self
            .block(shape)
            .ok_or_else(|| Error::Invalid(format!("The mesh has no shape named {shape}.")))?;
        let target = self.set.shapes[at].target.clone();
        if self
            .set
            .sliders
            .iter()
            .any(|s| s.data.iter().any(|d| d.target == target))
        {
            return invalid(format!(
                "What sliders do to {shape} is another project's, which would not be mirrored with it."
            ));
        }
        let fail = |e: apoc_nif::NifError| Error::Invalid(e.to_string());
        let before = self.nif.clone();
        let done = (|| {
            let mut positions = mesh::read(&self.nif, block)?.positions;
            for p in &mut positions {
                p[0] = -p[0];
            }
            mesh::set_positions(&mut self.nif, block, &positions)?;
            weights::flip_facing(&mut self.nif, block)?;
            // Its bones, each for its twin: all of them or, where one has
            // no twin to be placed by, none.
            if let Ok(mut skin) = weights::read(&self.nif, block) {
                let twins: Vec<String> = skin.bones.iter().map(|b| weights::across(b)).collect();
                if twins != skin.bones {
                    let like = mesh::shapes(&self.nif).into_iter().find(|other| {
                        let theirs = mesh::bone_names(&self.nif, *other);
                        *other != block
                            && twins
                                .iter()
                                .all(|t| theirs.contains(t) || skin.bones.contains(t))
                    });
                    skin.bones = twins;
                    // A twin that cannot be placed stops the mirror: a shape
                    // across the body still following this side's bones
                    // would bend with the wrong arm.
                    weights::write(&mut self.nif, block, &skin, like)?;
                }
            }
            Ok(())
        })();
        if done.is_err() {
            self.nif = before;
            return done.map_err(fail);
        }
        for own in &mut self.own {
            for o in own.iter_mut().filter(|o| o.target == target) {
                for d in Arc::make_mut(&mut o.diffs) {
                    d.offset[0] = -d.offset[0];
                }
            }
        }
        Ok(())
    }

    /// What one slider does to the project's own shapes, as a slider data
    /// file: a list for each shape, named as a saved project names them.
    pub fn slider_data(&self, slider: &str) -> Result<Vec<u8>, Error> {
        let index = self.slider(slider)?;
        let lists: Vec<(String, Vec<Diff>)> = self.own[index]
            .iter()
            .map(|o| (format!("{}{slider}", o.target), o.diffs.to_vec()))
            .collect();
        // Nothing the file would cut short.
        if lists
            .iter()
            .any(|(name, diffs)| name.len() > 255 || diffs.len() > u16::MAX as usize)
        {
            return invalid(format!(
                "{slider} has a name too long, or moves too much, for a slider data file."
            ));
        }
        if lists.is_empty() {
            return invalid(format!(
                "{slider} does nothing to this project's own shapes."
            ));
        }
        Ok(SliderData { version: 1, lists }.write())
    }

    /// Take what a slider does from a slider data file: each list in it
    /// named for one of the project's own shapes (the shape's name alone,
    /// or with a slider's name after it) replaces what the slider did to
    /// that shape. Returns how many shapes were given one.
    pub fn import_slider(&mut self, slider: &str, file: &[u8]) -> Result<usize, Error> {
        let index = self.slider(slider)?;
        let data = SliderData::read(file)?;
        let mut given = 0;
        for shape in self.set.shapes.clone() {
            if shape.data_folder.is_some() {
                continue;
            }
            // The list for this slider first, then any list for the shape.
            let named = format!("{}{slider}", shape.target);
            let Some((_, diffs)) = data
                .lists
                .iter()
                .find(|(n, _)| *n == named)
                .or_else(|| data.lists.iter().find(|(n, _)| *n == shape.target))
                .or_else(|| {
                    let only: Vec<_> = data
                        .lists
                        .iter()
                        .filter(|(n, _)| n.starts_with(&shape.target))
                        .collect();
                    (only.len() == 1).then(|| only[0])
                })
            else {
                continue;
            };
            let count = self
                .block(&shape.name)
                .and_then(|b| mesh::read(&self.nif, b).ok())
                .map_or(0, |m| m.positions.len());
            if diffs.iter().any(|d| d.vertex as usize >= count) {
                return invalid(format!(
                    "The file moves a vertex {} does not have: it is for another mesh.",
                    shape.name
                ));
            }
            let own = &mut self.own[index];
            own.retain(|o| o.target != shape.target);
            own.push(Own {
                target: shape.target.clone(),
                diffs: Arc::new(diffs.clone()),
            });
            given += 1;
        }
        if given == 0 {
            return invalid("Nothing in the file is named for a shape of this project.");
        }
        Ok(given)
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
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (slider, own) in set.sliders.iter_mut().zip(&self.own) {
            for o in own {
                // Named as the original names them, shape then slider; two
                // pairs that spell the same are told apart by a number, and
                // a name or a list the file cannot hold stops the save
                // before it is cut short.
                let mut list = format!("{}{}", o.target, slider.name);
                let mut again = 1;
                while !used.insert(list.clone()) {
                    again += 1;
                    list = format!("{}{} {again}", o.target, slider.name);
                }
                if list.len() > 255 {
                    return invalid(format!(
                        "{} for {} makes a name too long to save. Shorten one of them.",
                        slider.name, o.target
                    ));
                }
                if o.diffs.len() > u16::MAX as usize {
                    return invalid(format!(
                        "{} moves more of {} than a slider's data can hold.",
                        slider.name, o.target
                    ));
                }
                slider.data.push(DataRef {
                    name: list.clone(),
                    target: o.target.clone(),
                    local: true,
                    file: data_file.clone(),
                    list: list.clone(),
                });
                data.lists.push((list, o.diffs.to_vec()));
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
