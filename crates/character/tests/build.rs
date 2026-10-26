//! Builds from the game's own meshes, when they are unpacked in
//! `scratch/samples/skyrim` (skipped when not): the body blended at its
//! weight, head parts shaped by their tris, textures and tints, files that
//! are not there, and where each shape is placed.

use std::collections::HashMap;
use std::path::PathBuf;

use apoc_bodyslide::headtri::{self, HeadMorph, HeadTri};
use apoc_character::build::{tint, Built, Options};
use apoc_character::morph::Weights;
use apoc_character::place::bone_places;
use apoc_character::plan::{BodyPiece, HeadPiece, Plan};
use apoc_character::{build, Files};
use apoc_nif::copy::composed;
use apoc_nif::geometry::TriShape;
use apoc_nif::mesh::{self, Mesh};
use apoc_nif::node::Transform;
use apoc_nif::skin::{SkinData, SkinInstance};
use apoc_nif::Nif;
use apoc_plugin::records::{HeadPartKind, PartFileKind};

const ASSETS: &str = "meshes/actors/character/character assets/";
const LIGHT: &str = "meshes/actors/character/character assets/malebody_0.nif";
const HEAVY: &str = "meshes/actors/character/character assets/malebody_1.nif";
const HEAD: &str = "meshes/actors/character/character assets/malehead.nif";
const HAIR: &str = "meshes/actors/character/character assets/hair/femaledarkelfhair01.nif";
const EYES: &str = "meshes/actors/character/character assets/eyesmale.nif";
const MOUTH: &str = "meshes/actors/character/character assets/mouth/mouthhuman.nif";
const HANDS: &str = "meshes/actors/character/character assets/malehands_1.nif";
const SKELETON: &str = "meshes/actors/character/character assets/skeleton.nif";
const HEAD_BONE: &str = "NPC Head [Head]";

fn root() -> Option<PathBuf> {
    let r = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/skyrim");
    r.join(ASSETS).is_dir().then_some(r)
}

fn bytes(root: &std::path::Path, key: &str) -> Vec<u8> {
    std::fs::read(root.join(key)).unwrap_or_else(|e| panic!("{key}: {e}"))
}

fn files(root: &std::path::Path, keys: &[&str]) -> Files {
    let mut f = Files::new();
    for k in keys {
        f.insert(k, bytes(root, k));
    }
    f
}

fn nif(root: &std::path::Path, key: &str) -> Nif {
    Nif::read(&bytes(root, key)).unwrap()
}

/// Every shape of a file by name.
fn shapes_of(n: &Nif) -> HashMap<String, Mesh> {
    mesh::shapes(n)
        .into_iter()
        .map(|b| {
            let m = mesh::read(n, b).unwrap();
            (m.name.clone(), m)
        })
        .collect()
}

fn built_shape(built: &Built, name: &str) -> (usize, Mesh) {
    let s = built
        .shapes
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no shape {name} in {:?}", built.shapes));
    (s.block, mesh::read(&built.nif, s.block).unwrap())
}

fn far(a: &[[f32; 3]], b: &[[f32; 3]]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .flat_map(|(p, q)| (0..3).map(move |c| (p[c] - q[c]).abs()))
        .fold(0.0, f32::max)
}

fn head(name: &str, kind: HeadPartKind, model: &str) -> HeadPiece {
    HeadPiece {
        name: name.into(),
        kind: Some(kind),
        model: model.into(),
        tris: Vec::new(),
        textures: Default::default(),
    }
}

fn body(model: &str, weights: Option<(&str, &str)>) -> BodyPiece {
    BodyPiece {
        name: "Body".into(),
        model: model.into(),
        weights: weights.map(|(l, h)| (l.to_string(), h.to_string())),
        textures: Default::default(),
    }
}

const STILL: Options = Options {
    face_tint: None,
    keep_head_space: true,
};

