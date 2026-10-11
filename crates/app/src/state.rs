//! What the module holds, and each request it answers.

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
    presets: Vec<preset::Preset>,
    groups: Vec<group::Group>,
    categories: Vec<category::Category>,
    /// Files that could not be read, each with why.
    problems: Vec<String>,
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
    /// A set's shapes and sliders, as numbers for the 3D view.
    Preview { set: String },
    /// Build a set with each slider at these values.
    Build {
        set: String,
        small: Vec<f32>,
        big: Vec<f32>,
        /// Also the morph file RaceMenu reads, and the meshes' link to it.
        #[serde(default)]
        morphs: bool,
    },
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
        self.presets.clear();
        self.groups.clear();
        self.categories.clear();
        self.problems.clear();
        let mut paths: Vec<&String> = self.files.keys().collect();
        paths.sort();
        for path in paths {
            let bytes = &self.files[path];
            let read = if path.starts_with("slidersets/") && path.ends_with(".osp") {
                set::read(bytes).map(|v| self.sets.extend(v))
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
        self.sets.sort_by_key(|s| s.name.to_lowercase());
        self.sets.dedup_by(|a, b| a.name == b.name);
        let sets: Vec<Value> = self
            .sets
            .iter()
            .map(|s| {
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
                    "weights": s.gen_weights,
                    "shapes": s.shapes.iter().map(|sh| &sh.name).collect::<Vec<_>>(),
                    "groups": groups,
                    "sliders": sliders,
                })
            })
            .collect();
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

    fn preview(&self, name: &str) -> Result<Answer, String> {
        let set = self.set(name)?;
        let source = format!("ShapeData/{}/{}", set.data_folder, set.source_file);
        let nif = self
            .file(&source)
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
            let Some(m) = blocks
                .iter()
                .find_map(|at| mesh::read(&nif, *at).ok().filter(|m| m.name == shape.name))
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
                        self.file(&path)
                            .and_then(|b| osd::SliderData::read(&b).ok())
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
            shapes.push(json!({
                "name": m.name,
                "positions": positions,
                "triangles": triangles,
                "uvs": uvs,
                "textures": m.textures,
                "sliders": sliders,
                "zaps": zaps,
                "bones": mesh::bone_names(&nif, m.block),
                "reference": shape.data_folder.is_some(),
            }));
        }
        Ok(Answer::with(json!({ "shapes": shapes }), bytes))
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
        Request::Preview { set } => state.preview(&set).unwrap_or_else(Answer::failed),
        Request::Build {
            set,
            small,
            big,
            morphs,
        } => state
            .build(&set, &small, &big, morphs)
            .unwrap_or_else(Answer::failed),
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
