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

/**
 * Shapes as one FBX file. Each: `{ name, positions, uvs, triangles }`, flat
 * arrays, in the games' own space (Z up, Y forward).
 */
export function write(shapes) {
  const out = new Out();
  out.bytes(text(MAGIC));
  out.bytes(Uint8Array.of(0x1a, 0x00));
  out.u32(VERSION);
  const setting = (name, v) => ["P", [name, "int", "Integer", "", v]];
  const objects = [];
  const joins = [];
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

async function inflated(bytes) {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** The records of a file, each `{ name, values, records }`. */
async function records(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (new TextDecoder().decode(bytes.subarray(0, MAGIC.length)) !== MAGIC) throw new Error("That is not a binary FBX file.");
  const version = view.getUint32(23, true);
  // From 7500 on, the three numbers at a record's head are twice as wide.
  const wide = version >= 7500;
  const word = (at) => (wide ? Number(view.getBigUint64(at, true)) : view.getUint32(at, true));
  const head = wide ? 25 : 13;
  const array = async (at, size, make) => {
    const [count, packed, length] = [view.getUint32(at, true), view.getUint32(at + 4, true), view.getUint32(at + 8, true)];
    if (at + 12 + length > bytes.length || count > 50_000_000) throw new Error("The file ends before its contents do.");
    let data = bytes.subarray(at + 12, at + 12 + length);
    if (packed) data = await inflated(data);
    if (data.length < count * size) throw new Error("The file ends before its contents do.");
    return [make(data.slice(0, count * size).buffer), at + 12 + length];
  };
  const one = async (at, depth) => {
    const [end, count] = [word(at), word(at + (wide ? 8 : 4))];
    const nameLength = bytes[at + head - 1];
    if (end === 0) return [null, at + head];
    if (end > bytes.length || end < at + head || depth > 64) throw new Error("The file ends before its contents do.");
    const name = new TextDecoder().decode(bytes.subarray(at + head, at + head + nameLength));
    let p = at + head + nameLength;
    const values = [];
    for (let i = 0; i < count; i++) {
      const kind = String.fromCharCode(bytes[p]);
      p += 1;
      if (kind === "S" || kind === "R") {
        const n = view.getUint32(p, true);
        values.push(kind === "S" ? new TextDecoder().decode(bytes.subarray(p + 4, p + 4 + n)) : null);
        p += 4 + n;
      } else if (kind === "I") (values.push(view.getInt32(p, true)), (p += 4));
      else if (kind === "L") {
        // Whole numbers too large to hold exactly stay as text: they are names, not amounts.
        const n = view.getBigInt64(p, true);
        values.push(n >= -9007199254740991n && n <= 9007199254740991n ? Number(n) : String(n));
        p += 8;
      }
      else if (kind === "D") (values.push(view.getFloat64(p, true)), (p += 8));
      else if (kind === "F") (values.push(view.getFloat32(p, true)), (p += 4));
      else if (kind === "Y") (values.push(view.getInt16(p, true)), (p += 2));
      else if (kind === "C") (values.push(bytes[p] !== 0), (p += 1));
      else if (kind === "d") [values[i], p] = await array(p, 8, (b) => new Float64Array(b));
      else if (kind === "f") [values[i], p] = await array(p, 4, (b) => new Float32Array(b));
      else if (kind === "i") [values[i], p] = await array(p, 4, (b) => new Int32Array(b));
      else if (kind === "l") [values[i], p] = await array(p, 8, (b) => Array.from(new BigInt64Array(b), Number));
      else if (kind === "b") [values[i], p] = await array(p, 1, (b) => new Uint8Array(b));
      else throw new Error("The file has a kind of value this cannot read.");
      if (p > end) throw new Error("The file ends before its contents do.");
    }
    const inner = [];
    while (p < end) {
      const [r, next] = await one(p, depth + 1);
      p = next;
      if (r) inner.push(r);
      else break;
    }
    return [{ name, values, records: inner }, end];
  };
  const out = [];
  let at = 27;
  while (at + head <= bytes.length) {
    const [r, next] = await one(at, 0);
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

/** A turn given as degrees about X, then Y, then Z. */
function turned([x, y, z]) {
  const [a, b, c] = [x, y, z].map((d) => (d * Math.PI) / 180);
  const [ca, sa, cb, sb, cc, sc] = [Math.cos(a), Math.sin(a), Math.cos(b), Math.sin(b), Math.cos(c), Math.sin(c)];
  const rx = [1, 0, 0, 0, 0, ca, -sa, 0, 0, sa, ca, 0, 0, 0, 0, 1];
  const ry = [cb, 0, sb, 0, 0, 1, 0, 0, -sb, 0, cb, 0, 0, 0, 0, 1];
  const rz = [cc, -sc, 0, 0, sc, cc, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  return times(rz, times(ry, rx));
}

/** Where an object of the file puts what it holds, and where it puts its own mesh. */
function placement(object) {
  const set = {};
  for (const p of child(object, "Properties70")?.records || []) set[p.values[0]] = p.values.slice(4, 7).map(Number);
  const three = (name, or) => (set[name]?.length === 3 && set[name].every(Number.isFinite) ? set[name] : or);
  const own = [moved(three("Lcl Translation", [0, 0, 0])), turned(three("PreRotation", [0, 0, 0])), turned(three("Lcl Rotation", [0, 0, 0])), scaled(three("Lcl Scaling", [1, 1, 1]))].reduce(times);
  const mesh = [moved(three("GeometricTranslation", [0, 0, 0])), turned(three("GeometricRotation", [0, 0, 0])), scaled(three("GeometricScaling", [1, 1, 1]))].reduce(times);
  return { own, mesh };
}

/**
 * The meshes of an FBX file, each `{ name, positions, uvs, triangles, points }`
 * as flat arrays in the games' own space; `points` are the file's own
 * vertices in the file's order, before any is made two. A vertex used with two places in the
 * texture becomes two; a face of more than three corners becomes a fan of
 * triangles. Each mesh is put where its object, and the objects that hold
 * it, place it; the file's own up direction and unit are applied too.
 */
export async function read(bytes) {
  const tree = await records(bytes);
  const top = (name) => tree.find((r) => r.name === name);
  // Which way is up, and how long a unit is.
  let up = 1;
  let unit = 1;
  for (const p of child(top("GlobalSettings"), "Properties70")?.records || []) {
    if (p.values[0] === "UpAxis") up = p.values[4];
    if (p.values[0] === "UnitScaleFactor") unit = p.values[4];
  }
  const objects = top("Objects")?.records || [];
  const models = objects.filter((o) => o.name === "Model");
  const names = new Map(models.map((o) => [o.values[0], String(o.values[1]).split("\0")[0]]));
  const placed = new Map(models.map((o) => [o.values[0], placement(o)]));
  const model = new Map();
  for (const c of top("Connections")?.records || []) if (c.values[0] === "OO" && names.has(c.values[2])) model.set(c.values[1], c.values[2]);
  const meshes = [];
  for (const g of objects.filter((o) => o.name === "Geometry" && o.values[2] === "Mesh")) {
    const places = child(g, "Vertices")?.values[0];
    const corners = child(g, "PolygonVertexIndex")?.values[0];
    if (!places || !corners) continue;
    const layer = child(g, "LayerElementUV");
    const uv = child(layer, "UV")?.values[0];
    const uvIndex = child(layer, "UVIndex")?.values[0];
    const byCorner = child(layer, "MappingInformationType")?.values[0] === "ByPolygonVertex";
    // Where the mesh ends up: its own object's placement, inside each object that holds that one.
    let place = IDENTITY;
    const seen = new Set();
    for (let id = model.get(g.values[0]), first = true; placed.has(id) && !seen.has(id); id = model.get(id), first = false) {
      seen.add(id);
      const { own, mesh } = placed.get(id);
      place = times(own, first ? times(mesh, place) : place);
    }
    const m = place;
    const put = (v) => {
      const [px, py, pz] = [places[v * 3], places[v * 3 + 1], places[v * 3 + 2]];
      const [x, y, z] = [0, 4, 8].map((r) => (m[r] * px + m[r + 1] * py + m[r + 2] * pz + m[r + 3]) * unit);
      // A file that stands along Y is laid along Z, as the games' meshes are.
      return up === 1 ? [x, -z, y] : [x, y, z];
    };
    const mirrored = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8]) + m[2] * (m[4] * m[9] - m[5] * m[8]) < 0;
    const positions = [];
    const uvs = [];
    const triangles = [];
    const made = new Map();
    const count = places.length / 3;
    let face = [];
    corners.forEach((raw, at) => {
      const last = raw < 0;
      const v = last ? ~raw : raw;
      if (v >= count) throw new Error("The file has a face with a vertex it does not have.");
      // Its place in the texture: by this corner, or by its vertex.
      const t = uv ? (uvIndex ? uvIndex[byCorner ? at : v] : byCorner ? at : v) : -1;
      const key = `${v}/${t}`;
      let index = made.get(key);
      if (index === undefined) {
        index = positions.length / 3;
        made.set(key, index);
        positions.push(...put(v));
        if (t >= 0 && t * 2 + 1 < uv.length) uvs.push(uv[t * 2], 1 - uv[t * 2 + 1]);
        else uvs.push(0, 0);
      }
      face.push(index);
      if (last) {
        // A placement that mirrors turns every face over; turn them back.
        for (let k = 1; k + 1 < face.length; k++) triangles.push(face[0], face[mirrored ? k + 1 : k], face[mirrored ? k : k + 1]);
        face = [];
      }
    });
    if (!triangles.length) continue;
    if (positions.some((c) => !Number.isFinite(Math.fround(c)))) throw new Error("The file has a vertex that is not three numbers a mesh can hold.");
    const name = names.get(model.get(g.values[0])) || String(g.values[1]).split("\0")[0] || "Shape";
    const points = new Float32Array(count * 3);
    for (let v = 0; v < count; v++) points.set(put(v), v * 3);
    if (points.some((c) => !Number.isFinite(c))) throw new Error("The file has a vertex that is not three numbers a mesh can hold.");
    meshes.push({ name, positions: Float32Array.from(positions), uvs: Float32Array.from(uvs), triangles: Uint32Array.from(triangles), points });
  }
  if (!meshes.length) throw new Error("The file has no meshes.");
  return meshes;
}
