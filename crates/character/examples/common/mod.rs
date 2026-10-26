//! What the examples share: the load order and the unpacked game files.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use apoc_plugin::native::read_file;
use apoc_plugin::LoadOrder;

pub const WANTED: [[u8; 4]; 9] = [
    *b"RACE", *b"HDPT", *b"TXST", *b"ARMO", *b"ARMA", *b"CLFM", *b"NPC_", *b"LVLN", *b"KYWD",
];

pub const ORDER: [&str; 6] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
    "unofficial skyrim special edition patch.esp",
];

pub fn data_folder() -> PathBuf {
    std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(format!(
                "{}/.local/share/Steam/steamapps/common/Skyrim Special Edition/Data",
                std::env::var("HOME").unwrap_or_default()
            ))
        })
}

pub fn load_order() -> LoadOrder {
    let data = data_folder();
    let plugins = ORDER
        .iter()
        .map(|n| read_file(&data.join(n), &WANTED).expect(n).0)
        .collect();
    LoadOrder::new(plugins)
}

/// The unpacked game files: vanilla, then USSEP's replacements over them.
pub fn sample_roots() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples");
    vec![root.join("skyrim"), root.join("skyrim-ussep")]
}

/// Every file under the roots by its lower-case path with forward slashes,
/// later roots winning.
pub fn index() -> HashMap<String, PathBuf> {
    // `CHAR_DATA` names one folder to read in their place (a Data folder
    // with a mod's body in it).
    let roots = match std::env::var_os("CHAR_DATA") {
        Some(d) => vec![PathBuf::from(d)],
        None => sample_roots(),
    };
    let mut out = HashMap::new();
    for r in roots {
        walk(&r, &r, &mut out);
    }
    out
}

fn walk(root: &Path, dir: &Path, out: &mut HashMap<String, PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            let key = rel
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            out.insert(key, p);
        }
    }
}
