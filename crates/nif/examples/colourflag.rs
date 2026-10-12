//! Whether shapes whose vertices carry colours say so in their lighting's
//! switches too: for each of the second set's thirty-two, how many shapes
//! with colours and without have it on.
//!
//!     cargo run --release -p apoc-nif --example colourflag -- <file.nif>...

use apoc_nif::geometry::{is_tri_shape, TriShape};
use apoc_nif::shader::LightingShader;
use apoc_nif::Nif;

fn main() {
    let mut on = [[0usize; 32]; 2];
    let mut total = [0usize; 2];
    for path in std::env::args().skip(1) {
        let Ok(nif) = std::fs::read(&path)
            .map_err(drop)
            .and_then(|b| Nif::read(&b).map_err(drop))
        else {
            continue;
        };
        let stream = nif.header.stream.version;
        for at in 0..nif.blocks.len() {
            if !is_tri_shape(nif.kind_of(at)) {
                continue;
            }
            let Ok(s) = TriShape::read(&nif.blocks[at].data, stream) else {
                continue;
            };
            let Some(sh) = usize::try_from(s.shader)
                .ok()
                .filter(|i| *i < nif.blocks.len() && nif.kind_of(*i) == "BSLightingShaderProperty")
                .and_then(|i| LightingShader::read(&nif.blocks[i].data, stream).ok())
            else {
                continue;
            };
            let coloured = ((s.vertex_desc >> 44) & 0x20 != 0) as usize;
            total[coloured] += 1;
            for (bit, n) in on[coloured].iter_mut().enumerate() {
                *n += (sh.flags2 >> bit & 1) as usize;
            }
        }
    }
    println!("{} shapes with colours, {} without", total[1], total[0]);
    for (bit, (with, without)) in on[1].iter().zip(&on[0]).enumerate() {
        println!("switch {bit}: on for {with} with, {without} without");
    }
}
