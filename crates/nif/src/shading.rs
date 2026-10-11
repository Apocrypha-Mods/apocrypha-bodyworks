//! Normals and tangents worked out from a mesh's shape: what has to be done
//! again after its vertices are moved, or it is lit as the shape it was.

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

/// How close two vertices are to be the same point, each way.
const SAME_POINT: f32 = 0.001;

/// A smooth normal for every vertex: the sum of the normals of the triangles
/// around it, each counted by its area.
///
/// Vertices at the same point are lit together, whatever their texture
/// coordinates, when their normals are within `smooth_angle` of each other:
/// each takes the sum of its own normal and the others'. A mesh is cut
/// along its texture seams, and lit separately the two sides of a cut would
/// show as a line down it. Past that angle the cut is a hard edge its maker
/// meant, and each side keeps its own normal.
///
/// Measured against 841 meshes the original program built: this gives the
/// normal they store for 99.9% of vertices.
pub fn smooth_normals(positions: &[[f32; 3]], triangles: &[[u16; 3]]) -> Vec<[f32; 3]> {
    smooth_normals_within(positions, triangles, SMOOTH_ANGLE)
}

pub fn smooth_normals_within(
    positions: &[[f32; 3]],
    triangles: &[[u16; 3]],
    smooth_angle: f32,
) -> Vec<[f32; 3]> {
    let n = positions.len();
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
    let mut out: Vec<[f32; 3]> = own
        .iter()
        .map(|v| normalized(*v, [0.0, 0.0, 1.0]))
        .collect();
    // Each vertex with the others at its point: in order along X, so only
    // near neighbours in that order have to be looked at.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| positions[*a][0].total_cmp(&positions[*b][0]));
    let limit = smooth_angle.to_radians().cos();
    let units = out.clone();
    let mut sums = units.clone();
    for (i, a) in order.iter().enumerate() {
        for b in &order[i + 1..] {
            let (p, q) = (positions[*a], positions[*b]);
            if q[0] - p[0] > SAME_POINT {
                break;
            }
            if (q[1] - p[1]).abs() > SAME_POINT || (q[2] - p[2]).abs() > SAME_POINT {
                continue;
            }
            if dot(units[*a], units[*b]) >= limit {
                for k in 0..3 {
                    sums[*a][k] += units[*b][k];
                    sums[*b][k] += units[*a][k];
                }
            }
        }
    }
    for (v, sum) in sums.iter().enumerate() {
        out[v] = normalized(*sum, units[v]);
    }
    out
}

/// A tangent and a bitangent for every vertex, as these files store them.
///
/// What a file calls the tangent lies along the direction the texture's V
/// grows in: each triangle's direction, at full length whatever its size,
/// summed round the vertex and made square to the normal. What it calls the
/// bitangent is the same sum for U, made square to the normal and then to
/// the tangent.
///
/// That is the other way round from the usual naming. Measured against the
/// meshes the original program built, given the normals as a file stores
/// them, this gives the tangents and bitangents they store to the last
/// digit.
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
            // Only which way the texture is laid matters here, not how
            // stretched it is: each triangle counts the same.
            let r = if du1 * dv2 - du2 * dv1 >= 0.0 {
                1.0
            } else {
                -1.0
            };
            let u_dir = normalized(
                [
                    (e1[0] * dv2 - e2[0] * dv1) * r,
                    (e1[1] * dv2 - e2[1] * dv1) * r,
                    (e1[2] * dv2 - e2[2] * dv1) * r,
                ],
                [0.0; 3],
            );
            let v_dir = normalized(
                [
                    (e2[0] * du1 - e1[0] * du2) * r,
                    (e2[1] * du1 - e1[1] * du2) * r,
                    (e2[2] * du1 - e1[2] * du2) * r,
                ],
                [0.0; 3],
            );
            for v in [a, b, c] {
                for i in 0..3 {
                    along_u[v][i] += u_dir[i];
                    along_v[v][i] += v_dir[i];
                }
            }
        }
    }
    // What is left of `dir` with its part along `along` taken away.
    let without = |dir: [f32; 3], along: [f32; 3]| {
        let d = dot(dir, along);
        [
            dir[0] - along[0] * d,
            dir[1] - along[1] * d,
            dir[2] - along[2] * d,
        ]
    };
    let mut tangents = Vec::with_capacity(n);
    let mut bitangents = Vec::with_capacity(n);
    for i in 0..n {
        let normal = normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
        // Each sum brought to length one before anything is taken from
        // it: the order the original's numbers come out of, to the last bit.
        let v_dir = normalized(along_v[i], [0.0; 3]);
        let u_dir = normalized(along_u[i], [0.0; 3]);
        // Where the texture does not grow along V at all (every triangle
        // round the vertex has one V), the original leaves the normal with
        // its numbers turned round as the tangent, and what is square to
        // the two as the bitangent, neither brought to length one.
        let (tangent, bitangent) = if v_dir == [0.0; 3] {
            let tangent = [normal[1], normal[2], normal[0]];
            (tangent, cross(normal, tangent))
        } else {
            let tangent = normalized(without(v_dir, normal), [0.0; 3]);
            (
                tangent,
                normalized(without(without(u_dir, normal), tangent), [0.0; 3]),
            )
        };
        tangents.push(tangent);
        bitangents.push(bitangent);
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
