//! Presets: a named body, as a value for each slider on the lightest and on
//! the heaviest body.

use crate::xml::{escape, parse, text_of};
use crate::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct SliderValue {
    pub name: String,
    /// The value for the heaviest body (`big`) when true, the lightest
    /// (`small`) when false.
    pub big: bool,
    /// In percent.
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    /// The set it was made on.
    pub set: String,
    /// The groups of sets it is offered for.
    pub groups: Vec<String>,
    pub values: Vec<SliderValue>,
}

impl Preset {
    /// A slider's value for the lightest or the heaviest body, when the
    /// preset gives one.
    pub fn value(&self, slider: &str, big: bool) -> Option<f32> {
        self.values
            .iter()
            .rev()
            .find(|v| v.big == big && v.name == slider)
            .map(|v| v.value)
    }
}

/// Every preset in a file.
pub fn read(bytes: &[u8]) -> Result<Vec<Preset>, Error> {
    let text = text_of(bytes);
    let doc = parse(&text)?;
    let root = doc.root_element();
    if !root.has_tag_name("SliderPresets") {
        return Err(Error::NotThatKind);
    }
    Ok(root
        .children()
        .filter(|c| c.has_tag_name("Preset"))
        .map(|p| Preset {
            name: p.attribute("name").unwrap_or_default().to_string(),
            set: p.attribute("set").unwrap_or_default().to_string(),
            groups: p
                .children()
                .filter(|c| c.has_tag_name("Group"))
                .filter_map(|g| g.attribute("name"))
                .map(str::to_string)
                .collect(),
            values: p
                .children()
                .filter(|c| c.has_tag_name("SetSlider"))
                .filter_map(|s| {
                    Some(SliderValue {
                        name: s.attribute("name")?.to_string(),
                        big: s
                            .attribute("size")
                            .is_none_or(|v| v.eq_ignore_ascii_case("big")),
                        value: s
                            .attribute("value")?
                            .trim()
                            .parse::<f32>()
                            .ok()
                            .filter(|v| v.is_finite())?,
                    })
                })
                .collect(),
        })
        .collect())
}

/// A presets file holding `presets`.
pub fn write(presets: &[Preset]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SliderPresets>\n");
    for p in presets {
        out.push_str(&format!(
            "    <Preset name=\"{}\" set=\"{}\">\n",
            escape(&p.name),
            escape(&p.set)
        ));
        for g in &p.groups {
            out.push_str(&format!("        <Group name=\"{}\"/>\n", escape(g)));
        }
        for v in &p.values {
            let value = if v.value.fract() == 0.0 {
                format!("{}", v.value as i64)
            } else {
                format!("{}", v.value)
            };
            out.push_str(&format!(
                "        <SetSlider name=\"{}\" size=\"{}\" value=\"{value}\"/>\n",
                escape(&v.name),
                if v.big { "big" } else { "small" }
            ));
        }
        out.push_str("    </Preset>\n");
    }
    out.push_str("</SliderPresets>\n");
    out
}
