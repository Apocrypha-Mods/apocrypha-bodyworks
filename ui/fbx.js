// FBX, the binary kind every 3D program trades in: shapes written out, and
// the meshes of a file read back in. Written from the published layout of
// the format and checked against what Blender reads and writes.
//
// A file is a tree of named records. Each has a few values (numbers, text,
// or arrays of numbers, which may be compressed) and records of its own.

const MAGIC = "Kaydara FBX Binary  \0";
const VERSION = 7400;

// ── Writing ────────────────────────────────────────────────────────────────

class Out {
  constructor() {
    this.parts = [];
    this.length = 0;
  }
  bytes(b) {
    this.parts.push(b);
    this.length += b.length;
  }
  u8(v) {
    this.bytes(Uint8Array.of(v));
  }
  u32(v) {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v, true);
    this.bytes(b);
  }
  done() {
    const all = new Uint8Array(this.length);
    let at = 0;
    for (const p of this.parts) {
      all.set(p, at);
      at += p.length;
    }
    return all;
  }
}

const text = (s) => new TextEncoder().encode(s);

/** One value of a record, as its bytes: a letter for its kind, then the value. */
function value(v) {
  const out = new Out();
  if (typeof v === "string") {
    const b = text(v);
    out.u8(83); // S
    out.u32(b.length);
    out.bytes(b);
  } else if (typeof v === "bigint") {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigInt64(0, v, true);
    out.u8(76); // L
    out.bytes(b);
  } else if (Number.isInteger(v)) {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setInt32(0, v, true);
    out.u8(73); // I
    out.bytes(b);
  } else if (typeof v === "number") {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setFloat64(0, v, true);
    out.u8(68); // D
    out.bytes(b);
  } else if (v instanceof Float64Array || v instanceof Int32Array) {
    out.u8(v instanceof Float64Array ? 100 : 105); // d, i
    out.u32(v.length);
    out.u32(0); // not compressed
    out.u32(v.byteLength);
    out.bytes(new Uint8Array(v.buffer, v.byteOffset, v.byteLength));
  } else {
    throw new Error("not a value an FBX record holds");
  }
  return out.done();
}

/** A real number, written as one even when it is whole. */
const real = (n) => ({ real: n });

/** A record: `[name, values, records]`, written at the end of what is there. */
function record(out, [name, values = [], records = []]) {
  const props = values.map((v) => {
    if (v && typeof v === "object" && "real" in v) {
      const b = new Uint8Array(9);
      b[0] = 68;
      new DataView(b.buffer).setFloat64(1, v.real, true);
      return b;
    }
    return value(v);
  });
  const propsLength = props.reduce((n, p) => n + p.length, 0);
  const head = new Uint8Array(13);
  out.bytes(head);
  const named = text(name);
  out.bytes(named);
  for (const p of props) out.bytes(p);
  for (const r of records) record(out, r);
  // A record with records of its own ends with an empty one.
  if (records.length) out.bytes(new Uint8Array(13));
  const view = new DataView(head.buffer);
  view.setUint32(0, out.length, true);
  view.setUint32(4, props.length, true);
  view.setUint32(8, propsLength, true);
  // The name's length is the last byte of the head.
  head[12] = named.length;
}

/** A placement (`turn` a row at a time, then `at` and `scale`) as FBX keeps one: sixteen numbers, a column at a time. */
function matrix({ turn: m, at, scale: s }) {
  return Float64Array.of(m[0] * s, m[3] * s, m[6] * s, 0, m[1] * s, m[4] * s, m[7] * s, 0, m[2] * s, m[5] * s, m[8] * s, 0, at[0], at[1], at[2], 1);
}

/** What undoes a placement. */
function undone({ turn: m, at, scale }) {
  const s = 1 / scale;
  const turn = [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]];
  return { turn, scale: s, at: [0, 3, 6].map((r) => -(turn[r] * at[0] + turn[r + 1] * at[1] + turn[r + 2] * at[2]) * s) };
}

