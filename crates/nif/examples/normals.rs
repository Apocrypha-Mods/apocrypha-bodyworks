//! How close the normals and tangents this works out are to the ones a mesh
//! was shipped with.
fn main() {
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let nif = apoc_nif::Nif::read(&bytes).unwrap();
        for at in apoc_nif::mesh::shapes(&nif) {
            let m = match apoc_nif::mesh::read(&nif, at) {
                Ok(m) => m,
                Err(e) => {
                    println!("unread: {e}");
                    continue;
                }
            };
            if m.normals.len() != m.positions.len() {
                println!(
                    "{} normals for {} vertices",
                    m.normals.len(),
                    m.positions.len()
                );
                continue;
            }
            let limit: f32 = std::env::var("ANGLE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(apoc_nif::shading::SMOOTH_ANGLE);
            let ours = apoc_nif::shading::smooth_normals_within(&m.positions, &m.triangles, limit);
            let angle = |a: [f32; 3], b: [f32; 3]| {
                (a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees()
            };
            let mut errs: Vec<f32> = ours
                .iter()
                .zip(&m.normals)
                .map(|(a, b)| angle(*a, *b))
                .collect();
            errs.sort_by(f32::total_cmp);
            let mean = errs.iter().sum::<f32>() / errs.len() as f32;
            print!(
                "{} [{}] normals: mean {:.2}° median {:.2}° p99 {:.2}° worst {:.1}°",
                path.rsplit('/').next().unwrap(),
                m.name,
                mean,
                errs[errs.len() / 2],
                errs[errs.len() * 99 / 100],
                errs[errs.len() - 1]
            );
            // Tangents: which stored vector is which of ours?
            if let Some((st, sb)) = apoc_nif::mesh::stored_tangents(&nif, at)
                .filter(|_| m.uvs.len() == m.positions.len())
            {
                let (t, b) = apoc_nif::shading::tangent_space(
                    &m.positions,
                    &m.uvs,
                    &m.normals,
                    &m.triangles,
                );
                let neg = |x: [f32; 3]| [-x[0], -x[1], -x[2]];
                let mean = |stored: &[[f32; 3]], mine: &dyn Fn(usize) -> [f32; 3]| {
                    stored
                        .iter()
                        .enumerate()
                        .map(|(v, s)| angle(*s, mine(v)))
                        .sum::<f32>()
                        / stored.len() as f32
                };
                print!("  storedT~T {:.0}° ~B {:.0}° ~-B {:.0}° | storedB~T {:.0}° ~B {:.0}° ~-B {:.0}° ~-T {:.0}°", mean(&st, &|v| t[v]), mean(&st, &|v| b[v]), mean(&st, &|v| neg(b[v])), mean(&sb, &|v| t[v]), mean(&sb, &|v| b[v]), mean(&sb, &|v| neg(b[v])), mean(&sb, &|v| neg(t[v])));
            }
            println!();
        }
    }
}
