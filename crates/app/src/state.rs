//! What the module holds, and each request it answers.

use apoc_bodyslide::project::{Project, Settings};
use apoc_bodyslide::{build, category, group, osd, preset, set};
use apoc_nif::{mesh, Nif};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default)]
pub struct State {
    /// Files of the BodySlide folder, by path under it, lower case, with
    /// forward slashes.
    files: HashMap<String, Vec<u8>>,
    sets: Vec<set::SliderSet>,
    /// The project file each set was read from, in step with `sets`.
    origins: Vec<String>,
    /// The project open in Outfit Studio.
    work: Option<Work>,
    presets: Vec<preset::Preset>,
    groups: Vec<group::Group>,
    categories: Vec<category::Category>,
    /// Files that could not be read, each with why.
    problems: Vec<String>,
}

/// A project being worked on, with the way back from each change.
struct Work {
    project: Project,
    /// The project as it would be saved now: its set, and its files by
    /// lower-case path.
    set: set::SliderSet,
    files: HashMap<String, Vec<u8>>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    changed: bool,
}

/// A mesh from outside, as [`Change::ShapeFrom`] sends it.
type Outside = (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[u16; 3]>);

fn outside_mesh(data: &[u8]) -> Option<(Outside, &[u8])> {
    let word = |at: usize| -> Option<[u8; 4]> { data.get(at..at + 4)?.try_into().ok() };
    let (nv, nt) = (
        u32::from_le_bytes(word(0)?) as usize,
        u32::from_le_bytes(word(4)?) as usize,
    );
    // Sizes against the bytes there are, before room is made for them.
    let need = 8usize
        .checked_add(nv.checked_mul(20)?)?
        .checked_add(nt.checked_mul(12)?)?;
    if need > data.len() {
        return None;
    }
    let rest = &data[need..];
    let float = |at: usize| word(at).map(f32::from_le_bytes);
    let mut at = 8;
    let mut positions = Vec::with_capacity(nv);
    for _ in 0..nv {
        positions.push([float(at)?, float(at + 4)?, float(at + 8)?]);
        at += 12;
    }
    let mut uvs = Vec::with_capacity(nv);
    for _ in 0..nv {
        uvs.push([float(at)?, float(at + 4)?]);
        at += 8;
    }
    let mut triangles = Vec::with_capacity(nt);
    for _ in 0..nt {
        let mut t = [0u16; 3];
        for (k, slot) in t.iter_mut().enumerate() {
            *slot = u16::try_from(u32::from_le_bytes(word(at + k * 4)?)).ok()?;
        }
        triangles.push(t);
        at += 12;
    }
    Some(((positions, uvs, triangles), rest))
}

/// What each vertex follows, as [`Change::ShapeFrom`] sends it after the
/// mesh: four pairs a vertex, a bone's number and how much.
fn outside_weights(data: &[u8]) -> Vec<[(u32, f32); 4]> {
    data.as_chunks::<32>()
        .0
        .iter()
        .map(|c| {
            std::array::from_fn(|k| {
                let at = k * 8;
                (
                    u32::from_le_bytes([c[at], c[at + 1], c[at + 2], c[at + 3]]),
                    f32::from_le_bytes([c[at + 4], c[at + 5], c[at + 6], c[at + 7]]),
                )
            })
        })
        .collect()
}

/// Vertex numbers sent as a request's bytes, four bytes each.
fn numbers(data: &[u8]) -> Vec<u16> {
    data.as_chunks::<4>()
        .0
        .iter()
        .filter_map(|c| u16::try_from(u32::from_le_bytes(*c)).ok())
        .collect()
}

/// How many changes can be taken back, at most.
const UNDO: usize = 24;

/// And how many bytes of mesh the copies kept for that may add up to: a
/// large mesh keeps fewer steps, and always one.
const UNDO_BYTES: usize = 192 * 1024 * 1024;

impl Work {
    fn of(project: Project) -> Result<Work, String> {
        let mut work = Work {
            set: project.set.clone(),
            project,
            files: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            changed: false,
        };
        work.refresh()?;
        Ok(work)
    }

    fn refresh(&mut self) -> Result<(), String> {
        let (set, files) = self
            .project
            .save(&self.project.set.name)
            .map_err(|e| e.to_string())?;
        self.set = set;
        self.files = files.into_iter().map(|o| (key(&o.path), o.bytes)).collect();
        Ok(())
    }

