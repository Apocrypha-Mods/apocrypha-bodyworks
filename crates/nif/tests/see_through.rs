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

/// The alpha property a shape links, read here straight from its bytes: the
/// name, the extra-data list (a count, then that many links), the
/// controller, then the switches and the threshold.
fn alpha_bytes(nif: &Nif, block: usize) -> Option<(u16, u8)> {
    let stream = nif.header.stream.version;
    let link = if nif.kind_of(block) == "NiTriShape" {
        NiTriShape::read(&nif.blocks[block].data, stream)
            .ok()?
            .alpha?
    } else {
        TriShape::read(&nif.blocks[block].data, stream).ok()?.alpha
    };
    let at = usize::try_from(link).ok()?;
    if nif.kind_of(at) != "NiAlphaProperty" {
        return None;
    }
    let d = &nif.blocks[at].data;
    let extra = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
    let flags = 8 + 4 * extra + 4;
    Some((u16::from_le_bytes([d[flags], d[flags + 1]]), d[flags + 2]))
}

/// Every shape of the game's character meshes (body, head, hair, brows,
/// eyes, mouths) and of the older outfit, with the file it came from.
fn every_sample_shape() -> Vec<(PathBuf, Nif)> {
    let assets = samples().join("skyrim/meshes/actors/character/character assets");
    let outfit = samples().join("dcc-mse-cowgirl/.resources");
    let dirs = [
        assets.clone(),
        assets.join("hair"),
        assets.join("faceparts"),
        assets.join("mouth"),
        outfit.join("belt"),
        outfit.join("skirt"),
        outfit.join("stockings"),
    ];
    let mut files = Vec::new();
    for dir in dirs {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.path().extension().is_some_and(|x| x == "nif") {
                files.push(e.path());
            }
        }
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|p| read(&p).map(|n| (p, n)))
        .collect()
}

#[test]
fn hair_brows_and_lashes_carry_the_switches_and_threshold_their_alpha_property_holds() {
    let assets = samples().join("skyrim/meshes/actors/character/character assets");
    let Some(hair) = read(&assets.join("hair/femaledarkelfhair01.nif")) else {
        return;
    };
    // The game's hair is cut away under 100 and laid over nothing.
    let alpha = |n: &Nif| mesh::alpha(n, mesh::shapes(n)[0]);
    let cut = |threshold| mesh::Alpha {
        blend: false,
        test: true,
        threshold,
    };
    let laid = |threshold| mesh::Alpha {
        blend: true,
        test: false,
        threshold,
    };
    assert_eq!(alpha(&hair), Some(cut(100)));
    // Its hairlines and the mouth are cut at 128; brows and eyes are laid
    // over the face, their threshold left at 128.
    for (f, want) in [
        ("hair/argonianfemalehorns01hairline.nif", cut(128)),
        ("mouth/mouthhuman.nif", cut(128)),
        ("faceparts/femalebrows.nif", laid(128)),
        ("faceparts/malebrows.nif", laid(128)),
        ("eyesmale.nif", laid(128)),
    ] {
        let n = read(&assets.join(f)).unwrap();
        assert_eq!(alpha(&n), Some(want), "{f}");
    }
    // A body mod's lashes are laid over like the game's brows; its brows
    // are too, with the threshold at nothing.
    let ube = samples().join("ube");
    if let Some(n) = read(&ube.join("body/0base/meshes/!UBE/Lashes/EyeFemaleLashes.nif")) {
        assert_eq!(alpha(&n), Some(laid(128)));
    }
    if let Some(n) = read(&ube.join("fix/meshes/!UBE/Brows/FemaleBrows.nif")) {
        assert_eq!(alpha(&n), Some(laid(0)));
    }
    // The older outfit's stockings both blend and cut, at nothing.
    let legs = samples().join("dcc-mse-cowgirl/.resources/stockings/legs_0.nif");
    if let Some(n) = read(&legs) {
        let want = mesh::Alpha {
            blend: true,
            test: true,
            threshold: 0,
        };
        assert_eq!(alpha(&n), Some(want));
    }
    // Skin is drawn solid and has none.
    for f in [
        "malehead.nif",
        "femalehead.nif",
        "malebody_1.nif",
        "malehands_1.nif",
    ] {
        let n = read(&assets.join(f)).unwrap();
        for s in mesh::shapes(&n) {
            assert_eq!(mesh::alpha(&n, s), None, "{f} {s}");
        }
    }
}

