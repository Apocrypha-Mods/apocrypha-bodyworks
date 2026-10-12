//! Real projects, when there are any in `scratch/samples` (not tracked;
//! skipped when empty).

use apoc_bodyslide::{category, group, osd, preset, set};
use std::path::{Path, PathBuf};

fn files(ext: &str, under: Option<&str>) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut all = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples"),
        &mut all,
    );
    all.retain(|p| {
        p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext))
            && under.is_none_or(|u| {
                p.to_string_lossy()
                    .to_lowercase()
                    .contains(&u.to_lowercase())
            })
    });
    all.sort();
    all
}

#[test]
fn every_real_project_reads_and_reads_the_same_after_being_written() {
    let mut sets_seen = 0;
    for path in files("osp", None) {
        let bytes = std::fs::read(&path).unwrap();
        let sets = set::read(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let again = set::read(set::write(&sets).as_bytes()).unwrap();
        assert_eq!(again, sets, "{}", path.display());
        for s in &sets {
            assert!(
                !s.name.is_empty() && !s.source_file.is_empty(),
                "{}",
                path.display()
            );
            for slider in &s.sliders {
                assert!(!slider.name.is_empty(), "{}", path.display());
            }
        }
        sets_seen += sets.len();
    }
    eprintln!("{sets_seen} sets");
}

#[test]
fn every_real_slider_data_file_writes_back_byte_for_byte() {
    for path in files("osd", None) {
        let bytes = std::fs::read(&path).unwrap();
        let data =
            osd::SliderData::read(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!data.lists.is_empty());
        assert_eq!(data.write(), bytes, "{}", path.display());
    }
}

#[test]
fn every_real_preset_and_group_file_round_trips() {
    for path in files("xml", Some("SliderPresets")) {
        let presets = preset::read(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            preset::read(preset::write(&presets).as_bytes()).unwrap(),
            presets
        );
        assert!(
            presets.iter().all(|p| !p.values.is_empty()),
            "{}",
            path.display()
        );
    }
    for path in files("xml", Some("SliderCategories")) {
        let categories = category::read(&std::fs::read(&path).unwrap()).unwrap();
        assert!(
            categories.iter().all(|c| !c.sliders.is_empty()),
            "{}",
            path.display()
        );
        assert_eq!(
            category::read(category::write(&categories).as_bytes()).unwrap(),
            categories
        );
    }
    for path in files("xml", Some("SliderGroups")) {
        let groups = group::read(&std::fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            group::read(group::write(&groups).as_bytes()).unwrap(),
            groups
        );
    }
}

/// CBBE ships bodies its authors built with the original program at three of
/// its presets. A build here at the same preset lands on the same vertices.
#[test]
fn a_build_matches_the_bodies_cbbe_was_shipped_with() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/cbbe");
    let tools = root.join("@required/Data/CalienteTools/BodySlide");
    if !tools.is_dir() {
        return;
    }
    fn find(root: &Path, rel: &str) -> Option<PathBuf> {
        let mut at = root.to_path_buf();
        for part in rel.split('/') {
            at = std::fs::read_dir(&at)
                .ok()?
                .flatten()
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?
                .path();
        }
        Some(at)
    }
    let files = |path: &str| find(&tools, path).and_then(|p| std::fs::read(p).ok());
    let sets = set::read(&std::fs::read(tools.join("SliderSets/CBBE.osp")).unwrap()).unwrap();
    let body = sets.iter().find(|s| s.name == "CBBE Body").unwrap();
    let mut presets = Vec::new();
    for e in std::fs::read_dir(tools.join("SliderPresets"))
        .unwrap()
        .flatten()
    {
        presets.extend(preset::read(&std::fs::read(e.path()).unwrap()).unwrap());
    }
    let positions = |bytes: &[u8]| {
        let nif = apoc_nif::Nif::read(bytes).unwrap();
        let at = apoc_nif::mesh::shapes(&nif)[0];
        apoc_nif::mesh::read(&nif, at).unwrap().positions
    };
    let meshes = "Data/Meshes/actors/character/character assets";
    for (option, name) in [
        ("@required", "CBBE Slim"),
        ("step-0_group-0_curvy", "CBBE Curvy"),
        ("step-0_group-0_vanilla-shape", "CBBE Vanilla"),
    ] {
        let p = presets.iter().find(|p| p.name == name).unwrap();
        let built = apoc_bodyslide::build::build(body, Some(p), &files).unwrap();
        assert_eq!(built.outputs.len(), 2);
        assert!(built.outputs[0]
            .path
            .ends_with("character assets/femalebody_0.nif"));
        for (out, suffix) in built.outputs.iter().zip(["_0", "_1"]) {
            let shipped =
                find(&root, &format!("{option}/{meshes}/femalebody{suffix}.nif")).unwrap();
            let (ours, theirs) = (
                positions(&out.bytes),
                positions(&std::fs::read(shipped).unwrap()),
            );
            assert_eq!(ours.len(), theirs.len());
            let distances: Vec<f32> = ours
                .iter()
                .zip(&theirs)
                .map(|(a, b)| {
                    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
                })
                .collect();
            let mean = distances.iter().sum::<f32>() / distances.len() as f32;
            let worst = distances.iter().copied().fold(0.0f32, f32::max);
            assert!(
                mean < 0.002 && worst < 0.05,
                "{name}{suffix}: mean {mean}, worst {worst}"
            );
        }
    }
}

/// A game-like folder made of several, a later one's file replacing an
/// earlier one's, for the CBBE sample's option folders.
fn layered(roots: &[&str]) -> Option<std::collections::BTreeMap<String, PathBuf>> {
    fn walk(dir: &Path, rel: &str, into: &mut std::collections::BTreeMap<String, PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if e.path().is_dir() {
                walk(&e.path(), &rel, into);
            } else {
                into.insert(rel.to_lowercase(), e.path());
            }
        }
    }
    let mut all = std::collections::BTreeMap::new();
    for root in roots {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scratch/samples/cbbe")
            .join(root);
        if !dir.is_dir() {
            return None;
        }
        walk(&dir, "", &mut all);
    }
    Some(all)
}

#[test]
fn vertices_a_set_takes_out_are_the_ones_the_original_took_out() {
    use apoc_bodyslide::build;
    use apoc_nif::skin::{SkinInstance, SkinPartition};
    let Some(all) = layered(&["@required", "@conditional-3"]) else {
        return;
    };
    let tools = "data/calientetools/bodyslide/";
    let files = |path: &str| {
        all.get(&format!("{tools}{}", path.to_lowercase()))
            .and_then(|p| std::fs::read(p).ok())
    };
    let sets: Vec<set::SliderSet> = all
        .iter()
        .filter(|(k, _)| k.starts_with(&format!("{tools}slidersets/")) && k.ends_with(".osp"))
        .flat_map(|(_, p)| set::read(&std::fs::read(p).unwrap()).unwrap_or_default())
        .collect();
    let shapes = |bytes: &[u8]| {
        let nif = apoc_nif::Nif::read(bytes).unwrap();
        apoc_nif::mesh::shapes(&nif)
            .into_iter()
            .map(|at| apoc_nif::mesh::read(&nif, at).unwrap())
            .map(|m| (m.name, m.positions.len(), m.triangles.len()))
            .collect::<Vec<_>>()
    };
    // Gauntlets over hands: the list names every vertex of the hands and
    // marks 1,136 of them. A robe over a body: most of the body goes, and
    // two of its three parts with it.
    for (name, theirs) in [
        (
            "CBBE Vanilla - Hands - Steel",
            "data/meshes/armor/steel/f/gauntlets_1.nif",
        ),
        (
            "CBBE Dawnguard - Body - Blind Moth Priest",
            "data/meshes/dlc01/clothes/blindmothpriest/blindmothpriestrobesf_1.nif",
        ),
    ] {
        let s = sets.iter().find(|s| s.name == name).unwrap();
        let built = build::build(s, None, &files).unwrap();
        let ours = &built.outputs[1].bytes;
        assert_eq!(
            shapes(ours),
            shapes(&std::fs::read(&all[theirs]).unwrap()),
            "{name}"
        );
    }
    let s = sets
        .iter()
        .find(|s| s.name == "CBBE Dawnguard - Body - Blind Moth Priest")
        .unwrap();
    let built = build::build(s, None, &files).unwrap();
    let nif = apoc_nif::Nif::read(&built.outputs[1].bytes).unwrap();
    let body = apoc_nif::mesh::shapes(&nif)[0];
    let shape = apoc_nif::geometry::TriShape::read(&nif.blocks[body].data, 100).unwrap();
    let instance = SkinInstance::read(&nif.blocks[shape.skin as usize].data, true).unwrap();
    assert_eq!(instance.body_parts, Some(vec![(257, 32)]));
    let partition =
        SkinPartition::read(&nif.blocks[instance.partition as usize].data, 100).unwrap();
    assert_eq!(partition.partitions.len(), 1);
    assert_eq!(partition.partitions[0].vertex_count(), 321);
    assert_eq!(partition.vertex_data.len(), 321 * 32);
}

