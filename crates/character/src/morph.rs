//! Shaping a head part: which morphs of its tris a look turns on, and how
//! strongly.
//!
//! Found from the files (`--example fit`, `--example facegen`): solving, per
//! NPC, for the morph weights that best explain its pre-built face minus the
//! part's model gives these, and applying them gives the eyes and mouths of
//! every face exactly.
//!
//! - **Race morph** (`NAM0` 0): the morph named as the race's editor id, or
//!   its `RNAM` race's, at 1 (`NordRace`, and `NordRaceVampire` takes
//!   `NordRace`'s).
//! - **Weight** (`NAM0` 1, the part's own tri): its `SkinnyMorph` at
//!   1 − weight/100. Ulfric's weight is 70 and his hair is its model plus
//!   0.3 of it. The other morphs there are expressions, left off.
//! - **Sliders** (`NAM0` 2): each `NAM9` value turns on one of a pair of
//!   morphs, its sign picking which and its size the weight ([`SLIDERS`]).
//! - **Presets**: `NAMA` nose *n* turns on `NoseType<n>`, eyes `EyesType<n>`
//!   and lips `LipType<n>`, at 1, with no offset (Ulfric's 12, 5, 11 are
//!   `NoseType12`, `EyesType5`, `LipType11`). The second value is unused.
//! - Every part takes the morphs its own chargen tri has by those names; a
//!   name it lacks does nothing (the mouth's tri has only ten lip presets).
//!
//! Morph names are compared without case.

use std::collections::HashMap;

use apoc_bodyslide::headtri::HeadTri;
use apoc_plugin::records::PartFileKind;

use crate::look::Look;

/// The `NAM9` sliders in order: the morph a negative value turns on, then a
/// positive one. UESP gives the sliders' order and names; which morph is
/// which end was found by fitting (Ulfric's −0.82 for jaw narrow/wide is
/// `JawNarrow` at 0.82). UESP's "Eyes Forward/Back" is the other way round:
/// his −0.7 is `EyesBack` at 0.7.
pub const SLIDERS: [(&str, &str); 18] = [
    ("NoseShort", "NoseLong"),
    ("NoseDown", "NoseUp"),
    ("JawUp", "JawDown"),
    ("JawNarrow", "JawWide"),
    ("JawBack", "JawForward"),
    ("CheeksDown", "CheeksUp"),
    ("CheeksIn", "CheeksOut"),
    ("EyesMoveDown", "EyesMoveUp"),
    ("EyesMoveIn", "EyesMoveOut"),
    ("BrowDown", "BrowUp"),
    ("BrowIn", "BrowOut"),
    ("BrowBack", "BrowForward"),
    ("LipMoveDown", "LipMoveUp"),
    ("LipMoveIn", "LipMoveOut"),
    ("ChinThin", "ChinWide"),
    ("ChinMoveUp", "ChinMoveDown"),
    ("Overbite", "Underbite"),
    ("EyesBack", "EyesForward"),
];

/// The chargen morph a vampire race's faces take at 1. Found by fitting:
/// EncVampire03BossDarkElfM's face is `VampireMorph` at 0.998 and the living
/// race's (`DarkElfRace`) race morph at 1.000, and every vampire's face is
/// off by up to 3 units without them.
pub const VAMPIRE: &str = "vampiremorph";

/// The weight of each morph a look turns on, by kind of tri, names in
/// lower case.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Weights {
    pub race: HashMap<String, f32>,
    pub own: HashMap<String, f32>,
    pub chargen: HashMap<String, f32>,
}

impl Weights {
    /// `race` is the editor id of the race whose morphs the head takes.
    pub fn of(look: &Look, race: Option<&str>) -> Weights {
        let mut w = Weights::default();
        if let Some(r) = race {
            w.race.insert(r.to_ascii_lowercase(), 1.0);
        }
        let weight = if look.weight.is_finite() {
            look.weight.clamp(0.0, 100.0)
        } else {
            0.0
        };
        let skinny = 1.0 - weight / 100.0;
        if skinny != 0.0 {
            w.own.insert("skinnymorph".into(), skinny);
        }
        for ((neg, pos), v) in SLIDERS.iter().zip(&look.face_morphs) {
            if !v.is_finite() || *v == 0.0 {
                continue;
            }
            let (name, amount) = if *v < 0.0 { (neg, -v) } else { (pos, *v) };
            *w.chargen.entry(name.to_ascii_lowercase()).or_default() += amount;
        }
        for (i, prefix) in [(0usize, "nosetype"), (2, "eyestype"), (3, "liptype")] {
            if let Some(n) = look.face_parts.get(i).filter(|n| **n >= 0) {
                w.chargen.insert(format!("{prefix}{n}"), 1.0);
            }
        }
        w
    }

