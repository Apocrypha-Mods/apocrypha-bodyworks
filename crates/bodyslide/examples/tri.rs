//! Read morph files, say whether each writes back byte for byte, and
//! describe the first.
//!
//!     cargo run --release -p apoc-bodyslide --example tri -- <folder or file>...

use std::path::{Path, PathBuf};

fn walk(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
            walk(&e.path(), out);
        }
    } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("tri")) {
        out.push(p.to_path_buf());
    }
}

fn main() {
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        walk(Path::new(&a), &mut files);
    }
    let (mut same, mut bad) = (0, 0);
    for (i, f) in files.iter().enumerate() {
        let bytes = std::fs::read(f).unwrap();
        match apoc_bodyslide::tri::read(&bytes) {
            Ok(shapes) => {
                if apoc_bodyslide::tri::write(&shapes) == bytes {
                    same += 1;
                } else {
                    bad += 1;
                }
                if i == 0 || files.len() < 3 {
                    println!("{}", f.display());
                    for s in shapes.shapes.iter().chain(&shapes.uv) {
                        println!("  {}: {} morphs", s.name, s.morphs.len());
                        for m in s.morphs.iter().take(40) {
                            let top = m
                                .offsets
                                .iter()
                                .flat_map(|o| o.1)
                                .map(|c| c.unsigned_abs())
                                .max()
                                .unwrap_or(0);
                            println!(
                                "    {:28} x{:<12e} {:5} vertices, largest {top}",
                                m.name,
                                m.multiplier,
                                m.offsets.len()
                            );
                        }
                    }
                }
            }
            Err(e) => {
                bad += 1;
                println!("{}: {e}", f.display());
            }
        }
    }
    println!(
        "{} files: {same} write back the same, {bad} do not",
        files.len()
    );
}