/** A turn as degrees about X, then Y, then Z. */
function degrees(m) {
  const y = Math.asin(Math.max(-1, Math.min(1, -m[6])));
  // Looking straight along Z, the turns about X and Z are one turn: all of it is given to Z.
  const [x, z] = Math.abs(m[6]) > 1 - 1e-12 ? [0, Math.atan2(-m[1], m[4])] : [Math.atan2(m[7], m[8]), Math.atan2(m[3], m[0])];
  return [x, y, z].map((r) => (r * 180) / Math.PI);
}

/**
 * Shapes as one FBX file. Each: `{ name, positions, uvs, triangles }`, flat
 * arrays, in the games' own space (Z up, Y forward). One with a `rig`
 * (`{ bones, slots }`: each bone's `name`, `turn`, `at` and `scale` as it
 * stood when the shape was fitted, and for each vertex four pairs of a
 * bone's number and how much it is followed) is written with its bones and
 * how its vertices follow them.
 */
export function write(shapes) {
  const out = new Out();
  out.bytes(text(MAGIC));
  out.bytes(Uint8Array.of(0x1a, 0x00));
  out.u32(VERSION);
  const setting = (name, v) => ["P", [name, "int", "Integer", "", v]];
  const objects = [];
  const joins = [];
  // Every bone any shape follows, once, under one object that holds them.
  const bones = new Map();
  let next = 100_000;
  const three = (name, kind, [x, y, z]) => ["P", [name, name, "", kind, real(x), real(y), real(z)]];
  for (const s of shapes) {
    for (const b of s.rig?.bones || []) {
      if (bones.has(b.name)) continue;
      if (!bones.size) {
        objects.push(["Model", [99_999n, "Skeleton\0\x01Model", "Null"], [["Version", [232]], ["Properties70", [], []]]]);
        joins.push(["C", ["OO", 99_999n, 0n]]);
      }
      const [model, kind] = [BigInt(next++), BigInt(next++)];
      bones.set(b.name, { model, placed: b });
      const name = b.name.replace(/\0/g, "");
      objects.push(["NodeAttribute", [kind, `${name}\0\x01NodeAttribute`, "LimbNode"], [["TypeFlags", ["Skeleton"]]]]);
      objects.push([
        "Model",
        [model, `${name}\0\x01Model`, "LimbNode"],
        [["Version", [232]], ["Properties70", [], [three("Lcl Translation", "A", b.at), three("Lcl Rotation", "A", degrees(b.turn)), three("Lcl Scaling", "A", [b.scale, b.scale, b.scale])]]],
      ]);
      joins.push(["C", ["OO", kind, model]]);
      joins.push(["C", ["OO", model, 99_999n]]);
    }
  }
  shapes.forEach((s, i) => {
    const count = s.positions.length / 3;
    const textured = s.uvs && s.uvs.length === count * 2;
    const [geometry, model] = [BigInt(1000 + i * 2), BigInt(1001 + i * 2)];
    // The last corner of each triangle is marked by being turned over.
    const corners = new Int32Array(s.triangles.length);
    for (let t = 0; t < s.triangles.length; t += 3) {
      corners[t] = s.triangles[t];
      corners[t + 1] = s.triangles[t + 1];
      corners[t + 2] = ~s.triangles[t + 2];
    }
    const made = [
      ["Vertices", [Float64Array.from(s.positions)]],
      ["PolygonVertexIndex", [corners]],
      ["GeometryVersion", [124]],
    ];
    if (textured) {
      // The games' textures start at the top; FBX's at the bottom.
      const uv = new Float64Array(count * 2);
      for (let v = 0; v < count; v++) {
        uv[v * 2] = s.uvs[v * 2];
        uv[v * 2 + 1] = 1 - s.uvs[v * 2 + 1];
      }
      made.push([
        "LayerElementUV",
        [0],
        [
          ["Version", [101]],
          ["Name", ["UVMap"]],
          ["MappingInformationType", ["ByPolygonVertex"]],
          ["ReferenceInformationType", ["IndexToDirect"]],
          ["UV", [uv]],
          ["UVIndex", [Int32Array.from(s.triangles)]],
        ],
      ]);
      made.push(["Layer", [0], [["Version", [100]], ["LayerElement", [], [["Type", ["LayerElementUV"]], ["TypedIndex", [0]]]]]]);
    }
    const name = s.name.replace(/\0/g, "");
    objects.push(["Geometry", [geometry, `${name}\0\x01Geometry`, "Mesh"], made]);
    objects.push(["Model", [model, `${name}\0\x01Model`, "Mesh"], [["Version", [232]], ["Properties70", [], []]]]);
    joins.push(["C", ["OO", geometry, model]]);
    joins.push(["C", ["OO", model, 0n]]);
    if (!s.rig?.bones.length) return;
    // Which vertices follow each bone, and by how much.
    const followers = s.rig.bones.map(() => [[], []]);
    const slots = new DataView(s.rig.slots.buffer, s.rig.slots.byteOffset, s.rig.slots.byteLength);
    for (let v = 0; v < count && v * 32 + 32 <= slots.byteLength; v++) {
      for (let k = 0; k < 4; k++) {
        const [bone, weight] = [slots.getUint32(v * 32 + k * 8, true), slots.getFloat32(v * 32 + k * 8 + 4, true)];
        if (weight > 0 && followers[bone]) (followers[bone][0].push(v), followers[bone][1].push(weight));
      }
    }
    const skin = BigInt(next++);
    objects.push(["Deformer", [skin, `${name}\0\x01Deformer`, "Skin"], [["Version", [101]], ["Link_DeformAcuracy", [real(50)]]]]);
    joins.push(["C", ["OO", skin, geometry]]);
    s.rig.bones.forEach((own, at) => {
      // Where the bone stands is said once for every shape that follows it:
      // a program reading the file keeps one standing place a bone.
      const { model: bone, placed: b } = bones.get(own.name);
      const cluster = BigInt(next++);
      objects.push([
        "Deformer",
        [cluster, `${b.name.replace(/\0/g, "")}\0\x01SubDeformer`, "Cluster"],
        [
          ["Version", [100]],
          ["UserData", ["", ""]],
          ["Indexes", [Int32Array.from(followers[at][0])]],
          ["Weights", [Float64Array.from(followers[at][1])]],
          // Where the shape is as the bone sees it, and where the bone is.
          ["Transform", [matrix(undone(b))]],
          ["TransformLink", [matrix(b)]],
        ],
      ]);
      joins.push(["C", ["OO", cluster, skin]]);
      joins.push(["C", ["OO", bone, cluster]]);
    });
  });
  const tree = [
    ["FBXHeaderExtension", [], [["FBXHeaderVersion", [1003]], ["FBXVersion", [VERSION]], ["Creator", ["Apocrypha Bodyworks"]]]],
    [
      "GlobalSettings",
      [],
      [
        ["Version", [1000]],
        [
          "Properties70",
          [],
          [
            setting("UpAxis", 2),
            setting("UpAxisSign", 1),
            setting("FrontAxis", 1),
            setting("FrontAxisSign", -1),
            setting("CoordAxis", 0),
            setting("CoordAxisSign", 1),
            ["P", ["UnitScaleFactor", "double", "Number", "", real(1)]],
          ],
        ],
      ],
    ],
    ["Objects", [], objects],
    ["Connections", [], joins],
  ];
  for (const r of tree) record(out, r);
  out.bytes(new Uint8Array(13));
  return out.done();
}