#[test]
fn a_shape_has_an_alpha_exactly_when_it_is_see_through_and_it_is_the_one_its_bytes_hold() {
    let files = every_sample_shape();
    if files.is_empty() {
        return;
    }
    let (mut seen, mut switches) = ([0usize; 2], std::collections::BTreeSet::new());
    for (path, n) in &files {
        for b in mesh::shapes(n) {
            let alpha = mesh::alpha(n, b);
            assert_eq!(alpha.is_some(), mesh::see_through(n, b), "{path:?} {b}");
            seen[alpha.is_some() as usize] += 1;
            // What it says is what the property's own bytes say.
            let want = alpha_bytes(n, b).map(|(flags, threshold)| mesh::Alpha {
                blend: flags & 1 != 0,
                test: flags & 0x200 != 0,
                threshold,
            });
            assert_eq!(alpha, want, "{path:?} {b}");
            if let Some(a) = alpha {
                switches.insert((a.blend, a.test, a.threshold));
            }
        }
    }
    // Both answers came up, and more than one way of being see-through.
    assert!(seen[0] > 0 && seen[1] > 0, "{seen:?}");
    assert!(switches.len() > 2, "{switches:?}");
}

#[test]
fn the_switches_and_threshold_follow_the_bytes_of_the_property() {
    let assets = samples().join("skyrim/meshes/actors/character/character assets");
    let Some(hair) = read(&assets.join("hair/femaledarkelfhair01.nif")) else {
        return;
    };
    let b = mesh::shapes(&hair)[0];
    let at = first_of(&hair, "NiAlphaProperty").unwrap() as usize;
    let edited = |flags: u16, threshold: u8, extra: &[i32]| {
        let mut n = hair.clone();
        let d = &mut n.blocks[at].data;
        let mut out = d[..4].to_vec();
        out.extend((extra.len() as u32).to_le_bytes());
        for x in extra {
            out.extend(x.to_le_bytes());
        }
        out.extend(&d[8..12]);
        out.extend(flags.to_le_bytes());
        out.push(threshold);
        *d = out;
        mesh::alpha(&n, b).unwrap()
    };
    let a = |blend, test, threshold| mesh::Alpha {
        blend,
        test,
        threshold,
    };
    // Each switch on its own, both, and neither: the other bits of the
    // flags (how it blends, which test) change nothing.
    assert_eq!(edited(0x0001, 7, &[]), a(true, false, 7));
    assert_eq!(edited(0x0200, 255, &[]), a(false, true, 255));
    assert_eq!(edited(0x0201, 1, &[]), a(true, true, 1));
    assert_eq!(edited(0x0000, 0, &[]), a(false, false, 0));
    assert_eq!(edited(!0x0201, 200, &[]), a(false, false, 200));
    assert_eq!(edited(0xFFFF, 42, &[]), a(true, true, 42));
    // Extra data before them moves them on, and they are still found.
    assert_eq!(edited(0x0201, 9, &[3, 4]), a(true, true, 9));
    assert_eq!(edited(0x0000, 250, &[5]), a(false, false, 250));
}

#[test]
fn a_cut_short_alpha_property_has_no_alpha_and_nothing_panics() {
    let assets = samples().join("skyrim/meshes/actors/character/character assets");
    let Some(hair) = read(&assets.join("hair/femaledarkelfhair01.nif")) else {
        return;
    };
    let b = mesh::shapes(&hair)[0];
    let at = first_of(&hair, "NiAlphaProperty").unwrap() as usize;
    let whole = hair.blocks[at].data.clone();
    assert_eq!(whole.len(), 15);
    // Cut anywhere short of its threshold, it has none, though the shape
    // still links it and so is still see-through.
    for len in 0..whole.len() {
        let mut n = hair.clone();
        n.blocks[at].data.truncate(len);
        assert_eq!(mesh::alpha(&n, b), None, "{len}");
        assert!(mesh::see_through(&n, b));
    }
    // An extra-data count far past the end has none either.
    let mut n = hair.clone();
    n.blocks[at].data[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(mesh::alpha(&n, b), None);
    // Whole, it has its own.
    assert!(mesh::alpha(&hair, b).is_some());
}

#[test]
fn a_shape_not_linked_to_an_alpha_property_has_no_alpha() {
    let assets = samples().join("skyrim/meshes/actors/character/character assets");
    let Some(hair) = read(&assets.join("hair/femaledarkelfhair01.nif")) else {
        return;
    };
    let b = mesh::shapes(&hair)[0];
    let shader = first_of(&hair, "BSLightingShaderProperty").unwrap();
    // No link, a link to its shader, to the node at the top of the file
    // (begun as an alpha property is, so it would read as one), and a link
    // past the last block.
    let root = first_of(&hair, "NiNode").unwrap();
    for to in [-1, shader, root, hair.blocks.len() as i32, 100_000] {
        assert_eq!(mesh::alpha(&linked(&hair, b, to), b), None, "{to}");
    }
    // Linked back to its alpha property, it has its own again.
    let alpha = first_of(&hair, "NiAlphaProperty").unwrap();
    let unlinked = linked(&hair, b, -1);
    assert_eq!(
        mesh::alpha(&linked(&unlinked, b, alpha), b),
        mesh::alpha(&hair, b)
    );
    assert!(mesh::alpha(&hair, b).is_some());
    // Not a shape at all.
    assert_eq!(mesh::alpha(&hair, shader as usize), None);
    assert_eq!(mesh::alpha(&hair, alpha as usize), None);
}
