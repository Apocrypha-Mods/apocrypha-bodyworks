//! Categories: how sliders are grouped on screen, and the name each is
//! shown under.

use crate::xml::{escape, flag, parse, text_of};
use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub name: String,
    /// Starts folded away.
    pub default_hidden: bool,
    /// The sliders in it: each one's name in a set, and the name to show.
    pub sliders: Vec<(String, String)>,
}

pub fn read(bytes: &[u8]) -> Result<Vec<Category>, Error> {
    let text = text_of(bytes);
    let doc = parse(&text)?;
    let root = doc.root_element();
    if !root.has_tag_name("SliderCategories") {
        return Err(Error::NotThatKind);
    }
    Ok(root
        .children()
        .filter(|c| c.has_tag_name("Category"))
        .map(|c| Category {
            name: c.attribute("name").unwrap_or_default().to_string(),
            default_hidden: flag(c.attribute("defaultHidden")),
            sliders: c
                .children()
                .filter(|s| s.has_tag_name("Slider"))
                .filter_map(|s| {
                    let name = s.attribute("name")?;
                    Some((
                        name.to_string(),
                        s.attribute("displayname").unwrap_or(name).to_string(),
                    ))
                })
                .collect(),
        })
        .collect())
}

pub fn write(categories: &[Category]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SliderCategories>\n");
    for c in categories {
        out.push_str(&format!(
            "    <Category name=\"{}\" defaultHidden=\"{}\">\n",
            escape(&c.name),
            c.default_hidden
        ));
        for (name, shown) in &c.sliders {
            out.push_str(&format!(
                "        <Slider name=\"{}\" displayname=\"{}\" />\n",
                escape(name),
                escape(shown)
            ));
        }
        out.push_str("    </Category>\n");
    }
    out.push_str("</SliderCategories>\n");
    out
}