#[test]
fn a_shape_taken_out_of_a_project_is_out_of_what_it_builds() {
    use apoc_bodyslide::project::Project;
    use apoc_bodyslide::{build, morph};
    use std::collections::HashMap;

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let Some(set) = sets.iter().find(|s| s.shapes.len() > 1) else {
        return;
    };
    let mut project = Project::open(set, &on_disk).unwrap();
    assert_eq!(project.notes, Vec::<String>::new());
    let gone = set.shapes[0].name.clone();
    project.delete_shape(&gone).unwrap();
    project
        .rename_shape(&set.shapes[1].name, "Renamed")
        .unwrap();
    assert!(project.rename_shape("Renamed", "Renamed").is_ok());
    assert!(project.delete_shape(&gone).is_err());

    let (saved, files) = project.save("Without one").unwrap();
    let held: HashMap<String, Vec<u8>> = files.into_iter().map(|o| (o.path, o.bytes)).collect();
    let files = |path: &str| held.get(path).cloned();
    let values = morph::values(&saved, None, true);
    let built = build::build_at(&saved, &values, &values, &files).unwrap();
    assert_eq!(built.notes, Vec::<String>::new());
    let nif = apoc_nif::Nif::read(&built.outputs[0].bytes).unwrap();
    let names: Vec<String> = apoc_nif::mesh::shapes(&nif)
        .into_iter()
        .filter_map(|at| apoc_nif::mesh::name(&nif, at))
        .collect();
    assert!(!names.contains(&gone));
    assert!(names.contains(&"Renamed".to_string()));
    assert_eq!(names.len(), set.shapes.len() - 1);
}

#[test]
fn an_outfit_conformed_to_its_body_follows_it_much_as_it_did() {
    use apoc_bodyslide::project::Project;
    use apoc_bodyslide::{build, conform, morph};
    use std::collections::HashMap;

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let set = sets.iter().find(|s| s.name == "CBBE NeverNude").unwrap();
    let body = set.shapes.iter().find(|s| s.data_folder.is_some()).unwrap();
    let outfit = set.shapes.iter().find(|s| s.data_folder.is_none()).unwrap();
    let mut project = Project::open(set, &on_disk).unwrap();
    let values = morph::values(set, None, true);
    let built = |p: &Project| {
        let (saved, files) = p.save("Conformed").unwrap();
        let held: HashMap<String, Vec<u8>> = files.into_iter().map(|o| (o.path, o.bytes)).collect();
        let files = |path: &str| held.get(path).cloned().or_else(|| on_disk(path));
        let out = build::build_at(&saved, &values, &values, &files).unwrap();
        let nif = apoc_nif::Nif::read(&out.outputs[0].bytes).unwrap();
        apoc_nif::mesh::shapes(&nif)
            .into_iter()
            .filter_map(|at| apoc_nif::mesh::read(&nif, at).ok())
            .find(|m| m.name == outfit.name)
            .unwrap()
            .positions
    };
    let before = built(&project);
    // The body's own data is another project's, and stays so.
    assert!(project
        .conform(
            &body.name,
            &outfit.name,
            &on_disk,
            conform::Options::default(),
            &[],
        )
        .is_err());
    let moving = project
        .conform(
            &outfit.name,
            &body.name,
            &on_disk,
            conform::Options::default(),
            &[],
        )
        .unwrap();
    assert!(moving > 50, "{moving} sliders");
    let after = built(&project);
    // Built at the set's own values, no vertex is far from where the
    // outfit's own data put it: a small part of the size of a body.
    let worst = before
        .iter()
        .zip(&after)
        .map(|(a, b)| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        })
        .fold(0.0f32, f32::max);
    assert!(worst < 1.5, "a vertex {worst} from where it was");
}

#[test]
fn a_shape_copied_into_another_mesh_is_the_same_shape_there() {
    use apoc_nif::{copy, mesh, Nif};

    let shapes = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide/ShapeData");
    let read = |rel: &str| {
        std::fs::read(shapes.join(rel))
            .ok()
            .map(|b| Nif::read(&b).unwrap())
    };
    let (Some(body), Some(mut outfit)) =
        (read("CBBE/CBBE Body.nif"), read("CBBE/CBBE NeverNude.nif"))
    else {
        return;
    };
    let all = |nif: &Nif| -> Vec<mesh::Mesh> {
        mesh::shapes(nif)
            .into_iter()
            .map(|at| mesh::read(nif, at).unwrap())
            .collect()
    };
    let before = all(&outfit);
    let source = mesh::shapes(&body)[0];
    let was = mesh::read(&body, source).unwrap();
    let at = copy::copy_shape(&mut outfit, &body, source).unwrap();
    // The file is still a file, and what was in it is as it was.
    let outfit = Nif::read(&outfit.write()).unwrap();
    let after = all(&outfit);
    assert_eq!(after.len(), before.len() + 1);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(
            (&a.name, &a.positions, &a.triangles),
            (&b.name, &b.positions, &b.triangles)
        );
        assert_eq!(
            mesh::bone_names(&outfit, b.block).len(),
            mesh::bone_names(&outfit, a.block).len()
        );
    }
    let now = mesh::read(&outfit, at).unwrap();
    assert_eq!(
        (
            &now.name,
            &now.positions,
            &now.uvs,
            &now.triangles,
            &now.textures,
            now.skinned
        ),
        (
            &was.name,
            &was.positions,
            &was.uvs,
            &was.triangles,
            &was.textures,
            was.skinned
        )
    );
    // Its bones are the bones of the same names, none of them lost.
    let bones = mesh::bone_names(&outfit, at);
    assert_eq!(bones, mesh::bone_names(&body, source));
    assert!(bones.iter().all(|b| !b.is_empty()));

    // And a shape copied into its own file.
    let mut twice = outfit.clone();
    let first = mesh::shapes(&twice)[0];
    let copy = copy::copy_shape(&mut twice, &outfit, first).unwrap();
    let twice = Nif::read(&twice.write()).unwrap();
    assert_eq!(
        mesh::read(&twice, copy).unwrap().positions,
        mesh::read(&twice, first).unwrap().positions
    );
    assert_eq!(
        mesh::bone_names(&twice, copy),
        mesh::bone_names(&twice, first)
    );
}