    fn of_kind(&self, kind: PartFileKind) -> Option<&HashMap<String, f32>> {
        match kind {
            PartFileKind::RaceMorph => Some(&self.race),
            PartFileKind::Tri => Some(&self.own),
            PartFileKind::ChargenMorph => Some(&self.chargen),
            _ => None,
        }
    }
}

/// The most morphs of one tri looked at: the game's have under 200.
pub const MAX_MORPHS: usize = 4096;

/// Add to `positions` the morphs of `tri` that `weights` turn on for a tri
/// of `kind`. A tri with another number of vertices than the shape is
/// refused, and nothing is changed. Returns the names applied.
pub fn apply(
    positions: &mut [[f32; 3]],
    tri: &HeadTri,
    kind: PartFileKind,
    weights: &Weights,
) -> Result<Vec<String>, String> {
    if tri.vertices.len() != positions.len() {
        return Err(format!(
            "{} vertices in its tri, {} in its model",
            tri.vertices.len(),
            positions.len()
        ));
    }
    let Some(on) = weights.of_kind(kind) else {
        return Ok(Vec::new());
    };
    if let Some(m) = tri
        .morphs
        .iter()
        .find(|m| m.offsets.len() != positions.len())
    {
        return Err(format!("its morph {} is cut short", m.name));
    }
    let mut applied: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for m in tri.morphs.iter().take(MAX_MORPHS) {
        let name = m.name.to_ascii_lowercase();
        // Of two morphs of one name, only the first is the name's.
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        let Some(w) = on.get(&name) else {
            continue;
        };
        let s = m.scale * w;
        if !s.is_finite() {
            continue;
        }
        for (p, o) in positions.iter_mut().zip(&m.offsets) {
            for c in 0..3 {
                p[c] += o[c] as f32 * s;
            }
        }
        applied.push(m.name.clone());
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use apoc_bodyslide::headtri::HeadMorph;

    fn tri(morphs: Vec<HeadMorph>) -> HeadTri {
        HeadTri {
            vertices: vec![[0.0; 3]; 2],
            triangles: vec![],
            uvs: vec![],
            morphs,
        }
    }

    fn morph(name: &str, x: i16) -> HeadMorph {
        HeadMorph {
            name: name.into(),
            scale: 0.5,
            offsets: vec![[x, 0, 0], [0, x, 0]],
        }
    }

    #[test]
    fn sliders_pick_by_sign_and_presets_by_number() {
        let mut look = Look {
            weight: 70.0,
            face_parts: vec![12, -1, 5, -1],
            ..Look::default()
        };
        look.face_morphs = vec![0.0; 19];
        look.face_morphs[0] = -0.3;
        look.face_morphs[3] = 0.5;
        look.face_morphs[18] = f32::MAX;
        let w = Weights::of(&look, Some("NordRace"));
        assert_eq!(w.race.get("nordrace"), Some(&1.0));
        assert!((w.own["skinnymorph"] - 0.3).abs() < 1e-6);
        assert_eq!(w.chargen.get("noseshort"), Some(&0.3));
        assert_eq!(w.chargen.get("jawwide"), Some(&0.5));
        assert_eq!(w.chargen.get("nosetype12"), Some(&1.0));
        assert_eq!(w.chargen.get("eyestype5"), Some(&1.0));
        assert!(!w.chargen.keys().any(|k| k.starts_with("liptype")));
        assert_eq!(w.chargen.len(), 4);
    }

    #[test]
    fn applied_by_name_and_kind() {
        let look = Look {
            face_morphs: vec![0.5],
            weight: 100.0,
            ..Look::default()
        };
        let w = Weights::of(&look, None);
        let t = tri(vec![morph("NoseLong", 2), morph("NOSESHORT", 4)]);
        let mut p = vec![[1.0; 3]; 2];
        let names = apply(&mut p, &t, PartFileKind::ChargenMorph, &w).unwrap();
        assert_eq!(names, vec!["NoseLong"]);
        assert_eq!(p, vec![[1.5, 1.0, 1.0], [1.0, 1.5, 1.0]]);
        // The same names in a race tri are not chargen's.
        let mut q = vec![[1.0; 3]; 2];
        apply(&mut q, &t, PartFileKind::RaceMorph, &w).unwrap();
        assert_eq!(q, vec![[1.0; 3]; 2]);
    }

    /// The pairs written out here rather than taken from [`SLIDERS`], so a
    /// pair swapped or moved there is caught.
    #[test]
    fn each_slider_turns_on_one_of_its_pair_by_its_sign() {
        let pairs = [
            ("noseshort", "noselong"),
            ("nosedown", "noseup"),
            ("jawup", "jawdown"),
            ("jawnarrow", "jawwide"),
            ("jawback", "jawforward"),
            ("cheeksdown", "cheeksup"),
            ("cheeksin", "cheeksout"),
            ("eyesmovedown", "eyesmoveup"),
            ("eyesmovein", "eyesmoveout"),
            ("browdown", "browup"),
            ("browin", "browout"),
            ("browback", "browforward"),
            ("lipmovedown", "lipmoveup"),
            ("lipmovein", "lipmoveout"),
            ("chinthin", "chinwide"),
            ("chinmoveup", "chinmovedown"),
            ("overbite", "underbite"),
            ("eyesback", "eyesforward"),
        ];
        for (i, (neg, pos)) in pairs.iter().enumerate() {
            for (v, name, amount) in [(-0.25f32, neg, 0.25f32), (0.75, pos, 0.75)] {
                let mut look = Look {
                    weight: 100.0,
                    face_morphs: vec![0.0; 19],
                    ..Look::default()
                };
                look.face_morphs[i] = v;
                let w = Weights::of(&look, None);
                assert_eq!(
                    w.chargen,
                    HashMap::from([(name.to_string(), amount)]),
                    "slider {i} at {v}"
                );
            }
        }
        // Nothing at nought, nor for a value that is not a number.
        let look = Look {
            weight: 100.0,
            face_morphs: vec![0.0, f32::NAN, f32::INFINITY],
            ..Look::default()
        };
        assert!(Weights::of(&look, None).chargen.is_empty());
    }

    #[test]
    fn presets_are_nose_eyes_and_lips_and_the_second_is_unused() {
        let look = Look {
            weight: 100.0,
            face_parts: vec![3, 7, 5, 11],
            ..Look::default()
        };
        let w = Weights::of(&look, None);
        assert_eq!(
            w.chargen,
            HashMap::from([
                ("nosetype3".to_string(), 1.0),
                ("eyestype5".to_string(), 1.0),
                ("liptype11".to_string(), 1.0),
            ])
        );
        // Each one alone, so none is taken from another's place.
        for (at, name) in [(0, "nosetype4"), (2, "eyestype4"), (3, "liptype4")] {
            let mut parts = vec![-1; 4];
            parts[at] = 4;
            let look = Look {
                weight: 100.0,
                face_parts: parts,
                ..Look::default()
            };
            let w = Weights::of(&look, None);
            assert_eq!(w.chargen.keys().collect::<Vec<_>>(), [name], "{at}");
        }
    }

    #[test]
    fn the_skinny_morph_is_what_the_weight_leaves() {
        for (weight, skinny) in [
            (0.0, Some(1.0)),
            (25.0, Some(0.75)),
            (70.0, Some(0.3)),
            (100.0, None),
            (150.0, None),
            (-20.0, Some(1.0)),
            (f32::NAN, Some(1.0)),
            (f32::INFINITY, Some(1.0)),
        ] {
            let look = Look {
                weight,
                ..Look::default()
            };
            let w = Weights::of(&look, None);
            let got = w.own.get("skinnymorph").copied();
            assert_eq!(got.is_some(), skinny.is_some(), "weight {weight}");
            if let (Some(g), Some(s)) = (got, skinny) {
                assert!((g - s).abs() < 1e-6, "weight {weight}: {g}");
            }
            assert!(w.own.len() <= 1);
        }
        // It moves the part's own tri, and no other kind.
        let look = Look {
            weight: 70.0,
            ..Look::default()
        };
        let w = Weights::of(&look, None);
        let t = tri(vec![morph("SkinnyMorph", 10)]);
        let mut p = vec![[0.0; 3]; 2];
        assert_eq!(
            apply(&mut p, &t, PartFileKind::Tri, &w).unwrap(),
            ["SkinnyMorph"]
        );
        assert!((p[0][0] - 1.5).abs() < 1e-5 && (p[1][1] - 1.5).abs() < 1e-5);
        for kind in [
            PartFileKind::RaceMorph,
            PartFileKind::ChargenMorph,
            PartFileKind::Other(7),
            PartFileKind::Unstated,
        ] {
            let mut q = vec![[0.0; 3]; 2];
            assert!(apply(&mut q, &t, kind, &w).unwrap().is_empty());
            assert_eq!(q, vec![[0.0; 3]; 2]);
        }
    }

    #[test]
    fn a_tri_of_another_kind_shapes_nothing() {
        let look = Look {
            weight: 0.0,
            face_morphs: vec![0.5],
            ..Look::default()
        };
        let w = Weights::of(&look, Some("NordRace"));
        let t = tri(vec![
            morph("NoseLong", 2),
            morph("SkinnyMorph", 2),
            morph("NordRace", 2),
        ]);
        for kind in [PartFileKind::Other(7), PartFileKind::Unstated] {
            let mut p = vec![[0.0; 3]; 2];
            assert!(apply(&mut p, &t, kind, &w).unwrap().is_empty());
            assert_eq!(p, vec![[0.0; 3]; 2]);
        }
        // Each of the three takes its own one.
        for (kind, name) in [
            (PartFileKind::RaceMorph, "NordRace"),
            (PartFileKind::Tri, "SkinnyMorph"),
            (PartFileKind::ChargenMorph, "NoseLong"),
        ] {
            let mut p = vec![[0.0; 3]; 2];
            assert_eq!(apply(&mut p, &t, kind, &w).unwrap(), [name]);
        }
    }

    #[test]
    fn the_race_morph_is_the_race_named_at_one_without_case() {
        let w = Weights::of(&Look::default(), Some("NordRace"));
        assert_eq!(w.race, HashMap::from([("nordrace".to_string(), 1.0)]));
        assert!(Weights::of(&Look::default(), None).race.is_empty());
        let t = tri(vec![morph("ImperialRace", 6), morph("NORDRACE", 4)]);
        let mut p = vec![[0.0; 3]; 2];
        assert_eq!(
            apply(&mut p, &t, PartFileKind::RaceMorph, &w).unwrap(),
            ["NORDRACE"]
        );
        assert_eq!(p, vec![[2.0, 0.0, 0.0], [0.0, 2.0, 0.0]]);
    }

    #[test]
    fn of_two_morphs_of_one_name_only_the_first_counts() {
        let look = Look {
            face_morphs: vec![0.5],
            weight: 100.0,
            ..Look::default()
        };
        let w = Weights::of(&look, None);
        let t = tri(vec![
            morph("NoseLong", 2),
            morph("noselong", 8),
            morph("NoseLong", 8),
        ]);
        let mut p = vec![[0.0; 3]; 2];
        let names = apply(&mut p, &t, PartFileKind::ChargenMorph, &w).unwrap();
        assert_eq!(names, ["NoseLong"]);
        assert_eq!(p, vec![[0.5, 0.0, 0.0], [0.0, 0.5, 0.0]]);
    }

    #[test]
    fn a_tri_of_another_size_is_refused() {
        let t = tri(vec![morph("SkinnyMorph", 1)]);
        let mut p = vec![[0.0; 3]; 3];
        assert!(apply(&mut p, &t, PartFileKind::Tri, &Weights::default()).is_err());
        let mut short = tri(vec![morph("SkinnyMorph", 1)]);
        short.morphs[0].offsets.pop();
        let mut p = vec![[0.0; 3]; 2];
        let w = Weights::of(&Look::default(), None);
        assert!(apply(&mut p, &short, PartFileKind::Tri, &w).is_err());
        assert_eq!(p, vec![[0.0; 3]; 2]);
    }
}
