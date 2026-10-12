//! Take the first half of a mesh's first shape's vertices out and say what
//! the file then holds, read back: to see where a kind of shape is left
//! out of step.
//!
//!     cargo run --release -p apoc-bodyslide --example cutprobe -- <file.nif>

use apoc_bodyslide::project::Project;
use apoc_nif::{mesh, Nif};

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let mut p = Project::from_mesh("Probe", "meshes/probe.nif", &bytes).unwrap();
        let shape = p.set.shapes[0].name.clone();
        let at = mesh::shapes(&p.nif)[0];
        let was = mesh::read(&p.nif, at).unwrap();
        let half: Vec<u16> = (0..(was.positions.len() / 2) as u16).collect();
        println!(
            "{path}: {} vertices, {} triangles; taking {} out",
            was.positions.len(),
            was.triangles.len(),
            half.len()
        );
        println!(
            "  done: {:?}",
            p.delete_vertices(&shape, &half).map_err(|e| e.to_string())
        );
        let nif = Nif::read(&p.nif.write()).unwrap();
        for at in mesh::shapes(&nif) {
            match mesh::read(&nif, at) {
                Ok(m) => println!(
                    "  read back: {} vertices, {} triangles, {} normals, {} uvs",
                    m.positions.len(),
                    m.triangles.len(),
                    m.normals.len(),
                    m.uvs.len()
                ),
                Err(e) => println!("  read back: {e}"),
            }
        }
    }
}
