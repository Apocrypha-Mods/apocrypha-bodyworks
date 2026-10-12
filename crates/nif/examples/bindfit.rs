//! Whether a skin places each bone as the undoing of where the bone's node
//! is in the file: counted over real files, to know how a bone no shape
//! follows yet would have to be placed.
//!
//!     cargo run --release -p apoc-nif --example bindfit -- <folder>...

use apoc_nif::copy::{inverse, parents, placed};
use apoc_nif::geometry::{is_tri_shape, TriShape};
use apoc_nif::skin::{SkinData, SkinInstance};
use apoc_nif::Nif;
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            walk(&e.path(), out);
        } else if e
            .path()
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("nif"))
        {
            out.push(e.path());
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for root in std::env::args().skip(1) {
        walk(Path::new(&root), &mut files);
    }
    let (mut same, mut all, mut worst) = (0usize, 0usize, 0.0f32);
    let mut shown = 0;
    for path in files {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        let stream = nif.header.stream.version;
        let up = parents(&nif);
        for at in 0..nif.blocks.len() {
            if !is_tri_shape(nif.kind_of(at)) {
                continue;
            }
            let Ok(shape) = TriShape::read(&nif.blocks[at].data, stream) else {
                continue;
            };
            let Ok(skin) = usize::try_from(shape.skin) else {
                continue;
            };
            let kind = nif.kind_of(skin).to_string();
            let Ok(instance) = SkinInstance::read(&nif.blocks[skin].data, kind != "NiSkinInstance")
            else {
                continue;
            };
            let Some(data) = usize::try_from(instance.data)
                .ok()
                .and_then(|d| SkinData::read(&nif.blocks[d].data).ok())
            else {
                continue;
            };
            for (bone, stored) in instance.bones.iter().zip(&data.bones) {
                let Ok(node) = usize::try_from(*bone) else {
                    continue;
                };
                let Ok(place) = placed(&nif, node, &up) else {
                    continue;
                };
                let Some(undo) = inverse(&place) else {
                    continue;
                };
                let off = undo
                    .translation
                    .iter()
                    .zip(&stored.transform.translation)
                    .map(|(a, b)| (a - b).abs())
                    .chain(
                        undo.rotation
                            .iter()
                            .zip(&stored.transform.rotation)
                            .map(|(a, b)| (a - b).abs() * 10.0),
                    )
                    .fold(0.0f32, f32::max);
                all += 1;
                if off < 0.05 {
                    same += 1;
                } else if shown < 4 {
                    shown += 1;
                    println!(
                        "  {}: stored {:?}, undoing the node {:?}",
                        path.file_name().unwrap().to_string_lossy(),
                        stored.transform.translation,
                        undo.translation
                    );
                }
                worst = worst.max(off);
            }
        }
    }
    println!("{same} of {all} bones are placed as the undoing of their node; the furthest off by {worst}");
}
