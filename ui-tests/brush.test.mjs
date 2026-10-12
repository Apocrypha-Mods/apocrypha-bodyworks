// node --test ui-tests/
import test from "node:test";
import assert from "node:assert/strict";
import * as brush from "../ui/brush.js";

// A square in the plane z = 0, two triangles, facing +z.
const square = new Float32Array([0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0]);
const faces = new Uint32Array([0, 1, 2, 0, 2, 3]);

test("a ray meets the nearest triangle, and its normal faces the ray", () => {
  const found = brush.hit([0.25, 0.5, 5], [0, 0, -1], square, faces);
  assert.deepEqual(found.point, [0.25, 0.5, 0]);
  assert.deepEqual(found.normal, [0, 0, 1]);
  assert.equal(found.t, 5);
  // From behind, the normal still faces where the ray came from.
  assert.deepEqual(brush.hit([0.25, 0.5, -5], [0, 0, 1], square, faces).normal.map((c) => c + 0), [0, 0, -1]);
  assert.equal(brush.hit([2, 2, 5], [0, 0, -1], square, faces), null);
  assert.equal(brush.hit([0.5, 0.5, 5], [0, 0, 1], square, faces), null);
});

test("a brush reaches what is inside it, most at the middle", () => {
  const got = brush.reach([0, 0, 0], 1.2, square);
  assert.deepEqual([...got.vertices], [0, 1, 3]);
  assert.equal(got.weights[0], 1);
  assert.ok(got.weights[1] > 0 && got.weights[1] < 0.2);
  assert.equal(brush.reach([9, 9, 9], 1, square).vertices.length, 0);
});

test("inflating moves along the normals and deflating back", () => {
  const p = new Float32Array(square);
  const normals = new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]);
  const reached = brush.reach([0, 0, 0], 1.2, p);
  brush.inflate(p, normals, reached, 0.5);
  assert.equal(p[2], 0.5);
  assert.equal(p[8], 0);
  brush.inflate(p, normals, reached, -0.5);
  assert.deepEqual([...p], [...square]);
});

test("what is carried goes from where it was, not from where it got to", () => {
  const p = new Float32Array(square);
  const reached = brush.reach([0, 0, 0], 0.5, p);
  const from = new Float32Array([0, 0, 0]);
  brush.carry(p, from, reached, [1, 0, 0]);
  brush.carry(p, from, reached, [0, 2, 0]);
  assert.deepEqual([...p.subarray(0, 3)], [0, 2, 0]);
});

test("smoothing draws a vertex toward its neighbours, and vertices at one place alike", () => {
  // A fan round a raised middle, whose middle is two vertices at one place.
  const p = new Float32Array([0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, -1, 0, 0, 0, -1, 0]);
  const t = new Uint32Array([0, 2, 3, 0, 3, 4, 1, 4, 5, 1, 5, 2]);
  const weld = new Uint32Array([0, 0, 2, 3, 4, 5]);
  const near = brush.neighbours(t, weld);
  assert.deepEqual([...near.next.subarray(near.start[0], near.start[1])].sort(), [2, 3, 4, 5]);
  assert.equal(near.start[2] - near.start[1], 0);
  brush.smooth(p, weld, near, brush.reach([0, 0, 1], 0.1, p), 0.5);
  assert.deepEqual([...p.subarray(0, 6)], [0, 0, 0.5, 0, 0, 0.5]);
});

test("a stroke is what moved, scaled to what the slider stood at", () => {
  const after = new Float32Array(square);
  after[5] = 0.5;
  const got = brush.moved(square, after, 2);
  assert.deepEqual([...got.vertices], [1]);
  assert.deepEqual([...got.offsets], [0, 0, 1]);
  const view = new DataView(got.bytes.buffer);
  assert.equal(got.bytes.length, 16);
  assert.equal(view.getUint32(0, true), 1);
  assert.equal(view.getFloat32(12, true), 1);
  assert.deepEqual(brush.across([1, 2, 3]), [-1, 2, 3]);
});

test("a mask covers what it is painted on, and the brushes leave that alone", () => {
  const mask = new Float32Array(4);
  brush.cover(mask, brush.reach([0, 0, 0], 1.2, square), false);
  assert.equal(mask[0], 1);
  assert.ok(mask[1] > 0 && mask[1] < 1);
  assert.equal(mask[2], 0);
  const reached = brush.spared(brush.reach([0, 0, 0], 1.2, square), mask);
  assert.equal(reached.weights[0], 0);
  const p = new Float32Array(square);
  brush.inflate(p, new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]), reached, 1);
  assert.equal(p[2], 0);
  assert.ok(p[5] > 0);
  // Uncovered again, it is as it was.
  brush.cover(mask, brush.reach([0, 0, 0], 1.2, square), true);
  assert.equal(mask[0], 0);
  assert.ok(mask[1] < 1);
});

test("shapes go out as OBJ and a shape's vertices come back as they went", async () => {
  const obj = await import("../ui/obj.js");
  const uvs = new Float32Array([0, 0, 1, 0, 1, 1, 0, 1]);
  const text = obj.write([
    { name: "A shape", positions: square, uvs, triangles: faces },
    { name: "Bare", positions: square, uvs: null, triangles: faces.subarray(0, 3) },
  ]);
  const lines = text.trim().split("\n");
  assert.equal(lines.filter((l) => l.startsWith("v ")).length, 8);
  assert.equal(lines.filter((l) => l.startsWith("vt ")).length, 4);
  assert.ok(lines.includes("o A_shape"));
  assert.ok(lines.includes("vt 1 0"));
  assert.ok(lines.includes("f 1/1 2/2 3/3"));
  // The second shape's triangles count on from the first's vertices.
  assert.ok(lines.includes("f 5 6 7"));
  assert.deepEqual([...obj.positions(text)], [...square, ...square]);
  assert.deepEqual([...obj.positions("vn 0 0 1\r\nv 1 2 3 0.5\r\nvt 0 0\n")], [1, 2, 3]);
  assert.throws(() => obj.positions("v 1 two 3"));
});

