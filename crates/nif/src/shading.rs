//! Normals and tangents worked out from a mesh's shape: what has to be done
//! again after its vertices are moved, or it is lit as the shape it was.

use std::collections::HashMap;

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalized(v: [f32; 3], otherwise: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len > 1.0e-12 && len.is_finite() {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        otherwise
    }
}

/// The angle, in degrees, past which two faces meeting at a point are a
/// hard edge and are lit apart.
pub const SMOOTH_ANGLE: f32 = 60.0;

/// A smooth normal for every vertex: the sum of the normals of the triangles
/// around it, each counted by its area.
///
/// Vertices at the same point are lit together, whatever their texture
/// coordinates, when their own faces are within `smooth_angle` of each
/// other: a mesh is cut along its texture seams, and lit separately the two
/// sides of a cut would show as a line down it. Past that angle the cut is
/// a hard edge its maker meant, and each side keeps its own normal.
pub fn smooth_normals(positions: &[[f32; 3]], triangles: &[[u16; 3]]) -> Vec<[f32; 3]> {
    smooth_normals_within(positions, triangles, SMOOTH_ANGLE)
}

pub fn smooth_normals_within(
    positions: &[[f32; 3]],
    triangles: &[[u16; 3]],
    smooth_angle: f32,
) -> Vec<[f32; 3]> {
    let n = positions.len();
    // Each vertex's own faces first.
    let mut own = vec![[0.0f32; 3]; n];
    for t in triangles {
        let [a, b, c] = t.map(|i| i as usize);
        if a >= n || b >= n || c >= n {
            continue;
        }
        // Not normalised: its length is twice the triangle's area, which is
        // the weight.
        let face = cross(
            sub(positions[b], positions[a]),
            sub(positions[c], positions[a]),
        );
        for v in [a, b, c] {
            for i in 0..3 {
                own[v][i] += face[i];
            }
        }
    }
    // The vertices at each point.
    let mut at_point: HashMap<[u32; 3], Vec<usize>> = HashMap::with_capacity(n);
    for (i, p) in positions.iter().enumerate() {
        // -0.0 and 0.0 are the same point.
        at_point
            .entry(p.map(|c| (c + 0.0).to_bits()))
            .or_default()
            .push(i);
    }
    let limit = smooth_angle.to_radians().cos();
    let mut out: Vec<[f32; 3]> = own
        .iter()
        .map(|v| normalized(*v, [0.0, 0.0, 1.0]))
        .collect();
    for group in at_point.values().filter(|g| g.len() > 1) {
        let units: Vec<[f32; 3]> = group.iter().map(|v| out[*v]).collect();
        let mut joined = Vec::with_capacity(group.len());
        for (i, v) in group.iter().enumerate() {
            let mut sum = own[*v];
            for (j, other) in group.iter().enumerate() {
                if i != j && dot(units[i], units[j]) >= limit {
                    for k in 0..3 {
                        sum[k] += own[*other][k];
                    }
                }
            }
            joined.push(normalized(sum, units[i]));
        }
        for (v, normal) in group.iter().zip(joined) {
            out[*v] = normal;
        }
    }
    out
}

/// A tangent and a bitangent for every vertex, as these files store them:
/// the directions across the surface in which the texture's V and U grow,
/// each square to the normal.
///
/// That is the other way round from the usual naming. Measured on shipped
/// meshes, what a file calls the tangent lies along V and what it calls the
/// bitangent along U, and this returns them under the file's names.
pub fn tangent_space(
    positions: &[[f32; 3]],
    uvs: &[[f32; 2]],
    normals: &[[f32; 3]],
    triangles: &[[u16; 3]],
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let n = positions.len();
    let mut along_u = vec![[0.0f32; 3]; n];
    let mut along_v = vec![[0.0f32; 3]; n];
    if uvs.len() == n && normals.len() == n {
        for t in triangles {
            let [a, b, c] = t.map(|i| i as usize);
            if a >= n || b >= n || c >= n {
                continue;
            }
            let (e1, e2) = (
                sub(positions[b], positions[a]),
                sub(positions[c], positions[a]),
            );
            let (du1, dv1) = (uvs[b][0] - uvs[a][0], uvs[b][1] - uvs[a][1]);
            let (du2, dv2) = (uvs[c][0] - uvs[a][0], uvs[c][1] - uvs[a][1]);
            let det = du1 * dv2 - du2 * dv1;
            if det.abs() < 1.0e-20 {
                continue;
            }
            let r = 1.0 / det;
            let u_dir = [
                (e1[0] * dv2 - e2[0] * dv1) * r,
                (e1[1] * dv2 - e2[1] * dv1) * r,
                (e1[2] * dv2 - e2[2] * dv1) * r,
            ];
            let v_dir = [
                (e2[0] * du1 - e1[0] * du2) * r,
                (e2[1] * du1 - e1[1] * du2) * r,
                (e2[2] * du1 - e1[2] * du2) * r,
            ];
            for v in [a, b, c] {
                for i in 0..3 {
                    along_u[v][i] += u_dir[i];
                    along_v[v][i] += v_dir[i];
                }
            }
        }
    }
    let square = |dir: [f32; 3], normal: [f32; 3], otherwise: [f32; 3]| {
        let d = dot(dir, normal);
        normalized(
            [
                dir[0] - normal[0] * d,
                dir[1] - normal[1] * d,
                dir[2] - normal[2] * d,
            ],
            otherwise,
        )
    };
    let mut tangents = Vec::with_capacity(n);
    let mut bitangents = Vec::with_capacity(n);
    for i in 0..n {
        let normal = normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
        tangents.push(square(along_v[i], normal, [0.0, 1.0, 0.0]));
        bitangents.push(square(along_u[i], normal, [1.0, 0.0, 0.0]));
    }
    (tangents, bitangents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_square_faces_one_way_and_its_tangents_follow_the_texture_as_files_name_them() {
        // In the XY plane, U along X and V along Y.
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let triangles = [[0, 1, 2], [0, 2, 3]];
        let normals = smooth_normals(&positions, &triangles);
        assert!(normals.iter().all(|n| *n == [0.0, 0.0, 1.0]));
        let (t, b) = tangent_space(&positions, &uvs, &normals, &triangles);
        // The file's tangent is along V, its bitangent along U.
        assert!(t.iter().all(|v| *v == [0.0, 1.0, 0.0]));
        assert!(b.iter().all(|v| *v == [1.0, 0.0, 0.0]));
    }

    #[test]
    fn a_seam_is_lit_as_one_surface_and_a_hard_edge_is_not() {
        // Two triangles meeting at an edge whose ends are stored twice, as
        // on a texture seam. `lift` raises the far corner of the second.
        let folded = |lift: f32| {
            let positions = [
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [-1.0, 0.0, lift],
            ];
            smooth_normals(&positions, &[[0, 2, 1], [3, 4, 5]])
        };
        // A gentle fold: the two copies of each end share one normal.
        let gentle = folded(0.3);
        assert_eq!(gentle[0], gentle[3]);
        assert_eq!(gentle[1], gentle[4]);
        assert!(gentle[0][2] > 0.9);
        // Folded past a right angle: an edge, and each side keeps its own.
        let sharp = folded(5.0);
        assert_ne!(sharp[0], sharp[3]);
        assert_eq!(sharp[0], [0.0, 0.0, 1.0]);
        // A vertex no triangle uses still has a normal.
        assert_eq!(smooth_normals(&[[1.0, 2.0, 3.0]], &[]), [[0.0, 0.0, 1.0]]);
    }
}