#[test]
fn a_body_is_blended_at_its_weight_and_only_its_skin_textured_and_toned() {
    let Some(root) = root() else { return };
    let mut piece = body(HEAVY, Some((LIGHT, HEAVY)));
    piece.textures[0] = Some("Actors\\Mine\\Body.dds".into());
    piece.textures[7] = Some("textures\\Mine\\Spec.dds".into());
    let tone = [0.5, 0.25, 0.125];
    let plan = Plan {
        body: vec![piece],
        weight: 0.25,
        skin_tone: Some(tone),
        ..Plan::default()
    };
    let built = build(&plan, &files(&root, &[LIGHT, HEAVY]), &STILL).unwrap();
    assert!(built.missing.is_empty(), "{:?}", built.missing);
    let (light, heavy) = (shapes_of(&nif(&root, LIGHT)), shapes_of(&nif(&root, HEAVY)));
    assert_eq!(built.shapes.len(), heavy.len());
    let mut differ = 0.0f32;
    for (name, h) in &heavy {
        let l = &light[name];
        let (_, ours) = built_shape(&built, name);
        let blend: Vec<[f32; 3]> = l
            .positions
            .iter()
            .zip(&h.positions)
            .map(|(a, b)| [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * 0.25))
            .collect();
        assert!(far(&ours.positions, &blend) < 1e-4, "{name}");
        differ = differ.max(far(&l.positions, &h.positions));
    }
    // The two bodies differ, so the blend says something.
    assert!(differ > 0.5, "{differ}");

    let skin = "MaleUnderwearBody:0";
    let (block, ours) = built_shape(&built, skin);
    assert_eq!(ours.textures[0], "textures\\Actors\\Mine\\Body.dds");
    assert_eq!(ours.textures[1], heavy[skin].textures[1]);
    assert_eq!(ours.textures[7], "textures\\Mine\\Spec.dds");
    assert_eq!(tint(&built.nif, block), Some((5, tone)));
    // The underwear is not skin: its own textures, and no tone.
    let (block, ours) = built_shape(&built, "MaleUnderwear_1");
    assert_eq!(ours.textures, heavy["MaleUnderwear_1"].textures);
    assert_eq!(tint(&built.nif, block), None);
    assert!(built.shapes.iter().all(|s| !s.head && s.piece == "Body"));
}

#[test]
fn a_body_with_one_of_its_pair_is_that_one_and_with_neither_is_left_out() {
    let Some(root) = root() else { return };
    for (have, missing) in [(HEAVY, LIGHT), (LIGHT, HEAVY)] {
        let plan = Plan {
            body: vec![body(HEAVY, Some((LIGHT, HEAVY)))],
            weight: 0.5,
            ..Plan::default()
        };
        let built = build(&plan, &files(&root, &[have]), &STILL).unwrap();
        assert_eq!(built.missing, [missing]);
        for (name, m) in shapes_of(&nif(&root, have)) {
            assert_eq!(
                built_shape(&built, &name).1.positions,
                m.positions,
                "{name}"
            );
        }
    }
    // Neither: the body is left out and named, the head still built.
    let plan = Plan {
        body: vec![body(HEAVY, Some((LIGHT, HEAVY)))],
        head: vec![head("Hair", HeadPartKind::Hair, HAIR)],
        ..Plan::default()
    };
    let built = build(&plan, &files(&root, &[HAIR]), &STILL).unwrap();
    let mut missing = built.missing.clone();
    missing.sort();
    assert_eq!(missing, [LIGHT, HEAVY]);
    assert_eq!(built.shapes.len(), 1);
    assert!(built.shapes[0].head);
}

/// A chargen tri for `model`'s shape: `NoseLong` and `JawWide`, each
/// moving every vertex.
fn tri_for(m: &Mesh) -> (Vec<u8>, Vec<[i16; 3]>) {
    let n = m.positions.len();
    let offsets: Vec<[i16; 3]> = (0..n as i16).map(|v| [v % 7, -(v % 5), 3]).collect();
    let tri = HeadTri {
        vertices: m.positions.clone(),
        triangles: Vec::new(),
        uvs: vec![[0.0; 2]; n],
        morphs: vec![
            HeadMorph {
                name: "NoseLong".into(),
                scale: 0.01,
                offsets: offsets.clone(),
            },
            HeadMorph {
                name: "JawWide".into(),
                scale: 1.0,
                offsets: vec![[100, 100, 100]; n],
            },
        ],
    };
    (headtri::write(&tri), offsets)
}