    fn change(
        &mut self,
        change: Change,
        data: &[u8],
        files: &dyn build::Files,
        sets: &[set::SliderSet],
    ) -> Result<(), String> {
        let before = self.project.clone();
        let p = &mut self.project;
        let done = match change {
            Change::RenameShape { from, to } => p.rename_shape(&from, &to),
            Change::DeleteShape { name } => p.delete_shape(&name),
            Change::RenameSlider { from, to } => p.rename_slider(&from, &to),
            Change::DeleteSlider { name } => p.delete_slider(&name),
            Change::AddSlider { name } => p.add_slider(&name),
            Change::MoveSlider { name, to } => p.move_slider(&name, to),
            Change::Settings {
                name,
                small,
                big,
                invert,
                hidden,
                zap,
                uv,
                clamp,
            } => p.settings(
                &name,
                Settings {
                    small,
                    big,
                    invert,
                    hidden,
                    zap,
                    uv,
                    clamp,
                },
            ),
            Change::Output {
                folder,
                file,
                weights,
            } => p.output(&folder, &file, weights),
            Change::Duplicate { name, to } => p.duplicate_shape(&name, &to),
            Change::Reference { set } => match sets.iter().find(|s| s.name == set) {
                Some(other) => p.add_reference(other, files),
                None => Err(apoc_bodyslide::Error::Invalid(format!(
                    "There is no set named {set}."
                ))),
            },
            Change::Conform { shape, body } => p
                .conform(&shape, &body, files, Default::default(), &numbers(data))
                .map(drop),
            Change::Transform {
                shape,
                by,
                scale,
                turn,
            } => p.transform(&shape, by, scale, turn),
            Change::Weights { shape, body } => p
                .copy_weights(&shape, &body, Default::default(), &numbers(data))
                .map(drop),
            Change::ClearSlider { slider, shape } => p.clear_slider(&slider, &shape),
            Change::NegateSlider { slider } => p.negate_slider(&slider),
            Change::ConformAll { body } => {
                p.conform_all(&body, files, Default::default()).map(drop)
            }
            Change::SetBase { values } => p.set_base(&values),
            Change::DeleteVertices { shape } => {
                let vertices: Vec<u16> = data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter_map(|c| u16::try_from(u32::from_le_bytes(*c)).ok())
                    .collect();
                p.delete_vertices(&shape, &vertices)
            }
            Change::FlipUv { shape, across } => p.flip_uv(&shape, across),
            Change::AddZap { name, shape } => {
                let vertices: Vec<u16> = data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter_map(|c| u16::try_from(u32::from_le_bytes(*c)).ok())
                    .collect();
                p.add_zap(&name, &shape, &vertices)
            }
            Change::Textures { shape, textures } => p.set_textures(&shape, &textures),
            Change::AddColours { shape } => p.add_colors(&shape),
            Change::Look {
                shape,
                glow,
                glow_strength,
                alpha,
                gloss,
                shine,
                shine_strength,
            } => p.set_look(
                &shape,
                &mesh::Look {
                    glow,
                    glow_strength,
                    alpha,
                    gloss,
                    shine,
                    shine_strength,
                },
            ),
            Change::Separate { shape, to } => {
                let vertices: Vec<u16> = data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter_map(|c| u16::try_from(u32::from_le_bytes(*c)).ok())
                    .collect();
                p.separate(&shape, &to, &vertices)
            }
            Change::DropBone { shape, bone } => p.drop_bone(&shape, &bone),
            Change::BodyPart { shape, index, part } => p.set_part(&shape, index, part),
            Change::Edge { shape, a, b, op } => {
                let op = match op.as_str() {
                    "collapse" => Some(apoc_nif::weights::EdgeOp::Collapse),
                    "split" => Some(apoc_nif::weights::EdgeOp::Split),
                    "flip" => Some(apoc_nif::weights::EdgeOp::Flip),
                    _ => None,
                };
                match op {
                    Some(op) => p.edge(&shape, a, b, op),
                    None => Err(apoc_bodyslide::Error::Invalid(
                        "An edge is collapsed, split or flipped.".into(),
                    )),
                }
            }
            Change::Weld { shape } => p.weld(&shape).map(drop),
            Change::Refine { shape } => p.refine(&shape, &numbers(data)).map(drop),
            Change::NewBone { bone, at, turn } => p.new_bone(
                &bone,
                at,
                turn.unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
            ),
            Change::ShapeFrom {
                like,
                name,
                bones,
                stands,
            } => match outside_mesh(data) {
                Some(((positions, uvs, triangles), rest))
                    if rest.is_empty() || rest.len() == positions.len() * 32 =>
                {
                    let follows = outside_weights(rest);
                    let stands: Vec<_> = stands
                        .iter()
                        .map(|s| s.as_ref().map(|s| (s.at, s.turn)))
                        .collect();
                    let given = (!rest.is_empty()).then_some(apoc_bodyslide::project::Followed {
                        bones: &bones,
                        follows: &follows,
                        stands: &stands,
                    });
                    p.shape_from(&like, &name, &positions, &uvs, &triangles, given)
                }
                _ => Err(apoc_bodyslide::Error::Invalid(
                    "The file's mesh could not be read.".into(),
                )),
            },
            Change::Merge { into, from } => p.merge_shapes(&into, &from),
            Change::Mirror { shape } => p.mirror_shape(&shape),
            Change::ToPart { shape, to, part } => {
                p.move_to_part(&shape, &numbers(data), to, part).map(drop)
            }
            Change::Combine { name, values } => p.combine(&name, &values),
            Change::ImportMorphs => p.import_morphs(data).map(drop),
            Change::ImportSlider { slider } => p.import_slider(&slider, data).map(drop),
            Change::CopyBone { shape, body, bone } => {
                p.copy_bone(&shape, &body, &bone, Default::default(), &numbers(data))
            }
            Change::AddBone { bone } => p.add_bone(&bone, data),
            Change::RemovePart { shape, index, into } => p.remove_part(&shape, index, into),
            Change::Colour { shape } => {
                let colors: Vec<(u16, [u8; 4])> = data
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .filter_map(|c| {
                        Some((
                            u16::try_from(u32::from_le_bytes([c[0], c[1], c[2], c[3]])).ok()?,
                            [c[4], c[5], c[6], c[7]],
                        ))
                    })
                    .collect();
                p.paint_colors(&shape, &colors)
            }
            Change::Paint {
                shape,
                bone,
                twin,
                first,
            } => {
                let painted: Vec<(u16, f32)> = data
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .filter_map(|c| {
                        Some((
                            u16::try_from(u32::from_le_bytes([c[0], c[1], c[2], c[3]])).ok()?,
                            f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                        ))
                    })
                    .collect();
                let split = first.unwrap_or(painted.len()).min(painted.len());
                p.paint_weights(&shape, &bone, &painted[..split])
                    .and_then(|()| match &twin {
                        Some(twin) => p.paint_weights(&shape, twin, &painted[split..]),
                        None => Ok(()),
                    })
            }
            Change::Stroke { shape, slider } => {
                let moves: Vec<osd::Diff> = data
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .filter_map(|c| {
                        let part = |i: usize| [c[i], c[i + 1], c[i + 2], c[i + 3]];
                        Some(osd::Diff {
                            vertex: u16::try_from(u32::from_le_bytes(part(0))).ok()?,
                            offset: [4, 8, 12].map(|i| f32::from_le_bytes(part(i))),
                        })
                    })
                    .filter(|d| d.offset.iter().all(|c| c.is_finite()))
                    .collect();
                p.move_vertices(&shape, slider.as_deref(), &moves)
            }
        };
        if let Err(e) = done
            .map_err(|e| e.to_string())
            .and_then(|()| self.refresh())
        {
            self.project = before;
            self.refresh()?;
            return Err(e);
        }
        if self.project != before {
            self.redo.clear();
            self.undo.push(before);
            self.trim();
            self.redo.clear();
            self.changed = true;
        }
        Ok(())
    }

