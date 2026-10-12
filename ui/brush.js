// What a brush needs to know about a shape, and what each brush does to it.
// Nothing here touches the page: positions are flat arrays of x, y, z.

/**
 * Where a ray first meets a shape. `triangles` are vertex numbers in threes.
 * Returns `{ t, point, normal }` or null.
 */
export function hit(origin, dir, positions, triangles) {
  const [ox, oy, oz] = origin;
  const [dx, dy, dz] = dir;
  let best = Infinity;
  let found = -1;
  for (let i = 0; i < triangles.length; i += 3) {
    const a = triangles[i] * 3, b = triangles[i + 1] * 3, c = triangles[i + 2] * 3;
    const ax = positions[a], ay = positions[a + 1], az = positions[a + 2];
    const e1x = positions[b] - ax, e1y = positions[b + 1] - ay, e1z = positions[b + 2] - az;
    const e2x = positions[c] - ax, e2y = positions[c + 1] - ay, e2z = positions[c + 2] - az;
    const px = dy * e2z - dz * e2y, py = dz * e2x - dx * e2z, pz = dx * e2y - dy * e2x;
    const det = e1x * px + e1y * py + e1z * pz;
    if (det > -1e-12 && det < 1e-12) continue;
    const inv = 1 / det;
    const tx = ox - ax, ty = oy - ay, tz = oz - az;
    const u = (tx * px + ty * py + tz * pz) * inv;
    if (u < 0 || u > 1) continue;
    const qx = ty * e1z - tz * e1y, qy = tz * e1x - tx * e1z, qz = tx * e1y - ty * e1x;
    const v = (dx * qx + dy * qy + dz * qz) * inv;
    if (v < 0 || u + v > 1) continue;
    const t = (e2x * qx + e2y * qy + e2z * qz) * inv;
    if (t > 1e-6 && t < best) {
      best = t;
      found = i;
    }
  }
  if (found < 0) return null;
  const a = triangles[found] * 3, b = triangles[found + 1] * 3, c = triangles[found + 2] * 3;
  const e1 = [positions[b] - positions[a], positions[b + 1] - positions[a + 1], positions[b + 2] - positions[a + 2]];
  const e2 = [positions[c] - positions[a], positions[c + 1] - positions[a + 1], positions[c + 2] - positions[a + 2]];
  let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
  const l = Math.hypot(n[0], n[1], n[2]) || 1;
  n = [n[0] / l, n[1] / l, n[2] / l];
  // Toward where the ray came from, whichever way the triangle is wound.
  if (n[0] * dx + n[1] * dy + n[2] * dz > 0) n = [-n[0], -n[1], -n[2]];
  return { t: best, point: [ox + dx * best, oy + dy * best, oz + dz * best], normal: n, corners: [triangles[found], triangles[found + 1], triangles[found + 2]] };
}

/**
 * The vertices within `radius` of `point`, each with how much of the brush
 * reaches it: all of it at the middle, none at the edge, eased between.
 * Returns `{ vertices: Uint32Array, weights: Float32Array }`.
 */
export function reach(point, radius, positions) {
  const vertices = [];
  const weights = [];
  const r2 = radius * radius;
  for (let v = 0, i = 0; i < positions.length; v++, i += 3) {
    const x = positions[i] - point[0], y = positions[i + 1] - point[1], z = positions[i + 2] - point[2];
    const d2 = x * x + y * y + z * z;
    if (d2 >= r2) continue;
    const k = 1 - d2 / r2;
    vertices.push(v);
    weights.push(k * k);
  }
  return { vertices: Uint32Array.from(vertices), weights: Float32Array.from(weights) };
}

/**
 * For each group of vertices at one place (`weld[v]` is the group's first),
 * the groups its triangles join it to. Returns `{ start, next }`: the
 * neighbours of group `g` are `next[start[g]] … next[start[g + 1] - 1]`.
 */
