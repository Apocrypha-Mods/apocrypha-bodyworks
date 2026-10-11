//! Slider set projects (`.osp`): a mesh, where the built mesh goes, and the
//! sliders that shape it.

use crate::xml::{escape, flag, parse, text_of};
use crate::Error;

/// Where one slider's list for one shape is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataRef {
    /// The list's own name.
    pub name: String,
    /// The shape it moves, by the shape's `target`.
    pub target: String,
    /// The file is in the set's own data folder.
    pub local: bool,
    /// The slider data file, as written (`Body.osd`).
    pub file: String,
    /// The list's name inside that file.
    pub list: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slider {
    pub name: String,
    /// The list moves the mesh as the slider goes down, not up.
    pub invert: bool,
    /// Where the slider stands on the lightest and the heaviest body when a
    /// preset says nothing, in percent.
    pub small: f32,
    pub big: f32,
    pub hidden: bool,
    /// Removes the vertices its lists name instead of moving them.
    pub zap: bool,
    /// Moves texture coordinates, not positions.
    pub uv: bool,
    /// Never applied beyond its range.
    pub clamp: bool,
    /// Other zap sliders this one switches when it is switched.
    pub zap_toggles: Vec<String>,
    pub data: Vec<DataRef>,
}

/// A shape of the set's mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    /// The shape's name in the mesh.
    pub name: String,
    /// What sliders call it.
    pub target: String,
    /// Its data folder, when not the set's.
    pub data_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SliderSet {
    pub name: String,
    /// Under `ShapeData`.
    pub data_folder: String,
    /// The mesh, in the data folder.
    pub source_file: String,
    /// Where the built mesh goes, under the game's data folder.
    pub output_path: String,
    pub output_file: String,
    /// Build a lightest and a heaviest mesh (`_0`, `_1`), not one.
    pub gen_weights: bool,
    pub shapes: Vec<Shape>,
    pub sliders: Vec<Slider>,
}

fn child_text(node: roxmltree::Node, name: &str) -> String {
    node.children()
        .find(|c| c.has_tag_name(name))
        .and_then(|c| c.text())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn number(value: Option<&str>, otherwise: f32) -> f32 {
    value
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(otherwise)
}

/// Every set in a project file.
pub fn read(bytes: &[u8]) -> Result<Vec<SliderSet>, Error> {
    let text = text_of(bytes);
    let doc = parse(&text)?;
    let root = doc.root_element();
    if !root.has_tag_name("SliderSetInfo") {
        return Err(Error::NotThatKind);
    }
    let mut sets = Vec::new();
    for set in root.children().filter(|c| c.has_tag_name("SliderSet")) {
        let output = set.children().find(|c| c.has_tag_name("OutputFile"));
        let shapes = set
            .children()
            .filter(|c| c.has_tag_name("Shape"))
            .map(|s| {
                let name = s.text().unwrap_or_default().trim().to_string();
                Shape {
                    target: s.attribute("target").unwrap_or(&name).to_string(),
                    name,
                    data_folder: s.attribute("DataFolder").map(str::to_string),
                }
            })
            .collect();
        let sliders = set
            .children()
            .filter(|c| c.has_tag_name("Slider"))
            .map(|s| {
                // One default for both, when the two are not given.
                let default = s.attribute("default");
                let data = s
                    .children()
                    .filter(|c| c.has_tag_name("Data"))
                    .map(|d| {
                        let text = d.text().unwrap_or_default().trim();
                        // `File.osd\ListName`; an older set names the list alone.
                        let (file, list) = match text.split_once(['\\', '/']) {
                            Some((file, list)) => (file.to_string(), list.to_string()),
                            None => (String::new(), text.to_string()),
                        };
                        DataRef {
                            name: d.attribute("name").unwrap_or_default().to_string(),
                            target: d.attribute("target").unwrap_or_default().to_string(),
                            local: flag(d.attribute("local")),
                            file,
                            list,
                        }
                    })
                    .collect();
                Slider {
                    name: s.attribute("name").unwrap_or_default().to_string(),
                    invert: flag(s.attribute("invert")),
                    small: number(s.attribute("small").or(default), 0.0),
                    big: number(s.attribute("big").or(default), 0.0),
                    hidden: flag(s.attribute("hidden")),
                    zap: flag(s.attribute("zap")),
                    uv: flag(s.attribute("uv")),
                    clamp: flag(s.attribute("clamp")),
                    zap_toggles: s
                        .attribute("zaptoggles")
                        .unwrap_or_default()
                        .split(';')
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                        .map(str::to_string)
                        .collect(),
                    data,
                }
            })
            .collect();
        sets.push(SliderSet {
            name: set.attribute("name").unwrap_or_default().to_string(),
            data_folder: child_text(set, "DataFolder"),
            source_file: child_text(set, "SourceFile"),
            output_path: child_text(set, "OutputPath"),
            output_file: output
                .and_then(|o| o.text())
                .unwrap_or_default()
                .trim()
                .to_string(),
            gen_weights: output.is_none_or(|o| o.attribute("GenWeights").is_none_or(|v| flag(Some(v)))),
            shapes,
            sliders,
        });
    }
    Ok(sets)
}

fn percent(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A project file holding `sets`.
pub fn write(sets: &[SliderSet]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SliderSetInfo version=\"1\">\n");
    for set in sets {
        out.push_str(&format!("    <SliderSet name=\"{}\">\n", escape(&set.name)));
        out.push_str(&format!("        <DataFolder>{}</DataFolder>\n", escape(&set.data_folder)));
        out.push_str(&format!("        <SourceFile>{}</SourceFile>\n", escape(&set.source_file)));
        out.push_str(&format!("        <OutputPath>{}</OutputPath>\n", escape(&set.output_path)));
        out.push_str(&format!(
            "        <OutputFile GenWeights=\"{}\">{}</OutputFile>\n",
            set.gen_weights,
            escape(&set.output_file)
        ));
        for shape in &set.shapes {
            let folder = shape
                .data_folder
                .as_ref()
                .map(|f| format!(" DataFolder=\"{}\"", escape(f)))
                .unwrap_or_default();
            out.push_str(&format!(
                "        <Shape target=\"{}\"{folder}>{}</Shape>\n",
                escape(&shape.target),
                escape(&shape.name)
            ));
        }
        for s in &set.sliders {
            let mut attrs = format!(
                "name=\"{}\" invert=\"{}\" small=\"{}\" big=\"{}\"",
                escape(&s.name),
                s.invert,
                percent(s.small),
                percent(s.big)
            );
            for (name, on) in [("hidden", s.hidden), ("zap", s.zap), ("uv", s.uv), ("clamp", s.clamp)] {
                if on {
                    attrs.push_str(&format!(" {name}=\"true\""));
                }
            }
            if !s.zap_toggles.is_empty() {
                attrs.push_str(&format!(" zaptoggles=\"{};\"", escape(&s.zap_toggles.join(";"))));
            }
            out.push_str(&format!("        <Slider {attrs}>\n"));
            for d in &s.data {
                let local = if d.local { " local=\"true\"" } else { "" };
                let at = if d.file.is_empty() {
                    escape(&d.list)
                } else {
                    format!("{}\\{}", escape(&d.file), escape(&d.list))
                };
                out.push_str(&format!(
                    "            <Data name=\"{}\" target=\"{}\"{local}>{at}</Data>\n",
                    escape(&d.name),
                    escape(&d.target)
                ));
            }
            out.push_str("        </Slider>\n");
        }
        out.push_str("    </SliderSet>\n");
    }
    out.push_str("</SliderSetInfo>\n");
    out
}
