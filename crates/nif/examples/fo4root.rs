//! What Fallout 4's lighting keeps in the fifth number after its link to
//! its textures: whether it is always a text of the file (or none), and a
//! few of the texts.
//!
//!     cargo run --release -p apoc-nif --example fo4root -- <file.nif>...

use apoc_nif::shader::LightingShader;
use apoc_nif::Nif;
use std::collections::BTreeMap;

fn main() {
    let (mut text, mut none, mut odd) = (0usize, 0usize, 0usize);
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut sizes: BTreeMap<(u32, usize), usize> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        for at in 0..nif.blocks.len() {
            if nif.kind_of(at) != "BSLightingShaderProperty" {
                continue;
            }
            let Ok(s) = LightingShader::read(&nif.blocks[at].data, nif.header.stream.version)
            else {
                continue;
            };
            *sizes
                .entry((s.shader_type.unwrap_or(u32::MAX), s.rest.len()))
                .or_default() += 1;
            let Some(w) = s
                .rest
                .get(16..20)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
            else {
                continue;
            };
            match (w, nif.string(w)) {
                (-1, _) => none += 1,
                (_, Some(t)) => {
                    text += 1;
                    *seen
                        .entry(String::from_utf8_lossy(t).into_owned())
                        .or_default() += 1;
                }
                _ => odd += 1,
            }
        }
    }
    println!(
        "a text of the file {text} times, none {none}, neither {odd}; sizes by kind {sizes:?}"
    );
    for (t, n) in seen.iter().take(6) {
        println!("  {n} × {t:?}");
    }
}
