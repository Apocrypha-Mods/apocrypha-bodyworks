import assert from "node:assert/strict";
import test from "node:test";
import { follows, read, write } from "../ui/fbx.js";

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
  await assert.rejects(read(new TextEncoder().encode("; FBX 7.4.0 project file\n")), /no meshes/);
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
  // As the engine takes it: for each vertex, its bones and how much, heaviest kept.
  const sent = follows(back);
  const got = new DataView(sent.bytes.buffer);
  assert.equal(sent.bytes.length, 4 * 32);
  for (let v = 0; v < 4; v++) {
    const pairs = [0, 1, 2, 3].map((k) => [sent.bones[got.getUint32(v * 32 + k * 8, true)], got.getFloat32(v * 32 + k * 8 + 4, true)]).filter((x) => x[1] > 0);
    assert.deepEqual(pairs.sort((x, y) => y[1] - x[1]), [[v % 2 ? "Arm" : "Spine", 0.75], [v % 2 ? "Spine" : "Arm", 0.25]]);
  }
  // The bones are objects of the file, not meshes of it.
  assert.equal((await read(write([{ ...square, rig: { bones, slots } }]))).length, 1);
});

test("a bone a file names twice counts once among a vertex's four", () => {
  const cluster = (bone, weight) => ({ bone, points: Int32Array.of(0), weights: Float32Array.of(weight) });
  const mesh = { points: new Float32Array(3), from: Uint32Array.of(0), followed: [cluster("A", 0.2), cluster("A", 0.2), cluster("A", 0.2), cluster("B", 0.3), cluster("C", 0.25), cluster("D", 0.1), cluster("E", 0.05)] };
  const sent = follows(mesh);
  const got = new DataView(sent.bytes.buffer);
  const pairs = [0, 1, 2, 3].map((k) => [sent.bones[got.getUint32(k * 8, true)], Math.round(got.getFloat32(k * 8 + 4, true) * 100)]).sort((x, y) => y[1] - x[1]);
  assert.deepEqual(pairs, [["A", 60], ["B", 30], ["C", 25], ["D", 10]]);
});

test("bones two shapes follow stand in one place for both", async () => {
  const slots = new Uint8Array(4 * 32);
  new DataView(slots.buffer).setFloat32(4, 1, true);
  const bone = (x) => [{ name: "Spine", turn: [1, 0, 0, 0, 1, 0, 0, 0, 1], at: [x, 0, 0], scale: 1 }];
  const bytes = write([{ ...square, name: "One", rig: { bones: bone(0), slots } }, { ...square, name: "Two", rig: { bones: bone(10), slots } }]);
  // Every cluster's last four numbers but one are where its bone stands: all the first shape's.
  const text = new TextDecoder("latin1").decode(bytes);
  let at = -1;
  const stands = [];
  while ((at = text.indexOf("TransformLink", at + 1)) >= 0) stands.push(new DataView(bytes.buffer, bytes.byteOffset).getFloat64(at + 13 + 13 + 12 * 8, true));
  assert.deepEqual(stands, [0, 0]);
});

test("a bone under an unevenly scaled object still stands as a turn", async () => {
  const slots = new Uint8Array(4 * 32);
  new DataView(slots.buffer).setFloat32(4, 1, true);
  const bone = [{ name: "Spine", turn: [0.7071068, -0.7071068, 0, 0.7071068, 0.7071068, 0, 0, 0, 1], at: [1, 2, 3], scale: 1 }];
  const bytes = write([{ ...square, rig: { bones: bone, slots } }]);
  // Its standing place sheared, as a parent scaled along one axis leaves it.
  const at = new TextDecoder("latin1").decode(bytes).indexOf("TransformLink") + 13 + 13;
  const view = new DataView(bytes.buffer, bytes.byteOffset);
  for (const k of [0, 4, 8]) view.setFloat64(at + k * 8, view.getFloat64(at + k * 8, true) * 2, true);
  const [back] = await read(bytes);
  const t = back.followed[0].stands.turn;
  const row = (r) => [t[r * 3], t[r * 3 + 1], t[r * 3 + 2]];
  const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  for (const r of [0, 1, 2]) assert.ok(Math.abs(dot(row(r), row(r)) - 1) < 1e-9);
  assert.ok(Math.abs(dot(row(0), row(1))) < 1e-9 && Math.abs(dot(row(0), row(2))) < 1e-9 && Math.abs(dot(row(1), row(2))) < 1e-9);
});