#[test]
fn a_mesh_becomes_a_project_fitted_to_a_body() {
    use apoc_bodyslide::project::Project;
    use apoc_bodyslide::{build, conform};
    use std::collections::HashMap;

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let body = sets.iter().find(|s| s.name == "CBBE Body").unwrap();
    // An outfit as a mod ships it: a mesh, with a body inside it.
    let mesh = on_disk("ShapeData/CBBE/CBBE NeverNude.nif").unwrap();
    let mut project =
        Project::from_mesh("Mine", "meshes\\clothes/mine/underwear_1.nif", &mesh).unwrap();
    assert_eq!(project.set.output_path, "meshes\\clothes\\mine");
    assert_eq!(project.set.output_file, "underwear");
    assert!(project.set.gen_weights);
    assert_eq!(project.set.shapes.len(), 3);
    assert!(project.set.sliders.is_empty());

    // The body inside is in the way of the one brought in, until it goes.
    assert!(project.add_reference(body, &on_disk).is_err());
    assert_eq!(project.set.shapes.len(), 3);
    project.delete_shape("CBBE").unwrap();
    project.add_reference(body, &on_disk).unwrap();
    assert_eq!(project.set.sliders.len(), body.sliders.len());
    let reference = project.set.shapes.last().unwrap().clone();
    assert_eq!(reference.data_folder.as_deref(), Some("CBBE"));
    // Brought in again, it takes the place of the first.
    project.add_reference(body, &on_disk).unwrap();
    assert_eq!(project.set.shapes.len(), 3);

    project.duplicate_shape("Bra", "Bra again").unwrap();
    assert!(project.duplicate_shape("Bra", "Panty").is_err());
    // The copy, turned a quarter round Z, twice its size and moved up one:
    // what was at (x, y, z) is at (-2y, 2x, 2z + 1).
    let place = |p: &Project, name: &str| {
        let at = apoc_nif::mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| apoc_nif::mesh::name(&p.nif, *at).as_deref() == Some(name))
            .unwrap();
        apoc_nif::mesh::read(&p.nif, at).unwrap().positions
    };
    let was = place(&project, "Bra again");
    project
        .transform("Bra again", [0.0, 0.0, 1.0], 2.0, [0.0, 0.0, 90.0])
        .unwrap();
    for (a, b) in was.iter().zip(&place(&project, "Bra again")) {
        let want = [-2.0 * a[1], 2.0 * a[0], 2.0 * a[2] + 1.0];
        assert!(
            (0..3).all(|k| (want[k] - b[k]).abs() < 0.05),
            "{want:?} {b:?}"
        );
    }
    assert_eq!(place(&project, "Bra"), was);
    assert!(project.transform("Bra", [0.0; 3], 0.0, [0.0; 3]).is_err());
    project
        .transform("Bra again", [0.0, 0.0, -1.0], 1.0, [0.0; 3])
        .unwrap();
    project
        .transform("Bra again", [0.0; 3], 0.5, [0.0, 0.0, -90.0])
        .unwrap();
    for shape in ["Bra", "Panty"] {
        let moving = project
            .conform(
                shape,
                &reference.name,
                &on_disk,
                conform::Options::default(),
                &[],
            )
            .unwrap();
        assert!(moving > 20, "{shape}: {moving}");
    }

    // The same, of every shape at once; and a slider cleared, and turned.
    let mut all = project.clone();
    assert_eq!(
        all.conform_all(&reference.name, &on_disk, conform::Options::default())
            .unwrap(),
        3
    );
    let (with, _) = all.save("All").unwrap();
    let does = |set: &set::SliderSet, slider: &str, target: &str| {
        set.sliders
            .iter()
            .find(|s| s.name == slider)
            .unwrap()
            .data
            .iter()
            .any(|d| d.target == target)
    };
    assert!(does(&with, "Breasts", "Bra") && does(&with, "Breasts", "Bra again"));
    all.clear_slider("Breasts", "Bra").unwrap();
    assert!(all.clear_slider("Breasts", &reference.name).is_err());
    let (without, _) = all.save("All").unwrap();
    assert!(!does(&without, "Breasts", "Bra") && does(&without, "Breasts", "Bra again"));
    all.negate_slider("Breasts").unwrap();

    // The outfit as it stands at some sliders made its base shape: the
    // bra is where Breasts at full had it, and the body is where it was.
    let mut based = project.clone();
    let mut at = vec![0.0; based.set.sliders.len()];
    at[based
        .set
        .sliders
        .iter()
        .position(|s| s.name == "Breasts")
        .unwrap()] = 100.0;
    let (bra, body_was) = (place(&based, "Bra"), place(&based, &reference.name));
    based.set_base(&at).unwrap();
    assert_ne!(place(&based, "Bra"), bra);
    assert_eq!(place(&based, &reference.name), body_was);
    assert!(based.set_base(&[0.0]).is_err());

    // A slider that takes part of a shape out, and a texture turned over.
    let mut zapped = project.clone();
    zapped
        .add_zap("No straps", "Bra", &[0, 1, 2, 3, 4, 5])
        .unwrap();
    assert!(zapped.add_zap("No straps", "Bra", &[0]).is_err());
    assert!(zapped.add_zap("Nothing", "Bra", &[]).is_err());
    let (zap_set, zap_files) = zapped.save("Zap").unwrap();
    assert!(zap_set.sliders.last().unwrap().zap);
    let zap_held: HashMap<String, Vec<u8>> =
        zap_files.into_iter().map(|o| (o.path, o.bytes)).collect();
    let built_at = |on: f32| {
        let mut values = vec![0.0; zap_set.sliders.len()];
        *values.last_mut().unwrap() = on;
        let out = build::build_at(&zap_set, &values, &values, &|path: &str| {
            zap_held.get(path).cloned().or_else(|| on_disk(path))
        })
        .unwrap();
        let nif = apoc_nif::Nif::read(&out.outputs[0].bytes).unwrap();
        apoc_nif::mesh::shapes(&nif)
            .into_iter()
            .map(|at| apoc_nif::mesh::read(&nif, at).unwrap())
            .find(|m| m.name == "Bra")
            .unwrap()
            .positions
            .len()
    };
    assert_eq!(built_at(0.0), built_at(100.0) + 6);
    let uv = |p: &Project| {
        let at = apoc_nif::mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| apoc_nif::mesh::name(&p.nif, *at).as_deref() == Some("Bra"))
            .unwrap();
        apoc_nif::mesh::read(&p.nif, at).unwrap().uvs
    };
    let was_uv = uv(&zapped);
    zapped.flip_uv("Bra", true).unwrap();
    let now_uv = uv(&zapped);
    assert!(was_uv
        .iter()
        .zip(&now_uv)
        .all(|(a, b)| (a[0] + b[0] - 1.0).abs() < 0.002 && (a[1] - b[1]).abs() < 0.002));

    // Part of a shape made a shape of its own; textures named; a bone let go.
    let mut parted = project.clone();
    let whole = place(&parted, "Bra").len();
    parted
        .separate("Bra", "Straps", &[0, 1, 2, 3, 4, 5, 6, 7, 8])
        .unwrap();
    assert_eq!(place(&parted, "Bra").len(), whole - 9);
    assert_eq!(place(&parted, "Straps").len(), 9);
    assert!(parted.separate("Bra", "None", &[]).is_err());
    assert!(parted.separate("Bra", "Panty", &[0]).is_err());
    assert_eq!(place(&parted, "Bra").len(), whole - 9);
    let textures = |p: &Project| {
        let at = apoc_nif::mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| apoc_nif::mesh::name(&p.nif, *at).as_deref() == Some("Bra"))
            .unwrap();
        apoc_nif::mesh::read(&p.nif, at).unwrap().textures
    };
    let mut named = textures(&parted);
    assert!(parted.set_textures("Bra", &named[1..]).is_err());
    named[0] = "textures\\mine\\bra.dds".to_string();
    parted.set_textures("Bra", &named).unwrap();
    assert_eq!(textures(&parted), named);
    assert_ne!(textures(&project)[0], named[0]);
    let skin = |p: &Project| {
        let at = apoc_nif::mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| apoc_nif::mesh::name(&p.nif, *at).as_deref() == Some("Panty"))
            .unwrap();
        apoc_nif::weights::read(&p.nif, at).unwrap()
    };
    let had = skin(&parted);
    let let_go = had.bones[1].clone();
    parted.drop_bone("Panty", &let_go).unwrap();
    let has = skin(&parted);
    assert_eq!(has.bones.len(), had.bones.len() - 1);
    assert!(!has.bones.contains(&let_go));
    assert!(has
        .vertices
        .iter()
        .all(|v| (v.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 0.01));
    assert!(parted.drop_bone("Panty", &let_go).is_err());
    assert!(apoc_nif::Nif::read(&parted.nif.write()).is_ok());

    // Vertices taken out of a shape, and out of what sliders do to it.
    let mut cut = project.clone();
    let had = place(&cut, "Bra").len();
    cut.delete_vertices("Bra", &[0, 1, 2, 3, 4, 5, 6, 7])
        .unwrap();
    assert_eq!(place(&cut, "Bra").len(), had - 8);
    assert!(cut.delete_vertices("Bra", &[60000]).is_err());
    assert!(cut.delete_vertices(&reference.name, &[0]).is_err());
    let (cut_set, cut_files) = cut.save("Cut").unwrap();
    let cut_held: HashMap<String, Vec<u8>> =
        cut_files.into_iter().map(|o| (o.path, o.bytes)).collect();
    let values = vec![100.0; cut_set.sliders.len()];
    let out = build::build_at(&cut_set, &values, &values, &|path: &str| {
        cut_held.get(path).cloned().or_else(|| on_disk(path))
    })
    .unwrap();
    assert_eq!(out.notes, Vec::<String>::new());
    assert!(apoc_nif::Nif::read(&out.outputs[0].bytes).is_ok());

    let (saved, files) = project.save("Mine").unwrap();
    let held: HashMap<String, Vec<u8>> = files.into_iter().map(|o| (o.path, o.bytes)).collect();
    let files = |path: &str| held.get(path).cloned().or_else(|| on_disk(path));
    assert_eq!(
        set::read(&held["SliderSets/Mine.osp"]).unwrap(),
        vec![saved.clone()]
    );
    let shapes = |values: &[f32]| -> Vec<apoc_nif::mesh::Mesh> {
        let out = build::build_at(&saved, values, values, &files).unwrap();
        assert_eq!(out.notes, Vec::<String>::new());
        assert_eq!(out.outputs.len(), 2);
        let nif = apoc_nif::Nif::read(&out.outputs[1].bytes).unwrap();
        apoc_nif::mesh::shapes(&nif)
            .into_iter()
            .map(|at| apoc_nif::mesh::read(&nif, at).unwrap())
            .collect()
    };
    let none = vec![0.0; saved.sliders.len()];
    let mut some = none.clone();
    some[saved
        .sliders
        .iter()
        .position(|s| s.name == "Breasts")
        .unwrap()] = 100.0;
    let (flat, full) = (shapes(&none), shapes(&some));
    assert_eq!(flat.len(), 4);
    let moved = |name: &str| {
        let (a, b) = (
            flat.iter().find(|m| m.name == name).unwrap(),
            full.iter().find(|m| m.name == name).unwrap(),
        );
        a.positions
            .iter()
            .zip(&b.positions)
            .filter(|(x, y)| x != y)
            .count()
    };
    // The body moves, the bra on it follows, the copy made before the
    // fitting does not, and neither does what is nowhere near.
    assert!(moved(&reference.name) > 500);
    assert!(moved("Bra") > 100);
    assert_eq!(moved("Bra again"), 0);
    assert_eq!(moved("Panty"), 0);
}

