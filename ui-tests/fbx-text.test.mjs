import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import test from "node:test";
import { read, write } from "../ui/fbx.js";

const square = {
  name: "Square",
  positions: [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0.5],
  uvs: [0, 0, 1, 0, 1, 1, 0, 1],
  triangles: [0, 1, 2, 0, 2, 3],
};

const bytes = (s) => new TextEncoder().encode(s);

/** A file saved as text, laid out as the binary one `write()` makes: Z up, the front facing back along Y. */
const file = (objects, connections = "", version = 7400) => `; FBX ${String(version).split("").join(".")} project file
; ----------------------------------------------------

FBXHeaderExtension:  {
	FBXHeaderVersion: 1003
	FBXVersion: ${version}
	Creator: "Apocrypha Bodyworks"
}
GlobalSettings:  {
	Version: 1000
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",1
	}
}

; Object properties
;------------------------------------------------------------------

Objects:  {
${objects}
}

; Object connections
;------------------------------------------------------------------

Connections:  {
${connections}
}
`;

const geometry = (id, name, vertices = "a: 0,0,0,1,0,0,1,1,0,0,1,0.5") => `	Geometry: ${id}, "Geometry::${name}", "Mesh" {
		Vertices: *12 {
			${vertices}
		}
		PolygonVertexIndex: *6 {
			a: 0,1,-3,0,2,-4
		}
		GeometryVersion: 124
		LayerElementUV: 0 {
			Version: 101
			Name: "UVMap"
			MappingInformationType: "ByPolygonVertex"
			ReferenceInformationType: "IndexToDirect"
			UV: *8 {
				a: 0,1,1,1,1,0,0,0
			}
			UVIndex: *6 {
				a: 0,1,2,0,2,3
			}
		}
		Layer: 0 {
			Version: 100
			LayerElement:  {
				Type: "LayerElementUV"
				TypedIndex: 0
			}
		}
	}`;

const model = (id, name, kind = "Mesh", props = "") => `	Model: ${id}, "Model::${name}", "${kind}" {
		Version: 232
		Properties70:  {
${props}
		}
	}`;

const plain = file([geometry(1000, "Square"), model(1001, "Square")].join("\n"), `	C: "OO",1000,1001\n	C: "OO",1001,0`);

// The square with two bones, as `write()` lays it out: the same objects, in the same order, joined the same way.
const bones = [
  { name: "Spine", turn: [1, 0, 0, 0, 1, 0, 0, 0, 1], at: [0, 0, 5], scale: 1 },
  { name: "Arm", turn: [1, 0, 0, 0, 1, 0, 0, 0, 1], at: [3, 0, 5], scale: 1 },
];
const slots = new Uint8Array(4 * 32);
for (let v = 0; v < 4; v++) {
  const view = new DataView(slots.buffer);
  view.setUint32(v * 32, v % 2, true);
  view.setFloat32(v * 32 + 4, 0.75, true);
  view.setUint32(v * 32 + 8, 1 - (v % 2), true);
  view.setFloat32(v * 32 + 12, 0.25, true);
}
const limb = (attribute, id, { name, at }) => `	NodeAttribute: ${attribute}, "NodeAttribute::${name}", "LimbNode" {
		TypeFlags: "Skeleton"
	}
${model(id, name, "LimbNode", `			P: "Lcl Translation", "Lcl Translation", "", "A",${at.join(",")}\n			P: "Lcl Rotation", "Lcl Rotation", "", "A",0,0,0\n			P: "Lcl Scaling", "Lcl Scaling", "", "A",1,1,1`)}`;
const cluster = (id, name, weights, at) => `	Deformer: ${id}, "SubDeformer::${name}", "Cluster" {
		Version: 100
		UserData: "", ""
		Indexes: *4 {
			a: 0,1,2,3
		}
		Weights: *4 {
			a: ${weights}
		}
		Transform: *16 {
			a: 1,0,0,0,0,1,0,0,0,0,1,0,${at.map((c) => -c).join(",")},1
		}
		TransformLink: *16 {
			a: 1,0,0,0,0,1,0,0,0,0,1,0,${at.join(",")},1
		}
	}`;
