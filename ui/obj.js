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