#[test]
fn an_outfit_given_its_bodys_bones_bends_much_as_it_did() {
    use apoc_bodyslide::conform;
    use apoc_bodyslide::project::Project;
    use apoc_nif::{mesh, weights, Nif};

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let set = sets.iter().find(|s| s.name == "CBBE NeverNude").unwrap();
    let mut project = Project::open(set, &on_disk).unwrap();
    let skin_of = |nif: &Nif, name: &str| {
        let at = mesh::shapes(nif)
            .into_iter()
            .find(|at| mesh::name(nif, *at).as_deref() == Some(name))
            .unwrap();
        weights::read(nif, at).unwrap()
    };
    let heaviest = |skin: &weights::Skin| -> Vec<String> {
        skin.vertices
            .iter()
            .map(|v| skin.bones[v[0].0 as usize].clone())
            .collect()
    };
    let body = skin_of(&project.nif, "CBBE");
    for shape in ["Bra", "Panty"] {
        let before = skin_of(&project.nif, shape);
        let bones = project
            .copy_weights(shape, "CBBE", conform::Options::default(), &[])
            .unwrap();
        // The file is still a file, with the skin it was given.
        let nif = Nif::read(&project.nif.write()).unwrap();
        let after = skin_of(&nif, shape);
        assert_eq!(after.bones.len(), bones);
        assert_eq!(after.vertices.len(), before.vertices.len());
        assert!(
            after.bones.iter().all(|b| body.bones.contains(b)),
            "{shape}"
        );
        for v in &after.vertices {
            assert!(
                (v.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 0.01,
                "{shape}"
            );
        }
        // Most vertices follow, most of all, the bone they did as made.
        let (was, now) = (heaviest(&before), heaviest(&after));
        let same = was.iter().zip(&now).filter(|(a, b)| a == b).count();
        assert!(
            same * 10 >= was.len() * 8,
            "{shape}: {same} of {} follow the bone they did",
            was.len()
        );
        eprintln!(
            "{shape}: {same} of {} keep their heaviest bone, {bones} bones",
            was.len()
        );
    }
    assert!(project
        .copy_weights("Bra", "Bra", conform::Options::default(), &[])
        .is_err());
    // One bone of the body's alone: the bra follows it as the body under
    // it does, and still follows the bones it did, each making room.
    let mut one = Project::open(set, &on_disk).unwrap();
    let had = skin_of(&one.nif, "Bra");
    let bone = had.bones[0].clone();
    let was = one.bone_weights("Bra", &bone).unwrap();
    one.copy_bone(
        "Bra",
        "CBBE",
        &bone,
        conform::Options::default(),
        &[0, 1, 2],
    )
    .unwrap();
    let now = one.bone_weights("Bra", &bone).unwrap();
    assert_eq!(&now[..3], &was[..3]);
    assert!(now.iter().zip(&was).any(|(a, b)| (a - b).abs() > 0.001));
    let after = skin_of(&Nif::read(&one.nif.write()).unwrap(), "Bra");
    assert!(had.bones.iter().all(|b| after.bones.contains(b)));
    for v in &after.vertices {
        assert!((v.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 0.01);
    }
    assert!(one
        .copy_bone(
            "Bra",
            "CBBE",
            "No such bone",
            conform::Options::default(),
            &[]
        )
        .is_err());
    // Painted: vertex 3 follows a bone of the body's that the bra did not,
    // by half, and what it followed before by the other half between them.
    let fresh = body
        .bones
        .iter()
        .find(|b| !skin_of(&project.nif, "Bra").bones.contains(b))
        .unwrap()
        .clone();
    let was = skin_of(&project.nif, "Bra");
    project.paint_weights("Bra", &fresh, &[(3, 0.5)]).unwrap();
    let nif = Nif::read(&project.nif.write()).unwrap();
    let now = skin_of(&nif, "Bra");
    let painted = project.bone_weights("Bra", &fresh).unwrap();
    assert!((painted[3] - 0.5).abs() < 0.002);
    assert!(painted.iter().enumerate().all(|(v, w)| v == 3 || *w == 0.0));
    assert!((now.vertices[3].iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 0.01);
    for (bone, weight) in was.vertices[3].iter().filter(|x| x.1 > 0.0) {
        let name = &was.bones[*bone as usize];
        let after = project.bone_weights("Bra", name).unwrap()[3];
        assert!(
            (after - weight * 0.5).abs() < 0.01,
            "{name}: {after} from {weight}"
        );
    }
    assert_eq!(now.vertices[4], was.vertices[4]);
    assert!(project
        .paint_weights("Bra", "No such bone", &[(3, 0.5)])
        .is_err());
    assert_eq!(project.bone_weights("Bra", "No such bone").unwrap()[3], 0.0);
    // The body itself is as it was.
    assert_eq!(skin_of(&project.nif, "CBBE"), body);
}

#[test]
fn what_is_spared_keeps_its_sliders_and_its_bones() {
    use apoc_bodyslide::conform;
    use apoc_bodyslide::project::Project;
    use apoc_nif::{mesh, weights};

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let set = sets.iter().find(|s| s.name == "CBBE NeverNude").unwrap();
    let project = Project::open(set, &on_disk).unwrap();
    let skin = |p: &Project| {
        let at = mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| mesh::name(&p.nif, *at).as_deref() == Some("Bra"))
            .unwrap();
        weights::read(&p.nif, at).unwrap()
    };
    let by_name = |s: &weights::Skin, v: usize| -> Vec<(String, f32)> {
        let mut all: Vec<(String, f32)> = s.vertices[v]
            .iter()
            .filter(|x| x.1 > 0.0)
            .map(|x| (s.bones[x.0 as usize].clone(), x.1))
            .collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all
    };
    let spared: Vec<u16> = (0..50).collect();
    let was = skin(&project);
    // Painted first, so the spared vertices have weights no copy would give.
    let mut painted = project.clone();
    let odd = was.bones.last().unwrap().clone();
    painted
        .paint_weights(
            "Bra",
            &odd,
            &spared.iter().map(|v| (*v, 0.9)).collect::<Vec<_>>(),
        )
        .unwrap();
    let before = skin(&painted);
    painted
        .copy_weights("Bra", "CBBE", conform::Options::default(), &spared)
        .unwrap();
    let after = skin(&painted);
    for v in 0..50 {
        let (a, b) = (by_name(&before, v), by_name(&after, v));
        assert_eq!(a.len(), b.len(), "vertex {v}");
        assert!(a
            .iter()
            .zip(&b)
            .all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 0.002));
    }
    // The rest were given the body's, and are not as painted.
    assert!((50..200).any(|v| by_name(&before, v) != by_name(&after, v)) || was == before);

    // A slider's data out as a file, and back in to another slider.
    let mut swapped = project.clone();
    let file = swapped.slider_data("Breasts").unwrap();
    let lists = osd::SliderData::read(&file).unwrap();
    assert!(lists.lists.iter().any(|(n, _)| n == "BraBreasts"));
    swapped.add_slider("Copy").unwrap();
    assert!(swapped.slider_data("Copy").is_err());
    assert_eq!(
        swapped.import_slider("Copy", &file).unwrap(),
        lists.lists.len()
    );
    let again = osd::SliderData::read(&swapped.slider_data("Copy").unwrap()).unwrap();
    assert_eq!(
        again.lists.iter().map(|l| &l.1).collect::<Vec<_>>(),
        lists.lists.iter().map(|l| &l.1).collect::<Vec<_>>()
    );
    assert!(swapped.import_slider("Copy", b"not a file").is_err());

    // Morphs out of a build and back in as sliders: what each does to the
    // bra comes back as it went, to the fineness a morph file keeps.
    let (saved, files) = project.save("A").unwrap();
    let held: std::collections::HashMap<String, Vec<u8>> =
        files.into_iter().map(|o| (o.path, o.bytes)).collect();
    let zeros = vec![0.0; saved.sliders.len()];
    let built = apoc_bodyslide::build::build_with(
        &saved,
        &zeros,
        &zeros,
        &|path: &str| held.get(path).cloned().or_else(|| on_disk(path)),
        true,
    )
    .unwrap();
    let tri = built.morphs.unwrap().bytes;
    let mut bare = Project::from_mesh("Bare", "meshes/x_1.nif", &project.nif.write()).unwrap();
    assert!(bare.set.sliders.is_empty());
    let taken = bare.import_morphs(&tri).unwrap();
    assert!(taken > 20 && !bare.set.sliders.is_empty());
    let back = osd::SliderData::read(&bare.slider_data("Breasts").unwrap()).unwrap();
    let went = osd::SliderData::read(&project.slider_data("Breasts").unwrap()).unwrap();
    let of = |d: &osd::SliderData| -> Vec<osd::Diff> {
        let mut l = d.list("BraBreasts").unwrap().to_vec();
        l.retain(|x| x.offset.iter().any(|c| c.abs() >= 0.001));
        l.sort_by_key(|x| x.vertex);
        l
    };
    let (a, b) = (of(&went), of(&back));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.vertex, y.vertex);
        assert!((0..3).all(|k| (x.offset[k] - y.offset[k]).abs() < 0.001));
    }
    assert!(bare.import_morphs(b"not a file").is_err());

    // Two sliders as one: at full, the new one puts the bra where the two
    // together put it at the values they stood at.
    let mut combined = project.clone();
    let names: Vec<String> = combined
        .set
        .sliders
        .iter()
        .map(|s| s.name.clone())
        .collect();
    let mut at = vec![0.0; names.len()];
    at[names.iter().position(|n| n == "Breasts").unwrap()] = 100.0;
    at[names.iter().position(|n| n == "BigTorso").unwrap()] = 50.0;
    combined.combine("Both", &at).unwrap();
    assert!(combined.combine("Both", &at).is_err());
    assert!(combined
        .combine("None", &vec![0.0; names.len() + 1])
        .is_err());
    let both = osd::SliderData::read(&combined.slider_data("Both").unwrap()).unwrap();
    let (a, b) = (
        osd::SliderData::read(&project.slider_data("Breasts").unwrap()).unwrap(),
        osd::SliderData::read(&project.slider_data("BigTorso").unwrap()).unwrap(),
    );
    let at_vertex = |d: &osd::SliderData, list: &str, v: u16| {
        d.list(list)
            .and_then(|l| l.iter().find(|x| x.vertex == v).map(|x| x.offset))
            .unwrap_or([0.0; 3])
    };
    for v in (0..500u16).step_by(13) {
        let (x, y, z) = (
            at_vertex(&a, "BraBreasts", v),
            at_vertex(&b, "BraBigTorso", v),
            at_vertex(&both, "BraBoth", v),
        );
        assert!(
            (0..3).all(|k| (x[k] + y[k] * 0.5 - z[k]).abs() < 0.0001),
            "vertex {v}"
        );
    }

    // Sliders likewise: the spared keep the data the outfit came with.
    let (shipped, _) = project.save("A").unwrap();
    let mut conformed = project.clone();
    conformed
        .conform(
            "Bra",
            "CBBE",
            &on_disk,
            conform::Options::default(),
            &spared,
        )
        .unwrap();
    let (_, files_a) = project.save("A").unwrap();
    let (_, files_b) = conformed.save("A").unwrap();
    let lists = |files: &[apoc_bodyslide::build::Output]| {
        osd::SliderData::read(
            &files
                .iter()
                .find(|f| f.path.ends_with(".osd"))
                .unwrap()
                .bytes,
        )
        .unwrap()
    };
    let (a, b) = (lists(&files_a), lists(&files_b));
    let mut compared = 0;
    for (name, diffs) in &a.lists {
        if !name.starts_with("Bra") {
            continue;
        }
        let ours = b.list(name).unwrap_or(&[]);
        // The same moves, in whatever order the file lists them.
        let low = |d: &[osd::Diff]| {
            let mut kept: Vec<osd::Diff> = d.iter().filter(|x| x.vertex < 50).copied().collect();
            kept.sort_by_key(|x| x.vertex);
            kept
        };
        assert_eq!(low(diffs), low(ours), "{name}");
        compared += 1;
    }
    assert!(compared > 20 && a != b);
    assert!(!shipped.sliders.is_empty());
}