    /// Let the oldest steps go while there are too many, or while what is
    /// kept, each at its own size and both ways counted together, is too
    /// much. One step back is always kept.
    fn trim(&mut self) {
        let weight = |list: &[Project]| list.iter().map(Project::mesh_bytes).sum::<usize>();
        while self.undo.len() > UNDO {
            self.undo.remove(0);
        }
        while self.undo.len() + self.redo.len() > 1
            && weight(&self.undo) + weight(&self.redo) > UNDO_BYTES
        {
            // What can be put back goes before what can be taken back.
            if self.redo.is_empty() {
                self.undo.remove(0);
            } else {
                self.redo.remove(0);
            }
        }
    }

    /// Step back (`back`) or forward again through the changes.
    fn step(&mut self, back: bool) -> Result<(), String> {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        if let Some(then) = from.pop() {
            to.push(std::mem::replace(&mut self.project, then));
            self.changed = true;
            self.trim();
            self.refresh()?;
        }
        Ok(())
    }
}

/// Where a file stands a bone.
#[derive(Debug, Clone, Deserialize)]
struct Stand {
    at: [f32; 3],
    turn: [f32; 9],
}

#[derive(Deserialize)]
#[serde(tag = "do", rename_all = "camelCase")]
enum Change {
    RenameShape {
        from: String,
        to: String,
    },
    DeleteShape {
        name: String,
    },
    RenameSlider {
        from: String,
        to: String,
    },
    DeleteSlider {
        name: String,
    },
    AddSlider {
        name: String,
    },
    MoveSlider {
        name: String,
        to: usize,
    },
    Settings {
        name: String,
        small: f32,
        big: f32,
        invert: bool,
        hidden: bool,
        zap: bool,
        uv: bool,
        clamp: bool,
    },
    Output {
        folder: String,
        file: String,
        weights: bool,
    },
    /// The same shape again under another name.
    Duplicate {
        name: String,
        to: String,
    },
    /// Bring the first shape of a set in as the reference: the body the
    /// outfit is fitted to.
    Reference {
        set: String,
    },
    /// Give a shape what every slider does to another (a body), so that
    /// it follows it.
    Conform {
        shape: String,
        body: String,
    },
    /// Move a whole shape by `by`, scaled by `scale` and turned by `turn`
    /// degrees about each axis first.
    Transform {
        shape: String,
        by: [f32; 3],
        scale: f32,
        turn: [f32; 3],
    },
    /// Give a shape the bones of another (a body), so that it bends as
    /// that one does.
    Weights {
        shape: String,
        body: String,
    },
    /// Take away what a slider does to one shape.
    ClearSlider {
        slider: String,
        shape: String,
    },
    /// Turn what a slider does round.
    NegateSlider {
        slider: String,
    },
    /// Conform every shape of the project's own to a body.
    ConformAll {
        body: String,
    },
    /// Make the outfit as it stands, each slider at `values`, the shape
    /// every slider starts from.
    SetBase {
        values: Vec<f32>,
    },
    /// Take vertices out of a shape: the request's bytes are their numbers,
    /// four bytes each.
    DeleteVertices {
        shape: String,
    },
    /// Turn a shape's texture over, left to right or top to bottom.
    FlipUv {
        shape: String,
        across: bool,
    },
    /// A new slider that takes vertices of a shape out: the request's bytes
    /// are their numbers, four bytes each.
    AddZap {
        name: String,
        shape: String,
    },
    /// Change how a shape takes light.
    #[serde(rename_all = "camelCase")]
    Look {
        shape: String,
        glow: [f32; 3],
        glow_strength: f32,
        alpha: f32,
        gloss: f32,
        shine: [f32; 3],
        shine_strength: f32,
    },
    /// Give a shape's vertices colours to carry, where they carry none.
    AddColours {
        shape: String,
    },
    /// Name the texture files a shape is drawn with, slot by slot.
    Textures {
        shape: String,
        textures: Vec<String>,
    },
    /// Part vertices of a shape off as a shape named `to`: the request's
    /// bytes are their numbers, four bytes each.
    Separate {
        shape: String,
        to: String,
    },
    /// Stop a shape following a bone.
    DropBone {
        shape: String,
        bone: String,
    },
    /// Say which part of the body one part of a shape is.
    BodyPart {
        shape: String,
        index: usize,
        part: u16,
    },
    /// Collapse, split or flip the edge between two vertices of a shape:
    /// `op` is "collapse", "split" or "flip".
    Edge {
        shape: String,
        a: u16,
        b: u16,
        op: String,
    },
    /// Weld a shape: vertices at one place become one.
    Weld {
        shape: String,
    },
    /// Refine a shape: every triangle made four. The request's bytes: the
    /// vertices to leave as they are, four bytes each.
    Refine {
        shape: String,
    },
    /// A new shape of an outside mesh, in the likeness of `like`. The
    /// request's bytes: how many vertices and how many triangles (four
    /// bytes each), then each vertex's place (three floats), each vertex's
    /// place in the texture (two floats), and each triangle's three vertices
    /// (four bytes each).
    ///
    /// With `bones`, those bytes go on with what each vertex follows: four
    /// pairs a vertex, a bone's number in `bones` (four bytes) and how much
    /// (a float). A vertex given nothing, or only bones the mesh has not
    /// got, follows what its likeness does.
    ShapeFrom {
        like: String,
        name: String,
        #[serde(default)]
        bones: Vec<String>,
        /// Where the file stands each of `bones`, for one the mesh has not
        /// got to be made there; nothing for one it does not stand.
        #[serde(default)]
        stands: Vec<Option<Stand>>,
    },
    /// A bone made up, standing at `at`, turned as `turn` (a row at a
    /// time) or not at all.
    NewBone {
        bone: String,
        at: [f32; 3],
        #[serde(default)]
        turn: Option<[f32; 9]>,
    },
    /// Make two shapes one: `from` goes into `into`.
    Merge {
        into: String,
        from: String,
    },
    /// Mirror a shape across the middle of the body.
    Mirror {
        shape: String,
    },
    /// Move the triangles among the vertices in the request's bytes to a
    /// part of the shape, or with no `to` to a new part numbered `part`.
    ToPart {
        shape: String,
        #[serde(default)]
        to: Option<usize>,
        #[serde(default)]
        part: u16,
    },
    /// A new slider that does what the others do together at `values`.
    Combine {
        name: String,
        values: Vec<f32>,
    },
    /// Take sliders from a morph file: the request's bytes.
    ImportMorphs,
    /// Take what a slider does from a slider data file: the request's bytes.
    ImportSlider {
        slider: String,
    },
    /// Give a shape one bone of a body's, as the body's vertices nearest
    /// each of its own follow it. The request's bytes name vertices to
    /// leave as they are.
    CopyBone {
        shape: String,
        body: String,
        bone: String,
    },
    /// Bring a bone into the mesh from a skeleton: the request's bytes.
    AddBone {
        bone: String,
    },
    /// Take one part out of a shape, its triangles going to another.
    RemovePart {
        shape: String,
        index: usize,
        into: usize,
    },
    /// Give vertices of a shape colours. The request's bytes are, for each
    /// vertex, its number in four bytes and then red, green, blue and how
    /// solid, a byte each.
    Colour {
        shape: String,
    },
    /// Set how much vertices of a shape follow a bone. The request's bytes
    /// are, for each vertex, its number and the weight, four bytes each.
    /// With `twin`, the first `first` of them are the bone's and the rest
    /// its twin's across the body: one stroke on both sides, one change.
    Paint {
        shape: String,
        bone: String,
        #[serde(default)]
        twin: Option<String>,
        #[serde(default)]
        first: Option<usize>,
    },
    /// A brush stroke on a shape. The request's bytes say what moved: for
    /// each vertex its number and three offsets, as four-byte numbers (the
    /// first whole, the rest floating). With `slider`, it is what the slider
    /// does to the shape that changes.
    Stroke {
        shape: String,
        #[serde(default)]
        slider: Option<String>,
    },
}