#[test]
fn head_parts_are_shaped_by_their_tris_named_textured_and_tinted() {
    let Some(root) = root() else { return };
    let face_model = shapes_of(&nif(&root, HEAD)).into_values().next().unwrap();
    let (tri, offsets) = tri_for(&face_model);
    let mut face = head("MyFace", HeadPartKind::Face, HEAD);
    face.tris = vec![(PartFileKind::ChargenMorph, "meshes/test/face.tri".into())];
    face.textures[0] = Some("Actors\\Mine\\Face.dds".into());
    let mut weights = Weights::default();
    weights.chargen.insert("noselong".into(), 0.5);
    let (hair_colour, tone) = ([0.1, 0.2, 0.3], [0.9, 0.8, 0.7]);
    let plan = Plan {
        head: vec![face, head("MyHair", HeadPartKind::Hair, HAIR)],
        weights,
        hair_colour: Some(hair_colour),
        skin_tone: Some(tone),
        ..Plan::default()
    };
    let mut f = files(&root, &[HEAD, HAIR]);
    f.insert("Meshes\\Test\\Face.tri", tri);
    let options = Options {
        face_tint: Some("data\\Textures\\FaceTint\\Mine.dds".into()),
        keep_head_space: true,
    };
    let built = build(&plan, &f, &options).unwrap();
    assert!(
        built.missing.is_empty() && built.notes.is_empty(),
        "{built:?}"
    );

    let (block, ours) = built_shape(&built, "MyFace");
    let shaped: Vec<[f32; 3]> = face_model
        .positions
        .iter()
        .zip(&offsets)
        .map(|(p, o)| [0, 1, 2].map(|c| p[c] + o[c] as f32 * 0.01 * 0.5))
        .collect();
    assert!(far(&ours.positions, &shaped) < 1e-5);
    assert!(far(&face_model.positions, &shaped) > 0.02);
    assert_eq!(ours.textures[0], "textures\\Actors\\Mine\\Face.dds");
    assert_eq!(ours.textures[1], face_model.textures[1]);
    // The face tint goes in the face's seventh slot, as it is given.
    assert_eq!(ours.textures[6], "data\\Textures\\FaceTint\\Mine.dds");
    assert_eq!(tint(&built.nif, block), None);

    let hair_model = shapes_of(&nif(&root, HAIR)).into_values().next().unwrap();
    let (block, hair) = built_shape(&built, "MyHair");
    assert_eq!(hair.positions, hair_model.positions);
    assert_eq!(hair.textures, hair_model.textures);
    assert_eq!(tint(&built.nif, block), Some((6, hair_colour)));
    // Each shape says which piece it came from, and that it is the head's.
    assert!(built.shapes.iter().all(|s| s.head && s.piece == s.name));
}

#[test]
fn a_skin_shape_of_the_head_takes_the_tone_and_a_part_without_a_shader_kind_does_not() {
    let Some(root) = root() else { return };
    // The hands are skin (kind 5); put them on the head to see the head's
    // pieces take the tone too.
    let tone = [0.3, 0.6, 0.9];
    let plan = Plan {
        head: vec![
            head("Hands", HeadPartKind::Misc, HANDS),
            head("Eyes", HeadPartKind::Eyes, EYES),
        ],
        hair_colour: Some([1.0, 0.0, 0.0]),
        skin_tone: Some(tone),
        ..Plan::default()
    };
    let built = build(&plan, &files(&root, &[HANDS, EYES]), &STILL).unwrap();
    let (block, _) = built_shape(&built, "Hands");
    assert_eq!(tint(&built.nif, block), Some((5, tone)));
    let (block, _) = built_shape(&built, "Eyes");
    assert_eq!(tint(&built.nif, block), None);
}

#[test]
fn a_part_whose_files_are_not_there_or_wrong_is_left_out_and_said_why() {
    let Some(root) = root() else { return };
    let eyes_model = shapes_of(&nif(&root, EYES)).into_values().next().unwrap();
    let face_model = shapes_of(&nif(&root, HEAD)).into_values().next().unwrap();
    let mut face = head("Face", HeadPartKind::Face, HEAD);
    face.tris = vec![(PartFileKind::ChargenMorph, "meshes/test/gone.tri".into())];
    let mut eyes = head("Eyes", HeadPartKind::Eyes, EYES);
    eyes.tris = vec![(PartFileKind::ChargenMorph, "meshes/test/bad.tri".into())];
    let mut mouth = head("Mouth", HeadPartKind::Misc, MOUTH);
    mouth.tris = vec![(PartFileKind::ChargenMorph, "meshes/test/small.tri".into())];
    let mut weights = Weights::default();
    weights.chargen.insert("noselong".into(), 1.0);
    let plan = Plan {
        head: vec![
            face,
            eyes,
            mouth,
            head("Hair", HeadPartKind::Hair, "meshes/test/nohair.nif"),
        ],
        weights,
        notes: vec!["From the plan.".into()],
        ..Plan::default()
    };
    let mut f = files(&root, &[HEAD, EYES, MOUTH]);
    f.insert("meshes/test/bad.tri", b"rubbish".to_vec());
    // The eyes' tri, which is not the mouth's size.
    f.insert("meshes/test/small.tri", tri_for(&eyes_model).0);
    let built = build(&plan, &f, &STILL).unwrap();
    let mut missing = built.missing.clone();
    missing.sort();
    assert_eq!(missing, ["meshes/test/gone.tri", "meshes/test/nohair.nif"]);
    // A tri not there leaves its part unshaped by it.
    assert_eq!(
        built_shape(&built, "Face").1.positions,
        face_model.positions
    );
    let names: Vec<&str> = built.shapes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Face"]);
    assert_eq!(built.notes[0], "From the plan.");
    assert!(built
        .notes
        .contains(&"Eyes was left out: meshes/test/bad.tri: it is not a tri.".to_string()));
    assert!(
        built.notes.iter().any(
            |n| n.starts_with("Mouth was left out: meshes/test/small.tri: ")
                && n.contains("vertices in its tri")
        ),
        "{:?}",
        built.notes
    );
}

