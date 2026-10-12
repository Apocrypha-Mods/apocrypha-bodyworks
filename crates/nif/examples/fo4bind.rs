//! How Fallout 4's skins place their bones: whether each bone's transform
//! is the undoing of where the bone's node is in the file, whether every
//! block writes back as it was read, and what the scales and spheres hold.
//!
//!     cargo run --release -p apoc-nif --example fo4bind -- <file.nif>...

use apoc_nif::copy::{inverse, parents, placed};
use apoc_nif::geometry::{is_tri_shape, TriShape};
use apoc_nif::skin::{Fo4BoneData, Fo4Instance};
use apoc_nif::Nif;

fn main() {
    let (mut same, mut all, mut back, mut blocks, mut scaled, mut worst) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0.0f32);
    let mut shown = 0;
    for path in std::env::args().skip(1) {
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
            let Some(skin) = usize::try_from(shape.skin)
                .ok()
                .filter(|s| nif.kind_of(*s) == "BSSkin::Instance")
            else {
                continue;
            };
            let Ok(instance) = Fo4Instance::read(&nif.blocks[skin].data) else {
                continue;
            };
            let Some(d) = usize::try_from(instance.data).ok() else {
                continue;
            };
            let Ok(data) = Fo4BoneData::read(&nif.blocks[d].data) else {
                continue;
            };
            blocks += 2;
            back += (instance.write() == nif.blocks[skin].data) as usize
                + (data.write() == nif.blocks[d].data) as usize;
            scaled += instance.scales.iter().filter(|s| **s != [1.0; 3]).count();
            for (bone, node) in data.bones.iter().zip(&instance.bones) {
                let Ok(node) = usize::try_from(*node) else {
                    continue;
                };
                let Some(want) = placed(&nif, node, &up).ok().and_then(|p| inverse(&p)) else {
                    continue;
                };
                all += 1;
                let off = bone
                    .transform
                    .rotation
                    .iter()
                    .zip(&want.rotation)
                    .chain(bone.transform.translation.iter().zip(&want.translation))
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f32, f32::max);
                if off < 0.01 {
                    same += 1;
                } else {
                    worst = worst.max(off);
                    if shown < 3 {
                        shown += 1;
                        println!(
                            "{path}: bone at {:?}, its node undone {:?}",
                            bone.transform.translation, want.translation
                        );
                    }
                }
            }
        }
    }
    println!("{same} of {all} bones are placed as their node undone (worst other {worst}); {back} of {blocks} skin blocks write back the same; {scaled} scales are not one");
}
