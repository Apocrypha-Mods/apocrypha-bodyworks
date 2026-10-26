//! An NPC's hair colour, skin tone and tint layers beside each shape of
//! its pre-built face: lighting kind and tint colour. How the hair tint
//! (twice the colour) was found.
mod common;
use apoc_character::{build::tint, Look, Records};
use apoc_nif::{mesh, Nif};
use apoc_plugin::FormRef;
fn main() {
    let order = common::load_order();
    let idx = common::index();
    for a in std::env::args().skip(1) {
        let id = u32::from_str_radix(&a, 16).unwrap();
        let look = Look::of_form(&order, &FormRef::new("Skyrim.esm", id)).unwrap();
        let hc = look.hair_colour.as_ref().and_then(|c| order.colour(c));
        println!(
            "{a}: hair {:?} skin {:?} tints {:?}",
            hc.map(|c| (c.editor_id, c.rgba)),
            look.skin_tone,
            look.tints.iter().take(3).collect::<Vec<_>>()
        );
        let fg = Nif::read(
            &std::fs::read(
                &idx[&format!(
                    "meshes/actors/character/facegendata/facegeom/skyrim.esm/{id:08x}.nif"
                )],
            )
            .unwrap(),
        )
        .unwrap();
        for b in mesh::shapes(&fg) {
            let l = mesh::look(&fg, b);
            println!(
                "  {} tint {:?} kind {:?}",
                mesh::name(&fg, b).unwrap(),
                tint(&fg, b),
                l.map(|l| l.kind)
            );
        }
    }
}