// ── Reading ────────────────────────────────────────────────────────────────

const SHORT = "The file ends before its contents do.";

/** The first `want` bytes of what compressed bytes hold, and no more of it than that. */
async function inflated(bytes, want) {
  // Nothing compresses to less than a thousandth of itself: a claim of more is not believed.
  if (want > bytes.length * 1100 + 64) throw new Error(SHORT);
  const reader = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate")).getReader();
  const parts = [];
  let had = 0;
  try {
    while (had < want) {
      const { done, value } = await reader.read();
      if (done) break;
      const take = Math.min(value.length, want - had);
      parts.push(value.subarray(0, take));
      had += take;
    }
  } catch {
    throw new Error("The file has compressed contents that cannot be read.");
  } finally {
    reader.cancel().catch(() => {});
  }
  if (had < want) throw new Error(SHORT);
  const out = new Uint8Array(want);
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

/** The records of a file, each `{ name, values, records }`. */
async function records(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.length < 27 || new TextDecoder().decode(bytes.subarray(0, MAGIC.length)) !== MAGIC) throw new Error("That is not a binary FBX file.");
  const version = view.getUint32(23, true);
  // From 7500 on, the three numbers at a record's head are twice as wide.
  const wide = version >= 7500;
  const word = (at) => (wide ? Number(view.getBigUint64(at, true)) : view.getUint32(at, true));
  const head = wide ? 25 : 13;
  // No file is let to make more work than its own size accounts for.
  let left = 4_000_000;
  let room = 1 << 29;
  const array = async (at, end, size, make) => {
    const [count, packed, length] = [view.getUint32(at, true), view.getUint32(at + 4, true), view.getUint32(at + 8, true)];
    const want = count * size;
    room -= want;
    if (at + 12 + length > end || room < 0) throw new Error(SHORT);
    let data = bytes.subarray(at + 12, at + 12 + length);
    if (packed) data = await inflated(data, want);
    else if (data.length < want) throw new Error(SHORT);
    return [make(data.slice(0, want).buffer), at + 12 + length];
  };
  const one = async (at, limit, depth) => {
    if (at + head > limit) throw new Error(SHORT);
    const [end, count] = [word(at), word(at + (wide ? 8 : 4))];
    const nameLength = bytes[at + head - 1];
    if (end === 0) return [null, at + head];
    left -= 1;
    // A record lies inside the one that holds it, and ends after it starts.
    if (end > limit || end < at + head + nameLength || depth > 64 || left < 0) throw new Error(SHORT);
    const name = new TextDecoder().decode(bytes.subarray(at + head, at + head + nameLength));
    let p = at + head + nameLength;
    const values = [];
    for (let i = 0; i < count; i++) {
      if (p >= end) throw new Error(SHORT);
      const kind = String.fromCharCode(bytes[p]);
      p += 1;
      if (kind === "S" || kind === "R") {
        const n = view.getUint32(p, true);
        if (p + 4 + n > end) throw new Error(SHORT);
        values.push(kind === "S" ? new TextDecoder().decode(bytes.subarray(p + 4, p + 4 + n)) : null);
        p += 4 + n;
      } else if (kind === "I") (values.push(view.getInt32(p, true)), (p += 4));
      else if (kind === "L") {
        // Whole numbers too large to hold exactly stay as text: they are names, not amounts.
        const n = view.getBigInt64(p, true);
        values.push(n >= -9007199254740991n && n <= 9007199254740991n ? Number(n) : String(n));
        p += 8;
      } else if (kind === "D") (values.push(view.getFloat64(p, true)), (p += 8));
      else if (kind === "F") (values.push(view.getFloat32(p, true)), (p += 4));
      else if (kind === "Y") (values.push(view.getInt16(p, true)), (p += 2));
      else if (kind === "C") (values.push(bytes[p] !== 0), (p += 1));
      else if (kind === "d") [values[i], p] = await array(p, end, 8, (b) => new Float64Array(b));
      else if (kind === "f") [values[i], p] = await array(p, end, 4, (b) => new Float32Array(b));
      else if (kind === "i") [values[i], p] = await array(p, end, 4, (b) => new Int32Array(b));
      else if (kind === "l") [values[i], p] = await array(p, end, 8, (b) => Array.from(new BigInt64Array(b), Number));
      else if (kind === "b") [values[i], p] = await array(p, end, 1, (b) => new Uint8Array(b));
      else throw new Error("The file has a kind of value this cannot read.");
      if (p > end) throw new Error(SHORT);
    }
    const inner = [];
    while (p < end) {
      const [r, next] = await one(p, end, depth + 1);
      p = next;
      if (r) inner.push(r);
      else break;
    }
    return [{ name, values, records: inner }, end];
  };
  const out = [];
  let at = 27;
  while (at + head <= bytes.length) {
    const [r, next] = await one(at, bytes.length, 0);
    if (!r) break;
    out.push(r);
    at = next;
  }
  return out;
}

