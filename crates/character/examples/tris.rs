//! Reads every head part tri in the unpacked archives with this crate's
//! reader, and says how many read and why the rest do not.
//!
//!     cargo run --release -p apoc-character --example tris
mod common;
use std::collections::BTreeMap;
fn main() {
    let (mut ok, mut modifiers) = (0, 0);
    let mut bad: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, p) in common::index() {
        if !k.ends_with(".tri") {
            continue;
        }
        let b = std::fs::read(&p).unwrap();
        if b.get(..8) != Some(b"FRTRI003") {
            bad.entry("not FRTRI003".into()).or_default().push(k);
            continue;
        }
        match apoc_character::tri::read(&b) {
            Ok(_) => {
                ok += 1;
                if b[40..44] != [0; 4] {
                    modifiers += 1;
                }
            }
            Err(e) => bad.entry(e).or_default().push(k),
        }
    }
    println!("{ok} tris read, {modifiers} of them with modifiers");
    for (e, v) in bad {
        println!("{} {e}: {:?}", v.len(), &v[..v.len().min(5)]);
    }
}