test("joined keeps what is reached through the surface, and erase takes a slider's effect back", () => {
  // Two squares apart in z, one above the other: the brush reaches both.
  const two = new Float32Array([...square, 0, 0, 0.1, 1, 0, 0.1, 1, 1, 0.1, 0, 1, 0.1]);
  const tris = new Uint32Array([0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
  const weld = new Uint32Array([0, 1, 2, 3, 4, 5, 6, 7]);
  const near = brush.neighbours(tris, weld);
  const reached = brush.reach([0, 0, 0], 5, two);
  assert.equal(reached.vertices.length, 8);
  const top = brush.joined(reached, weld, near, [0, 1, 2]);
  assert.deepEqual([...top.vertices], [0, 1, 2, 3]);
  assert.equal(top.weights[0], reached.weights[0]);
  assert.equal(brush.hit([0.25, 0.5, 5], [0, 0, -1], two, tris).corners.length, 3);

  const p = new Float32Array([0, 0, 2, 5, 5, 5]);
  const left = new Float32Array([0, 0, 2, 0, 0, 0]);
  brush.erase(p, left, { vertices: new Uint32Array([0]), weights: new Float32Array([1]) }, 0.5);
  assert.deepEqual([...p], [0, 0, 1, 5, 5, 5]);
  assert.deepEqual([...left], [0, 0, 1, 0, 0, 0]);
  brush.erase(p, left, { vertices: new Uint32Array([0]), weights: new Float32Array([1]) }, 9);
  assert.equal(p[2], 0);
});

test("a mask grows and shrinks by a ring, and a bone has its twin", () => {
  // A strip of four vertices in a row: 0-1-2-3.
  const weld = new Uint32Array([0, 1, 2, 3]);
  const near = brush.neighbours(new Uint32Array([0, 1, 1, 1, 2, 2, 2, 3, 3]), weld);
  const grown = brush.spread(new Float32Array([1, 0, 0, 0]), weld, near, false);
  assert.deepEqual([...grown], [1, 1, 0, 0]);
  assert.deepEqual([...brush.spread(grown, weld, near, true)], [1, 0, 0, 0]);
  assert.equal(brush.twin("NPC L Thigh [LThg]"), "NPC R Thigh [RThg]");
  assert.equal(brush.twin("NPC R Clavicle [RClv]"), "NPC L Clavicle [LClv]");
  assert.equal(brush.twin("NPC Spine2 [Spn2]"), "NPC Spine2 [Spn2]");
});

test("vertices at one place are found as one, however many there are", async () => {
  const { welded } = await import("../ui/view.js");
  const p = new Float32Array([0, 0, 0, 1, 2, 3, 0, 0, 0.0001, 1, 2, 3, -1, 2, 3, 0, 0, 0.001]);
  assert.deepEqual([...welded(p)], [0, 1, 0, 1, 4, 5]);
  // Many vertices, every one twice: each second finds its first.
  const n = 50000;
  const big = new Float32Array(n * 6);
  for (let i = 0; i < n; i++) {
    const at = [Math.sin(i) * 60, Math.cos(i * 1.7) * 60, (i % 997) * 0.13];
    big.set(at, i * 3);
    big.set(at, (n + i) * 3);
  }
  const start = performance.now();
  const w = welded(big);
  const took = performance.now() - start;
  for (let i = 0; i < n; i += 97) assert.equal(w[n + i], w[i]);
  assert.ok(took < 200, `${took} ms for ${n * 2} vertices`);
});

test("an OBJ file becomes a mesh: seams split, faces fanned, textures the right way up", async () => {
  const obj = await import("../ui/obj.js");
  const text = ["v 0 0 0", "v 1 0 0", "v 1 1 0", "v 0 1 0", "vt 0 0", "vt 1 0", "vt 1 1", "vt 0 1", "vt 0.5 0.25", "f 1/1 2/2 3/3 4/4", "f 1/5 2/2 -2/3"].join("\n");
  const got = obj.mesh(text);
  // Vertex 1 is used with two places in the texture: two vertices.
  assert.equal(got.positions.length / 3, 5);
  assert.deepEqual([...got.triangles], [0, 1, 2, 0, 2, 3, 4, 1, 2]);
  assert.deepEqual([...got.uvs.subarray(0, 2)], [0, 1]);
  assert.deepEqual([...got.uvs.subarray(8, 10)], [0.5, 0.75]);
  assert.throws(() => obj.mesh("v 0 0 0\nf 1 2 3"));
  assert.throws(() => obj.mesh("v 0 0 0"));
  // Out and back in: the same mesh.
  const back = obj.mesh(obj.write([{ name: "s", positions: got.positions, uvs: got.uvs, triangles: got.triangles }]));
  assert.deepEqual([...back.positions], [...got.positions]);
  assert.deepEqual([...back.triangles], [...got.triangles]);
  assert.deepEqual([...back.uvs].map((x) => Math.round(x * 1000)), [...got.uvs].map((x) => Math.round(x * 1000)));
});
