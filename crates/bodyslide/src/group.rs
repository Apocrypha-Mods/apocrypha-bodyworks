//! Groups: named lists of sets, so a preset made for one body is offered for
//! every outfit made for that body.

use crate::xml::{escape, parse, text_of};
use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    /// The sets in it, by name.
    pub members: Vec<String>,
}

pub fn read(bytes: &[u8]) -> Result<Vec<Group>, Error> {
    let text = text_of(bytes);
    let doc = parse(&text)?;
    let root = doc.root_element();
    if !root.has_tag_name("SliderGroups") {
        return Err(Error::NotThatKind);
    }
    Ok(root
        .children()
        .filter(|c| c.has_tag_name("Group"))
        .map(|g| Group {
            name: g.attribute("name").unwrap_or_default().to_string(),
            members: g
                .children()
                .filter(|c| c.has_tag_name("Member"))
                .filter_map(|m| m.attribute("name"))
                .map(str::to_string)
                .collect(),
        })
        .collect())
}

pub fn write(groups: &[Group]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SliderGroups>\n");
    for g in groups {
        out.push_str(&format!("    <Group name=\"{}\">\n", escape(&g.name)));
        for m in &g.members {
            out.push_str(&format!("        <Member name=\"{}\"/>\n", escape(m)));
        }
        out.push_str("    </Group>\n");
    }
    out.push_str("</SliderGroups>\n");
    out
}