/** The records of a binary file as written here (7.4, no compressed arrays), every value kept, a picture's bytes too. */
function tree(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const decode = (a, b) => new TextDecoder().decode(bytes.subarray(a, b));
  const one = (at) => {
    const [end, count] = [view.getUint32(at, true), view.getUint32(at + 4, true)];
    if (end === 0) return [null, at + 13];
    const n = bytes[at + 12];
    const name = decode(at + 13, at + 13 + n);
    let p = at + 13 + n;
    const values = [];
    for (let i = 0; i < count; i++) {
      const kind = String.fromCharCode(bytes[p++]);
      if (kind === "S") (values.push(decode(p + 4, p + 4 + view.getUint32(p, true))), (p += 4 + view.getUint32(p, true)));
      else if (kind === "R") (values.push(bytes.slice(p + 4, p + 4 + view.getUint32(p, true))), (p += 4 + view.getUint32(p, true)));
      else if (kind === "I") (values.push(view.getInt32(p, true)), (p += 4));
      else if (kind === "L") (values.push(view.getBigInt64(p, true)), (p += 8));
      else if (kind === "D") (values.push(view.getFloat64(p, true)), (p += 8));
      else if (kind === "d" || kind === "i") {
        const [len, packed, size] = [view.getUint32(p, true), view.getUint32(p + 4, true), view.getUint32(p + 8, true)];
        assert.equal(packed, 0);
        const raw = bytes.slice(p + 12, p + 12 + size).buffer;
        values.push(kind === "d" ? new Float64Array(raw, 0, len) : new Int32Array(raw, 0, len));
        p += 12 + size;
      } else throw new Error(`kind ${kind}`);
    }
    const records = [];
    while (p < end) {
      const [r, next] = one(p);
      p = next;
      if (!r) break;
      records.push(r);
    }
    return [{ name, values, records }, end];
  };
  const out = [];
  for (let at = 27; ; ) {
    const [r, next] = one(at);
    if (!r) break;
    out.push(r);
    at = next;
  }
  return out;
}

const kid = (r, name) => r.records.find((c) => c.name === name);