export function neighbours(triangles, weld) {
  const sets = new Map();
  const join = (a, b) => {
    if (a === b) return;
    let s = sets.get(a);
    if (!s) sets.set(a, (s = new Set()));
    s.add(b);
  };
  for (let i = 0; i < triangles.length; i += 3) {
    const a = weld[triangles[i]], b = weld[triangles[i + 1]], c = weld[triangles[i + 2]];
    join(a, b), join(b, a), join(b, c), join(c, b), join(a, c), join(c, a);
  }
  const start = new Uint32Array(weld.length + 1);
  let total = 0;
  for (let g = 0; g < weld.length; g++) {
    start[g] = total;
    total += sets.get(g)?.size || 0;
  }
  start[weld.length] = total;
  const next = new Uint32Array(total);
  for (const [g, s] of sets) next.set([...s], start[g]);
  return { start, next };
}

/** What a brush reaches, less what a mask covers of each vertex (0 nothing, 1 all of it). */
export function spared(reached, mask) {
  const { vertices, weights } = reached;
  for (let k = 0; k < vertices.length; k++) weights[k] *= 1 - mask[vertices[k]];
  return reached;
}

/** Cover what the brush reaches with a mask, soft at its edge; or with `off` uncover it. */
export function cover(mask, reached, off) {
  const { vertices, weights } = reached;
  for (let k = 0; k < vertices.length; k++) {
    const much = Math.min(1, weights[k] * 4);
    const v = vertices[k];
    mask[v] = off ? Math.min(mask[v], 1 - much) : Math.max(mask[v], much);
  }
}

/**
 * Of what a brush reaches, only what is joined to `from` (vertices of the
 * triangle under the brush) through other vertices it reaches: the sleeve
 * under the brush and not the body behind it.
 */
export function joined(reached, weld, near, from) {
  const inside = new Map();
  reached.vertices.forEach((v, k) => {
    const g = weld[v];
    if (!inside.has(g)) inside.set(g, []);
    inside.get(g).push(k);
  });
  const seen = new Set();
  const todo = from.map((v) => weld[v]).filter((g) => inside.has(g));
  for (const g of todo) seen.add(g);
  while (todo.length) {
    const g = todo.pop();
    for (let n = near.start[g]; n < near.start[g + 1]; n++) {
      const to = near.next[n];
      if (inside.has(to) && !seen.has(to)) {
        seen.add(to);
        todo.push(to);
      }
    }
  }
  const keep = [];
  for (const g of seen) keep.push(...inside.get(g));
  keep.sort((a, b) => a - b);
  return { vertices: Uint32Array.from(keep, (k) => reached.vertices[k]), weights: Float32Array.from(keep, (k) => reached.weights[k]) };
}

/**
 * Take back some of what a slider has done to what the brush reaches.
 * `left` is how far the slider still has each vertex from where it started,
 * x, y, z a vertex, and is brought down with the positions.
 */
export function erase(positions, left, reached, amount) {
  const { vertices, weights } = reached;
  for (let k = 0; k < vertices.length; k++) {
    const i = vertices[k] * 3;
    const by = Math.min(1, amount * weights[k]);
    for (let c = 0; c < 3; c++) {
      const back = left[i + c] * by;
      positions[i + c] -= back;
      left[i + c] -= back;
    }
  }
}

/** A mask grown by one ring of vertices, or with `less` shrunk by one. */
export function spread(mask, weld, near, less) {
  const full = (v) => (mask[v] >= 0.5) !== Boolean(less);
  const out = new Float32Array(mask);
  for (let v = 0; v < mask.length; v++) {
    if (full(v)) continue;
    const g = weld[v];
    for (let n = near.start[g]; n < near.start[g + 1]; n++) {
      if (full(near.next[n])) {
        out[v] = less ? 0 : 1;
        break;
      }
    }
  }
  // Vertices at one place are covered alike.
  for (let v = 0; v < mask.length; v++) if (weld[v] !== v && out[weld[v]] !== out[v]) out[v] = out[weld[v]];
  return out;
}

/** The bone on the other side of the body, as the games name theirs; the same name for one of the middle. */
export function twin(bone) {
  for (const [l, r] of [[" L ", " R "], ["Left", "Right"], ["left", "right"]]) {
    if (bone.includes(l)) return bone.replaceAll(l, r).replaceAll("[L", "[R");
    if (bone.includes(r)) return bone.replaceAll(r, l).replaceAll("[R", "[L");
  }
  return bone;
}

