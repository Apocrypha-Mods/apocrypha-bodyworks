//! What can be done with meshes that follow no bones (weapons, clutter):
//! each file opened as a project and each change tried on its first shape,
//! with how many took it and why the first that did not refused.
//!
//!     cargo run --release -p apoc-bodyslide --example statics -- <file.nif>...

use apoc_bodyslide::project::Project;
use apoc_nif::{mesh, Nif};
use std::collections::BTreeMap;

fn main() {
    let mut tally: BTreeMap<&str, (usize, usize, String)> = BTreeMap::new();
    let mut note = |what: &'static str, done: Result<(), String>| {
        let t = tally.entry(what).or_default();
        match done {
            Ok(()) => t.0 += 1,
            Err(e) => {
                t.1 += 1;
                if t.2.is_empty() {
                    t.2 = e;
                }
            }
        }
    };
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let nif = match Nif::read(&bytes) {
            Ok(n) => n,
            Err(e) => {
                note("read", Err(format!("{path}: {e}")));
                continue;
            }
        };
        note("read", Ok(()));
        note(
            "write back the same",
            (nif.write() == bytes).then_some(()).ok_or(path.clone()),
        );
        let project = match Project::from_mesh("Probe", "meshes/probe.nif", &bytes) {
            Ok(p) => p,
            Err(e) => {
                note("open as a project", Err(format!("{path}: {e}")));
                continue;
            }
        };
        let Some(shape) = project.set.shapes.first().map(|s| s.name.clone()) else {
            note("open as a project", Err(format!("{path}: no shapes")));
            continue;
        };
        note("open as a project", Ok(()));
        let said = |e: apoc_bodyslide::Error| format!("{path}: {e}");
        let block = mesh::shapes(&project.nif)
            .into_iter()
            .find(|at| mesh::name(&project.nif, *at).as_deref() == Some(shape.as_str()));
        let count = block
            .and_then(|b| mesh::read(&project.nif, b).ok())
            .map_or(0, |m| m.positions.len());
        note(
            "read its vertices",
            (count > 0).then_some(()).ok_or(path.clone()),
        );
        let half: Vec<u16> = (0..(count / 2) as u16).collect();
        let mut p = project.clone();
        note(
            "move, turn, scale",
            p.transform(&shape, [1.0, 0.0, 0.0], 1.1, [0.0, 0.0, 10.0])
                .map_err(said),
        );
        let mut p = project.clone();
        note("mirror", p.mirror_shape(&shape).map_err(said));
        let mut p = project.clone();
        note("flip texture", p.flip_uv(&shape, true).map_err(said));
        let mut p = project.clone();
        note(
            "duplicate",
            p.duplicate_shape(&shape, "Copy of it").map_err(said),
        );
        // Written and read again: as many vertices and triangles as it should then have.
        let after = |p: &Project| -> Option<(usize, usize)> {
            let nif = Nif::read(&p.nif.write()).ok()?;
            let at = mesh::shapes(&nif)
                .into_iter()
                .find(|at| mesh::name(&nif, *at).as_deref() == Some(shape.as_str()))?;
            let m = mesh::read(&nif, at).ok()?;
            Some((m.positions.len(), m.triangles.len()))
        };
        let was = after(&project).unwrap_or_default();
        let mut p = project.clone();
        let refined = p.refine(&shape, &[]).map_err(said);
        if let Ok(added) = &refined {
            note(
                "refined: four triangles for one, read back",
                (after(&p) == Some((was.0 + added, was.1 * 4)))
                    .then_some(())
                    .ok_or(path.clone()),
            );
        }
        note("refine", refined.map(drop));
        let first = block
            .and_then(|b| mesh::read(&project.nif, b).ok())
            .and_then(|m| m.triangles.first().copied());
        if let Some(t) = first {
            use apoc_nif::weights::EdgeOp;
            for (what, op, by) in [
                ("split an edge", EdgeOp::Split, 1isize),
                ("collapse an edge", EdgeOp::Collapse, -1),
            ] {
                let mut p = project.clone();
                let done = p.edge(&shape, t[0], t[1], op).map_err(said);
                if done.is_ok() {
                    let now = after(&p).unwrap_or_default();
                    note(
                        "edge: one vertex more or fewer, read back",
                        (now.0 as isize == was.0 as isize + by)
                            .then_some(())
                            .ok_or(format!("{path}: {was:?} to {now:?}")),
                    );
                }
                note(what, done);
            }
        }
        // The first shape taken out, where there is another: every block
        // after it moves up, and what is written goes to STATICS_OUT for
        // `--example havok` to check its links by.
        if project.set.shapes.len() > 1 {
            let mut p = project.clone();
            let done = p.delete_shape(&shape).map_err(said);
            if let (Ok(()), Ok(out)) = (&done, std::env::var("STATICS_OUT")) {
                let name = path.replace('/', "_");
                let bytes = p.nif.write();
                note(
                    "without its first shape: read back",
                    Nif::read(&bytes)
                        .map(drop)
                        .map_err(|e| format!("{path}: {e}")),
                );
                std::fs::write(std::path::Path::new(&out).join(name), bytes).ok();
            }
            note("delete the first shape", done);
        }
        let mut p = project.clone();
        note(
            "duplicate and merge",
            p.duplicate_shape(&shape, "Copy of it")
                .and_then(|()| p.merge_shapes(&shape, "Copy of it"))
                .map_err(said),
        );
        if let Some(m) = block.and_then(|b| mesh::read(&project.nif, b).ok()) {
            let mut p = project.clone();
            note(
                "new shape from a mesh",
                p.shape_from(
                    &shape,
                    "From outside",
                    &m.positions,
                    &m.uvs,
                    &m.triangles,
                    None,
                )
                .map_err(said),
            );
        }
        let mut p = project.clone();
        note("weld", p.weld(&shape).map(drop).map_err(said));
        let mut p = project.clone();
        note(
            "delete vertices",
            p.delete_vertices(&shape, &half).map_err(said),
        );
        let mut p = project.clone();
        note(
            "separate",
            p.separate(&shape, "Part of it", &half).map_err(said),
        );
        let mut p = project.clone();
        note(
            "paint colour",
            p.paint_colors(&shape, &[(0, [255, 0, 0, 255])])
                .map_err(said),
        );
        // Colours given to a shape with none: everything else of it as it
        // was, and one to paint.
        if let Some(m) = block.and_then(|b| mesh::read(&project.nif, b).ok()) {
            if block.is_some_and(|b| mesh::colors(&project.nif, b).is_ok_and(|c| c.is_empty())) {
                let mut p = project.clone();
                let done = p
                    .add_colors(&shape)
                    .and_then(|()| p.paint_colors(&shape, &[(0, [255, 0, 0, 255])]))
                    .map_err(said);
                if done.is_ok() {
                    let nif = Nif::read(&p.nif.write()).unwrap();
                    let at = mesh::shapes(&nif)
                        .into_iter()
                        .find(|at| mesh::name(&nif, *at).as_deref() == Some(shape.as_str()));
                    let now = at.and_then(|a| mesh::read(&nif, a).ok());
                    let same = now.is_some_and(|n| {
                        n.positions == m.positions
                            && n.uvs == m.uvs
                            && n.triangles == m.triangles
                            && n.normals == m.normals
                    });
                    let skin = |n: &Nif, a: Option<usize>| {
                        a.and_then(|a| apoc_nif::weights::read(n, a).ok())
                    };
                    let skinned = skin(&nif, at) == skin(&project.nif, block);
                    let colours = at
                        .and_then(|a| mesh::colors(&nif, a).ok())
                        .unwrap_or_default();
                    let painted = colours.first() == Some(&[255, 0, 0, 255])
                        && colours.len() == m.positions.len()
                        && colours[1..].iter().all(|c| *c == [255; 4]);
                    note(
                        "given colours: the rest as it was, read back",
                        (same && skinned && painted).then_some(()).ok_or(format!(
                            "{path}: same {same} skin {skinned} painted {painted}"
                        )),
                    );
                }
                note("give colours", done);
            }
        }
        // Parted from its bones: the same vertices and triangles, read back
        // as a shape that follows none, and every change still taken.
        if let Some(m) = block
            .and_then(|b| mesh::read(&project.nif, b).ok())
            .filter(|m| m.skinned)
        {
            let mut p = project.clone();
            let done = p.remove_skinning(&shape).map_err(said);
            if done.is_ok() {
                let nif = Nif::read(&p.nif.write()).unwrap();
                let now = mesh::shapes(&nif)
                    .into_iter()
                    .find(|at| mesh::name(&nif, *at).as_deref() == Some(shape.as_str()))
                    .and_then(|a| mesh::read(&nif, a).ok());
                let mut tris = (
                    m.triangles.clone(),
                    now.as_ref()
                        .map(|n| n.triangles.clone())
                        .unwrap_or_default(),
                );
                tris.0.sort();
                tris.1.sort();
                let same = now.is_some_and(|n| {
                    !n.skinned
                        && n.positions == m.positions
                        && n.uvs == m.uvs
                        && n.normals == m.normals
                }) && tris.0 == tris.1;
                note(
                    "parted from its bones: the rest as it was, read back",
                    same.then_some(()).ok_or(path.clone()),
                );
                note(
                    "parted, then refined",
                    p.refine(&shape, &[]).map(drop).map_err(said),
                );
            }
            note("part from its bones", done);
        }
        // A weight painted: the vertex follows the bone that much, the
        // shape is otherwise as it was, and the file reads back.
        if let Some(skin) = block
            .and_then(|b| apoc_nif::weights::read(&project.nif, b).ok())
            .filter(|s| !s.bones.is_empty())
        {
            let mut p = project.clone();
            let bone = skin.bones[skin.bones.len() / 2].clone();
            let done = p.paint_weights(&shape, &bone, &[(0, 0.5)]).map_err(said);
            if done.is_ok() {
                let nif = Nif::read(&p.nif.write()).unwrap();
                let at = mesh::shapes(&nif)
                    .into_iter()
                    .find(|at| mesh::name(&nif, *at).as_deref() == Some(shape.as_str()));
                let now = at.and_then(|a| apoc_nif::weights::read(&nif, a).ok());
                let got = now.as_ref().and_then(|n| {
                    let k = n.bones.iter().position(|b| *b == bone)?;
                    n.vertices
                        .first()?
                        .iter()
                        .find(|x| x.0 as usize == k && x.1 > 0.0)
                        .map(|x| x.1)
                });
                let rest = now.as_ref().is_some_and(|n| {
                    n.vertices.len() == skin.vertices.len()
                        && n.vertices[1..]
                            .iter()
                            .zip(&skin.vertices[1..])
                            .all(|(a, b)| a == b)
                });
                let m = block.and_then(|b| mesh::read(&project.nif, b).ok());
                let n = at.and_then(|a| mesh::read(&nif, a).ok());
                let same = m.zip(n).is_some_and(|(m, n)| {
                    m.positions == n.positions && m.triangles == n.triangles && m.uvs == n.uvs
                });
                note(
                    "painted: half to the bone, the rest as it was, read back",
                    (got.is_some_and(|w| (0.49..0.7).contains(&w)) && rest && same)
                        .then_some(())
                        .ok_or(format!("{path}: got {got:?} rest {rest} same {same}")),
                );
            }
            note("paint a weight", done);
        }
        let mut p = project.clone();
        note("new slider", p.add_slider("Probe").map_err(said));
        note("save", project.save("Probe").map(drop).map_err(said));
    }
    for (what, (ok, failed, why)) in tally {
        println!(
            "{what}: {ok} did, {failed} did not{}",
            if why.is_empty() {
                String::new()
            } else {
                format!(" ({why})")
            }
        );
    }
}
