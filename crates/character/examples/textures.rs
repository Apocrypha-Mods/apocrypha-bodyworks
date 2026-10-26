//! An NPC's texture sets: each head part's `TNAM`, its face texture set,
//! its skin's, with each set's flags, beside its pre-built face's slots.
mod common;
use apoc_character::{Look, Records};
use apoc_nif::{mesh, Nif};
use apoc_plugin::FormRef;
fn main() {
    let order = common::load_order();
    let idx = common::index();
    for a in std::env::args().skip(1) {
        let id = u32::from_str_radix(&a, 16).unwrap();
        let f = FormRef::new("Skyrim.esm", id);
        let look = Look::of_form(&order, &f).unwrap();
        let show = |t: &Option<FormRef>| {
            t.as_ref()
                .and_then(|t| order.texture_set(t))
                .map(|t| format!("{:?} flags {:x} {:?}", t.editor_id, t.flags, t.textures))
        };
        println!(
            "{a}: FTST {:?}\n  skin {:?}",
            show(&look.face_texture),
            look.skin
                .as_ref()
                .and_then(|s| order.armor(s))
                .map(|a| a.editor_id)
        );
        let race = order.race(look.race.as_ref().unwrap()).unwrap();
        println!(
            "  race default face {:?}",
            show(&race.sexes[look.female as usize].head.default_face_texture)
        );
        for p in apoc_character::parts::head_parts(&order, &race, look.female, &look.head_parts) {
            let h = order.head_part(&p).unwrap();
            println!("  {:?}: TNAM {:?}", h.editor_id, show(&h.texture_set));
        }
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
            let m = mesh::read(&fg, b).unwrap();
            println!("  fg {}: {:?}", m.name, m.textures);
        }
    }
}