test("pictures are written once each, as a video and a texture, and joined to each shape's material by slot", async () => {
  const png = (...b) => Uint8Array.of(0x89, 0x50, 0x4e, 0x47, ...b);
  const [face, normal, other] = [png(1, 2, 3), png(4, 5), png(6)];
  const shapes = [
    { ...square, name: "A", pictures: { colour: { name: "textures\\actors\\Face.dds", png: face }, normal: { name: "textures/actors/face_n.dds", png: normal.buffer } } },
    // The same picture by its path, however it is spelled, and given again.
    { ...square, name: "B", pictures: { colour: { name: "TEXTURES/ACTORS/FACE.DDS", png: png(9, 9, 9) }, normal: null } },
    // Another folder's picture of the same file name.
    { ...square, name: "C", pictures: { colour: { name: "other\\face.dds", png: other }, normal: null } },
    { ...square, name: "D" },
    // Nothing to show: no name, or no picture.
    { ...square, name: "E", pictures: { colour: { name: "", png: face }, normal: { name: "x.dds", png: null } } },
  ];
  const bytes = write(shapes);
  const objects = tree(bytes).find((r) => r.name === "Objects").records;
  const joins = tree(bytes).find((r) => r.name === "Connections").records.map((c) => c.values);
  const of = (kind) => objects.filter((o) => o.name === kind);
  const label = (o) => o.values[1].split("\0")[0];

  const videos = of("Video");
  assert.deepEqual(videos.map(label), ["face", "face_n", "face 2"]);
  const contents = videos.map((v) => [...kid(v, "Content").values[0]]);
  assert.deepEqual(contents, [[...face], [...normal], [...other]]);
  for (const v of videos) {
    const file = `${label(v)}.png`;
    assert.equal(v.values[2], "Clip");
    assert.equal(kid(v, "Filename").values[0], file);
    assert.equal(kid(v, "RelativeFilename").values[0], file);
    assert.deepEqual(kid(kid(v, "Properties70"), "P").values, ["Path", "KString", "Url", "", file]);
  }

  const textures = of("Texture");
  assert.deepEqual(textures.map(label), ["face", "face_n", "face 2"]);
  for (const t of textures) {
    assert.equal(kid(t, "Media").values[0], `${label(t)}\0\x01Video`);
    assert.equal(kid(t, "FileName").values[0], `${label(t)}.png`);
    assert.equal(kid(t, "Type").values[0], "TextureVideoClip");
  }

  const materials = of("Material");
  assert.deepEqual(materials.map(label), ["A", "B", "C"]);
  const id = (list, name) => list.find((o) => label(o) === name).values[0];
  const models = of("Model").filter((m) => m.values[2] === "Mesh");
  // Each video into its texture, once.
  assert.deepEqual(
    joins.filter((j) => j[0] === "OO" && videos.some((v) => v.values[0] === j[1])).map((j) => [j[1], j[2]]),
    ["face", "face_n", "face 2"].map((n) => [id(videos, n), id(textures, n)]),
  );
  // Each texture into the materials that show it, by slot.
  const shown = joins.filter((j) => j[0] === "OP").map((j) => [label(textures.find((t) => t.values[0] === j[1])), label(materials.find((m) => m.values[0] === j[2])), j[3]]);
  assert.deepEqual(shown, [
    ["face", "A", "DiffuseColor"],
    ["face_n", "A", "NormalMap"],
    ["face", "B", "DiffuseColor"],
    ["face 2", "C", "DiffuseColor"],
  ]);
  // Each material onto its shape's model.
  for (const n of ["A", "B", "C"]) assert.ok(joins.some((j) => j[0] === "OO" && j[1] === id(materials, n) && j[2] === id(models, n)), n);
  // A shape with a material says so in its layer; one without has none.
  for (const g of of("Geometry")) {
    const types = kid(g, "Layer").records.filter((r) => r.name === "LayerElement").map((r) => kid(r, "Type").values[0]);
    const has = ["A", "B", "C"].includes(label(g));
    assert.deepEqual(types, has ? ["LayerElementUV", "LayerElementMaterial"] : ["LayerElementUV"], label(g));
    assert.equal(!!kid(g, "LayerElementMaterial"), has, label(g));
  }
  // Every id is one object's.
  const ids = objects.map((o) => o.values[0]);
  assert.equal(new Set(ids).size, ids.length);
  for (const [, from, to] of joins) for (const end of [from, to]) assert.ok(end === 0n || ids.includes(end), `${end}`);

  // The meshes read back as ever.
  const back = await read(bytes);
  assert.deepEqual(back.map((m) => m.name), ["A", "B", "C", "D", "E"]);
  for (const m of back) assert.deepEqual([...m.positions], square.positions);
});

test("an untextured shape with a picture still gets a layer for its material", () => {
  const p = { name: "a.dds", png: Uint8Array.of(1) };
  const objects = tree(write([{ ...square, uvs: null, pictures: { colour: p, normal: null } }])).find((r) => r.name === "Objects").records;
  const g = objects.find((o) => o.name === "Geometry");
  const layer = kid(g, "Layer");
  assert.deepEqual(layer.records.filter((r) => r.name === "LayerElement").map((r) => kid(r, "Type").values[0]), ["LayerElementMaterial"]);
  assert.equal(objects.filter((o) => o.name === "Video").length, 1);
});