/** Of a triangle's three edges, the one nearest a point on it: its two vertices. */
export function nearestEdge(corners, point, positions) {
  let best = null;
  let least = Infinity;
  for (let k = 0; k < 3; k++) {
    const [a, b] = [corners[k], corners[(k + 1) % 3]];
    const p = [0, 1, 2].map((c) => positions[a * 3 + c]);
    const d = [0, 1, 2].map((c) => positions[b * 3 + c] - p[c]);
    const w = [0, 1, 2].map((c) => point[c] - p[c]);
    const length = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] || 1;
    const t = Math.max(0, Math.min(1, (w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / length));
    const off = Math.hypot(...[0, 1, 2].map((c) => w[c] - d[c] * t));
    if (off < least) {
      least = off;
      best = [a, b];
    }
  }
  return best;
}

/** Push what the brush reaches out along its normals (or in, with a negative `amount`). */
export function inflate(positions, normals, reached, amount) {
  const { vertices, weights } = reached;
  for (let k = 0; k < vertices.length; k++) {
    const i = vertices[k] * 3;
    const by = amount * weights[k];
    positions[i] += normals[i] * by;
    positions[i + 1] += normals[i + 1] * by;
    positions[i + 2] += normals[i + 2] * by;
  }
}

/** Carry what the brush reached from where it was (`from`) by `by`, most at the middle. */
export function carry(positions, from, reached, by) {
  const { vertices, weights } = reached;
  for (let k = 0; k < vertices.length; k++) {
    const i = vertices[k] * 3;
    positions[i] = from[k * 3] + by[0] * weights[k];
    positions[i + 1] = from[k * 3 + 1] + by[1] * weights[k];
    positions[i + 2] = from[k * 3 + 2] + by[2] * weights[k];
  }
}

/** Draw what the brush reaches toward the middle of its neighbours. */
export function smooth(positions, weld, near, reached, amount) {
  const { vertices, weights } = reached;
  const to = new Float32Array(vertices.length * 3);
  for (let k = 0; k < vertices.length; k++) {
    const g = weld[vertices[k]];
    const from = near.start[g], end = near.start[g + 1];
    const i = vertices[k] * 3;
    if (end === from) {
      to.set([positions[i], positions[i + 1], positions[i + 2]], k * 3);
      continue;
    }
    let x = 0, y = 0, z = 0;
    for (let n = from; n < end; n++) {
      const j = near.next[n] * 3;
      x += positions[j], y += positions[j + 1], z += positions[j + 2];
    }
    const count = end - from;
    to.set([x / count, y / count, z / count], k * 3);
  }
  for (let k = 0; k < vertices.length; k++) {
    const i = vertices[k] * 3;
    const by = Math.min(1, amount * weights[k]);
    positions[i] += (to[k * 3] - positions[i]) * by;
    positions[i + 1] += (to[k * 3 + 1] - positions[i + 1]) * by;
    positions[i + 2] += (to[k * 3 + 2] - positions[i + 2]) * by;
  }
}

/** The same place on the other side of the body: the games' meshes are mirrored across X. */
export const across = (p) => [-p[0], p[1], p[2]];

/**
 * What moved between `before` and `after`, as a stroke the module reads:
 * for each vertex its number and how far it went, sixteen bytes each.
 * Returns `{ bytes: Uint8Array, vertices: Uint32Array, offsets: Float32Array }`.
 */
export function moved(before, after, scale = 1) {
  const vertices = [];
  const offsets = [];
  for (let v = 0, i = 0; i < before.length; v++, i += 3) {
    const x = after[i] - before[i], y = after[i + 1] - before[i + 1], z = after[i + 2] - before[i + 2];
    if (x === 0 && y === 0 && z === 0) continue;
    vertices.push(v);
    offsets.push(x * scale, y * scale, z * scale);
  }
  const bytes = new Uint8Array(vertices.length * 16);
  const view = new DataView(bytes.buffer);
  for (let k = 0; k < vertices.length; k++) {
    view.setUint32(k * 16, vertices[k], true);
    for (let c = 0; c < 3; c++) view.setFloat32(k * 16 + 4 + c * 4, offsets[k * 3 + c], true);
  }
  return { bytes, vertices: Uint32Array.from(vertices), offsets: Float32Array.from(offsets) };
}