#[test]
fn a_model_that_is_not_a_mesh_is_noted_and_left_out() {
    let Some(root) = root() else { return };
    let plan = Plan {
        head: vec![
            head("Hair", HeadPartKind::Hair, HAIR),
            head("Broken", HeadPartKind::Misc, "meshes/test/broken.nif"),
        ],
        ..Plan::default()
    };
    let mut f = files(&root, &[HAIR]);
    f.insert(
        "meshes/test/broken.nif",
        b"Gamebryo File Format, Version 20.2.0.7\nrubbish".to_vec(),
    );
    let built = build(&plan, &f, &STILL).unwrap();
    assert!(built.missing.is_empty());
    assert_eq!(built.shapes.len(), 1);
    assert!(
        built
            .notes
            .iter()
            .any(|n| n.starts_with("meshes/test/broken.nif: ")),
        "{:?}",
        built.notes
    );
}

/// The skin data of the shape at `block`, and its block.
fn skin(n: &Nif, block: usize) -> (usize, SkinData) {
    let stream = n.header.stream.version;
    let shape = TriShape::read(&n.blocks[block].data, stream).unwrap();
    let at = shape.skin as usize;
    let inst = SkinInstance::read(&n.blocks[at].data, n.kind_of(at) != "NiSkinInstance").unwrap();
    let d = inst.data as usize;
    (d, SkinData::read(&n.blocks[d].data).unwrap())
}

fn own_transform(n: &Nif, block: usize) -> Transform {
    TriShape::read(&n.blocks[block].data, n.header.stream.version)
        .unwrap()
        .av
        .transform
}

/// The model at `key` with the bind of its shape's bone `bone` moved 5
/// units along X, as a part made for another skeleton disagrees with this
/// one.
fn disagreeing(root: &std::path::Path, key: &str, bone: usize) -> Vec<u8> {
    let mut n = nif(root, key);
    let b = mesh::shapes(&n)[0];
    let (at, mut data) = skin(&n, b);
    data.bones[bone].transform.translation[0] += 5.0;
    n.blocks[at].data = data.write();
    n.write()
}

/// Every vertex of `key`'s first shape where its head bone takes it, W·T·v.
fn by_head_bone(
    model: &Nif,
    places: &HashMap<String, Transform>,
) -> (Mesh, SkinData, Vec<[f32; 3]>) {
    let b = mesh::shapes(model)[0];
    let before = mesh::read(model, b).unwrap();
    let names = mesh::bone_names(model, b);
    let (_, data) = skin(model, b);
    let by = names.iter().position(|n| n == HEAD_BONE).unwrap();
    let m = composed(&places[HEAD_BONE], &data.bones[by].transform);
    let expected = before.positions.iter().map(|p| m.apply(*p)).collect();
    (before, data, expected)
}

/// Each bone takes each vertex where it took it before.
fn binds_agree(old: &SkinData, new: &SkinData, before: &Mesh, after: &Mesh) {
    assert_eq!(old.bones.len(), new.bones.len());
    for (o, n) in old.bones.iter().zip(&new.bones) {
        for (p, q) in before.positions.iter().zip(&after.positions).step_by(37) {
            let (a, b) = (o.transform.apply(*p), n.transform.apply(*q));
            assert!((0..3).all(|c| (a[c] - b[c]).abs() < 1e-3), "{a:?} {b:?}");
        }
    }
}