const child = (r, name) => r?.records.find((x) => x.name === name);

// Placements, as sixteen numbers, a row at a time.
const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

function times(a, b) {
  const out = new Array(16).fill(0);
  for (let r = 0; r < 4; r++) for (let c = 0; c < 4; c++) for (let k = 0; k < 4; k++) out[r * 4 + c] += a[r * 4 + k] * b[k * 4 + c];
  return out;
}

const moved = ([x, y, z]) => [1, 0, 0, x, 0, 1, 0, y, 0, 0, 1, z, 0, 0, 0, 1];
const scaled = ([x, y, z]) => [x, 0, 0, 0, 0, y, 0, 0, 0, 0, z, 0, 0, 0, 0, 1];
const back = ([x, y, z]) => [-x, -y, -z];

// The orders a file may turn about its axes in, by the number it gives: the first named is turned about first.
const ORDERS = ["xyz", "xzy", "yzx", "yxz", "zxy", "zyx"];

/** A turn given as degrees about each axis, made in `order`. */
function turned([x, y, z], order = "xyz") {
  const [a, b, c] = [x, y, z].map((d) => (d * Math.PI) / 180);
  const [ca, sa, cb, sb, cc, sc] = [Math.cos(a), Math.sin(a), Math.cos(b), Math.sin(b), Math.cos(c), Math.sin(c)];
  const about = {
    x: [1, 0, 0, 0, 0, ca, -sa, 0, 0, sa, ca, 0, 0, 0, 0, 1],
    y: [cb, 0, sb, 0, 0, 1, 0, 0, -sb, 0, cb, 0, 0, 0, 0, 1],
    z: [cc, -sc, 0, 0, sc, cc, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
  };
  return times(about[order[2]], times(about[order[1]], about[order[0]]));
}

/** A turn undone: its rows made its columns. */
const unturned = (m) => [m[0], m[4], m[8], 0, m[1], m[5], m[9], 0, m[2], m[6], m[10], 0, 0, 0, 0, 1];

/** Where an object of the file puts what it holds, and where it puts its own mesh. */
function placement(object) {
  const set = {};
  for (const p of child(object, "Properties70")?.records || []) set[p.values[0]] = p.values.slice(4, 7).map(Number);
  const three = (name, or) => (set[name]?.length === 3 && set[name].every(Number.isFinite) ? set[name] : or);
  const none = [0, 0, 0];
  // The order of turning, and the turns before and after, count only where the file says they do.
  const active = Boolean(set.RotationActive?.[0]);
  const order = (active && ORDERS[set.RotationOrder?.[0]]) || "xyz";
  const [before, after] = active ? [three("PreRotation", none), three("PostRotation", none)] : [none, none];
  const [turnAbout, scaleAbout] = [three("RotationPivot", none), three("ScalingPivot", none)];
  // As the format lays it out: moved, then turned about one point, then scaled about another.
  const own = [
    moved(three("Lcl Translation", none)),
    moved(three("RotationOffset", none)),
    moved(turnAbout),
    turned(before),
    turned(three("Lcl Rotation", none), order),
    unturned(turned(after)),
    moved(back(turnAbout)),
    moved(three("ScalingOffset", none)),
    moved(scaleAbout),
    scaled(three("Lcl Scaling", [1, 1, 1])),
    moved(back(scaleAbout)),
  ].reduce(times);
  const mesh = [moved(three("GeometricTranslation", none)), turned(three("GeometricRotation", none)), scaled(three("GeometricScaling", [1, 1, 1]))].reduce(times);
  return { own, mesh };
}

/**
 * The meshes of an FBX file, each `{ name, positions, uvs, triangles, points,
 * from, followed }` as flat arrays in the games' own space. `points` are the
 * file's own vertices in the file's order, before any is made two; `from`
 * says which of them each vertex was. `followed` lists the bones the mesh
 * follows: each a `bone`, the `points` that follow it and by how much
 * (`weights`). A vertex used with two places in the texture becomes two; a
 * face of more than three corners becomes a fan of triangles. Each mesh is
 * put where its object, and the objects that hold it, place it; the file's
 * own directions and unit are applied too. A mesh that several objects
 * place is given once for each.
 */
export async function read(bytes) {
  try {
    return await meshes(bytes);
  } catch (e) {
    // Reading past the end of the file is the file ending early, whatever it was reading.
    if (e instanceof RangeError) throw new Error(SHORT);
    throw e;
  }
}

async function meshes(bytes) {
  const tree = await records(bytes);
  const top = (name) => tree.find((r) => r.name === name);
  // Which way the file's axes point, and how long its unit is.
  const said = { UpAxis: 1, UpAxisSign: 1, FrontAxis: 2, FrontAxisSign: 1, CoordAxis: 0, CoordAxisSign: 1, UnitScaleFactor: 1 };
  for (const p of child(top("GlobalSettings"), "Properties70")?.records || []) if (p.values[0] in said && Number.isFinite(Number(p.values[4]))) said[p.values[0]] = Number(p.values[4]);
  const axes = [said.CoordAxis, said.FrontAxis, said.UpAxis];
  // Three different axes, or the file is taken at its word as standing along Y.
  const [across, front, up] = new Set(axes).size === 3 && axes.every((a) => a === 0 || a === 1 || a === 2) ? axes : [0, 2, 1];
  const unit = said.UnitScaleFactor > 0 ? said.UnitScaleFactor : 1;
  // The games' meshes: across along X, up along Z, and the front of a thing facing back along Y.
  const laid = (p) => [p[across] * Math.sign(said.CoordAxisSign || 1) * unit, -p[front] * Math.sign(said.FrontAxisSign || 1) * unit, p[up] * Math.sign(said.UpAxisSign || 1) * unit];
  // Directions that make a left hand of a right one turn every face over.
  const [lx, ly, lz] = [laid([1, 0, 0]), laid([0, 1, 0]), laid([0, 0, 1])];
  const flipped = lx[0] * (ly[1] * lz[2] - ly[2] * lz[1]) - lx[1] * (ly[0] * lz[2] - ly[2] * lz[0]) + lx[2] * (ly[0] * lz[1] - ly[1] * lz[0]) < 0;
  const objects = top("Objects")?.records || [];
  const models = objects.filter((o) => o.name === "Model");
  const names = new Map(models.map((o) => [o.values[0], String(o.values[1]).split("\0")[0]]));
  const placed = new Map(models.map((o) => [o.values[0], placement(o)]));
  // What each thing is joined to: an object to the one holding it, a mesh to its objects, a skin to its mesh.
  const holder = new Map();
  const users = new Map();
  const within = new Map();
  for (const c of top("Connections")?.records || []) {
    if (c.values[0] !== "OO") continue;
    const [from, to] = [c.values[1], c.values[2]];
    if (names.has(from) && names.has(to)) holder.set(from, to);
    if (!users.has(to)) users.set(to, []);
    users.get(to).push(from);
    if (!within.has(from)) within.set(from, []);
    within.get(from).push(to);
  }
  let budget = 8_000_000;
  const deformers = new Map(objects.filter((o) => o.name === "Deformer").map((o) => [o.values[0], o]));
  const out = [];
  for (const g of objects.filter((o) => o.name === "Geometry" && o.values[2] === "Mesh")) {
    const places = child(g, "Vertices")?.values[0];
    const corners = child(g, "PolygonVertexIndex")?.values[0];
    if (!(places instanceof Float64Array) || !(corners instanceof Int32Array)) continue;
    const count = Math.floor(places.length / 3);
    const layer = child(g, "LayerElementUV");
    const uv = child(layer, "UV")?.values[0];
    const uvIndex = child(layer, "UVIndex")?.values[0];
    const textured = uv instanceof Float64Array && (!uvIndex || uvIndex instanceof Int32Array);
    // Whose place in the texture each is: a corner's, a vertex's, or one for all. Any other way of saying is not read.
    const how = { ByPolygonVertex: "corner", ByVertice: "vertex", ByVertex: "vertex", ByControlPoint: "vertex", AllSame: "all" }[child(layer, "MappingInformationType")?.values[0]];
    const direct = !uvIndex || child(layer, "ReferenceInformationType")?.values[0] === "Direct";
    const uvCount = textured && how ? Math.floor(uv.length / 2) : 0;
    if (uvCount && uv.some((c) => !Number.isFinite(Math.fround(c)))) throw new Error("The file has a place in the texture that is not two numbers a mesh can hold.");
    // The bones it follows: its skins' clusters, each named by the object joined to it.
    const followed = [];
    for (const skin of users.get(g.values[0]) || []) {
      if (deformers.get(skin)?.values[2] !== "Skin") continue;
      for (const id of users.get(skin) || []) {
        const cluster = deformers.get(id);
        if (cluster?.values[2] !== "Cluster") continue;
        const bone = (users.get(id) || []).map((b) => names.get(b)).find(Boolean);
        const [points, weights] = [child(cluster, "Indexes")?.values[0], child(cluster, "Weights")?.values[0]];
        if (!bone || !(points instanceof Int32Array) || !(weights instanceof Float64Array) || points.length !== weights.length) continue;
        followed.push({ bone, points, weights: Float32Array.from(weights) });
      }
    }
    // Once for each object that places it, or once where it lies if none does.
    const holders = [...new Set(within.get(g.values[0]) || [])].filter((id) => placed.has(id));
    for (const first of holders.length ? holders : [null]) {
      // Where the mesh ends up: its own object's placement, inside each object that holds that one.
      let m = IDENTITY;
      const seen = new Set();
      for (let id = first; placed.has(id) && !seen.has(id); id = holder.get(id)) {
        const { own, mesh } = placed.get(id);
        m = times(own, id === first ? times(mesh, m) : m);
        seen.add(id);
      }
      const mirrored = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8]) + m[2] * (m[4] * m[9] - m[5] * m[8]) < 0 !== flipped;
      // Every placing of a mesh is the mesh again: no file is let to ask for more than a budget of them.
      budget -= count + corners.length;
      if (budget < 0) throw new Error("The file places its meshes more times than this will make of them.");
      const points = new Float32Array(count * 3);
      for (let v = 0; v < count; v++) {
        const [px, py, pz] = [places[v * 3], places[v * 3 + 1], places[v * 3 + 2]];
        points.set(laid([0, 4, 8].map((r) => m[r] * px + m[r + 1] * py + m[r + 2] * pz + m[r + 3])), v * 3);
      }
      if (points.some((c) => !Number.isFinite(c))) throw new Error("The file has a vertex that is not three numbers a mesh can hold.");
      const from = [];
      const uvs = [];
      const triangles = [];
      const made = new Map();
      let face = [];
      for (let at = 0; at < corners.length; at++) {
        const raw = corners[at];
        const last = raw < 0;
        const v = last ? ~raw : raw;
        if (v >= count) throw new Error("The file has a face with a vertex it does not have.");
        // Its place in the texture: by this corner, or by its vertex; one it does not have is none.
        let t = -1;
        if (uvCount) {
          const slot = how === "corner" ? at : how === "vertex" ? v : 0;
          t = direct ? slot : slot < uvIndex.length ? uvIndex[slot] : -1;
          if (t < 0 || t >= uvCount) t = -1;
        }
        const key = v * (uvCount + 1) + t + 1;
        let index = made.get(key);
        if (index === undefined) {
          index = from.length;
          made.set(key, index);
          from.push(v);
          if (t >= 0) uvs.push(uv[t * 2], 1 - uv[t * 2 + 1]);
          else uvs.push(0, 0);
        }
        face.push(index);
        if (last) {
          // A placement that mirrors turns every face over; turn them back.
          for (let k = 1; k + 1 < face.length; k++) triangles.push(face[0], face[mirrored ? k + 1 : k], face[mirrored ? k : k + 1]);
          face = [];
        }
      }
      if (!triangles.length) continue;
      const positions = new Float32Array(from.length * 3);
      from.forEach((v, i) => positions.set(points.subarray(v * 3, v * 3 + 3), i * 3));
      const name = names.get(first) || String(g.values[1]).split("\0")[0] || "Shape";
      out.push({ name, positions, uvs: Float32Array.from(uvs), triangles: Uint32Array.from(triangles), points, from: Uint32Array.from(from), followed });
    }
  }
  if (!out.length) throw new Error("The file has no meshes.");
  return out;
}

