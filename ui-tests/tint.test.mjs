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