const rigged = file(
  [
    model(99999, "Skeleton", "Null"),
    limb(100001, 100000, bones[0]),
    limb(100003, 100002, bones[1]),
    geometry(1000, "Square"),
    model(1001, "Square"),
    `	Deformer: 100004, "Deformer::Square", "Skin" {\n		Version: 101\n		Link_DeformAcuracy: 50\n	}`,
    cluster(100005, "Spine", "0.75,0.25,0.75,0.25", bones[0].at),
    cluster(100006, "Arm", "0.25,0.75,0.25,0.75", bones[1].at),
  ].join("\n"),
  [
    ["99999", "0"],
    ["100001", "100000"],
    ["100000", "99999"],
    ["100003", "100002"],
    ["100002", "99999"],
    ["1000", "1001"],
    ["1001", "0"],
    ["100004", "1000"],
    ["100005", "100004"],
    ["100000", "100005"],
    ["100006", "100004"],
    ["100002", "100006"],
  ]
    .map(([from, to]) => `	;${from}, ${to}\n	C: "OO",${from},${to}`)
    .join("\n\n"),
);

test("a mesh saved as text reads as the binary file of it does, bones and all", async () => {
  const text = await read(bytes(rigged));
  assert.deepEqual(text, await read(write([{ ...square, rig: { bones, slots } }])));
  assert.equal(text[0].name, "Square");
  assert.deepEqual([...text[0].positions], square.positions);
  assert.deepEqual([...text[0].uvs], square.uvs);
  assert.deepEqual([...text[0].triangles], square.triangles);
  assert.deepEqual(
    text[0].followed.map((f) => [f.bone, [...f.points], [...f.weights], f.stands.at]),
    [
      ["Spine", [0, 1, 2, 3], [0.75, 0.25, 0.75, 0.25], [0, 0, 5]],
      ["Arm", [0, 1, 2, 3], [0.25, 0.75, 0.25, 0.75], [3, 0, 5]],
    ],
  );
  assert.deepEqual(await read(bytes(plain)), await read(write([square])));
});

test("an array's numbers go on across line ends and past comments", async () => {
  const vertices = "a: 0,0,0, ; the first corner, of four\n			; a line of nothing but a comment\n\n			1,0,0,1,1,0 ; two more\n			,0,1,0.5";
  const [got] = await read(bytes(file([geometry(1000, "Square", vertices), model(1001, "Square")].join("\n"), `	C: "OO",1000,1001`)));
  assert.deepEqual([...got.positions], square.positions);
  assert.deepEqual([...got.triangles], square.triangles);
  // An array that holds fewer or more numbers than it says is not read.
  await assert.rejects(read(bytes(file(geometry(1000, "Square", "a: 0,0,0,1,0,0,1,1,0,0,1")))), /cannot read/);
  await assert.rejects(read(bytes(file(geometry(1000, "Square", "a: 0,0,0,1,0,0,1,1,0,0,1,0.5,7")))), /cannot read/);
  // A comment on the last line, with no line end after it, ends the file.
  assert.deepEqual([...(await read(bytes(`${plain}; the end`)))[0].positions], square.positions);
});

test("a record's values may begin on the next line, and a comma carries them on past a line end", async () => {
  const props = `			P: "Lcl Translation", "Lcl Translation", "", "A",\n				10, 20 ; across two lines\n				, 30`;
  const objects = [geometry(1000, "Square").replace("Vertices: *12 {", "Vertices:\n			*12 {"), model(1001, "Square", "Mesh", props)].join("\n");
  const [got] = await read(bytes(file(objects, `	C: "OO",1000,\n		1001`)));
  assert.deepEqual([...got.positions], [10, 20, 30, 11, 20, 30, 11, 21, 30, 10, 21, 30.5]);
});

test("a thing named Kind::Name in text is read by its name", async () => {
  const objects = [geometry(1000, "Under"), model(1001, "Body"), geometry(2000, "Loose"), geometry(3000, "Two"), model(3001, "Left::Right")].join("\n");
  const got = await read(bytes(file(objects, `	C: "OO",1000,1001\n	C: "OO",3000,3001`)));
  assert.deepEqual(
    got.map((m) => m.name),
    ["Body", "Loose", "Left::Right"],
  );
});

test("a quote and line ends written as names in text are read as themselves", async () => {
  const [got] = await read(bytes(file([geometry(1000, "Square"), model(1001, "&quot;Said&quot;&cr;&lf;&amp;")].join("\n"), `	C: "OO",1000,1001`)));
  assert.equal(got.name, '"Said"\r\n&amp;');
});

test("a record written in pieces from `Content: ,` is read past", async () => {
  const video = `	Video: 4000, "Video::Picture", "Clip" {
		Type: "Clip"
		Content: ,
			"aVZCT1J3MEtHZ29BQUFBTlNVaEVVZ0FBQWdBQUFBSUFDQVlBQUFEMGVOVDZ}",
			"QUFBQUNYQklXWE1BQUE3REFBQU93d0hIYjZoa0FBQWdBRWxFUVZSNG5PM2RT;"
			,"M2t5ekx1bWVpcFZtamQwVGI0M3hya1EK"
	}`;
  const [got] = await read(bytes(file([video, geometry(1000, "Square"), model(1001, "Square")].join("\n"), `	C: "OO",1000,1001`)));
  assert.equal(got.name, "Square");
  assert.deepEqual([...got.positions], square.positions);
});