#[test]
fn a_shape_is_mirrored_and_its_triangles_moved_between_parts() {
    use apoc_bodyslide::project::Project;
    use apoc_nif::{mesh, weights, Nif};

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let set = sets.iter().find(|s| s.name == "CBBE Body").unwrap();
    let mut project = Project::open(set, &on_disk).unwrap();
    let name = set.shapes[0].name.clone();
    let block = |nif: &Nif| {
        mesh::shapes(nif)
            .into_iter()
            .find(|at| mesh::name(nif, *at).as_deref() == Some(name.as_str()))
            .unwrap()
    };
    let was = mesh::read(&project.nif, block(&project.nif)).unwrap();
    let was_skin = weights::read(&project.nif, block(&project.nif)).unwrap();
    let was_parts = weights::parts(&project.nif, block(&project.nif)).unwrap();
    assert_eq!(was_parts.len(), 3);

    // The vertices of the first hundred triangles of the largest part, to
    // the first part, and then to a part of their own.
    let largest = (0..3)
        .max_by_key(|i| was_parts[*i].triangles.len())
        .unwrap();
    let other = (largest + 1) % 3;
    let marked: Vec<u16> = was_parts[largest].triangles[..100]
        .iter()
        .flatten()
        .copied()
        .collect();
    let moved = project
        .move_to_part(&name, &marked, Some(other), 0)
        .unwrap();
    assert!(moved >= 100);
    let nif = Nif::read(&project.nif.write()).unwrap();
    let now_parts = weights::parts(&nif, block(&nif)).unwrap();
    assert_eq!(
        now_parts[other].triangles.len(),
        was_parts[other].triangles.len() + moved
    );
    assert_eq!(
        now_parts[largest].triangles.len(),
        was_parts[largest].triangles.len() - moved
    );
    let total = |p: &[weights::Part]| p.iter().map(|x| x.triangles.len()).sum::<usize>();
    assert_eq!(total(&now_parts), total(&was_parts));
    // The shape and its skin are what they were.
    assert_eq!(
        mesh::read(&nif, block(&nif)).unwrap().positions,
        was.positions
    );
    // Written again its weights add up to one, which the file's did not
    // quite: the same bones, each within a hair of what it was.
    let near = |a: &weights::Skin, b: &weights::Skin| {
        a.vertices.iter().zip(&b.vertices).all(|(x, y)| {
            x.iter().filter(|s| s.1 > 0.01).all(|(bone, w)| {
                y.iter()
                    .any(|(other, v)| other == bone && (w - v).abs() < 0.01)
            })
        })
    };
    let kept = weights::read(&nif, block(&nif)).unwrap();
    assert_eq!(kept.bones, was_skin.bones);
    assert!(near(&was_skin, &kept));
    let again = project.move_to_part(&name, &marked, None, 44).unwrap();
    assert_eq!(again, moved);
    let nif = Nif::read(&project.nif.write()).unwrap();
    let four = weights::parts(&nif, block(&nif)).unwrap();
    assert_eq!(
        (four.len(), four[3].part, four[3].triangles.len()),
        (4, 44, moved)
    );
    // The new part taken out again, its triangles back where they came from.
    let mut fewer = project.clone();
    fewer.remove_part(&name, 3, largest).unwrap();
    assert!(fewer.remove_part(&name, 3, 0).is_err());
    assert!(fewer.remove_part(&name, 1, 1).is_err());
    let nif = Nif::read(&fewer.nif.write()).unwrap();
    let three = weights::parts(&nif, block(&nif)).unwrap();
    assert_eq!(three.len(), 3);
    assert_eq!(total(&three), total(&was_parts));
    assert_eq!(
        three[largest].triangles.len(),
        was_parts[largest].triangles.len()
    );

    // Nothing marked, or a whole part marked, is refused and changes nothing.
    assert!(project.move_to_part(&name, &[], Some(0), 0).is_err());
    let all: Vec<u16> = (0..was.positions.len() as u16).collect();
    assert!(project.move_to_part(&name, &all, Some(0), 0).is_err());
    assert_eq!(
        weights::parts(&project.nif, block(&project.nif)).unwrap(),
        four
    );

    // Mirrored: every vertex across, every triangle turned, each bone its twin.
    let mut mirrored = Project::open(set, &on_disk).unwrap();
    mirrored.mirror_shape(&name).unwrap();
    let nif = Nif::read(&mirrored.nif.write()).unwrap();
    let now = mesh::read(&nif, block(&nif)).unwrap();
    assert!(was
        .positions
        .iter()
        .zip(&now.positions)
        .all(|(a, b)| (a[0] + b[0]).abs() < 0.01 && a[1] == b[1] && a[2] == b[2]));
    let turned = |t: &[u16; 3]| {
        let mut t = *t;
        t.sort_unstable();
        t
    };
    let mut a: Vec<[u16; 3]> = was.triangles.iter().map(turned).collect();
    let mut b: Vec<[u16; 3]> = now.triangles.iter().map(turned).collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
    assert!(was
        .triangles
        .iter()
        .zip(&now.triangles)
        .any(|(x, y)| x != y));
    let now_skin = weights::read(&nif, block(&nif)).unwrap();
    let left = was_skin
        .bones
        .iter()
        .position(|b| b.contains(" L Thigh"))
        .unwrap();
    assert!(now_skin.bones[left].contains(" R Thigh"));
    assert!(near(&was_skin, &now_skin));
    // Twice is back where it began.
    mirrored.mirror_shape(&name).unwrap();
    let twice = mesh::read(&mirrored.nif, block(&mirrored.nif)).unwrap();
    assert!(twice
        .positions
        .iter()
        .zip(&was.positions)
        .all(|(a, b)| (0..3).all(|k| (a[k] - b[k]).abs() < 0.01)));
}

#[test]
fn a_bone_brought_in_from_a_skeleton_is_placed_as_a_skin_that_has_it_places_it() {
    use apoc_bodyslide::project::Project;
    use apoc_nif::geometry::TriShape;
    use apoc_nif::skin::{SkinData, SkinInstance};
    use apoc_nif::{mesh, weights, Nif};

    let cbbe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/cbbe");
    let body = cbbe.join("@required/Data/CalienteTools/BodySlide/ShapeData/CBBE/CBBE Body.nif");
    let gloves = cbbe.join(
        "@conditional-3/Data/CalienteTools/BodySlide/ShapeData/CBBE Dawnguard/1st Person/Dawnguard Gauntlets.nif",
    );
    let (Ok(body), Ok(gloves)) = (std::fs::read(body), std::fs::read(gloves)) else {
        return;
    };
    // Where a skin has a bone: its transform, by the bone's name.
    let placed = |nif: &Nif, shape: usize, bone: &str| {
        let stream = nif.header.stream.version;
        let s = TriShape::read(&nif.blocks[shape].data, stream).unwrap();
        let skin = s.skin as usize;
        let instance = SkinInstance::read(
            &nif.blocks[skin].data,
            nif.kind_of(skin) != "NiSkinInstance",
        )
        .unwrap();
        let data = SkinData::read(&nif.blocks[instance.data as usize].data).unwrap();
        let at = mesh::bone_names(nif, shape)
            .iter()
            .position(|b| b == bone)
            .unwrap();
        data.bones[at].transform
    };
    let skeleton = Nif::read(&body).unwrap();
    let whole = mesh::shapes(&skeleton)[0];
    let mut project = Project::from_mesh("Gloves", "meshes/gloves_1.nif", &gloves).unwrap();
    let shape = project.set.shapes[0].name.clone();
    let block = |p: &Project| {
        mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| mesh::name(&p.nif, *at).as_deref() == Some(shape.as_str()))
            .unwrap()
    };
    // A bone of the body's that the gloves' mesh does not have at all.
    let have = apoc_nif::copy::bone_names(&project.nif);
    let bone = mesh::bone_names(&skeleton, whole)
        .into_iter()
        .find(|b| !have.contains(b))
        .expect("a bone the gloves lack");
    assert!(project.paint_weights(&shape, &bone, &[(0, 0.5)]).is_err());
    project.add_bone(&bone, &body).unwrap();
    project.add_bone(&bone, &body).unwrap();
    assert_eq!(
        apoc_nif::copy::bone_names(&project.nif)
            .iter()
            .filter(|b| **b == bone)
            .count(),
        1
    );
    project.paint_weights(&shape, &bone, &[(0, 0.5)]).unwrap();
    let nif = Nif::read(&project.nif.write()).unwrap();
    assert!(
        (weights::read(&nif, block(&project)).unwrap().vertices[0]
            .iter()
            .map(|x| x.1)
            .sum::<f32>()
            - 1.0)
            .abs()
            < 0.01
    );
    let (ours, theirs) = (
        placed(&nif, block(&project), &bone),
        placed(&skeleton, whole, &bone),
    );
    for (a, b) in ours.translation.iter().zip(&theirs.translation) {
        assert!(
            (a - b).abs() < 0.05,
            "{bone}: {ours:?} and the body's {theirs:?}"
        );
    }
    for (a, b) in ours.rotation.iter().zip(&theirs.rotation) {
        assert!(
            (a - b).abs() < 0.005,
            "{bone}: {ours:?} and the body's {theirs:?}"
        );
    }
    assert!(project.add_bone("No such bone", &body).is_err());
}