pub struct Answer {
    pub json: String,
    pub bytes: Vec<u8>,
}

impl Answer {
    fn ok(value: Value) -> Self {
        Answer {
            json: json!({ "ok": value }).to_string(),
            bytes: Vec::new(),
        }
    }

    fn with(value: Value, bytes: Vec<u8>) -> Self {
        Answer {
            json: json!({ "ok": value }).to_string(),
            bytes,
        }
    }

    fn failed(why: impl std::fmt::Display) -> Self {
        Answer {
            json: json!({ "error": why.to_string() }).to_string(),
            bytes: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Request {
    /// Hold a file. Its bytes come with the request.
    Put { path: String },
    /// Forget everything.
    Clear,
    /// Forget the held files under a folder; what was read from them stays.
    Drop { under: String },
    /// Read the sets, presets, groups and categories among the held files.
    Load,
    /// The files a set needs that are not held yet.
    Needs { set: String },
    /// A set's shapes and sliders, as numbers for the 3D view. With `work`,
    /// the open project's instead.
    Preview {
        set: String,
        #[serde(default)]
        work: bool,
    },
    /// Open a set as a project to work on. Any open project is let go.
    Open { set: String },
    /// Begin a project from a mesh: the request's bytes, which the mods
    /// have at `path` under the game's data folder. Any open project is
    /// let go.
    New { name: String, path: String },
    /// How much each vertex of a shape of the open project follows a bone:
    /// a number a vertex, after the answer.
    BoneWeights { shape: String, bone: String },
    /// The bones a shape of the open project follows and how each stood
    /// when the shape was fitted to it. After the answer, for each vertex,
    /// four pairs: a bone's number in that list in four bytes, and how much
    /// the vertex follows it (nothing for a pair not used).
    Rig { shape: String },
    /// The open project packed as one zip file for sharing, laid out as a
    /// mod is, after the answer.
    Pack,
    /// Every slider of the open project as a morph file, after the answer.
    Morphs,
    /// What one slider of the open project does, as a slider data file
    /// after the answer.
    SliderData { slider: String },
    /// The names of the bones of a skeleton: the request's bytes.
    Skeleton,
    /// Change the open project.
    Change { change: Change },
    /// Take the last change back, or with `again` put it back.
    Undo {
        #[serde(default)]
        again: bool,
    },
    /// The open project as files to save, under `name`.
    Save { name: String },
    /// Build a set with each slider at these values.
    Build {
        set: String,
        small: Vec<f32>,
        big: Vec<f32>,
        /// Also the morph file RaceMenu reads, and the meshes' link to it.
        #[serde(default)]
        morphs: bool,
    },
    /// One mesh of a set as it stands with each slider at `values`: what
    /// Outfit Studio exports. `path` is where it goes under the data folder.
    Export {
        set: String,
        values: Vec<f32>,
        path: String,
        /// The open project, as it stands, and not the set it was opened from.
        #[serde(default)]
        work: bool,
    },
    /// A groups file (the request's bytes, or none yet) with a set added to
    /// a group, as text to save.
    GroupAdd { group: String, set: String },
    /// A groups file (the request's bytes) with a set taken out of a group,
    /// as text to save. A group left with nothing goes too.
    GroupRemove { group: String, set: String },
    /// A presets file holding one preset, as text to save.
    PresetFile { preset: PresetIn },
}

#[derive(Deserialize)]
struct PresetIn {
    name: String,
    set: String,
    groups: Vec<String>,
    /// Slider name, value for the lightest body, value for the heaviest.
    values: Vec<(String, f32, f32)>,
}

fn key(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

/// A path with forward slashes and no empty parts, spelled as given.
fn key_keeping_case(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SliderOut<'a> {
    name: &'a str,
    shown: &'a str,
    small: f32,
    big: f32,
    invert: bool,
    hidden: bool,
    zap: bool,
    uv: bool,
    clamp: bool,
    category: Option<&'a str>,
    /// For a slider that takes vertices out: the others it switches when
    /// it is switched.
    toggles: &'a [String],
}

impl State {
    fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(&key(path)).cloned()
    }

    fn set(&self, name: &str) -> Result<&set::SliderSet, String> {
        self.sets
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("There is no set named {name}."))
    }

    /// The name a slider is shown under and the category it is in: the
    /// set's own name for it first, then the categories'.
    fn shown<'a>(&'a self, slider: &'a set::Slider) -> (&'a str, Option<&'a str>) {
        let found = self.categories.iter().find_map(|c| {
            c.sliders
                .iter()
                .find(|(name, _)| *name == slider.name)
                .map(|(_, shown)| (shown.as_str(), c.name.as_str()))
        });
        (
            slider
                .display_name
                .as_deref()
                .or(found.map(|f| f.0))
                .unwrap_or(&slider.name),
            found.map(|f| f.1),
        )
    }

    fn load(&mut self) -> Value {
        self.sets.clear();
        self.origins.clear();
        let mut found: Vec<(set::SliderSet, String)> = Vec::new();
        self.presets.clear();
        self.groups.clear();
        self.categories.clear();
        self.problems.clear();
        let mut paths: Vec<&String> = self.files.keys().collect();
        paths.sort();
        for path in paths {
            let bytes = &self.files[path];
            let read = if path.starts_with("slidersets/") && path.ends_with(".osp") {
                set::read(bytes).map(|v| found.extend(v.into_iter().map(|s| (s, path.clone()))))
            } else if path.starts_with("sliderpresets/") && path.ends_with(".xml") {
                preset::read(bytes).map(|v| self.presets.extend(v))
            } else if path.starts_with("slidergroups/") && path.ends_with(".xml") {
                group::read(bytes).map(|v| self.groups.extend(v))
            } else if path.starts_with("slidercategories/") && path.ends_with(".xml") {
                category::read(bytes).map(|v| self.categories.extend(v))
            } else {
                Ok(())
            };
            if let Err(e) = read {
                self.problems.push(format!("{path}: {e}"));
            }
        }
        found.sort_by_key(|(s, _)| s.name.to_lowercase());
        found.dedup_by(|a, b| a.0.name == b.0.name);
        (self.sets, self.origins) = found.into_iter().unzip();
        let sets: Vec<Value> = self.sets.iter().map(|s| self.described(s)).collect();
        let presets: Vec<Value> = self
            .presets
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "set": p.set,
                    "groups": p.groups,
                    "values": p.values.iter().map(|v| json!([v.name, v.big, v.value])).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({
            "sets": sets,
            "presets": presets,
            "groups": self.groups.iter().map(|g| &g.name).collect::<Vec<_>>(),
            "categories": self.categories.iter().map(|c| json!({ "name": c.name, "hidden": c.default_hidden })).collect::<Vec<_>>(),
            "problems": self.problems,
        })
    }

    /// A set as the interface lists it.
    fn described(&self, s: &set::SliderSet) -> Value {
        {
            {
                let sliders: Vec<SliderOut> = s
                    .sliders
                    .iter()
                    .map(|sl| {
                        let (shown, category) = self.shown(sl);
                        SliderOut {
                            name: &sl.name,
                            shown,
                            small: sl.small,
                            big: sl.big,
                            invert: sl.invert,
                            hidden: sl.hidden,
                            zap: sl.zap,
                            uv: sl.uv,
                            clamp: sl.clamp,
                            category,
                            toggles: &sl.zap_toggles,
                        }
                    })
                    .collect();
                let groups: Vec<&str> = self
                    .groups
                    .iter()
                    .filter(|g| g.members.contains(&s.name))
                    .map(|g| g.name.as_str())
                    .collect();
                json!({
                    "name": s.name,
                    "output": format!("{}/{}", s.output_path.replace('\\', "/"), s.output_file),
                    "outputFolder": s.output_path.replace('\\', "/"),
                    "outputFile": s.output_file,
                    "weights": s.gen_weights,
                    "shapes": s.shapes.iter().map(|sh| &sh.name).collect::<Vec<_>>(),
                    "groups": groups,
                    "sliders": sliders,
                })
            }
        }
    }

    /// The open project as the interface shows it.
    fn working(&self) -> Result<Value, String> {
        let work = self.work.as_ref().ok_or("No project is open.")?;
        let mut out = self.described(&work.set);
        out["changed"] = json!(work.changed);
        out["undo"] = json!(!work.undo.is_empty());
        out["redo"] = json!(!work.redo.is_empty());
        out["notes"] = json!(work.project.notes);
        Ok(out)
    }

    /// The open project's files first, then the held ones.
    fn file_at_work(&self, path: &str) -> Option<Vec<u8>> {
        self.work
            .as_ref()
            .and_then(|w| w.files.get(&key(path)).cloned())
            .or_else(|| self.file(path))
    }

    /// Every file a set reads: its mesh, and each slider data file.
    fn files_of(set: &set::SliderSet) -> Vec<String> {
        let mut out = vec![format!("ShapeData/{}/{}", set.data_folder, set.source_file)];
        for slider in &set.sliders {
            for d in &slider.data {
                if d.file.is_empty() {
                    continue;
                }
                let folder = if d.local {
                    set.data_folder.as_str()
                } else {
                    set.shapes
                        .iter()
                        .find(|s| s.target == d.target)
                        .and_then(|s| s.data_folder.as_deref())
                        .unwrap_or(&set.data_folder)
                };
                let path = format!("ShapeData/{folder}/{}", d.file);
                if !out.contains(&path) {
                    out.push(path);
                }
            }
        }
        out
    }

    fn preview(&self, name: &str, work: bool) -> Result<Answer, String> {
        let set = match &self.work {
            Some(w) if work => &w.set,
            _ if work => return Err("No project is open.".into()),
            _ => self.set(name)?,
        };
        let file = |path: &str| {
            if work {
                self.file_at_work(path)
            } else {
                self.file(path)
            }
        };
        let source = format!("ShapeData/{}/{}", set.data_folder, set.source_file);
        let nif = file(&source)
            .ok_or_else(|| format!("{source} is missing."))
            .and_then(|b| Nif::read(&b).map_err(|e| format!("{source}: {e}")))?;
        let blocks = mesh::shapes(&nif);
        let mut bytes: Vec<u8> = Vec::new();
        // Everything after the JSON is four-byte numbers, so an offset is a
        // count of numbers from its start.
        let put_f32 =
            |bytes: &mut Vec<u8>, values: &mut dyn Iterator<Item = f32>| -> (usize, usize) {
                let at = bytes.len() / 4;
                for v in values {
                    bytes.extend(v.to_le_bytes());
                }
                (at, bytes.len() / 4 - at)
            };
        let put_u32 =
            |bytes: &mut Vec<u8>, values: &mut dyn Iterator<Item = u32>| -> (usize, usize) {
                let at = bytes.len() / 4;
                for v in values {
                    bytes.extend(v.to_le_bytes());
                }
                (at, bytes.len() / 4 - at)
            };
        let mut shapes = Vec::new();
        let mut cache: HashMap<String, Option<osd::SliderData>> = HashMap::new();
        for shape in &set.shapes {
            // Found by its name, and only then read: reading is the cost.
            let Some(m) = blocks
                .iter()
                .find(|at| mesh::name(&nif, **at).as_deref() == Some(shape.name.as_str()))
                .and_then(|at| mesh::read(&nif, *at).ok())
            else {
                continue;
            };
            let positions = put_f32(&mut bytes, &mut m.positions.iter().flatten().copied());
            let triangles = put_u32(
                &mut bytes,
                &mut m.triangles.iter().flatten().map(|i| *i as u32),
            );
            let uvs = put_f32(&mut bytes, &mut m.uvs.iter().flatten().copied());
            let mut sliders = Vec::new();
            let mut zaps = Vec::new();
            for (index, slider) in set.sliders.iter().enumerate() {
                if slider.uv {
                    continue;
                }
                for d in slider.data.iter().filter(|d| d.target == shape.target) {
                    let folder = if d.local {
                        set.data_folder.as_str()
                    } else {
                        shape.data_folder.as_deref().unwrap_or(&set.data_folder)
                    };
                    let path = format!("ShapeData/{folder}/{}", d.file);
                    let data = cache.entry(key(&path)).or_insert_with(|| {
                        file(&path).and_then(|b| osd::SliderData::read(&b).ok())
                    });
                    let Some(list) = data.as_ref().and_then(|x| x.list(&d.list)) else {
                        continue;
                    };
                    if slider.zap {
                        // Not a move: the vertices it marks are taken out.
                        let marked = put_u32(
                            &mut bytes,
                            &mut list
                                .iter()
                                .filter(|x| apoc_bodyslide::morph::marks(x))
                                .map(|x| x.vertex as u32),
                        );
                        zaps.push(json!({ "slider": index, "vertices": marked }));
                        continue;
                    }
                    let vertices = put_u32(&mut bytes, &mut list.iter().map(|x| x.vertex as u32));
                    let offsets = put_f32(&mut bytes, &mut list.iter().flat_map(|x| x.offset));
                    sliders
                        .push(json!({ "slider": index, "vertices": vertices, "offsets": offsets }));
                }
            }
            let parts: Vec<Value> = apoc_nif::weights::parts(&nif, m.block)
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    let triangles = put_u32(
                        &mut bytes,
                        &mut p.triangles.iter().flatten().map(|i| *i as u32),
                    );
                    json!({ "part": p.part, "flags": p.flags, "triangles": triangles })
                })
                .collect();
            let places = put_f32(
                &mut bytes,
                &mut apoc_nif::weights::bone_places(&nif, m.block)
                    .unwrap_or_default()
                    .into_iter()
                    .flatten(),
            );
            // Colours as they are in the file: four bytes a vertex.
            let colors = put_u32(
                &mut bytes,
                &mut mesh::colors(&nif, m.block)
                    .unwrap_or_default()
                    .into_iter()
                    .map(u32::from_le_bytes),
            );
            shapes.push(json!({
                "colors": colors,
                "parts": parts,
                "bonePlaces": places,
                "name": m.name,
                "positions": positions,
                "triangles": triangles,
                "uvs": uvs,
                "textures": m.textures,
                "lookShared": mesh::look_shared(&nif, m.block)
                    .into_iter()
                    .filter_map(|at| mesh::name(&nif, at))
                    .collect::<Vec<_>>(),
                "look": mesh::look(&nif, m.block).map(|l| json!({
                    "glow": l.glow, "glowStrength": l.glow_strength, "alpha": l.alpha,
                    "gloss": l.gloss, "shine": l.shine, "shineStrength": l.shine_strength,
                })),
                "sliders": sliders,
                "zaps": zaps,
                "bones": mesh::bone_names(&nif, m.block),
                "reference": shape.data_folder.is_some(),
            }));
        }
        // Every bone the mesh has, followed by a shape or not.
        let bones = apoc_nif::copy::bone_names(&nif);
        Ok(Answer::with(
            json!({ "shapes": shapes, "bones": bones }),
            bytes,
        ))
    }

    /// The open project as files under the BodySlide folder, named `name`.
    /// They are held from then on as if the mods had them, and the project
    /// goes on under that name.
    fn save(&mut self, name: &str) -> Result<Answer, String> {
        let work = self.work.as_ref().ok_or("No project is open.")?;
        let (saved, files) = work.project.save(name).map_err(|e| e.to_string())?;
        // Another file's set of that name would be told apart from this one
        // by nothing, and a file of that name holding other sets would lose
        // them.
        let at = key(&files[0].path);
        for (other, origin) in self.sets.iter().zip(&self.origins) {
            if other.name == saved.name && *origin != at {
                return Err(format!(
                    "There is already a project named {}. Save under another name.",
                    saved.name
                ));
            }
            if other.name != saved.name && *origin == at {
                return Err(format!(
                    "{} holds other projects. Save under another name.",
                    files[0].path
                ));
            }
        }
        let mut bytes = Vec::new();
        let outputs: Vec<Value> = files
            .iter()
            .map(|o| {
                let start = bytes.len();
                bytes.extend(&o.bytes);
                json!({ "path": o.path, "at": start, "length": o.bytes.len() })
            })
            .collect();
        for o in files {
            self.files.insert(key(&o.path), o.bytes);
        }
        let work = self.work.as_mut().ok_or("No project is open.")?;
        work.project.set.name = saved.name.clone();
        work.changed = false;
        work.refresh()?;
        Ok(Answer::with(
            json!({ "outputs": outputs, "name": saved.name }),
            bytes,
        ))
    }

    fn build(
        &self,
        name: &str,
        small: &[f32],
        big: &[f32],
        morphs: bool,
    ) -> Result<Answer, String> {
        let set = self.set(name)?;
        let files = |path: &str| self.file(path);
        let built =
            build::build_with(set, small, big, &files, morphs).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        let outputs: Vec<Value> = built
            .outputs
            .iter()
            .chain(&built.morphs)
            .map(|o| {
                let at = bytes.len();
                bytes.extend(&o.bytes);
                json!({ "path": o.path, "at": at, "length": o.bytes.len() })
            })
            .collect();
        Ok(Answer::with(
            json!({ "outputs": outputs, "notes": built.notes }),
            bytes,
        ))
    }
}

