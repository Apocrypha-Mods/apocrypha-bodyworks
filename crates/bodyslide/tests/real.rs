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
