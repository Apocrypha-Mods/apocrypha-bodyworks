//! Each Skyrim.esm NPC with a baked face tint texture, as JSON lines: its
//! race's tint masks for its sex, its tint layers as its record lists them,
//! and the baked texture's path. A scratch script lays the masks on and
//! measures the result against the baked texture: how the layers are laid
//! together was found that way.
//!
//! `cargo run --release -p apoc-character --example tints > tints.jsonl`
mod common;
use apoc_character::{look::face_source, Records};
use apoc_plugin::FormRef;

fn main() {
    let order = common::load_order();
    let idx = common::index();
    let prefix = "textures/actors/character/facegendata/facetint/skyrim.esm/";
    let mut baked: Vec<_> = idx
        .iter()
        .filter_map(|(k, p)| {
            let id = k.strip_prefix(prefix)?.strip_suffix(".dds")?;
            Some((u32::from_str_radix(id, 16).ok()?, p.clone()))
        })
        .collect();
    baked.sort();
    for (id, path) in baked {
        // The face comes from the template where the flags say so, as in
        // `Look::of_form`.
        let Ok(npc) = face_source(&order, &FormRef::new("Skyrim.esm", id)) else {
            continue;
        };
        let Some(race) = npc.race.as_ref().and_then(|r| order.race(r)) else {
            continue;
        };
        let sex = &race.sexes[usize::from(npc.female())];
        let masks: Vec<String> = sex
            .head
            .tints
            .iter()
            .map(|m| {
                format!(
                    "{{\"index\":{},\"kind\":{},\"mask\":{}}}",
                    m.index,
                    m.kind.map_or(-1, i32::from),
                    json(m.mask.as_deref().unwrap_or(""))
                )
            })
            .collect();
        let layers: Vec<String> = npc
            .tints
            .iter()
            .map(|t| {
                format!(
                    "{{\"index\":{},\"rgba\":{:?},\"value\":{}}}",
                    t.index, t.rgba, t.value
                )
            })
            .collect();
        println!(
            "{{\"id\":\"{id:08x}\",\"race\":{},\"female\":{},\"skin\":{:?},\"masks\":[{}],\"layers\":[{}],\"baked\":{}}}",
            json(race.editor_id.as_deref().unwrap_or("")),
            npc.female(),
            npc.skin_tone.map(|c| c.map(|v| (v * 255.0).round() as u8)).unwrap_or([255; 3]),
            masks.join(","),
            layers.join(","),
            json(&path.to_string_lossy())
        );
    }
}

fn json(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
