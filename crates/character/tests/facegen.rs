//! A handful of Skyrim.esm's NPCs built from their records and the game's
//! base files and held to the game's own pre-built faces, shape by shape,
//! as `--example facegen` does for all 2,550. Needs the game's plugins
//! (`SKYRIM_DATA`, or Steam's folder) and the unpacked archives in
//! `scratch/samples`; skipped without them.
//!
//! The bounds are the README's: eyes and mouths as the game's to about a
//! millionth (1e-5), every other part within a few hundredths of a unit
//! (held to 0.1; over all 2,550 faces, 31 heads and 26 beards are further
//! off than that, none of them among these).

use std::path::{Path, PathBuf};

use apoc_character::build::{tint, Options};
use apoc_character::{build, plan, Files, Look, Records};
use apoc_nif::{mesh, Nif};
use apoc_plugin::native::read_file;
use apoc_plugin::records::HeadPartKind;
use apoc_plugin::{FormRef, LoadOrder};

const WANTED: [[u8; 4]; 9] = [
    *b"RACE", *b"HDPT", *b"TXST", *b"ARMO", *b"ARMA", *b"CLFM", *b"NPC_", *b"LVLN", *b"KYWD",
];

const ORDER: [&str; 6] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
    "unofficial skyrim special edition patch.esp",
];

const FACEGEN: &str = "meshes/actors/character/facegendata/facegeom/skyrim.esm/";

/// Spread across the folder of pre-built faces, men and women of several
/// races; not chosen by how near they come.
const NPCS: [u32; 8] = [
    0x0001_414D,
    0x0001_3366,
    0x0001_A670,
    0x0003_9D48,
    0x0004_5CCD,
    0x0006_D261,
    0x000C_E084,
    0x0010_91BE,
];

fn data_folder() -> PathBuf {
    std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(format!(
                "{}/.local/share/Steam/steamapps/common/Skyrim Special Edition/Data",
                std::env::var("HOME").unwrap_or_default()
            ))
        })
}

/// A game file by key: USSEP's over the game's.
fn game_file(key: &str) -> Option<Vec<u8>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples");
    ["skyrim-ussep", "skyrim"]
        .iter()
        .find_map(|r| std::fs::read(root.join(r).join(key)).ok())
}

/// A texture path as the game finds it.
fn texture(p: &str) -> String {
    let k = apoc_character::files::key(p);
    k.strip_prefix("textures/").map(str::to_string).unwrap_or(k)
}

#[test]
fn a_handful_of_npcs_come_out_as_the_games_own_faces() {
    let data = data_folder();
    if !ORDER.iter().all(|p| data.join(p).is_file()) {
        return;
    }
    if game_file(&format!("{FACEGEN}{:08x}.nif", NPCS[0])).is_none() {
        return;
    }
    let order = LoadOrder::new(
        ORDER
            .iter()
            .map(|n| read_file(&data.join(n), &WANTED).expect(n).0)
            .collect(),
    );
    let mut files = Files::new();
    let mut shapes = 0;
    for id in NPCS {
        let theirs = Nif::read(&game_file(&format!("{FACEGEN}{id:08x}.nif")).unwrap()).unwrap();
        let form = FormRef::new("Skyrim.esm", id);
        let look = Look::of_form(&order, &form).unwrap();
        let mut p = plan(&look, &order).unwrap();
        // The pre-built faces hold the head only.
        p.body.clear();
        for k in p.files() {
            if let Some(b) = game_file(&k) {
                files.insert(&k, b);
            }
        }
        let tint_path = format!(
            "data\\Textures\\Actors\\Character\\FaceGenData\\FaceTint\\Skyrim.esm\\{id:08X}.dds"
        );
        let options = Options {
            face_tint: Some(tint_path),
            keep_head_space: true,
        };
        let built = build(&p, &files, &options).unwrap();
        assert!(built.missing.is_empty(), "{id:08X}: {:?}", built.missing);
        assert!(order.npc(&form).is_some());
        let theirs_shapes = mesh::shapes(&theirs);
        assert_eq!(built.shapes.len(), theirs_shapes.len(), "{id:08X}");
        for tb in theirs_shapes {
            let t = mesh::read(&theirs, tb).unwrap();
            let s = built
                .shapes
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&t.name))
                .unwrap_or_else(|| panic!("{id:08X}: no {}", t.name));
            let ours = mesh::read(&built.nif, s.block).unwrap();
            let piece = p
                .head
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case(&t.name))
                .unwrap();
            let kind = piece.kind;
            let what = format!("{id:08X} {} ({kind:?})", t.name);
            assert_eq!(ours.positions.len(), t.positions.len(), "{what}");
            let off = ours
                .positions
                .iter()
                .zip(&t.positions)
                .flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs()))
                .fold(0.0f32, f32::max);
            let bound = match kind {
                Some(HeadPartKind::Eyes) => 1e-5,
                _ if piece.model.contains("/mouth/") => 1e-5,
                // "A few hundredths": a beard's is up to 0.085 among these.
                _ => 0.1,
            };
            assert!(off <= bound, "{what}: off by {off}, held to {bound}");
            assert_eq!(ours.triangles, t.triangles, "{what}");
            let mine: Vec<String> = ours.textures.iter().map(|x| texture(x)).collect();
            let game: Vec<String> = t.textures.iter().map(|x| texture(x)).collect();
            // A few scars of the game's faces (18 of 634, `--example
            // facegen`) show the other side's gash than their record names;
            // which the game draws is not known, so scars' textures are not held.
            if kind != Some(HeadPartKind::Scar) {
                assert_eq!(mine, game, "{what}");
            }
            match (tint(&built.nif, s.block), tint(&theirs, tb)) {
                (Some((k1, a)), Some((k2, b))) => {
                    assert_eq!(k1, k2, "{what}");
                    assert!(
                        (0..3).all(|c| (a[c] - b[c]).abs() <= 1e-3),
                        "{what}: {a:?} {b:?}"
                    );
                }
                (a, b) => assert_eq!(a.map(|x| x.0), b.map(|x| x.0), "{what}"),
            }
            shapes += 1;
        }
    }
    assert!(shapes >= NPCS.len() * 4, "{shapes}");
}