#[test]
fn two_shapes_merged_are_one_shape_that_builds_as_the_two_did() {
    use apoc_bodyslide::project::Project;
    use apoc_bodyslide::{build, morph};
    use apoc_nif::{mesh, weights, Nif};
    use std::collections::HashMap;

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let set = sets.iter().find(|s| s.name == "CBBE NeverNude").unwrap();
    let project = Project::open(set, &on_disk).unwrap();
    let built = |p: &Project| -> Vec<mesh::Mesh> {
        let (saved, files) = p.save("Merged").unwrap();
        let held: HashMap<String, Vec<u8>> = files.into_iter().map(|o| (o.path, o.bytes)).collect();
        let files = |path: &str| held.get(path).cloned().or_else(|| on_disk(path));
        let values = morph::values(&saved, None, true);
        let out = build::build_at(&saved, &values, &values, &files).unwrap();
        assert_eq!(out.notes, Vec::<String>::new());
        let nif = Nif::read(&out.outputs[0].bytes).unwrap();
        mesh::shapes(&nif)
            .into_iter()
            .map(|at| mesh::read(&nif, at).unwrap())
            .collect()
    };
    let two = built(&project);
    let of = |all: &[mesh::Mesh], name: &str| all.iter().find(|m| m.name == name).unwrap().clone();
    let (bra, panty) = (of(&two, "Bra"), of(&two, "Panty"));

    let mut merged = project.clone();
    // The body keeps other things for each vertex, and its sliders are another project's.
    assert!(merged.merge_shapes("Bra", "CBBE").is_err());
    assert!(merged.merge_shapes("Bra", "Bra").is_err());
    assert_eq!(merged.set.shapes.len(), 3);
    merged.merge_shapes("Bra", "Panty").unwrap();
    assert_eq!(merged.set.shapes.len(), 2);
    let one = built(&merged);
    assert_eq!(one.len(), 2);
    let both = of(&one, "Bra");
    // The bra's vertices, then the panty's, each where the sliders put it
    // when they were two; and every triangle of both.
    let mut want = bra.positions.clone();
    want.extend(&panty.positions);
    assert_eq!(both.positions.len(), want.len());
    for (a, b) in both.positions.iter().zip(&want) {
        assert!((0..3).all(|k| (a[k] - b[k]).abs() < 0.01), "{a:?} {b:?}");
    }
    assert_eq!(
        both.triangles.len(),
        bra.triangles.len() + panty.triangles.len()
    );
    let n = bra.positions.len() as u16;
    assert_eq!(&both.triangles[..bra.triangles.len()], &bra.triangles[..]);
    assert!(both.triangles[bra.triangles.len()..]
        .iter()
        .zip(&panty.triangles)
        .all(|(a, b)| *a == b.map(|v| v + n)));
    // Its skin: every bone either had, each vertex following what it did.
    let nif = Nif::read(&merged.nif.write()).unwrap();
    let at = mesh::shapes(&nif)
        .into_iter()
        .find(|at| mesh::name(&nif, *at).as_deref() == Some("Bra"))
        .unwrap();
    let skin = weights::read(&nif, at).unwrap();
    let skin_of = |name: &str| {
        let at = mesh::shapes(&project.nif)
            .into_iter()
            .find(|at| mesh::name(&project.nif, *at).as_deref() == Some(name))
            .unwrap();
        weights::read(&project.nif, at).unwrap()
    };
    let (sa, sb) = (skin_of("Bra"), skin_of("Panty"));
    assert!(sa
        .bones
        .iter()
        .chain(&sb.bones)
        .all(|b| skin.bones.contains(b)));
    let heaviest = |s: &weights::Skin, v: usize| s.bones[s.vertices[v][0].0 as usize].clone();
    for v in (0..sb.vertices.len()).step_by(37) {
        assert_eq!(heaviest(&skin, sa.vertices.len() + v), heaviest(&sb, v));
    }
    assert_eq!(weights::parts(&nif, at).unwrap().len(), 2);
}

