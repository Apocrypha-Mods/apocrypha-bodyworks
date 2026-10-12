import assert from "node:assert/strict";
import { test } from "node:test";
import * as obj from "../ui/obj.js";

test("a vertex's place in the texture is the one its faces give it, not the one listed in step", () => {
  const text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0.1 0.9\nvt 0.2 0.8\nvt 0.3 0.7\nf 1/3 2/1 3/2\n";
  const near = (a, b) => a.every((c, i) => Math.abs(c - b[i]) < 1e-6);
  assert.ok(near([...obj.uvs(text)], [0.3, 0.3, 0.1, 0.1, 0.2, 0.2]));
  // Shared by two vertices, a place is each one's.
  assert.ok(near([...obj.uvs("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0.5 0.5\nf 1/1 2/1 3/1\n")], [0.5, 0.5, 0.5, 0.5, 0.5, 0.5]));
  // One no face gives a place has none, and one given two is refused.
  assert.ok(Number.isNaN(obj.uvs("v 0 0 0\nv 1 0 0\nv 0 1 0\nv 2 2 2\nvt 0.5 0.5\nf 1/1 2/1 3/1\n")[6]));
  assert.throws(() => obj.uvs("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 1\nf 1/1 2/1 3/1\nf 1/2 2/1 3/1\n"), /two places/);
  assert.throws(() => obj.uvs("v 0 0 0\nvt 0 0\nf 1/5 1/1 1/1\n"), /does not list/);
});
