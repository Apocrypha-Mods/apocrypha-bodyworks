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

test("what is read is never anything but a mesh or a plain refusal", async () => {
  const whole = write([square]);
  // Every byte in turn changed: a reading, or an Error that says what is wrong, and never a hang.
  for (let n = 0; n < whole.length; n++) {
    for (const to of [0, 1, 0x7f, 0xff]) {
      const bad = whole.slice();
      bad[n] = to;
      const got = await read(bad).catch((e) => e);
      if (got instanceof Error) assert.equal(got.constructor, Error, `byte ${n} to ${to}: ${got}`);
    }
  }
  await assert.rejects(read(whole.subarray(0, 24)), /not a binary FBX/);
});

test("a compressed array is opened no further than it says it goes", async () => {
  // A record whose one array says a single number and holds eight million bytes of nothing.
  const packed = new Uint8Array(await new Response(new Blob([new Uint8Array(8 << 20)]).stream().pipeThrough(new CompressionStream("deflate"))).arrayBuffer());
  const head = write([square]).subarray(0, 27);
  const name = new TextEncoder().encode("Objects");
  const body = new Uint8Array(13 + name.length + 13 + packed.length + 13);
  const view = new DataView(body.buffer);
  view.setUint32(0, 27 + body.length - 13, true);
  view.setUint32(4, 1, true);
  view.setUint32(8, 13 + packed.length, true);
  body[12] = name.length;
  body.set(name, 13);
  let at = 13 + name.length;
  body[at] = 100;
  view.setUint32(at + 1, 1, true);
  view.setUint32(at + 5, 1, true);
  view.setUint32(at + 9, packed.length, true);
  body.set(packed, at + 13);
  const file = new Uint8Array(27 + body.length);
  file.set(head);
  file.set(body, 27);
  const before = process.memoryUsage().arrayBuffers;
  await assert.rejects(read(file), /no meshes/);
  assert.ok(process.memoryUsage().arrayBuffers - before < 4 << 20);
});

test("a shape's bones and what follows them are written", async () => {
  const slots = new Uint8Array(4 * 32);
  const view = new DataView(slots.buffer);
  for (let v = 0; v < 4; v++) {
    view.setUint32(v * 32, v % 2, true);
    view.setFloat32(v * 32 + 4, 0.75, true);
    view.setUint32(v * 32 + 8, 1 - (v % 2), true);
    view.setFloat32(v * 32 + 12, 0.25, true);
  }
  const bones = [
    { name: "Spine", turn: [1, 0, 0, 0, 1, 0, 0, 0, 1], at: [0, 0, 5], scale: 1 },
    { name: "Arm", turn: [0, -1, 0, 1, 0, 0, 0, 0, 1], at: [3, 0, 5], scale: 1 },
  ];
  const [back] = await read(write([{ ...square, rig: { bones, slots } }]));
  assert.deepEqual(back.followed.map((f) => f.bone).sort(), ["Arm", "Spine"]);
  const spine = back.followed.find((f) => f.bone === "Spine");
  assert.deepEqual([...spine.points], [0, 1, 2, 3]);
  assert.deepEqual([...spine.weights], [0.75, 0.25, 0.75, 0.25]);
  // The bones are objects of the file, not meshes of it.
  assert.equal((await read(write([{ ...square, rig: { bones, slots } }]))).length, 1);
});
