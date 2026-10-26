//! Whether a shape is drawn see-through, on the game's own meshes and a
//! Skyrim (2011) outfit's when they are in `scratch/` (skipped when not):
//! a shape is when it links an alpha property, and only then.

use std::path::{Path, PathBuf};

use apoc_nif::geometry::TriShape;
use apoc_nif::geometry_le::NiTriShape;
use apoc_nif::{mesh, Nif};

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scratch/samples")
}

fn read(path: &Path) -> Option<Nif> {
    Some(Nif::read(&std::fs::read(path).ok()?).unwrap())
}

/// The shape at `block` with its alpha link set to `to`.
fn linked(nif: &Nif, block: usize, to: i32) -> Nif {
    let mut n = nif.clone();
    let stream = n.header.stream.version;
    if n.kind_of(block) == "NiTriShape" {
        let mut s = NiTriShape::read(&n.blocks[block].data, stream).unwrap();
        s.alpha = Some(to);
        n.blocks[block].data = s.write(stream);
    } else {
        let mut s = TriShape::read(&n.blocks[block].data, stream).unwrap();
        s.alpha = to;
        n.blocks[block].data = s.write(stream);
    }
    n
}

fn first_of(nif: &Nif, kind: &str) -> Option<i32> {
    (0..nif.blocks.len())
        .find(|b| nif.kind_of(*b) == kind)
        .map(|b| b as i32)
}

#[test]
fn hair_eyes_and_mouths_are_see_through_and_skin_is_not() {
    let assets = samples().join("skyrim/meshes/actors/character/character assets");
    let Some(hair) = read(&assets.join("hair/femaledarkelfhair01.nif")) else {
        return;
    };
    let b = mesh::shapes(&hair)[0];
    assert!(mesh::see_through(&hair, b));
    for f in ["eyesmale.nif", "mouth/mouthhuman.nif"] {
        let n = read(&assets.join(f)).unwrap();
        assert!(mesh::see_through(&n, mesh::shapes(&n)[0]), "{f}");
    }
    for f in ["malehead.nif", "malebody_1.nif", "malehands_1.nif"] {
        let n = read(&assets.join(f)).unwrap();
        for s in mesh::shapes(&n) {
            assert!(!mesh::see_through(&n, s), "{f} {s}");
        }
    }
    // Its link taken away, or pointed at a block that is not an alpha
    // property, it is not.
    assert!(!mesh::see_through(&linked(&hair, b, -1), b));
    let shader = first_of(&hair, "BSLightingShaderProperty").unwrap();
    assert!(!mesh::see_through(&linked(&hair, b, shader), b));
    assert!(!mesh::see_through(&linked(&hair, b, 100_000), b));
    // Linked back to its alpha property, it is again.
    let alpha = first_of(&hair, "NiAlphaProperty").unwrap();
    let unlinked = linked(&hair, b, -1);
    assert!(mesh::see_through(&linked(&unlinked, b, alpha), b));
    // Not a shape at all.
    assert!(!mesh::see_through(&hair, shader as usize));
}

#[test]
fn an_older_games_shape_is_see_through_by_its_own_alpha_link() {
    let outfit = samples().join("dcc-mse-cowgirl/.resources");
    let mut files = Vec::new();
    for dir in ["belt", "skirt", "stockings"] {
        for e in std::fs::read_dir(outfit.join(dir))
            .into_iter()
            .flatten()
            .flatten()
        {
            if e.path().extension().is_some_and(|x| x == "nif") {
                files.push(e.path());
            }
        }
    }
    if files.is_empty() {
        return;
    }
    let (mut shapes, mut seen) = (0, [0usize; 2]);
    for path in &files {
        let n = read(path).unwrap();
        assert_eq!(n.header.stream.version, 83);
        let stream = n.header.stream.version;
        for b in mesh::shapes(&n) {
            assert_eq!(n.kind_of(b), "NiTriShape");
            shapes += 1;
            let alpha = NiTriShape::read(&n.blocks[b].data, stream).unwrap().alpha;
            let has = alpha
                .and_then(|a| usize::try_from(a).ok())
                .is_some_and(|a| n.kind_of(a) == "NiAlphaProperty");
            assert_eq!(mesh::see_through(&n, b), has, "{path:?} {b}");
            seen[has as usize] += 1;
            assert!(!mesh::see_through(&linked(&n, b, -1), b));
            if let Some(a) = first_of(&n, "NiAlphaProperty") {
                assert!(mesh::see_through(&linked(&n, b, a), b), "{path:?} {b}");
                seen[1] += 1;
            }
        }
    }
    assert!(shapes > 0);
    // Both answers came up.
    assert!(seen[0] > 0 && seen[1] > 0, "{seen:?}");
}
