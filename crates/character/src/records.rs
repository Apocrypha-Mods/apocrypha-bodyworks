//! Where a build finds the records a character is made from.
//!
//! A trait rather than [`LoadOrder`] itself, so the rules can be tested on
//! records made by hand.

use apoc_plugin::records::{Armor, ArmorAddon, Colour, HeadPart, Npc, Race, TextureSet, Typed};
use apoc_plugin::{FormRef, LoadOrder};

/// The winning record of each form, read as its type. `None` when no plugin
/// holds the form, it is of another type, deleted, or does not read.
pub trait Records {
    /// The record type the winning copy of a form is (`NPC_`, `LVLN`…).
    fn kind(&self, form: &FormRef) -> Option<[u8; 4]>;
    fn npc(&self, form: &FormRef) -> Option<Npc>;
    fn race(&self, form: &FormRef) -> Option<Race>;
    fn head_part(&self, form: &FormRef) -> Option<HeadPart>;
    fn texture_set(&self, form: &FormRef) -> Option<TextureSet>;
    fn armor(&self, form: &FormRef) -> Option<Armor>;
    fn armor_addon(&self, form: &FormRef) -> Option<ArmorAddon>;
    fn colour(&self, form: &FormRef) -> Option<Colour>;
    /// What a race says that `apoc_plugin`'s [`Race`] does not read.
    fn race_extra(&self, _race: &FormRef) -> RaceExtra {
        RaceExtra::default()
    }
}

/// Fields of a `RACE` read here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RaceExtra {
    /// `NAM8`: the race whose head morphs this one uses (UESP: "Morph
    /// race"). The vampire races name their living race (NordRaceVampire,
    /// NordRace), and so do NordRaceAstrid and DLC1NordRace. `RNAM`,
    /// which [`Race::morph_race`] reads, is UESP's armour race.
    pub morph_race: Option<FormRef>,
    /// `KWDA`: its keywords (`KYWD`).
    pub keywords: Vec<FormRef>,
}

/// Skyrim.esm's `Vampire` keyword (`KYWD` 0A82BB), which every vampire race
/// has and no other race does.
pub fn vampire_keyword() -> FormRef {
    FormRef::new("Skyrim.esm", 0x0A_82BB)
}

fn typed<T: Typed>(order: &LoadOrder, form: &FormRef) -> Option<T> {
    order.get::<T>(form).and_then(Result::ok)
}

impl Records for LoadOrder {
    fn kind(&self, form: &FormRef) -> Option<[u8; 4]> {
        self.winner(form).map(|(_, r)| r.kind)
    }
    fn npc(&self, form: &FormRef) -> Option<Npc> {
        typed(self, form)
    }
    fn race(&self, form: &FormRef) -> Option<Race> {
        typed(self, form)
    }
    fn head_part(&self, form: &FormRef) -> Option<HeadPart> {
        typed(self, form)
    }
    fn texture_set(&self, form: &FormRef) -> Option<TextureSet> {
        typed(self, form)
    }
    fn armor(&self, form: &FormRef) -> Option<Armor> {
        typed(self, form)
    }
    fn armor_addon(&self, form: &FormRef) -> Option<ArmorAddon> {
        typed(self, form)
    }
    fn colour(&self, form: &FormRef) -> Option<Colour> {
        typed(self, form)
    }
    fn race_extra(&self, race: &FormRef) -> RaceExtra {
        let mut out = RaceExtra::default();
        let Some((plugin, record)) = self.winner(race) else {
            return out;
        };
        if &record.kind != b"RACE" {
            return out;
        }
        for s in record.subrecords().flatten() {
            let word = |at: usize| {
                s.data
                    .get(at..at + 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            match &s.kind {
                b"NAM8" if s.data.len() == 4 => {
                    out.morph_race = word(0).and_then(|w| plugin.names.form(w))
                }
                b"KWDA" => {
                    for at in (0..s.data.len() / 4).take(1024) {
                        if let Some(k) = word(at * 4).and_then(|w| plugin.names.form(w)) {
                            out.keywords.push(k);
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}