#[test]
fn a_shape_is_refined_and_a_shape_is_made_of_an_outside_mesh() {
    use apoc_bodyslide::project::Project;
    use apoc_bodyslide::{build, morph};
    use apoc_nif::{mesh, weights, Nif};
    use std::collections::HashMap;

    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/cbbe/@required/Data/CalienteTools/BodySlide");
    let on_disk = |path: &str| std::fs::read(tools.join(path)).ok();
    let Some(sets) = on_disk("SliderSets/CBBE.osp").and_then(|b| set::read(&b).ok()) else {
        return;
    };
    let set = sets.iter().find(|s| s.name == "CBBE NeverNude").unwrap();
    let project = Project::open(set, &on_disk).unwrap();
    let shape = |p: &Project, name: &str| {
        let nif = Nif::read(&p.nif.write()).unwrap();
        let at = mesh::shapes(&nif)
            .into_iter()
            .find(|at| mesh::name(&nif, *at).as_deref() == Some(name))
            .unwrap();
        (
            mesh::read(&nif, at).unwrap(),
            weights::read(&nif, at).unwrap(),
        )
    };
    let built = |p: &Project, name: &str| {
        let (saved, files) = p.save("Refined").unwrap();
        let held: HashMap<String, Vec<u8>> = files.into_iter().map(|o| (o.path, o.bytes)).collect();
        let files = |path: &str| held.get(path).cloned().or_else(|| on_disk(path));
        let values = morph::values(&saved, None, true);
        let out = build::build_at(&saved, &values, &values, &files).unwrap();
        assert_eq!(out.notes, Vec::<String>::new());
        let nif = Nif::read(&out.outputs[0].bytes).unwrap();
        mesh::shapes(&nif)
            .into_iter()
            .map(|at| mesh::read(&nif, at).unwrap())
            .find(|m| m.name == name)
            .unwrap()
    };

    // Refined: four triangles for each, the old vertices where they were,
    // each new one halfway between two of them, in place and when built.
    let (was, was_skin) = shape(&project, "Bra");
    let was_built = built(&project, "Bra");
    let mut refined = project.clone();
    let added = refined.refine("Bra", &[]).unwrap();
    assert!(refined.refine("CBBE", &[]).is_err());
    let (now, now_skin) = shape(&refined, "Bra");
    assert_eq!(now.triangles.len(), was.triangles.len() * 4);
    assert_eq!(now.positions.len(), was.positions.len() + added);
    assert_eq!(&now.positions[..was.positions.len()], &was.positions[..]);
    assert_eq!(&now.uvs[..was.uvs.len()], &was.uvs[..]);
    assert_eq!(now_skin.bones, was_skin.bones);
    for v in &now_skin.vertices {
        assert!((v.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 0.01);
    }
    let now_built = built(&refined, "Bra");
    // Every new vertex of the first new triangle's middle lies between two old ones.
    let t = now.triangles[3];
    for v in t {
        assert!(v as usize >= was.positions.len());
    }
    let first = was.triangles[0];
    let mid = |m: &mesh::Mesh, a: u16, b: u16| {
        let (p, q) = (m.positions[a as usize], m.positions[b as usize]);
        [
            (p[0] + q[0]) / 2.0,
            (p[1] + q[1]) / 2.0,
            (p[2] + q[2]) / 2.0,
        ]
    };
    for (corner, (a, b)) in [
        (first[0], first[1]),
        (first[1], first[2]),
        (first[2], first[0]),
    ]
    .into_iter()
    .enumerate()
    {
        for (flat, wanted) in [(&now, mid(&was, a, b)), (&now_built, mid(&was_built, a, b))] {
            let got = flat.positions[t[corner] as usize];
            assert!(
                (0..3).all(|k| (got[k] - wanted[k]).abs() < 0.02),
                "{got:?} {wanted:?}"
            );
        }
    }

    // Refined in part: the left half spared. Its vertices keep every
    // triangle they had, a triangle with one spared corner becomes two,
    // and no halved edge is left whole on its other side.
    let spared: Vec<u16> = (0..was.positions.len() as u16)
        .filter(|v| was.positions[*v as usize][0] < 0.0)
        .collect();
    let is_spared = |v: u16| spared.binary_search(&v).is_ok();
    let mut part = project.clone();
    let part_added = part.refine("Bra", &spared).unwrap();
    assert!(part_added > 0 && part_added < added);
    let (half, half_skin) = shape(&part, "Bra");
    assert_eq!(half.positions.len(), was.positions.len() + part_added);
    assert_eq!(&half.positions[..was.positions.len()], &was.positions[..]);
    let expected: usize = was
        .triangles
        .iter()
        .map(|t| match t.iter().filter(|v| is_spared(**v)).count() {
            0 => 4,
            1 => 2,
            _ => 1,
        })
        .sum();
    assert_eq!(half.triangles.len(), expected);
    let edges: std::collections::HashSet<(u16, u16)> = half
        .triangles
        .iter()
        .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    for t in &was.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let halved = !is_spared(a) && !is_spared(b);
            assert_eq!(edges.contains(&(a.min(b), a.max(b))), !halved);
        }
    }
    for v in &half_skin.vertices {
        assert!((v.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 0.01);
    }
    built(&part, "Bra");

    // A shape made of an outside mesh, here the panty turned into one:
    // its vertices and triangles as given, drawn and boned as its likeness.
    let (panty, panty_skin) = shape(&project, "Panty");
    let mut made = project.clone();
    made.shape_from(
        "Bra",
        "From outside",
        &panty.positions,
        &panty.uvs,
        &panty.triangles,
        None,
    )
    .unwrap();
    assert!(made
        .shape_from(
            "Bra",
            "From outside",
            &panty.positions,
            &panty.uvs,
            &panty.triangles,
            None,
        )
        .is_err());
    assert!(made
        .shape_from("Bra", "Empty", &[], &[], &[], None)
        .is_err());

    // With what its file says each vertex follows: a bone of the mesh's is
    // taken by its name, whether its likeness follows it or not; one the
    // mesh has not got is left out, and a vertex left with nothing follows
    // what its likeness does.
    let names = vec![
        "NPC Pelvis [Pelv]".to_string(),
        "No such bone".to_string(),
        "NPC L Thigh [LThg]".to_string(),
    ];
    let follows: Vec<[(u32, f32); 4]> = (0..panty.positions.len())
        .map(|v| match v % 3 {
            0 => [(0, 0.75), (2, 0.25), (0, 0.0), (0, 0.0)],
            1 => [(1, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)],
            _ => [(1, 0.5), (2, 0.5), (0, 0.0), (0, 0.0)],
        })
        .collect();
    // The bone the mesh has not got is made where the file stands it, when
    // the file stands it anywhere.
    let stands = vec![
        None,
        Some((
            [1.0, 2.0, 3.0],
            [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        )),
        None,
    ];
    use apoc_bodyslide::project::Followed;
    let mut rigged = project.clone();
    assert!(rigged
        .shape_from(
            "Bra",
            "Rigged",
            &panty.positions,
            &panty.uvs,
            &panty.triangles,
            Some(Followed {
                bones: &names,
                follows: &follows[..1],
                stands: &[],
            }),
        )
        .is_err());
    rigged
        .shape_from(
            "Bra",
            "Rigged",
            &panty.positions,
            &panty.uvs,
            &panty.triangles,
            Some(Followed {
                bones: &names,
                follows: &follows,
                stands: &stands,
            }),
        )
        .unwrap();
    let (_, plain_skin) = shape(&made, "From outside");
    let (_, rigged_skin) = shape(&rigged, "Rigged");
    let bone = |s: &apoc_nif::weights::Skin, slot: (u16, f32)| s.bones[slot.0 as usize].clone();
    assert!(rigged_skin.bones.contains(&names[1]));
    let made_up = apoc_nif::copy::bone_names(&rigged.nif);
    assert!(made_up.contains(&names[1]));
    // Its bones are those something follows, and no others of its likeness.
    for (i, name) in rigged_skin.bones.iter().enumerate() {
        assert!(
            rigged_skin
                .vertices
                .iter()
                .flatten()
                .any(|x| x.1 > 0.0 && x.0 as usize == i),
            "{name}"
        );
    }
    for (v, slots) in rigged_skin.vertices.iter().enumerate() {
        let mut used: Vec<(String, f32)> = slots
            .iter()
            .filter(|x| x.1 > 0.0)
            .map(|x| (bone(&rigged_skin, *x), x.1))
            .collect();
        used.sort_by(|a, b| a.0.cmp(&b.0));
        match v % 3 {
            0 => {
                assert_eq!(used.len(), 2);
                assert_eq!(used[1].0, names[0]);
                assert!((used[1].1 - 0.75).abs() < 0.01 && (used[0].1 - 0.25).abs() < 0.01);
            }
            1 => assert_eq!(used, vec![(names[1].clone(), 1.0)]),
            _ => {
                assert_eq!(used.len(), 2);
                assert!(used.iter().all(|x| (x.1 - 0.5).abs() < 0.01));
            }
        }
    }
    // Without the file standing it anywhere it is left out, and a vertex
    // left with nothing follows what its likeness does.
    let mut left = project.clone();
    left.shape_from(
        "Bra",
        "Rigged",
        &panty.positions,
        &panty.uvs,
        &panty.triangles,
        Some(Followed {
            bones: &names,
            follows: &follows,
            stands: &[],
        }),
    )
    .unwrap();
    let (_, left_skin) = shape(&left, "Rigged");
    assert!(!left_skin.bones.contains(&names[1]));
    let named = |s: &apoc_nif::weights::Skin, v: usize| -> Vec<(String, f32)> {
        s.vertices[v]
            .iter()
            .filter(|x| x.1 > 0.0)
            .map(|x| (bone(s, *x), x.1))
            .collect()
    };
    assert_eq!(named(&left_skin, 1), named(&plain_skin, 1));
    assert_eq!(named(&left_skin, 2), vec![(names[2].clone(), 1.0)]);
    // How a shape takes light: changed, read back, and nothing else of the
    // file with it.
    let lit = |p: &Project| {
        let at = mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| mesh::name(&p.nif, *at).as_deref() == Some("Bra"))
            .unwrap();
        mesh::look(&p.nif, at).unwrap()
    };
    let mut look = lit(&project);
    assert!((0.0..=1.0).contains(&look.alpha) && look.gloss > 0.0);
    look.glow = [1.0, 0.5, 0.25];
    look.glow_strength = 2.0;
    look.gloss = 80.0;
    // Switches are changed, but for the two that say what the shape is.
    let kept = [look.switches[0] & (1 << 1), look.switches[1] & (1 << 5)];
    look.switches = [
        look.switches[0] ^ ((1 << 1) | (1 << 9)),
        look.switches[1] ^ ((1 << 5) | (1 << 4)),
    ];
    let mut glowing = project.clone();
    glowing.set_look("Bra", &look).unwrap();
    let got = lit(&glowing);
    assert_eq!(
        [got.switches[0] & (1 << 1), got.switches[1] & (1 << 5)],
        kept
    );
    look.switches = got.switches;
    assert_eq!(got, look);
    let untouched = lit(&project).switches;
    assert_eq!(
        (
            got.switches[0] ^ untouched[0],
            got.switches[1] ^ untouched[1]
        ),
        (1 << 9, 1 << 4)
    );
    let changed = glowing
        .nif
        .blocks
        .iter()
        .zip(&project.nif.blocks)
        .filter(|(a, b)| a.data != b.data)
        .count();
    assert_eq!(changed, 1);
    look.alpha = 2000.0;
    assert!(glowing.set_look("Bra", &look).is_err());
    assert!(boned_name_refused(&project));
    built(&glowing, "Bra");

    // Colours for a shape that carries none: every vertex white, the shape
    // otherwise as it was, built as before but for them.
    let coloured = |p: &Project, name: &str| {
        let at = mesh::shapes(&p.nif)
            .into_iter()
            .find(|at| mesh::name(&p.nif, *at).as_deref() == Some(name))
            .unwrap();
        mesh::colors(&p.nif, at).unwrap()
    };
    if coloured(&project, "Bra").is_empty() {
        let mut tinted = project.clone();
        tinted.add_colors("Bra").unwrap();
        let (now, now_skin) = shape(&tinted, "Bra");
        assert_eq!(
            coloured(&tinted, "Bra"),
            vec![[255u8; 4]; now.positions.len()]
        );
        assert_eq!(
            (&now.positions, &now.uvs, &now.triangles),
            (&was.positions, &was.uvs, &was.triangles)
        );
        assert_eq!(now_skin, was_skin);
        // Every place the file says what a vertex carries says the same.
        for at in 0..tinted.nif.blocks.len() {
            if tinted.nif.kind_of(at) != "NiSkinPartition" {
                continue;
            }
            let p = apoc_nif::skin::SkinPartition::read(&tinted.nif.blocks[at].data, 100).unwrap();
            for part in &p.partitions {
                assert_eq!(part.vertex_desc, p.vertex_desc);
            }
        }
        tinted
            .paint_colors("Bra", &[(3, [10, 20, 30, 40])])
            .unwrap();
        assert_eq!(coloured(&tinted, "Bra")[3], [10, 20, 30, 40]);
        assert_eq!(built(&tinted, "Bra").positions, was_built.positions);
        // Asked again, nothing more is done.
        let again = tinted.clone();
        tinted.add_colors("Bra").unwrap();
        assert_eq!(tinted.nif.write(), again.nif.write());
    }

    // Parted from its bones: the shape as it was but for them, built as it
    // was, and the file without its skin.
    let mut loose = project.clone();
    loose.remove_skinning("Bra").unwrap();
    let at = mesh::shapes(&loose.nif)
        .into_iter()
        .find(|at| mesh::name(&loose.nif, *at).as_deref() == Some("Bra"))
        .unwrap();
    let now = mesh::read(&loose.nif, at).unwrap();
    assert!(!now.skinned);
    assert_eq!((&now.positions, &now.uvs), (&was.positions, &was.uvs));
    assert_eq!(now.triangles.len(), was.triangles.len());
    assert_eq!(loose.nif.blocks.len(), project.nif.blocks.len() - 3);
    assert!(loose.remove_skinning("Bra").is_err());
    assert_eq!(built(&loose, "Bra").positions, was_built.positions);

    // A bone made up by hand: once, and where it was put.
    let mut boned = project.clone();
    boned
        .new_bone(
            "Tail",
            [0.0, -5.0, 60.0],
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        )
        .unwrap();
    assert!(boned
        .new_bone(
            "Tail",
            [0.0; 3],
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        )
        .is_err());
    assert!(boned
        .new_bone(
            "Bent",
            [0.0; 3],
            [2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        )
        .is_err());
    assert!(boned
        .new_bone(" ", [0.0; 3], [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
        .is_err());
    boned.paint_weights("Bra", "Tail", &[(0, 1.0)]).unwrap();
    let (_, tailed) = shape(&boned, "Bra");
    assert!(tailed.bones.iter().any(|b| b == "Tail"));
    built(&boned, "Bra");
    built(&rigged, "Rigged");
    let (got, got_skin) = shape(&made, "From outside");
    assert_eq!(got.triangles, panty.triangles);
    assert_eq!(got.positions.len(), panty.positions.len());
    for (a, b) in got.positions.iter().zip(&panty.positions) {
        assert!((0..3).all(|k| (a[k] - b[k]).abs() < 0.01));
    }
    for (a, b) in got.uvs.iter().zip(&panty.uvs) {
        assert!((a[0] - b[0]).abs() < 0.002 && (a[1] - b[1]).abs() < 0.002);
    }
    assert_eq!(got.textures, shape(&project, "Bra").0.textures);
    assert_eq!(got_skin.vertices.len(), panty_skin.vertices.len());
    assert_eq!(
        built(&made, "From outside").positions.len(),
        panty.positions.len()
    );
    assert_eq!(shape(&made, "Bra").0.positions, was.positions);

    // One edge of the bra, split, flipped and collapsed: the shape builds
    // each time, and its vertices and triangles count as they should.
    use apoc_nif::weights::EdgeOp;
    let mut edged = project.clone();
    let t = was.triangles[0];
    let (a, b) = (t[0], t[1]);
    let on = |m: &mesh::Mesh| {
        m.triangles
            .iter()
            .filter(|t| t.contains(&a) && t.contains(&b))
            .count()
    };
    let shared = on(&was);
    edged.edge("Bra", a, b, EdgeOp::Split).unwrap();
    let (split, split_skin) = shape(&edged, "Bra");
    assert_eq!(split.positions.len(), was.positions.len() + 1);
    assert_eq!(split.triangles.len(), was.triangles.len() + shared);
    assert_eq!(split_skin.vertices.len(), split.positions.len());
    let new = was.positions.len();
    let (p, q) = (was.positions[a as usize], was.positions[b as usize]);
    assert!((0..3).all(|k| (split.positions[new][k] - (p[k] + q[k]) / 2.0).abs() < 0.02));
    assert_eq!(built(&edged, "Bra").positions.len(), split.positions.len());
    // The new vertex follows a slider by half of what each end does.
    let breasts = osd::SliderData::read(&edged.slider_data("Breasts").unwrap()).unwrap();
    let does = |v: u16| {
        breasts
            .list("BraBreasts")
            .and_then(|l| l.iter().find(|d| d.vertex == v).map(|d| d.offset))
            .unwrap_or([0.0; 3])
    };
    let (da, db, dn) = (does(a), does(b), does(new as u16));
    assert!((0..3).all(|k| (dn[k] - (da[k] + db[k]) / 2.0).abs() < 0.0001));
    if shared == 2 {
        edged.edge("Bra", a, new as u16, EdgeOp::Flip).ok();
        assert_eq!(
            shape(&edged, "Bra").0.triangles.len(),
            split.triangles.len()
        );
    }
    edged.edge("Bra", b, new as u16, EdgeOp::Collapse).unwrap();
    let (collapsed, _) = shape(&edged, "Bra");
    assert_eq!(collapsed.positions.len(), was.positions.len());
    assert!(collapsed.triangles.len() < split.triangles.len());
    assert_eq!(built(&edged, "Bra").positions.len(), was.positions.len());
    assert!(edged.edge("Bra", 0, 0, EdgeOp::Split).is_err());
    assert!(edged.edge("Bra", 0, 60000, EdgeOp::Split).is_err());
    assert!(edged.edge("CBBE", 0, 1, EdgeOp::Split).is_err());

    // A mesh with every vertex doubled, welded: one of each again, the
    // triangles the same, and nothing to weld the second time.
    let n = panty.positions.len();
    let mut doubled = panty.positions.clone();
    doubled.extend(&panty.positions);
    let mut doubled_uv = panty.uvs.clone();
    doubled_uv.extend(&panty.uvs);
    // Every other triangle on the second copy of its vertices.
    let split: Vec<[u16; 3]> = panty
        .triangles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if i % 2 == 0 {
                *t
            } else {
                t.map(|v| v + n as u16)
            }
        })
        .collect();
    let mut welded = project.clone();
    welded
        .shape_from("Bra", "Doubled", &doubled, &doubled_uv, &split, None)
        .unwrap();
    assert_eq!(shape(&welded, "Doubled").0.positions.len(), n * 2);
    let gone = welded.weld("Doubled").unwrap();
    let (one, one_skin) = shape(&welded, "Doubled");
    assert_eq!(one.positions.len() + gone, n * 2);
    assert!(one.positions.len() <= n);
    assert_eq!(one.triangles.len(), panty.triangles.len());
    assert_eq!(one_skin.vertices.len(), one.positions.len());
    assert_eq!(welded.weld("Doubled").unwrap(), 0);
    assert_eq!(
        built(&welded, "Doubled").positions.len(),
        one.positions.len()
    );
}

#[test]
fn a_mesh_that_follows_no_bones_takes_the_changes_a_skinned_one_does() {
    use apoc_bodyslide::project::Project;
    use apoc_nif::{mesh, Nif};
    // One of the game's own, with what says how it collides after its shapes.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch/samples/statics/meshes/weapons/steel/steelmace.nif");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let project = Project::from_mesh("Mace", "meshes/mace.nif", &bytes).unwrap();
    let name = project.set.shapes[0].name.clone();
    let read = |p: &Project, name: &str| {
        let nif = Nif::read(&p.nif.write()).unwrap();
        let at = mesh::shapes(&nif)
            .into_iter()
            .find(|at| mesh::name(&nif, *at).as_deref() == Some(name))
            .unwrap();
        mesh::read(&nif, at).unwrap()
    };
    let was = read(&project, &name);

    let mut refined = project.clone();
    let added = refined.refine(&name, &[]).unwrap();
    let now = read(&refined, &name);
    assert_eq!(now.positions.len(), was.positions.len() + added);
    assert_eq!(now.triangles.len(), was.triangles.len() * 4);
    assert_eq!(&now.positions[..was.positions.len()], &was.positions[..]);

    let mut welded = project.clone();
    welded.weld(&name).unwrap();
    assert!(read(&welded, &name).positions.len() <= was.positions.len());

    // A copy made and merged back: twice the shape, and every other block
    // of the file as it was, what says how it collides among them.
    let mut merged = project.clone();
    merged.duplicate_shape(&name, "Copy").unwrap();
    merged.merge_shapes(&name, "Copy").unwrap();
    let both = read(&merged, &name);
    assert_eq!(both.positions.len(), was.positions.len() * 2);
    assert_eq!(both.triangles.len(), was.triangles.len() * 2);
    let kinds = |nif: &Nif| -> Vec<String> {
        (0..nif.blocks.len())
            .map(|i| nif.kind_of(i).to_string())
            .filter(|k| k.starts_with("bhk"))
            .collect()
    };
    assert!(!kinds(&project.nif).is_empty());
    assert_eq!(kinds(&merged.nif), kinds(&project.nif));

    let mut made = project.clone();
    made.shape_from(
        &name,
        "From outside",
        &was.positions,
        &was.uvs,
        &was.triangles,
        None,
    )
    .unwrap();
    assert_eq!(
        read(&made, "From outside").positions.len(),
        was.positions.len()
    );
}

/// A bone's name is letters a file can hold: none that end it early.
fn boned_name_refused(project: &apoc_bodyslide::project::Project) -> bool {
    let mut p = project.clone();
    let unturned = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    p.new_bone("Review\0hidden", [0.0; 3], unturned).is_err()
}
