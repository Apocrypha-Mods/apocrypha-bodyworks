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