#[test]
fn head_parts_are_moved_by_the_head_bone_and_body_pieces_keep_their_own_placement() {
    let Some(root) = root() else { return };
    let plan = Plan {
        head: vec![
            head("Face", HeadPartKind::Face, HEAD),
            head("Hair", HeadPartKind::Hair, HAIR),
        ],
        body: vec![body(HANDS, None)],
        skeleton: Some(SKELETON.into()),
        ..Plan::default()
    };
    let mut f = files(&root, &[HEAD, SKELETON]);
    // The hair's spine bone (its first; the head is its second) and the
    // hands' first bone disagree with the skeleton.
    let hair_bytes = disagreeing(&root, HAIR, 0);
    assert_eq!(
        mesh::bone_names(&nif(&root, HAIR), mesh::shapes(&nif(&root, HAIR))[0])[1],
        HEAD_BONE
    );
    f.insert(HAIR, hair_bytes.clone());
    let hands_bytes = disagreeing(&root, HANDS, 0);
    f.insert(HANDS, hands_bytes.clone());
    let built = build(&plan, &f, &Options::default()).unwrap();
    let places = bone_places(&nif(&root, SKELETON));

    for (name, model) in [
        ("Face", nif(&root, HEAD)),
        ("Hair", Nif::read(&hair_bytes).unwrap()),
    ] {
        let (before, old, expected) = by_head_bone(&model, &places);
        let (block, after) = built_shape(&built, name);
        assert!(far(&after.positions, &expected) < 1e-3, "{name}");
        // Moved a long way: onto the head.
        assert!(far(&after.positions, &before.positions) > 50.0, "{name}");
        let (_, new) = skin(&built.nif, block);
        binds_agree(&old, &new, &before, &after);
        // Nothing is left to move it again: neither its own transform nor
        // its skin's.
        assert_eq!(new.transform, Transform::IDENTITY, "{name}");
        assert_eq!(
            own_transform(&built.nif, block),
            Transform::IDENTITY,
            "{name}"
        );
    }
    // The face kept both before, 120 up and down, so they were undone.
    let face = nif(&root, HEAD);
    let fb = mesh::shapes(&face)[0];
    assert_ne!(own_transform(&face, fb), Transform::IDENTITY);
    assert_ne!(skin(&face, fb).1.transform, Transform::IDENTITY);

    // The hands: their own transform baked in, not their first bone's.
    let hands = Nif::read(&hands_bytes).unwrap();
    let hb = mesh::shapes(&hands)[0];
    let before = mesh::read(&hands, hb).unwrap();
    let own = own_transform(&hands, hb);
    let expected: Vec<[f32; 3]> = before.positions.iter().map(|p| own.apply(*p)).collect();
    let (block, after) = built_shape(&built, &before.name);
    assert!(far(&after.positions, &expected) < 1e-3);
    let names = mesh::bone_names(&hands, hb);
    let (_, old) = skin(&hands, hb);
    let by_bone = composed(&places[&names[0]], &old.bones[0].transform);
    let wrong: Vec<[f32; 3]> = before.positions.iter().map(|p| by_bone.apply(*p)).collect();
    assert!(far(&after.positions, &wrong) > 4.0);
    let (_, new) = skin(&built.nif, block);
    binds_agree(&old, &new, &before, &after);

    // Each placed shape says how far its bones disagree: the face's agree,
    // the hair's spine is 5 off and that is said; the body's is not measured.
    let placing: HashMap<_, _> = built.placing.iter().cloned().collect();
    assert_eq!(placing.len(), 3);
    assert_eq!(placing[&before.name], 0.0);
    assert!(placing["Face"] < 1e-3, "{}", placing["Face"]);
    assert!((placing["Hair"] - 5.0).abs() < 1e-2, "{}", placing["Hair"]);
    assert_eq!(
        built.notes,
        ["Hair's bones disagree with the skeleton by 5.00; it is placed by the head bone."]
    );
}

#[test]
fn without_the_skeleton_head_parts_stay_about_their_bone_and_it_is_said() {
    let Some(root) = root() else { return };
    let plan = Plan {
        head: vec![head("Hair", HeadPartKind::Hair, HAIR)],
        skeleton: Some(SKELETON.into()),
        ..Plan::default()
    };
    let built = build(&plan, &files(&root, &[HAIR]), &Options::default()).unwrap();
    let hair = shapes_of(&nif(&root, HAIR)).into_values().next().unwrap();
    assert_eq!(built_shape(&built, "Hair").1.positions, hair.positions);
    assert_eq!(built.missing, [SKELETON]);
    assert!(built
        .notes
        .iter()
        .any(|n| n.starts_with("Without the race's skeleton")));
    assert!(built.placing.is_empty());
    // With it, the same hair is moved.
    let built = build(&plan, &files(&root, &[HAIR, SKELETON]), &Options::default()).unwrap();
    assert!(far(&built_shape(&built, "Hair").1.positions, &hair.positions) > 50.0);
}
