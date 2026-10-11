//! Real projects, when there are any in `scratch/samples` (not tracked;
//! skipped when empty).

use apoc_bodyslide::{group, osd, preset, set};
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
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples"), &mut all);
    all.retain(|p| {
        p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext))
            && under.is_none_or(|u| p.to_string_lossy().to_lowercase().contains(&u.to_lowercase()))
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
            assert!(!s.name.is_empty() && !s.source_file.is_empty(), "{}", path.display());
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
        let data = osd::SliderData::read(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!data.lists.is_empty());
        assert_eq!(data.write(), bytes, "{}", path.display());
    }
}

#[test]
fn every_real_preset_and_group_file_round_trips() {
    for path in files("xml", Some("SliderPresets")) {
        let presets = preset::read(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(preset::read(preset::write(&presets).as_bytes()).unwrap(), presets);
        assert!(presets.iter().all(|p| !p.values.is_empty()), "{}", path.display());
    }
    for path in files("xml", Some("SliderGroups")) {
        let groups = group::read(&std::fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(group::read(group::write(&groups).as_bytes()).unwrap(), groups);
    }
}
