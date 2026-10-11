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
    let (mut meshes, mut vertices) = (0usize, 0usize);
    let mut read: BTreeMap<String, usize> = BTreeMap::new();
    let mut odd: BTreeMap<String, usize> = BTreeMap::new();
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
                // Every shape reads as a mesh, and moving its vertices to
                // where they already are changes nothing but, at most, the
                // bounding sphere.
                for at in apoc_nif::mesh::shapes(&nif) {
                    match apoc_nif::mesh::read(&nif, at) {
                        Ok(m) => {
                            meshes += 1;
                            vertices += m.positions.len();
                            if m.positions.iter().flatten().any(|c| !c.is_finite()) {
                                *odd.entry("a position that is not a number".into())
                                    .or_default() += 1;
                            }
                            let mut moved = nif.clone();
                            match apoc_nif::mesh::set_positions(&mut moved, at, &m.positions) {
                                Ok(()) => {
                                    let again = apoc_nif::mesh::read(&moved, at).ok();
                                    if again.as_ref().map(|a| &a.positions) != Some(&m.positions) {
                                        *odd.entry(
                                            "positions changed by writing them back".into(),
                                        )
                                        .or_default() += 1;
                                    }
                                    if moved.write().len() != bytes.len() {
                                        *odd.entry("the file changed length".into())
                                            .or_default() += 1;
                                    }
                                }
                                Err(e) => {
                                    *odd.entry(format!("set positions: {e}")).or_default() += 1
                                }
                            }
                        }
                        Err(e) => *odd.entry(format!("mesh: {e}")).or_default() += 1,
                    }
                }
                let stream = nif.header.stream.version;
                for i in 0..nif.blocks.len() {
                    let kind = nif.kind_of(i).to_string();
                    let data = &nif.blocks[i].data;
                    // A kind this reads into a structure must write back the
                    // bytes it read.
                    let typed: Option<Result<Vec<u8>, apoc_nif::NifError>> =
                        if apoc_nif::node::is_node(&kind) {
                            Some(apoc_nif::node::Node::read(data, stream).map(|n| n.write(stream)))
                        } else if apoc_nif::geometry::is_tri_shape(&kind) {
                            Some(apoc_nif::geometry::TriShape::read(data, stream).map(|t| {
                                if t.has_vertices() && t.layout(stream).is_none() {
                                    *odd.entry(format!("{kind} layout {:#x}", t.vertex_desc))
                                        .or_default() += 1;
                                }
                                t.write(stream)
                            }))
                        } else if kind == "NiSkinInstance" || kind == "BSDismemberSkinInstance" {
                            Some(
                                apoc_nif::skin::SkinInstance::read(data, kind != "NiSkinInstance")
                                    .map(|x| x.write()),
                            )
                        } else if kind == "BSLightingShaderProperty" {
                            Some(
                                apoc_nif::shader::LightingShader::read(data, stream).map(|x| {
                                    // The link must be to a texture set, or to nothing.
                                    let to = usize::try_from(x.texture_set)
                                        .ok()
                                        .map(|i| nif.kind_of(i).to_string());
                                    if to.as_deref().is_some_and(|k| k != "BSShaderTextureSet") {
                                        *odd.entry(format!(
                                            "shader's textures are a {}",
                                            to.unwrap()
                                        ))
                                        .or_default() += 1;
                                    }
                                    x.write()
                                }),
                            )
                        } else if kind == "BSShaderTextureSet" {
                            Some(apoc_nif::shader::TextureSet::read(data).map(|x| x.write()))
                        } else if kind == "NiSkinData" {
                            Some(apoc_nif::skin::SkinData::read(data).map(|x| x.write()))
                        } else if kind == "NiSkinPartition" {
                            Some(
                                apoc_nif::skin::SkinPartition::read(data, stream)
                                    .map(|x| x.write(stream)),
                            )
                        } else {
                            None
                        };
                    match typed {
                        Some(Ok(bytes)) if bytes == *data => {
                            *read.entry(kind.clone()).or_default() += 1
                        }
                        Some(Ok(_)) => {
                            *odd.entry(format!("{kind} written differently"))
                                .or_default() += 1
                        }
                        Some(Err(e)) => *odd.entry(format!("{kind}: {e}")).or_default() += 1,
                        None => {}
                    }
                    *kinds.entry(kind).or_default() += 1;
                }
            }
            Err(e) => failed.entry(e.to_string()).or_default().push(path.clone()),
        }
    }
    println!(
        "{} files: {same} written back the same, {differ} differ",
        files.len()
    );
    for (why, paths) in &failed {
        println!(
            "{} could not be read: {why} (e.g. {})",
            paths.len(),
            paths[0].display()
        );
    }
    println!("games: {games:?}");
    println!("read into structures and written back the same: {read:?}");
    println!("{meshes} shapes read as meshes, {vertices} vertices");
    println!("not as expected: {odd:?}");
    let mut by_count: Vec<_> = kinds.into_iter().collect();
    by_count.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (kind, n) in by_count {
        println!("{n:>8}  {kind}");
    }
}
