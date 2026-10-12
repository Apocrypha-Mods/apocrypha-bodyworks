//! Whether Fallout 4's lighting keeps its numbers where supposed after the
//! link to its textures: the colour it glows and how strongly, a text, how
//! the texture repeats, how solid, a bending of light, how smooth, and the
//! colour and strength of its shine.
//!
//!     cargo run --release -p apoc-nif --example fo4look -- <file.nif>...

use apoc_nif::shader::LightingShader;
use apoc_nif::Nif;

fn main() {
    let (mut plain, mut seen, mut shown) = (0usize, 0usize, 0);
    let (mut smooth, mut solid) = ((f32::MAX, f32::MIN), (f32::MAX, f32::MIN));
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        if !(130..=139).contains(&nif.header.stream.version) {
            continue;
        }
        for at in 0..nif.blocks.len() {
            if nif.kind_of(at) != "BSLightingShaderProperty" {
                continue;
            }
            let Ok(s) = LightingShader::read(&nif.blocks[at].data, nif.header.stream.version)
            else {
                continue;
            };
            if s.rest.len() < 52 {
                continue;
            }
            let f = |i: usize| f32::from_le_bytes(s.rest[i * 4..i * 4 + 4].try_into().unwrap());
            let clamp = u32::from_le_bytes(s.rest[20..24].try_into().unwrap());
            seen += 1;
            let fits = (0..3).all(|i| (0.0..=1.0).contains(&f(i)))
                && (0.0..=1000.0).contains(&f(3))
                && clamp <= 3
                && (0.0..=1.0).contains(&f(6))
                && (0.0..=1.0).contains(&f(7))
                && (0.0..=1.0).contains(&f(8))
                && (9..12).all(|i| (0.0..=1.0).contains(&f(i)))
                && (0.0..=1000.0).contains(&f(12));
            plain += fits as usize;
            if fits {
                smooth = (smooth.0.min(f(8)), smooth.1.max(f(8)));
                solid = (solid.0.min(f(6)), solid.1.max(f(6)));
            } else if shown < 4 {
                shown += 1;
                println!("odd: {path}: glow {:?} x{} clamp {clamp} alpha {} refraction {} smooth {} shine {:?} x{}", [f(0), f(1), f(2)], f(3), f(6), f(7), f(8), [f(9), f(10), f(11)], f(12));
            }
        }
    }
    println!("{plain} of {seen} read as plain values; smoothness {smooth:?}, how solid {solid:?}");
}