test("numbers that are not numbers are read however a program spells them", async () => {
  const spelt = ["nan", "-1.#IND", "1.#INF", "-0.#INF", "-nan(ind)", "1.#QNAN", "inf", "-Infinity"];
  const objects = [
    geometry(1000, "Square"),
    model(1001, "Square"),
    model(1002, "Bone", "LimbNode"),
    `	Deformer: 1003, "Deformer::Skin", "Skin" {\n		Version: 101\n	}`,
    `	Deformer: 1004, "SubDeformer::Bone", "Cluster" {\n		Indexes: *8 {\n			a: 0,1,2,3,0,1,2,3\n		}\n		Weights: *8 {\n			a: ${spelt.join(",")}\n		}\n	}`,
  ].join("\n");
  const [got] = await read(bytes(file(objects, `	C: "OO",1000,1001\n	C: "OO",1003,1000\n	C: "OO",1004,1003\n	C: "OO",1002,1004`)));
  assert.deepEqual([...got.followed[0].weights], [NaN, NaN, Infinity, -Infinity, NaN, NaN, Infinity, -Infinity]);
  // Where a mesh cannot hold one, it is refused as that, not as text that cannot be read.
  await assert.rejects(read(bytes(file(geometry(1000, "Square", "a: 0,0,0,1,0,0,1,-1.#IND,0,0,1,0.5")))), /vertex that is not three numbers/);
  await assert.rejects(read(bytes(file(geometry(1000, "Square", "a: 0,0,0,1,0,0,1,1.#INF,0,0,1,0.5")))), /vertex that is not three numbers/);
  await assert.rejects(read(bytes(file(geometry(1000, "Square", "a: 0,0,0,1,0,0,1,one,0,0,1,0.5")))), /cannot read/);
});

test("an id too large to hold exactly stays as it is written", async () => {
  // As numbers both ids are 9007199254740992: the mesh would be placed by the wrong object. A sign before one is no part of it.
  const near = model("9007199254740993", "Near");
  const far = model("9007199254740992", "Far", "Mesh", `			P: "Lcl Translation", "Lcl Translation", "", "A",100,0,0`);
  const got = await read(bytes(file([geometry(1000, "Square"), near, far].join("\n"), `	C: "OO",1000,+9007199254740993`)));
  assert.deepEqual(
    got.map((m) => [m.name, [...m.positions]]),
    [["Near", square.positions]],
  );
});

test("FBX from before 7.1 is refused, saved as text or binary", async () => {
  await assert.rejects(read(bytes(file([geometry(1000, "Square"), model(1001, "Square")].join("\n"), `	C: "OO",1000,1001`, 6100))), /before 7\.1/);
  const binary = write([square]);
  new DataView(binary.buffer).setUint32(23, 6100, true);
  await assert.rejects(read(binary), /before 7\.1/);
  // The first that is read.
  assert.equal((await read(bytes(file([geometry(1000, "Square"), model(1001, "Square")].join("\n"), `	C: "OO",1000,1001`, 7100))))[0].name, "Square");
  new DataView(binary.buffer).setUint32(23, 7100, true);
  assert.equal((await read(binary))[0].name, "Square");
});

test("text cut short is refused, or read as far as it is whole", async () => {
  const whole = bytes(rigged);
  let refused = 0;
  for (let n = 0; n < whole.length; n++) {
    const got = await read(whole.subarray(0, n)).catch((e) => e);
    if (got instanceof Error) {
      assert.equal(got.constructor, Error, `cut at ${n}: ${got}`);
      refused += 1;
    } else assert.deepEqual([...got[0].positions], square.positions, `cut at ${n}`);
  }
  assert.ok(refused > whole.length / 2);
});

test("text with any one byte changed is read as a mesh or plainly refused", async () => {
  const whole = bytes(plain);
  let refused = 0;
  for (let n = 0; n < whole.length; n++) {
    for (const to of [0, 0x0a, 0x22, 0x2a, 0x2c, 0x3a, 0x3b, 0x7b, 0x7d, 0xff]) {
      const bad = whole.slice();
      bad[n] = to;
      const got = await read(bad).catch((e) => e);
      if (got instanceof Error) {
        assert.equal(got.constructor, Error, `byte ${n} to ${to}: ${got}`);
        refused += 1;
      } else assert.ok(got.length > 0 && got.every((m) => m.positions instanceof Float32Array), `byte ${n} to ${to}`);
    }
  }
  assert.ok(refused > 0);
});