/**
 * What a mesh read from a file follows, as the engine takes it: the bones'
 * names, and for each vertex four pairs of a bone's number among them and
 * how much, the heaviest four it has.
 */
export function follows(mesh) {
  const count = mesh.points.length / 3;
  // One number a bone, however many times the file names it.
  const bones = [...new Set(mesh.followed.map((f) => f.bone))];
  const number = new Map(bones.map((b, i) => [b, i]));
  // Everything each of the file's own vertices follows, a vertex after another.
  const starts = new Uint32Array(count + 1);
  const good = (v, w) => v >= 0 && v < count && w > 0 && Number.isFinite(w);
  for (const f of mesh.followed) for (let i = 0; i < f.points.length; i++) if (good(f.points[i], f.weights[i])) starts[f.points[i] + 1] += 1;
  for (let v = 0; v < count; v++) starts[v + 1] += starts[v];
  const [which, much, filled] = [new Uint32Array(starts[count]), new Float32Array(starts[count]), starts.slice(0, count)];
  for (const f of mesh.followed) {
    const b = number.get(f.bone);
    for (let i = 0; i < f.points.length; i++) {
      const v = f.points[i];
      if (!good(v, f.weights[i])) continue;
      which[filled[v]] = b;
      much[filled[v]++] = Math.min(f.weights[i], 1);
    }
  }
  // For each, a bone named twice counted once, then its heaviest four.
  const bone = new Uint32Array(count * 4);
  const most = new Float32Array(count * 4);
  for (let v = 0; v < count; v++) {
    for (let i = starts[v]; i < starts[v + 1]; i++) {
      for (let j = starts[v]; j < i; j++) {
        if (which[j] === which[i] && much[j] > 0) {
          much[j] += much[i];
          much[i] = 0;
          break;
        }
      }
    }
    for (let i = starts[v]; i < starts[v + 1]; i++) {
      if (!(much[i] > 0)) continue;
      // In place of the lightest of the four, where it is heavier.
      let least = v * 4;
      for (let k = v * 4 + 1; k < v * 4 + 4; k++) if (most[k] < most[least]) least = k;
      if (much[i] > most[least]) [bone[least], most[least]] = [which[i], much[i]];
    }
  }
  const bytes = new Uint8Array(mesh.from.length * 32);
  const view = new DataView(bytes.buffer);
  mesh.from.forEach((v, i) => {
    for (let k = 0; k < 4; k++) {
      view.setUint32(i * 32 + k * 8, bone[v * 4 + k], true);
      view.setFloat32(i * 32 + k * 8 + 4, most[v * 4 + k], true);
    }
  });
  return { bones, bytes };
}
