//! What lighting shaders keep after the link to their textures: how long
//! it is for each kind of shader, and what the numbers there look like when
//! read as a colour it glows, how strongly, how see-through, how glossy,
//! and the colour and strength of its shine.
//!
//!     cargo run -p apoc-nif --example look -- <file.nif>...

use apoc_nif::shader::LightingShader;
use apoc_nif::Nif;
use std::collections::BTreeMap;

fn main() {
    let mut lengths: BTreeMap<(u32, usize), usize> = BTreeMap::new();
    let (mut seen, mut plain) = (0usize, 0usize);
    let mut shown = 0;
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        let stream = nif.header.stream.version;
        for at in 0..nif.blocks.len() {
            if nif.kind_of(at) != "BSLightingShaderProperty" {
                continue;
            }
            let Ok(s) = LightingShader::read(&nif.blocks[at].data, stream) else {
                continue;
            };
            *lengths
                .entry((s.shader_type.unwrap_or(u32::MAX), s.rest.len()))
                .or_default() += 1;
            if s.rest.len() < 52 {
                continue;
            }
            let f = |i: usize| f32::from_le_bytes(s.rest[i * 4..i * 4 + 4].try_into().unwrap());
            let clamp = u32::from_le_bytes(s.rest[16..20].try_into().unwrap());
            seen += 1;
            // What these would be if the layout is as supposed.
            let fits = (0..3).all(|i| (0.0..=1.0).contains(&f(i)))
                && clamp <= 3
                && (0.0..=1.0).contains(&f(5))
                && (0.0..=1.0).contains(&f(6))
                && (0.0..=1000.0).contains(&f(7))
                && (8..11).all(|i| (0.0..=1.0).contains(&f(i)))
                && (0.0..=1000.0).contains(&f(11));
            plain += fits as usize;
            if !fits && shown < 6 {
                shown += 1;
                println!(
                    "odd: {path}: glow {:?} x{} clamp {clamp} alpha {} refraction {} gloss {} shine {:?} x{}",
                    [f(0), f(1), f(2)],
                    f(3),
                    f(5),
                    f(6),
                    f(7),
                    [f(8), f(9), f(10)],
                    f(11)
                );
            }
        }
    }
    for ((kind, len), n) in &lengths {
        println!("kind {kind}: {len} bytes after the textures, {n} times");
    }
    println!("{plain} of {seen} read as plain values");
}