test("what is neither kind of FBX is said not to be one", async () => {
  for (const what of [Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13), bytes("{ }"), bytes("12, 13"), bytes('"Objects": {}'), bytes("\n\n  ; a comment\n*3 { a: 1,2,3 }")]) {
    await assert.rejects(read(what), { constructor: Error, message: "That is not an FBX file." });
  }
  await assert.rejects(read(bytes("Kaydara FBX Binary")), /not a binary FBX/);
});

const samples = new URL("../scratch/samples/", import.meta.url);
const twins = new URL("fbx-twins/", samples);
const other = new URL("fbx-other/", samples);
const load = (url) => new Uint8Array(readFileSync(url));

// Saved twice, these differ in what they hold, not in how it is read: an object named `&quot;`,
// which text cannot tell from a quote, and animated objects whose saves stand them in different places.
const DIFFER = ['max_quote_7500: name "&quot;" as "\\""', "maya_auto_clamp_7700: positions", "maya_resampled_7500: positions", "maya_transform_animation_7500: positions"];

test("files other programs saved both ways read the same as text as binary", { skip: !existsSync(twins) && "no FBX twins in scratch/samples" }, async () => {
  const pairs = readdirSync(twins)
    .filter((f) => f.endsWith("_ascii.fbx"))
    .map((f) => f.slice(0, -"_ascii.fbx".length));
  const near = (a, b) => a.length === b.length && a.every((c, i) => Math.abs(c - b[i]) <= 1e-5 * Math.max(1, Math.abs(b[i])));
  const differ = [];
  let both = 0;
  let boned = 0;
  for (const pair of pairs) {
    const text = await read(load(new URL(`${pair}_ascii.fbx`, twins))).catch((e) => e);
    const binary = await read(load(new URL(`${pair}_binary.fbx`, twins))).catch((e) => e);
    if (!(binary instanceof Error)) assert.ok(!(text instanceof Error), `${pair}: the binary reads, the text says ${text}`);
    if (text instanceof Error || binary instanceof Error) continue;
    both += 1;
    if (binary.some((m) => m.followed.length)) boned += 1;
    assert.equal(text.length, binary.length, `${pair}: meshes`);
    const what = new Set();
    text.forEach((t, k) => {
      const b = binary[k];
      if (t.name !== b.name) what.add(`name ${JSON.stringify(b.name)} as ${JSON.stringify(t.name)}`);
      if (!near(t.positions, b.positions) || !near(t.points, b.points)) what.add("positions");
      if (!near(t.uvs, b.uvs)) what.add("uvs");
      if (String(t.triangles) !== String(b.triangles)) what.add("triangles");
      if (String(t.from) !== String(b.from)) what.add("from");
      const follows = (m) => m.followed.map((f) => [f.bone, [...f.points], [...f.weights]]);
      try {
        assert.deepEqual(follows(t), follows(b));
      } catch {
        what.add("bones");
      }
    });
    for (const w of what) differ.push(`${pair}: ${w}`);
  }
  assert.ok(both >= 100, `${both} pairs both read`);
  assert.ok(boned >= 5, `${boned} pairs with bones`);
  assert.deepEqual(differ, DIFFER);
});

test("a file saved as text with comments everywhere reads as the cube saved without them", { skip: !(existsSync(twins) && existsSync(other)) && "no FBX samples in scratch/samples" }, async () => {
  const [commented] = await read(load(new URL("synthetic_comment_cube_7500_ascii.fbx", other)));
  const [cube] = await read(load(new URL("maya_cube_7500_binary.fbx", twins)));
  assert.equal(commented.name, cube.name);
  assert.deepEqual(commented.positions, cube.positions);
  assert.deepEqual(commented.uvs, cube.uvs);
  assert.deepEqual(commented.triangles, cube.triangles);
});

test("real files from before 7.1 are refused, text and binary", { skip: !existsSync(other) && "no FBX samples in scratch/samples" }, async () => {
  const old = readdirSync(other).filter((f) => /_6100_(ascii|binary)\.fbx$/.test(f));
  assert.ok(old.some((f) => f.endsWith("_ascii.fbx")) && old.some((f) => f.endsWith("_binary.fbx")));
  for (const f of old) await assert.rejects(read(load(new URL(f, other))), /before 7\.1/, f);
});
