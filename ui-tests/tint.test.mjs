import assert from "node:assert/strict";
import test from "node:test";
import * as tint from "../ui/tint.js";

const image = (width, height, red) => {
  const data = new Uint8ClampedArray(width * height * 4);
  red.forEach((v, i) => (data[i * 4] = v));
  return { width, height, data };
};
const px = (rgba, i) => [...rgba.slice(i * 4, i * 4 + 4)];

test("with no layers the tint is the skin tone", () => {
  const out = tint.compose([1, 0.5, 0], [], 2, 1);
  assert.deepEqual(px(out, 0), [255, 128, 0, 255]);
  assert.deepEqual(px(out, 1), [255, 128, 0, 255]);
});

test("a layer lerps toward its colour by the mask's red times its strength, in order", () => {
  const mask = image(2, 1, [255, 51]);
  const out = tint.compose([1, 1, 1], [{ mask, colour: [0, 0, 0], strength: 0.5 }], 2, 1);
  assert.deepEqual(px(out, 0), [128, 128, 128, 255]);
  assert.deepEqual(px(out, 1), [230, 230, 230, 255]);
  const full = image(1, 1, [255]);
  const two = tint.compose([0, 0, 0], [{ mask: full, colour: [1, 0, 0], strength: 1 }, { mask: full, colour: [0, 1, 0], strength: 1 }], 1, 1);
  assert.deepEqual(px(two, 0), [0, 255, 0, 255]);
});

test("layers without a mask or strength are skipped", () => {
  const out = tint.compose([0.2, 0.2, 0.2], [{ mask: null, colour: [1, 1, 1], strength: 1 }, { mask: image(1, 1, [255]), colour: [1, 1, 1], strength: 0 }], 1, 1);
  assert.deepEqual(px(out, 0), [51, 51, 51, 255]);
});

test("a mask of another size is scaled", () => {
  const cover = tint.coverage(image(1, 1, [255]), 4, 4);
  assert.ok(cover.every((v) => v === 1));
  const half = tint.coverage(image(2, 1, [0, 255]), 4, 1);
  assert.deepEqual([...half].map((v) => Math.round(v * 100)), [0, 25, 75, 100]);
});

test("the face is the head times twice the tint, clamped, its alpha kept", () => {
  const head = { width: 1, height: 1, data: new Uint8ClampedArray([100, 200, 50, 7]) };
  const t = { width: 1, height: 1, data: new Uint8ClampedArray([128, 255, 0, 255]) };
  assert.deepEqual([...tint.face(head, t).data], [100, 255, 0, 7]);
});

test("512 by 512 with 30 layers is quick", () => {
  const mask = image(512, 512, Array.from({ length: 512 * 512 }, (_, i) => i % 256));
  const layers = Array.from({ length: 30 }, () => ({ mask, colour: [0.5, 0.2, 0.1], strength: 0.4 }));
  const t0 = performance.now();
  tint.compose([0.8, 0.7, 0.6], layers, 512, 512);
  assert.ok(performance.now() - t0 < 1000);
});

test("a mask with no size, or fewer pixels than its size says, lays nothing", () => {
  const layer = (mask) => ({ mask, colour: [1, 1, 1], strength: 1 });
  const base = [0.2, 0.4, 0.6];
  const plain = tint.compose(base, [], 2, 2);
  const broken = [
    { width: 0, height: 2, data: new Uint8ClampedArray(0) },
    { width: 2, height: 0, data: new Uint8ClampedArray(0) },
    { width: 2, height: 2, data: new Uint8ClampedArray(2 * 2 * 4 - 1).fill(255) },
    { width: 2, height: 2 },
    { width: NaN, height: 2, data: new Uint8ClampedArray(16).fill(255) },
    { width: -2, height: -2, data: new Uint8ClampedArray(16).fill(255) },
  ];
  for (const mask of broken) assert.deepEqual([...tint.compose(base, [layer(mask)], 2, 2)], [...plain], JSON.stringify({ w: mask.width, h: mask.height, n: mask.data?.length }));
  // The same layer over a whole mask lays its colour.
  const whole = { width: 2, height: 2, data: new Uint8ClampedArray(16).fill(255) };
  assert.deepEqual(px(tint.compose(base, [layer(whole)], 2, 2), 3), [255, 255, 255, 255]);
  // A broken mask among good ones is skipped and the rest still laid.
  const out = tint.compose(base, [layer(broken[2]), { mask: whole, colour: [1, 0, 0], strength: 0.5 }], 1, 1);
  // 51 + 204 / 2, 102 / 2, and 153 / 2 = 76.5 kept as 76 by the byte array.
  assert.deepEqual(px(out, 0), [153, 51, 76, 255]);
});

test("a strength past one is one, and the result is always opaque", () => {
  const mask = image(1, 1, [255]);
  const out = tint.compose([0, 0, 0], [{ mask, colour: [0.5, 0.5, 0.5], strength: 7 }], 1, 1);
  assert.deepEqual(px(out, 0), [128, 128, 128, 255]);
});

test("a mask's coverage is its red alone, and scales down and along both ways", () => {
  // Green, blue and alpha are not coverage.
  const coloured = { width: 1, height: 1, data: new Uint8ClampedArray([51, 255, 255, 255]) };
  assert.deepEqual([...tint.coverage(coloured, 1, 1)].map((v) => Math.round(v * 100)), [20]);
  // Down: two of four columns, each the mean of its pair.
  const wide = image(4, 1, [0, 100, 200, 255]);
  const down = tint.coverage(wide, 2, 1);
  assert.ok(Math.abs(down[0] * 255 - 50) < 1e-3 && Math.abs(down[1] * 255 - 227.5) < 1e-3, `${[...down]}`);
  // Up and down: a column stretched along its height.
  const tall = image(1, 2, [0, 255]);
  assert.deepEqual([...tint.coverage(tall, 1, 4)].map((v) => Math.round(v * 100)), [0, 25, 75, 100]);
  // A mask scaled across a grid: rows and columns are not swapped.
  const corner = image(2, 2, [255, 0, 0, 0]);
  const got = tint.coverage(corner, 4, 4);
  assert.equal(got[0], 1);
  assert.equal(got[3], 0);
  assert.equal(got[12], 0);
  assert.equal(got[15], 0);
});

test("a tint of another size than the head is scaled to it, each channel its own", () => {
  const head = { width: 2, height: 1, data: new Uint8ClampedArray([100, 100, 100, 9, 100, 100, 100, 200]) };
  // One pixel: red full, green half, blue none.
  const one = { width: 1, height: 1, data: new Uint8ClampedArray([255, 128, 0, 255]) };
  const out = tint.face(head, one);
  assert.equal(out.width, 2);
  assert.equal(out.height, 1);
  assert.deepEqual([...out.data], [200, 100, 0, 9, 200, 100, 0, 200]);
  // Two pixels to the head's one: each channel scaled apart from the others.
  const small = { width: 1, height: 1, data: new Uint8ClampedArray([100, 100, 100, 255]) };
  const two = { width: 2, height: 1, data: new Uint8ClampedArray([255, 0, 64, 255, 0, 255, 64, 255]) };
  assert.deepEqual([...tint.face(small, two).data], [100, 100, 50, 255]);
});