pub fn handle(state: &mut State, request: &[u8], data: &[u8]) -> Answer {
    let request: Request = match serde_json::from_slice(request) {
        Ok(r) => r,
        Err(e) => return Answer::failed(format!("not a request: {e}")),
    };
    match request {
        Request::Put { path } => {
            state.files.insert(key(&path), data.to_vec());
            Answer::ok(json!(null))
        }
        Request::Clear => {
            *state = State::default();
            Answer::ok(json!(null))
        }
        Request::Drop { under } => {
            let under = format!("{}/", key(&under));
            state.files.retain(|path, _| !path.starts_with(&under));
            Answer::ok(json!(null))
        }
        Request::Load => Answer::ok(state.load()),
        Request::Needs { set } => match state.set(&set) {
            Ok(s) => {
                let missing: Vec<String> = State::files_of(s)
                    .into_iter()
                    .filter(|p| !state.files.contains_key(&key(p)))
                    .collect();
                Answer::ok(json!(missing))
            }
            Err(e) => Answer::failed(e),
        },
        Request::Preview { set, work } => state.preview(&set, work).unwrap_or_else(Answer::failed),
        Request::Open { set } => {
            state.work = None;
            let opened = state.set(&set).and_then(|found| {
                let files = |path: &str| state.file(path);
                Project::open(found, &files)
                    .map_err(|e| e.to_string())
                    .and_then(Work::of)
            });
            match opened {
                Ok(work) => {
                    state.work = Some(work);
                    state.working().map_or_else(Answer::failed, Answer::ok)
                }
                Err(e) => Answer::failed(e),
            }
        }
        Request::New { name, path } => {
            state.work = None;
            match Project::from_mesh(&name, &path, data)
                .map_err(|e| e.to_string())
                .and_then(Work::of)
            {
                Ok(mut work) => {
                    work.changed = true;
                    state.work = Some(work);
                    state.working().map_or_else(Answer::failed, Answer::ok)
                }
                Err(e) => Answer::failed(e),
            }
        }
        Request::BoneWeights { shape, bone } => match &state.work {
            Some(work) => match work.project.bone_weights(&shape, &bone) {
                Ok(weights) => Answer::with(
                    json!({ "vertices": weights.len() }),
                    weights.iter().flat_map(|w| w.to_le_bytes()).collect(),
                ),
                Err(e) => Answer::failed(e),
            },
            None => Answer::failed("No project is open."),
        },
        Request::Rig { shape } => match &state.work {
            Some(work) => match work.project.rig(&shape) {
                Ok((bones, vertices)) => {
                    let mut bytes = Vec::with_capacity(vertices.len() * 32);
                    for slots in &vertices {
                        let mut used = slots.iter().filter(|x| x.1 > 0.0).take(4);
                        for _ in 0..4 {
                            let (bone, weight) = used.next().copied().unwrap_or((0, 0.0));
                            bytes.extend((bone as u32).to_le_bytes());
                            bytes.extend(weight.to_le_bytes());
                        }
                    }
                    let bones: Vec<_> = bones
                        .iter()
                        .map(|(name, t)| {
                            json!({ "name": name, "turn": t.rotation, "at": t.translation, "scale": t.scale })
                        })
                        .collect();
                    Answer::with(json!({ "bones": bones, "vertices": vertices.len() }), bytes)
                }
                Err(e) => Answer::failed(e),
            },
            None => Answer::failed("No project is open."),
        },
        Request::Pack => match &state.work {
            Some(work) => match work.project.save(&work.project.set.name) {
                Ok((set, files)) => {
                    match apoc_bodyslide::pack::zip("CalienteTools/BodySlide", &files) {
                        Some(zip) => Answer::with(
                            json!({ "name": format!("{}.zip", apoc_bodyslide::project::file_name(&set.name)) }),
                            zip,
                        ),
                        None => Answer::failed("The project is too large to pack as one file."),
                    }
                }
                Err(e) => Answer::failed(e),
            },
            None => Answer::failed("No project is open."),
        },
        Request::Morphs => match &state.work {
            Some(work) => {
                let zeros = vec![0.0; work.set.sliders.len()];
                let files = |path: &str| state.file_at_work(path);
                match build::build_with(&work.set, &zeros, &zeros, &files, true) {
                    Ok(built) => match built.morphs {
                        Some(out) => Answer::with(
                            json!({ "name": format!("{}.tri", work.set.output_file) }),
                            out.bytes,
                        ),
                        None => Answer::failed("The project has no sliders to make morphs of."),
                    },
                    Err(e) => Answer::failed(e),
                }
            }
            None => Answer::failed("No project is open."),
        },
        Request::SliderData { slider } => match &state.work {
            Some(work) => match work.project.slider_data(&slider) {
                Ok(bytes) => Answer::with(json!({ "name": format!("{slider}.osd") }), bytes),
                Err(e) => Answer::failed(e),
            },
            None => Answer::failed("No project is open."),
        },
        Request::Skeleton => match Nif::read(data) {
            Ok(nif) => Answer::ok(json!(apoc_nif::copy::bone_names(&nif))),
            Err(e) => Answer::failed(e),
        },
        Request::Change { change } => {
            let (held, sets) = (&state.files, &state.sets);
            let done = match &mut state.work {
                Some(work) => work.change(
                    change,
                    data,
                    &|path: &str| held.get(&key(path)).cloned(),
                    sets,
                ),
                None => Err("No project is open.".to_string()),
            };
            match done {
                Ok(()) => state.working().map_or_else(Answer::failed, Answer::ok),
                Err(e) => Answer::failed(e),
            }
        }
        Request::Undo { again } => match &mut state.work {
            Some(work) => match work.step(!again) {
                Ok(()) => state.working().map_or_else(Answer::failed, Answer::ok),
                Err(e) => Answer::failed(e),
            },
            None => Answer::failed("No project is open."),
        },
        Request::Save { name } => state.save(&name).unwrap_or_else(Answer::failed),
        Request::Build {
            set,
            small,
            big,
            morphs,
        } => state
            .build(&set, &small, &big, morphs)
            .unwrap_or_else(Answer::failed),
        Request::Export {
            set,
            values,
            path,
            work,
        } => {
            let source = match &state.work {
                Some(w) if work => Ok(&w.set),
                _ if work => Err("No project is open.".to_string()),
                _ => state.set(&set),
            };
            let made = source.and_then(|found| {
                let path = key_keeping_case(&path);
                let path = path.strip_suffix(".nif").unwrap_or(&path);
                let (folder, file) = path.rsplit_once('/').unwrap_or(("meshes", path));
                if file.is_empty() {
                    return Err("Give the mesh a name.".to_string());
                }
                // The same set, asked for one mesh at one place.
                let mut one = found.clone();
                one.gen_weights = false;
                one.output_path = folder.to_string();
                one.output_file = file.to_string();
                let files = |path: &str| {
                    if work {
                        state.file_at_work(path)
                    } else {
                        state.file(path)
                    }
                };
                build::build_at(&one, &values, &values, &files).map_err(|e| e.to_string())
            });
            match made {
                Ok(mut built) if !built.outputs.is_empty() => {
                    let out = built.outputs.remove(0);
                    Answer::with(json!({ "path": out.path, "notes": built.notes }), out.bytes)
                }
                Ok(_) => Answer::failed("Nothing was made."),
                Err(e) => Answer::failed(e),
            }
        }
        Request::GroupAdd { group: name, set } => {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Answer::failed("Give the group a name.");
            }
            let mut groups = if data.is_empty() {
                Vec::new()
            } else {
                match group::read(data) {
                    Ok(g) => g,
                    Err(e) => return Answer::failed(e),
                }
            };
            match groups.iter_mut().find(|g| g.name == name) {
                Some(g) if !g.members.contains(&set) => g.members.push(set),
                Some(_) => {}
                None => groups.push(group::Group {
                    name,
                    members: vec![set],
                }),
            }
            Answer::ok(json!(group::write(&groups)))
        }
        Request::GroupRemove { group: name, set } => {
            let theirs = || format!("{set} is in {name} by another mod, which it stays in.");
            let Ok(mut groups) = group::read(data) else {
                return Answer::failed(theirs());
            };
            let Some(g) = groups
                .iter_mut()
                .find(|g| g.name == name && g.members.contains(&set))
            else {
                return Answer::failed(theirs());
            };
            g.members.retain(|m| *m != set);
            groups.retain(|g| !g.members.is_empty());
            Answer::ok(json!(group::write(&groups)))
        }
        Request::PresetFile { preset: p } => {
            let values = p
                .values
                .into_iter()
                .flat_map(|(name, small, big)| {
                    [
                        preset::SliderValue {
                            name: name.clone(),
                            big: true,
                            value: big,
                        },
                        preset::SliderValue {
                            name,
                            big: false,
                            value: small,
                        },
                    ]
                })
                .collect();
            Answer::ok(json!(preset::write(&[preset::Preset {
                name: p.name,
                set: p.set,
                groups: p.groups,
                values,
            }])))
        }
    }
}
