//! Read every NIF under the folders given, write each back, and say whether
//! the bytes are the same and which kinds of block were seen.
//!
//!     cargo run --release -p apoc-nif --example scan -- <folder>…

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("nif"))
        {
            out.push(path);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for arg in std::env::args().skip(1) {
        walk(Path::new(&arg), &mut files);
    }
    let (mut same, mut differ) = (0usize, 0usize);
    let mut failed: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut games: BTreeMap<String, usize> = BTreeMap::new();
    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        match apoc_nif::Nif::read(&bytes) {
            Ok(nif) => {
                if nif.write() == bytes {
                    same += 1;
                } else {
                    differ += 1;
                    println!("DIFFERS {}", path.display());
                }
                *games.entry(format!("{:?}", nif.game())).or_default() += 1;
                for i in 0..nif.blocks.len() {
                    *kinds.entry(nif.kind_of(i).to_string()).or_default() += 1;
                }
            }
            Err(e) => failed.entry(e.to_string()).or_default().push(path.clone()),
        }
    }
    println!("{} files: {same} written back the same, {differ} differ", files.len());
    for (why, paths) in &failed {
        println!("{} could not be read: {why} (e.g. {})", paths.len(), paths[0].display());
    }
    println!("games: {games:?}");
    let mut by_count: Vec<_> = kinds.into_iter().collect();
    by_count.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (kind, n) in by_count {
        println!("{n:>8}  {kind}");
    }
}
