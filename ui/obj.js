// Wavefront OBJ, the plain-text mesh every 3D program reads: shapes written
// out, and a shape's vertices read back in.

/**
 * Shapes as one OBJ file. Each: `{ name, positions, uvs, triangles }`, flat
 * arrays. A vertex keeps its number within its shape, so a file edited
 * elsewhere without reordering can be read back onto the shape.
 */
export function write(shapes) {
  const out = ["# Apocrypha BodySlide"];
  let before = 0;
  for (const s of shapes) {
    const count = s.positions.length / 3;
    const textured = s.uvs && s.uvs.length === count * 2;
    out.push(`o ${s.name.replace(/\s+/g, "_")}`);
    for (let i = 0; i < count; i++) {
      out.push(`v ${s.positions[i * 3]} ${s.positions[i * 3 + 1]} ${s.positions[i * 3 + 2]}`);
    }
    // The games' textures start at the top; OBJ's at the bottom.
    if (textured) for (let i = 0; i < count; i++) out.push(`vt ${s.uvs[i * 2]} ${1 - s.uvs[i * 2 + 1]}`);
    for (let t = 0; t < s.triangles.length; t += 3) {
      const [a, b, c] = [s.triangles[t], s.triangles[t + 1], s.triangles[t + 2]].map((v) => v + before + 1);
      out.push(textured ? `f ${a}/${a} ${b}/${b} ${c}/${c}` : `f ${a} ${b} ${c}`);
    }
    before += count;
  }
  return out.join("\n") + "\n";
}

/** The vertices of an OBJ file, in the file's order, as a flat array. */
export function positions(text) {
  const out = [];
  for (const line of text.split(/\r?\n/)) {
    if (line[0] !== "v" || (line[1] !== " " && line[1] !== "\t")) continue;
    const [x, y, z] = line.slice(2).trim().split(/\s+/).map(Number);
    // As a mesh will hold them: a number too large for it is not a number there.
    if (![x, y, z].every((c) => Number.isFinite(Math.fround(c)))) throw new Error("The file has a vertex that is not three numbers a mesh can hold.");
    out.push(x, y, z);
  }
  return Float32Array.from(out);
}

/**
 * An OBJ file as a mesh a shape can be made of: `{ positions, uvs, triangles }`,
 * flat arrays. A vertex used with two places in the texture becomes two
 * vertices, and a face of more than three corners becomes a fan of
 * triangles. Throws what is wrong with the file.
 */
export function mesh(text) {
  const places = [];
  const textures = [];
  const positions = [];
  const uvs = [];
  const triangles = [];
  const made = new Map();
  const corner = (word) => {
    const [v, vt] = word.split("/");
    // Counted from one; a number below nothing counts back from the last.
    const at = (n, count) => (n < 0 ? count + n : n - 1);
    const p = at(Number(v), places.length / 3);
    const t = vt ? at(Number(vt), textures.length / 2) : -1;
    if (!Number.isInteger(p) || p < 0 || p >= places.length / 3) throw new Error("The file has a face with a vertex it does not have.");
    if (vt && !Number.isInteger(t)) throw new Error("The file has a face with a place in the texture that is not a whole number.");
    const key = `${p}/${t}`;
    let index = made.get(key);
    if (index === undefined) {
      index = positions.length / 3;
      made.set(key, index);
      positions.push(places[p * 3], places[p * 3 + 1], places[p * 3 + 2]);
      // The games' textures start at the top; OBJ's at the bottom.
      if (t >= 0 && t < textures.length / 2) uvs.push(textures[t * 2], 1 - textures[t * 2 + 1]);
      else uvs.push(0, 0);
    }
    return index;
  };
  for (const line of text.split(/\r?\n/)) {
    const words = line.trim().split(/\s+/);
    if (words[0] === "v") {
      const [x, y, z] = words.slice(1, 4).map(Number);
      if (![x, y, z].every((c) => Number.isFinite(Math.fround(c)))) throw new Error("The file has a vertex that is not three numbers a mesh can hold.");
      places.push(x, y, z);
    } else if (words[0] === "vt") {
      const [u, v] = words.slice(1, 3).map(Number);
      // As a mesh will hold them: a number too large for it is nowhere.
      const held = (c) => (Number.isFinite(Math.fround(c)) ? c : 0);
      textures.push(held(u), held(v));
    } else if (words[0] === "f") {
      const face = words.slice(1).map(corner);
      for (let k = 1; k + 1 < face.length; k++) triangles.push(face[0], face[k], face[k + 1]);
    }
  }
  if (!triangles.length) throw new Error("The file has no faces.");
  if (positions.length / 3 > 65535 || triangles.length / 3 > 65535) throw new Error("The file has more vertices or triangles than one shape can hold.");
  return { positions: Float32Array.from(positions), uvs: Float32Array.from(uvs), triangles: Uint32Array.from(triangles) };
}
