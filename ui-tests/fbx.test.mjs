import assert from "node:assert/strict";
import test from "node:test";
import { read, write } from "../ui/fbx.js";

const square = {
  name: "Square",
  positions: [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0.5],
  uvs: [0, 0, 1, 0, 1, 1, 0, 1],
  triangles: [0, 1, 2, 0, 2, 3],
};

test("what is written reads back the same", async () => {
  const [back, bare] = await read(write([square, { ...square, name: "Bare", uvs: null }]));
  assert.equal(back.name, "Square");
  assert.deepEqual([...back.positions], square.positions);
  assert.deepEqual([...back.points], square.positions);
  assert.deepEqual([...back.uvs], square.uvs);
  assert.deepEqual([...back.triangles], square.triangles);
  assert.equal(bare.name, "Bare");
  assert.deepEqual([...bare.uvs], [0, 0, 0, 0, 0, 0, 0, 0]);
});

test("a file cut short is refused, or read as far as it is whole", async () => {
  const whole = write([square]);
  let refused = 0;
  for (let n = 0; n < whole.length; n++) {
    const got = await read(whole.subarray(0, n)).catch((e) => e);
    if (got instanceof Error) refused += 1;
    else assert.deepEqual([...got[0].positions], square.positions, `cut at ${n}`);
  }
  assert.ok(refused > whole.length / 2);
  await assert.rejects(read(new TextEncoder().encode("; FBX 7.4.0 project file\n")), /not a binary FBX/);
});

test("a face that names a vertex the mesh lacks is refused", async () => {
  await assert.rejects(read(write([{ ...square, triangles: [0, 1, 9] }])), /does not have/);
});

test("a file with nothing to draw is refused", async () => {
  await assert.rejects(read(write([])), /no meshes/);
});
