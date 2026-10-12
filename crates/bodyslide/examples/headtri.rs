//! Whether the morph files a head's parts come with (`FRTRI003`) are laid
//! out as supposed: a head of 64 bytes, the part's vertices, its triangles,
//! its places in the texture, then each morph as a name, a scale and three
//! small whole numbers a vertex, then each modifier as a name and a list.
//!
//!     cargo run --release -p apoc-bodyslide --example headtri -- <file.tri>...

fn parse(d: &[u8]) -> Result<String, String> {
    if d.get(..8) != Some(b"FRTRI003") {
        return Err("not one of these".into());
    }
    let word = |at: usize| {
        d.get(at..at + 4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or(format!("ends at {at}"))
    };
    let (vertices, tris, quads, u2, u3, uvs, ext, morphs, mods, mod_vertices) = (
        word(8)?,
        word(12)?,
        word(16)?,
        word(20)?,
        word(24)?,
        word(28)?,
        word(32)?,
        word(36)?,
        word(40)?,
        word(44)?,
    );
    let mut at =
        64 + vertices * 12 + mod_vertices * 12 + tris * 12 + quads * 16 + u2 * 4 + u3 * 4 + uvs * 8;
    if ext & 1 == 1 {
        at += tris * 12 + quads * 16;
    }
    let mut names = Vec::new();
    for _ in 0..morphs {
        let n = word(at)?;
        let name = d.get(at + 4..at + 4 + n).ok_or("ends in a name")?;
        names.push(
            String::from_utf8_lossy(name)
                .trim_end_matches('\0')
                .to_string(),
        );
        at += 4 + n + 4 + vertices * 6;
    }
    let mut listed = 0usize;
    for _ in 0..mods {
        let n = word(at)?;
        at += 4 + n;
        let count = word(at)?;
        listed += count;
        at += 4 + count * 4;
    }
    if at != d.len() || listed != mod_vertices {
        return Err(format!("{} bytes read of {}; modifiers list {listed} of {mod_vertices}; head {vertices} v {tris} t {quads} q {u2} {u3} uv {uvs} ext {ext} morphs {morphs} mods {mods}", at, d.len()));
    }
    Ok(format!(
        "{vertices} vertices, {tris} triangles, {uvs} uvs, {morphs} morphs ({}), {mods} modifiers",
        names.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
    ))
}

fn main() {
    let (mut ok, mut odd, mut shown, mut back) = (0, 0, 0, 0);
    for path in std::env::args().skip(1) {
        let Ok(d) = std::fs::read(&path) else {
            continue;
        };
        if d.get(..8) != Some(b"FRTRI003") {
            continue;
        }
        // And through the reader the Studio uses: read, and written back the same.
        match apoc_bodyslide::headtri::read(&d) {
            Ok(t) if apoc_bodyslide::headtri::write(&t) == d => back += 1,
            Ok(_) => println!("NOT THE SAME BACK {path}"),
            Err(e) => println!("REFUSED {path}: {e}"),
        }
        match parse(&d) {
            Ok(what) => {
                ok += 1;
                if shown < 3 {
                    shown += 1;
                    println!("{path}: {what}");
                }
            }
            Err(why) => {
                odd += 1;
                if odd <= 4 {
                    println!("ODD {path}: {why}");
                }
            }
        }
    }
    println!("{ok} as supposed, {odd} not; {back} read and write back the same");
}
