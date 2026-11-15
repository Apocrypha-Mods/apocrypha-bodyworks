//! Real presets: every `.jslot` under `scratch/samples/jslot`, when there
//! are any (they are not in the repository; skipped when there are none).
//! These check properties, never values read from the presets.

use apoc_jslot::{parse, write, Preset};
use std::path::{Path, PathBuf};

fn presets() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("jslot"))
            {
                out.push(path);
            }
        }
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples/jslot");
    let mut all = Vec::new();
    walk(&dir, &mut all);
    if all.is_empty() {
        eprintln!("skipped: no presets under {}", dir.display());
    }
    all.sort();
    all
}

/// Where two files first differ, by line, for a failure message.
fn first_difference(read: &[u8], written: &[u8]) -> String {
    let mut a = read.split(|&b| b == b'\n');
    let mut b = written.split(|&b| b == b'\n');
    for line in 1.. {
        match (a.next(), b.next()) {
            (None, None) => break,
            (x, y) if x == y => {}
            (x, y) => {
                let show = |l: Option<&[u8]>| {
                    l.map_or("(the end)".to_string(), |l| {
                        format!("{:?}", String::from_utf8_lossy(l))
                    })
                };
                return format!("line {line}: read {}, written {}", show(x), show(y));
            }
        }
    }
    "the same lines".to_string()
}

#[test]
fn every_real_preset_writes_back_byte_for_byte() {
    for path in presets() {
        let bytes = std::fs::read(&path).unwrap();
        let doc = parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let written = write(&doc).unwrap();
        assert!(
            written == bytes,
            "{}: {}",
            path.display(),
            first_difference(&bytes, &written)
        );
    }
}

#[test]
fn every_real_preset_has_the_fields_a_preset_has() {
    for path in presets() {
        let doc = parse(&std::fs::read(&path).unwrap()).unwrap();
        let preset = Preset::of(&doc);
        let problems: Vec<String> = preset.problems().iter().map(|m| m.to_string()).collect();
        assert!(problems.is_empty(), "{}: {problems:?}", path.display());
    }
}
