// The BodySlide screen: pick a body or outfit, pick a preset, move its
// sliders with the shape following, and build it into a mod.

import * as host from "./host.js";
import * as core from "./core.js";
import { View, welded, shade } from "./view.js";
import * as brush from "./brush.js";
import * as obj from "./obj.js";

const TOOLS = "Data/CalienteTools/BodySlide";
/** The mod builds go into, unless another is chosen. */
const OUTPUT = "BodySlide output";
const FOLDERS = ["SliderSets", "SliderPresets", "SliderGroups", "SliderCategories"];

const CHEVRON =
  '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>';
const GLASS =
  '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/></svg>';

/** An element: `h("button.btn.primary", { onclick }, "Build")`. */
function h(spec, props, ...children) {
  const [tag, ...classes] = spec.split(".");
  const el = document.createElement(tag || "div");
  if (classes.length) el.className = classes.join(" ");
  for (const [k, v] of Object.entries(props || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k === "html") el.innerHTML = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k in el && k !== "list") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) if (c !== null && c !== undefined && c !== false) el.append(c);
  return el;
}

/** What is kept between visits; and where the frame has no storage of its own, for this visit at least. */
const kept = new Map();
const remembered = {
  get(key) {
    if (kept.has(key)) return kept.get(key);
    try {
      return localStorage.getItem(`bodyslide.${key}`);
    } catch {
      return null;
    }
  },
  set(key, value) {
    kept.set(key, value);
    try {
      localStorage.setItem(`bodyslide.${key}`, value);
    } catch {
      // A frame with no storage of its own remembers for this visit only.
    }
  },
};

const state = {
  game: null,
  /** Lower-case path under the BodySlide folder → the path the mods have it at. */
  paths: new Map(),
  catalog: { sets: [], presets: [], groups: [], categories: [] },
  set: null,
  preset: null,
  small: [],
  big: [],
  heavy: true,
  search: "",
  shut: new Set(),
  shapes: [],
  built: null,
  /** The mod builds go into. */
  output: remembered.get("output") || OUTPUT,
  /** BodySlide: the group whose outfits are listed, or null for all. */
  group: null,
  /** Brushes work only on what is joined to what is under them. */
  connected: remembered.get("brush.connected") === "1",
  /** Whether a build also writes the morph file RaceMenu reads. */
  morphs: remembered.get("morphs") === "1",
  /** Which of the two tools is showing: "bodyslide" or "studio". */
  mode: remembered.get("mode") === "studio" ? "studio" : "bodyslide",
  /** Outfit Studio: where each slider stands while the outfit is looked at. */
  at: [],
  /** Outfit Studio: the shape picked in the list, and which list is showing. */
  picked: 0,
  side: "shapes",
  /** Outfit Studio: the brush in hand ("inflate", "deflate", "move", "smooth"), or null to turn the view. */
  tool: null,
  /** The brush's size and strength, each 1 to 100, and whether it works both sides of the body at once. */
  size: Number(remembered.get("brush.size")) || 30,
  strength: Number(remembered.get("brush.strength")) || 40,
  mirror: remembered.get("brush.mirror") !== "0",
  /** How wide the view sees, in degrees, and how brightly it is lit, in percent. */
  fov: Number(remembered.get("view.fov")) || 34,
  light: Number(remembered.get("view.light")) || 100,
  /** How far a brush goes between one dab and the next, 1 to 100 as a share of its reach. */
  spacing: Number(remembered.get("brush.spacing")) || 25,
  /** How much of a brush's reach has its full strength, 1 to 100: a point, up to nearly all of it. */
  focus: Number(remembered.get("brush.focus")) || 50,
  /** The slider the brushes change (its place in the list), or -1 for the shape itself. */
  editing: -1,
  /** The colour brush's colour ("#rrggbb") and how solid it paints, 0 to 100. */
  colour: remembered.get("brush.colour") || "#c8643c",
  alpha: Number(remembered.get("brush.alpha") ?? 100),
  /** Every bone the mesh on screen has, whether a shape follows it or not. */
  bones: [],
  /** The bone the weight brush paints, by name, and how much each vertex of the picked shape follows it. */
  bone: null,
  heat: null,
  /** Outfit Studio: the part of the picked shape drawn alone, by its place in the list, or null. */
  part: null,
  /** Outfit Studio: the open project as it stands, changes and all. */
  work: null,
  /** Files saved here that the mods do not have yet, by lower-case path. */
  kept: new Map(),
  busy: null,
  problem: null,
};

let view = null;
const root = document.getElementById("app");

// ── Files ──────────────────────────────────────────────────────────────────

const under = (path) => path.slice(TOOLS.length + 1);

/** Hand the module the file the mods have at `rel` (under the BodySlide folder). */
async function give(rel) {
  const at = rel.replace(/\\/g, "/").toLowerCase();
  if (state.kept.has(at)) {
    core.ask({ op: "put", path: rel }, new Uint8Array(state.kept.get(at)));
    return true;
  }
  const real = state.paths.get(at);
  if (!real) return false;
  const bytes = await host.request("files.read", { path: real });
  core.ask({ op: "put", path: rel }, new Uint8Array(bytes));
  return true;
}

async function inBatches(items, size, each) {
  for (let i = 0; i < items.length; i += size) await Promise.all(items.slice(i, i + size).map(each));
}

async function readCatalog() {
  state.paths.clear();
  core.ask({ op: "clear" });
  const lists = await Promise.all(
    [...FOLDERS, "ShapeData"].map((f) => host.request("files.list", { folder: `${TOOLS}/${f}` }).catch(() => [])),
  );
  const projects = [];
  lists.forEach((list, i) => {
    for (const f of list) {
      const rel = under(f.path);
      state.paths.set(rel.toLowerCase(), f.path);
      if (i < FOLDERS.length && /\.(osp|xml)$/i.test(rel)) projects.push(rel);
    }
  });
  let done = 0;
  await inBatches(projects, 12, async (rel) => {
    await give(rel);
    done += 1;
    if (done % 24 === 0) busy(`Reading ${done} of ${projects.length}`);
  });
  [state.catalog] = core.ask({ op: "load" });
}

// ── Sliders and the shape ──────────────────────────────────────────────────

function applied(slider, percent) {
  let f = percent / 100;
  if (slider.clamp) f = Math.max(0, Math.min(1, f));
  return slider.invert ? 1 - f : f;
}

/** The set on screen: in Outfit Studio the open project, changes and all. */
function cur() {
  return (state.mode === "studio" && state.work) || state.set;
}

/** The triangles of a shape that the sliders taking vertices out leave. */
function remaining(s, values) {
  const on = s.zaps.filter((z) => applied(cur().sliders[z.slider], values[z.slider]) !== 0);
  const key = on.map((z) => z.slider).join(",");
  if (key === s.zapped) return null;
  s.zapped = key;
  if (!on.length) return s.triangles;
  const gone = new Uint8Array(s.base.length / 3);
  for (const z of on) for (const v of z.vertices) gone[v] = 1;
  const kept = new Uint32Array(s.triangles.length);
  let n = 0;
  for (let t = 0; t < s.triangles.length; t += 3) {
    const [a, b, c] = [s.triangles[t], s.triangles[t + 1], s.triangles[t + 2]];
    if (gone[a] || gone[b] || gone[c]) continue;
    kept[n++] = a;
    kept[n++] = b;
    kept[n++] = c;
  }
  return kept.subarray(0, n);
}

/** Move every shape to where the sliders put it, and show it. */
function reshape() {
  const values = state.mode === "studio" ? state.at : state.heavy ? state.big : state.small;
  state.shapes.forEach((s, i) => {
    const left = remaining(s, values);
    if (left) {
      s.shown = left;
      view.cut(i, left);
    }
    s.now.set(s.base);
    for (const d of s.sliders) {
      const amount = applied(cur().sliders[d.slider], values[d.slider]);
      if (amount === 0) continue;
      const { vertices, offsets } = d;
      for (let k = 0; k < vertices.length; k++) {
        const v = vertices[k] * 3;
        s.now[v] += offsets[k * 3] * amount;
        s.now[v + 1] += offsets[k * 3 + 1] * amount;
        s.now[v + 2] += offsets[k * 3 + 2] * amount;
      }
    }
    // A part seen alone stays alone, whatever a slider took out or put back.
    if (left && s.live && i === state.picked && state.part !== null && s.parts[state.part]) {
      s.shown = s.parts[state.part].triangles;
      view.cut(i, s.shown);
    }
    shade(s.now, s.shown, s.weld, s.normals, s.sum);
    view.move(i, s.now, s.normals);
  });
}

function presetValues(set, preset) {
  const small = set.sliders.map((s) => s.small);
  const big = set.sliders.map((s) => s.big);
  if (preset) {
    const at = new Map(set.sliders.map((s, i) => [s.name, i]));
    for (const [name, heavy, value] of preset.values) {
      const i = at.get(name);
      if (i === undefined) continue;
      (heavy ? big : small)[i] = value;
    }
  }
  return { small, big };
}

/** The presets made for a set: those naming one of the groups it is in. */
function presetsFor(set) {
  const groups = new Set(set.groups);
  const fits = state.catalog.presets.filter((p) => p.groups.some((g) => groups.has(g)));
  return fits.length ? fits : state.catalog.presets.filter((p) => p.set === set.name);
}

async function prepare(set) {
  const [needs] = core.ask({ op: "needs", set: set.name });
  const missing = [];
  await inBatches(needs, 8, async (rel) => {
    if (!(await give(rel))) missing.push(rel);
  });
  return missing;
}

/** Read the shapes of what is on screen, and put them in the view. */
function showShapes(frame) {
  const studio = state.mode === "studio" && Boolean(state.work);
  const hidden = new Set(studio ? state.shapes.filter((sh) => sh.hidden).map((sh) => sh.name) : []);
  const masks = new Map(studio ? state.shapes.map((sh) => [sh.name, sh.mask]) : []);
  const ghosts = new Set(studio ? state.shapes.filter((sh) => sh.ghost).map((sh) => sh.name) : []);
  const pickedName = state.shapes[state.picked]?.name;
  state.part = null;
  const [preview, numbers] = core.ask({ op: "preview", set: state.set.name, work: studio });
  const f32 = new Float32Array(numbers);
  const u32 = new Uint32Array(numbers);
  state.bones = preview.bones || [];
  state.shapes = preview.shapes.map((s) => {
    const base = f32.slice(s.positions[0], s.positions[0] + s.positions[1]);
    const made = {
      name: s.name,
      base,
      now: new Float32Array(base),
      normals: new Float32Array(base.length),
      sum: new Float32Array(base.length),
      weld: welded(base),
      triangles: u32.slice(s.triangles[0], s.triangles[0] + s.triangles[1]),
      bones: s.bones || [],
      reference: Boolean(s.reference),
      hidden: hidden.has(s.name),
      ghost: ghosts.has(s.name),
      uvs: f32.slice(s.uvs[0], s.uvs[0] + s.uvs[1]),
      texture: (s.textures || [])[0] || null,
      textures: s.textures || [],
      colors: s.colors && s.colors[1] ? new Uint8Array(numbers.slice(s.colors[0] * 4, (s.colors[0] + s.colors[1]) * 4)) : new Uint8Array(0),
      parts: (s.parts || []).map((part) => ({
        part: part.part,
        triangles: u32.slice(part.triangles[0], part.triangles[0] + part.triangles[1]),
      })),
      places: s.bonePlaces ? f32.slice(s.bonePlaces[0], s.bonePlaces[0] + s.bonePlaces[1]) : new Float32Array(0),
      shown: null,
      zapped: "",
      zaps: (s.zaps || []).map((z) => ({
        slider: z.slider,
        vertices: u32.subarray(z.vertices[0], z.vertices[0] + z.vertices[1]),
      })),
      sliders: s.sliders.map((d) => ({
        slider: d.slider,
        vertices: u32.subarray(d.vertices[0], d.vertices[0] + d.vertices[1]),
        offsets: f32.subarray(d.offsets[0], d.offsets[0] + d.offsets[1]),
      })),
    };
    made.shown = made.triangles;
    // A mask stays on the shape it was painted on, while it has the same vertices.
    const mask = masks.get(s.name);
    made.mask = mask && mask.length === base.length / 3 ? mask : new Float32Array(base.length / 3);
    return made;
  });
  if (!view) return;
  view.show(state.shapes.map((s) => ({ positions: s.base, triangles: s.triangles, uvs: s.uvs, mask: s.mask, colors: s.colors })));
  state.shapes.forEach((sh, i) => {
    if (sh.hidden) view.hide(i, true);
    if (sh.ghost) view.ghost(i, true);
  });
  // The same shape stays picked, wherever it now is in the list.
  const again = state.shapes.findIndex((sh) => sh.name === pickedName);
  state.picked = again >= 0 ? again : Math.min(state.picked, Math.max(0, state.shapes.length - 1));
  view.look({ picked: state.mode === "studio" ? state.picked : -1 });
  if (!state.shapes.length) return;
  reshape();
  paint(state.set);
  if (studio) heat();
  if (frame) view.frame(state.shapes.map((s) => s.base));
}

async function choose(set, presetName) {
  state.set = set;
  state.note = null;
  view?.forget();
  state.work = null;
  state.editing = -1;
  stroke = null;
  state.exported = null;
  state.saved = null;
  state.built = null;
  state.problem = null;
  state.shapes = [];
  remembered.set(`set.${state.game?.id}`, set.name);
  busy("Opening");
  let missing = [];
  try {
    missing = await prepare(set);
    try {
      [state.work] = core.ask({ op: "open", set: set.name });
    } catch {
      // What cannot be opened to work on can still be looked at and built.
    }
    const fits = presetsFor(set);
    state.preset = fits.find((p) => p.name === presetName) || fits.find((p) => p.name === state.preset?.name) || fits[0] || null;
    Object.assign(state, presetValues(set, state.preset));
    // In Outfit Studio the outfit starts as it was made: nothing moved, and
    // only what a set takes out by default taken out.
    state.at = set.sliders.map((sl) => (sl.zap ? sl.big : 0));
  } catch (e) {
    state.problem = e.message;
  }
  state.busy = null;
  render();
  try {
    showShapes(true);
    state.picked = Math.max(0, state.shapes.findIndex((sh) => !sh.reference));
    view.look({ picked: state.mode === "studio" ? state.picked : -1 });
  } catch (e) {
    state.problem = state.problem || e.message;
    view.show([]);
  }
  if (!state.shapes.length && !state.problem) {
    state.problem = missing.length ? `${missing[0].split("/").pop()} is missing.` : "This set has no shapes to show.";
  }
  render();
}

/** Pictures already fetched, by the path a shape names. */
const pictures = new Map();

/** Paint each shape with its texture, where the mods have it. */
function paint(set) {
  state.shapes.forEach((s, i) => {
    if (!s.texture) return;
    const path = `Data/${s.texture.replace(/\\/g, "/").replace(/^\/+/, "")}`;
    if (!pictures.has(path)) {
      pictures.set(
        path,
        host
          .request("files.picture", { path, longest: 2048 })
          .then((png) => createImageBitmap(new Blob([png], { type: "image/png" })))
          .catch(() => null),
      );
    }
    pictures.get(path).then((picture) => {
      // Another set may have been opened while this was on its way.
      if (picture && state.set === set && state.shapes[i] === s) view.paint_with(i, picture, path);
    });
  });
}

// ── Building ───────────────────────────────────────────────────────────────

function filesOf(built, bytes) {
  return built.outputs.map((o) => ({
    path: `Data/${o.path}`,
    bytes: bytes.slice(o.at, o.at + o.length),
  }));
}

async function write(files) {
  await host.request(
    "output.write",
    { name: state.output, files },
    files.map((f) => f.bytes),
  );
}

async function build() {
  busy("Building");
  try {
    const [built, bytes] = core.ask({ op: "build", set: state.set.name, small: state.small, big: state.big, morphs: state.morphs });
    const files = filesOf(built, bytes);
    await write(files);
    state.built = files.length;
  } catch (e) {
    state.problem = e.message;
  }
  state.busy = null;
  render();
}

/** Build the outfit on screen into a folder the person picks, laid out as the game's data folder is. */
async function buildToFolder() {
  busy("Building");
  try {
    const [built, bytes] = core.ask({ op: "build", set: state.set.name, small: state.small, big: state.big, morphs: state.morphs });
    // Under the folder as under the game's data folder: `meshes/…`.
    const files = built.outputs.map((o) => ({ path: o.path, bytes: bytes.slice(o.at, o.at + o.length) }));
    const { saved } = await host.request("disk.folder", { files }, files.map((f) => f.bytes));
    state.problem = null;
    state.note = saved ? `${saved} ${saved === 1 ? "file" : "files"} built into the folder` : null;
  } catch (e) {
    state.problem = e.message;
  }
  state.busy = null;
  render();
}

/** Build each of `sets` with the sliders as they stand, matched by name. */
async function buildSeveral(sets, progress) {
  const small = new Map(state.set.sliders.map((s, i) => [s.name, state.small[i]]));
  const big = new Map(state.set.sliders.map((s, i) => [s.name, state.big[i]]));
  const files = [];
  const failed = [];
  for (let i = 0; i < sets.length; i++) {
    const set = sets[i];
    progress(`Building ${i + 1} of ${sets.length}`);
    try {
      await prepare(set);
      const [built, bytes] = core.ask({
        op: "build",
        set: set.name,
        small: set.sliders.map((s) => small.get(s.name) ?? s.small),
        big: set.sliders.map((s) => big.get(s.name) ?? s.big),
        morphs: state.morphs,
      });
      files.push(...filesOf(built, bytes));
    } catch {
      failed.push(set.name);
    }
    // Slider data is large and each set has its own; keep only what is open.
    if (set.name !== state.set.name) core.ask({ op: "drop", under: "ShapeData" });
    await new Promise((r) => setTimeout(r));
  }
  if (files.length) {
    progress("Saving");
    await write(files);
  }
  await prepare(state.set);
  return { files: files.length, failed };
}

// ── Menus and sheets ───────────────────────────────────────────────────────

function close(el) {
  el.remove();
}

/** A list to pick one of, dropped from `anchor`. Items: `{ name, note, on, value }`. */
function menu(anchor, items, onPick, placeholder, above) {
  const box = anchor.getBoundingClientRect();
  // Where there is no room for it below, it opens above.
  const tall = Math.min(480, innerHeight * 0.7, items.length * 30 + (items.length > 8 ? 60 : 16));
  if (box.bottom + 6 + tall > innerHeight && box.top > innerHeight - box.bottom) above = true;
  const rows = h("div.rows");
  const fill = (query) => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const shown = items.filter((it) => words.every((w) => `${it.name} ${it.note || ""}`.toLowerCase().includes(w)));
    rows.replaceChildren(
      ...shown.slice(0, 400).map((it) =>
        [
          it.rule && h("hr"),
          h(
            "button" + (it.on ? ".on" : "") + (it.danger ? ".danger" : ""),
            {
              type: "button",
              onclick: () => {
                close(scrim);
                onPick(it.value);
              },
            },
            h("span.nm.cut", null, it.name),
            it.note && h("span.sub.quiet.cut", null, it.note),
          ),
        ],
      ).flat().filter(Boolean),
    );
    if (!shown.length) rows.append(h("div.sub.quiet", { style: "padding: 8px 10px" }, "Nothing found"));
  };
  const input = h("input", { type: "text", placeholder, "aria-label": placeholder, oninput: () => fill(input.value) });
  const panel = h(
    "div.menu",
    {
      role: "dialog",
      style: `left: ${Math.max(8, Math.min(box.left, innerWidth - 328))}px; ${above ? `bottom: ${innerHeight - box.top + 6}px` : `top: ${box.bottom + 6}px`}`,
    },
    items.length > 8 && h("label.search", { html: GLASS }, input),
    rows,
  );
  const scrim = h(
    "div.scrim",
    {
      onpointerdown: (e) => e.target === scrim && close(scrim),
      onkeydown: (e) => e.key === "Escape" && close(scrim),
    },
    panel,
  );
  fill("");
  document.body.append(scrim);
  if (items.length > 8) input.focus();
  rows.querySelector(".on")?.scrollIntoView({ block: "center" });
}

function sheet(title, side, body, foot) {
  const scrim = h(
    "div.scrim.dim",
    { onkeydown: (e) => e.key === "Escape" && close(scrim) },
    h(
      "div.sheet",
      { role: "dialog", "aria-label": title },
      h("div.sheet-head", null, h("span.sheet-title", null, title), side),
      h("div.sheet-body", null, body),
      h("div.sheet-foot", null, foot),
    ),
  );
  document.body.append(scrim);
  return scrim;
}

function savePreset() {
  const name = h("input.input", { type: "text", value: state.preset?.name || "", "aria-label": "Name", maxLength: 80 });
  const save = h("button.btn.primary", { type: "button" }, "Save");
  const said = h("span.sub.warn.grow");
  const go = async () => {
    const wanted = name.value.trim();
    if (!wanted || /[\\/:*?"<>|]/.test(wanted)) {
      said.textContent = wanted ? "Leave out \\ / : * ? \" < > |" : "";
      return;
    }
    save.disabled = true;
    const preset = {
      name: wanted,
      set: state.set.name,
      groups: state.set.groups,
      values: state.set.sliders.map((s, i) => [s.name, state.small[i], state.big[i]]),
    };
    try {
      const [text] = core.ask({ op: "presetFile", preset });
      const bytes = new TextEncoder().encode(text).buffer;
      const rel = `SliderPresets/${wanted}.xml`;
      core.ask({ op: "put", path: rel }, new Uint8Array(bytes));
      await host.request("output.write", { name: state.output, files: [{ path: `${TOOLS}/${rel}`, bytes }] }, [bytes]);
      const made = {
        name: wanted,
        set: preset.set,
        groups: preset.groups,
        values: preset.values.flatMap(([n, lo, hi]) => [
          [n, true, hi],
          [n, false, lo],
        ]),
      };
      state.catalog.presets = [...state.catalog.presets.filter((p) => p.name !== wanted), made];
      state.preset = made;
      close(el);
      render();
    } catch (e) {
      said.textContent = e.message;
      save.disabled = false;
    }
  };
  save.addEventListener("click", go);
  name.addEventListener("keydown", (e) => e.key === "Enter" && go());
  const el = sheet("Save preset", null, h("label.field", null, h("span", null, "Name"), name), [
    said,
    h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"),
    save,
  ]);
  name.focus();
  name.select();
}

/** BodySlide: put the outfit on screen in a group, an old one or a new one, kept in the output mod. */
async function addToGroup() {
  const set = state.set;
  // The groups made here before, where the mods have the file from an earlier visit.
  const key = "slidergroups/apocrypha.xml";
  if (!state.kept.has(key) && state.paths.has(key)) {
    try {
      state.kept.set(key, await host.request("files.read", { path: state.paths.get(key) }));
    } catch {
      // Unreadable: the file is begun again.
    }
  }
  rename(
    "Add to group",
    state.group || "",
    (to) => {
      const name = to.trim();
      const rel = "SliderGroups/Apocrypha.xml";
      const had = state.kept.get(rel.toLowerCase());
      const [text] = core.ask({ op: "groupAdd", group: name, set: set.name }, had ? new Uint8Array(had) : undefined);
      const bytes = new TextEncoder().encode(text).buffer;
      state.kept.set(rel.toLowerCase(), bytes);
      core.ask({ op: "put", path: rel }, new Uint8Array(bytes));
      host.request("output.write", { name: state.output, files: [{ path: `${TOOLS}/${rel}`, bytes: bytes.slice(0) }] }).catch((e) => {
        state.problem = e.message;
        render();
      });
      if (!set.groups.includes(name)) set.groups.push(name);
      if (!state.catalog.groups.includes(name)) state.catalog.groups.push(name);
      render();
    },
    "Add",
  );
}

/**
 * BodySlide: delete the preset or the project on screen, where it was saved
 * here into the output mod. Another mod's is that mod's to remove.
 */
async function remove(what) {
  const set = state.set;
  const file = (name) => name.replace(/[\\/:*?"<>|]/g, "_").trim().replace(/\.+$/, "");
  const paths =
    what === "preset"
      ? [`${TOOLS}/SliderPresets/${state.preset.name}.xml`]
      : ["SliderSets/NAME.osp", "ShapeData/NAME/NAME.nif", "ShapeData/NAME/NAME.osd"].map((p) => `${TOOLS}/${p.replaceAll("NAME", file(set.name))}`);
  busy("Deleting");
  try {
    const { removed } = await host.request("output.remove", { name: state.output, paths });
    if (!removed) throw new Error(`That ${what} is another mod's: it goes when that mod does.`);
    for (const p of paths) state.kept.delete(under(p).toLowerCase());
    await readCatalog();
    state.busy = null;
    const next = state.catalog.sets.find((s) => s.name === set.name) || state.catalog.sets[0];
    if (next) return choose(next, what === "preset" ? undefined : state.preset?.name);
  } catch (e) {
    state.problem = e.message;
  }
  state.busy = null;
  render();
}

/** BodySlide: take the outfit on screen out of the group shown, where it was put in it here. */
async function outOfGroup() {
  const set = state.set;
  const group = state.group;
  const rel = "SliderGroups/Apocrypha.xml";
  const key = rel.toLowerCase();
  try {
    if (!state.kept.has(key) && state.paths.has(key)) state.kept.set(key, await host.request("files.read", { path: state.paths.get(key) }));
    const had = state.kept.get(key);
    const [text] = core.ask({ op: "groupRemove", group, set: set.name }, had ? new Uint8Array(had) : undefined);
    const bytes = new TextEncoder().encode(text).buffer;
    state.kept.set(key, bytes);
    core.ask({ op: "put", path: rel }, new Uint8Array(bytes));
    await host.request("output.write", { name: state.output, files: [{ path: `${TOOLS}/${rel}`, bytes: bytes.slice(0) }] });
    set.groups = set.groups.filter((g) => g !== group);
    if (!state.catalog.sets.some((s) => s.groups.includes(group))) {
      state.catalog.groups = state.catalog.groups.filter((g) => g !== group);
      state.group = null;
    }
    state.problem = null;
  } catch (e) {
    state.problem = e.message;
  }
  render();
}

function several() {
  const groups = new Set(state.set.groups);
  let group = state.set.groups[0] || null;
  let query = "";
  // One set per output file: of those in the group, the first by name.
  const picked = new Set();
  const pickGroup = () => {
    picked.clear();
    const taken = new Set();
    for (const s of state.catalog.sets) {
      if (group && !s.groups.includes(group)) continue;
      const out = s.output.toLowerCase();
      if (taken.has(out)) continue;
      taken.add(out);
      picked.add(s.name);
    }
  };
  pickGroup();
  const rows = h("div.rows", { style: "height: 320px" });
  const count = h("span.sub");
  const said = h("span.sub.grow");
  const go = h("button.btn.primary", { type: "button" });
  const fill = () => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const shown = state.catalog.sets.filter(
      (s) => (!group || s.groups.includes(group)) && words.every((w) => s.name.toLowerCase().includes(w)),
    );
    rows.replaceChildren(
      ...shown.map((s) =>
        h(
          "button.check" + (picked.has(s.name) ? ".on" : ""),
          {
            type: "button",
            role: "checkbox",
            "aria-checked": picked.has(s.name) ? "true" : "false",
            onclick: () => {
              if (picked.has(s.name)) picked.delete(s.name);
              else {
                // Two sets cannot both write one file: this one takes it.
                for (const o of state.catalog.sets) if (o.output.toLowerCase() === s.output.toLowerCase()) picked.delete(o.name);
                picked.add(s.name);
              }
              const top = rows.scrollTop;
              fill();
              rows.scrollTop = top;
            },
          },
          h("span.box"),
          h("span.nm.cut", null, s.name),
          h("span.sub.quiet.cut", { style: "max-width: 45%" }, s.output.split("/").pop()),
        ),
      ),
    );
    const total = state.catalog.sets.filter((s) => !group || s.groups.includes(group)).length;
    count.textContent = `${picked.size} of ${total}`;
    go.textContent = `Build ${picked.size}`;
    go.disabled = picked.size === 0;
  };
  const groupPick = h("button.pick", { type: "button" });
  const label = () => (groupPick.innerHTML = `<span class="quiet">Group</span><span class="cut"></span>${CHEVRON}`) && (groupPick.children[1].textContent = group || "All");
  groupPick.addEventListener("click", () =>
    menu(
      groupPick,
      [{ name: "All", value: null, on: !group }, ...state.catalog.groups.map((g) => ({ name: g, value: g, on: g === group, note: groups.has(g) ? "This outfit" : "" }))],
      (g) => {
        group = g;
        pickGroup();
        label();
        fill();
      },
      "Search groups",
    ),
  );
  label();
  const input = h("input", { type: "text", placeholder: "Search outfits", "aria-label": "Search outfits", oninput: () => ((query = input.value), fill()) });
  const cancel = h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel");
  go.addEventListener("click", async () => {
    go.disabled = true;
    cancel.disabled = true;
    const sets = state.catalog.sets.filter((s) => picked.has(s.name));
    try {
      const done = await buildSeveral(sets, (t) => (said.textContent = t));
      state.built = done.files;
      state.problem = done.failed.length ? `${done.failed.length} could not be built: ${done.failed.slice(0, 3).join(", ")}${done.failed.length > 3 ? "…" : ""}` : null;
      close(el);
      render();
    } catch (e) {
      said.textContent = e.message;
      said.classList.add("warn");
      cancel.disabled = false;
      go.disabled = false;
    }
  });
  const el = sheet("Build several", count, [h("div.bar", null, groupPick, h("span.sub.quiet", null, "With the sliders as they are")), h("label.search", { html: GLASS }, input), rows], [
    said,
    cancel,
    go,
  ]);
  fill();
}

// ── The screen ─────────────────────────────────────────────────────────────

function busy(text) {
  state.busy = text;
  const el = document.getElementById("status");
  if (el) el.textContent = text;
  else render();
}

/**
 * Handles between the panels of `main`, to drag one wider or narrower. Each
 * of `panels` is `{ el, edge, key, least }`: the handle sits on that edge
 * ("left" or "right") of the panel, whose width is kept under `key`.
 * A double click gives a panel back the width it starts with.
 */
function splits(main, panels) {
  const made = panels.map((p) => {
    const kept = Number(remembered.get(`wide.${p.key}`));
    if (kept > 0) main.style.setProperty(`--${p.key}`, `${kept}px`);
    const handle = h("div.split", { role: "separator", "aria-orientation": "vertical", "aria-label": "Resize" });
    let from = null;
    handle.addEventListener("pointerdown", (e) => {
      from = { x: e.clientX, wide: p.el.getBoundingClientRect().width };
      handle.setPointerCapture(e.pointerId);
      handle.classList.add("held");
    });
    handle.addEventListener("pointermove", (e) => {
      if (!from) return;
      const by = (e.clientX - from.x) * (p.edge === "left" ? -1 : 1);
      // Never so wide that the others have no room left.
      const others = panels.filter((o) => o !== p).reduce((sum, o) => sum + o.el.getBoundingClientRect().width, 0);
      const most = main.clientWidth - others - 220 - 12 * panels.length;
      const wide = Math.round(Math.max(p.least, Math.min(most, from.wide + by)));
      main.style.setProperty(`--${p.key}`, `${wide}px`);
      remembered.set(`wide.${p.key}`, String(wide));
      place();
    });
    const up = () => {
      from = null;
      handle.classList.remove("held");
    };
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
    handle.addEventListener("dblclick", () => {
      main.style.removeProperty(`--${p.key}`);
      remembered.set(`wide.${p.key}`, "");
      place();
    });
    main.append(handle);
    return [p, handle];
  });
  const place = () => {
    const box = main.getBoundingClientRect();
    for (const [p, handle] of made) {
      const r = p.el.getBoundingClientRect();
      handle.style.left = `${(p.edge === "left" ? r.left - 12 : r.right) - box.left}px`;
    }
  };
  new ResizeObserver(place).observe(main);
  place();
}

function pickButton(label, value, onclick) {
  const b = h("button.pick", { type: "button", onclick: () => onclick(b), html: `<span class="quiet">${label}</span><span class="cut"></span>${CHEVRON}` });
  b.children[1].textContent = value;
  return b;
}

function track(index, heavy) {
  const values = heavy ? state.big : state.small;
  const slider = state.set.sliders[index];
  const out = h("output", null, String(Math.round(values[index])));
  const input = h("input", {
    type: "range",
    min: Math.min(0, values[index]),
    max: Math.max(100, values[index]),
    step: 1,
    value: values[index],
    "aria-label": `${slider.shown}, ${heavy ? "high" : "low"} weight`,
  });
  const paint = () => input.style.setProperty("--at", `${((input.value - input.min) / (input.max - input.min)) * 100}%`);
  paint();
  input.addEventListener("input", () => {
    values[index] = Number(input.value);
    out.textContent = input.value;
    paint();
    if (state.heavy !== heavy) setWeight(heavy);
    else reshape();
    state.built = null;
    foot();
  });
  // Back to where the preset has it.
  input.addEventListener("dblclick", () => {
    const from = presetValues(state.set, state.preset);
    input.value = (heavy ? from.big : from.small)[index];
    input.dispatchEvent(new Event("input"));
  });
  return h("span.tr", null, input, out);
}

/** A slider that takes part of the outfit away: on or off, for both weights. */
function toggle(index) {
  const slider = state.set.sliders[index];
  const on = state.big[index] !== 0 || state.small[index] !== 0;
  return h(
    "button.check" + (on ? ".on" : ""),
    {
      type: "button",
      role: "checkbox",
      "aria-checked": on ? "true" : "false",
      onclick: () => {
        const flip = (i) => {
          const now = state.big[i] !== 0 || state.small[i] !== 0 ? 0 : 100;
          state.small[i] = now;
          state.big[i] = now;
        };
        flip(index);
        // Some switch others with them: sleeves off shows the arms again.
        for (const name of slider.toggles || []) {
          const other = state.set.sliders.findIndex((s) => s.name === name);
          if (other >= 0) flip(other);
        }
        state.built = null;
        list();
        foot();
        reshape();
      },
    },
    h("span.box"),
    h("span.nm.cut", null, slider.shown),
  );
}

function sliderRows() {
  const set = state.set;
  const two = set.weights;
  const words = state.search.toLowerCase().split(/\s+/).filter(Boolean);
  // Two mods can name the same category: it is one category.
  const order = [...new Set(state.catalog.categories.map((c) => c.name)), null];
  const by = new Map(order.map((c) => [c, []]));
  set.sliders.forEach((s, i) => {
    if (s.hidden) return;
    if (!words.every((w) => s.shown.toLowerCase().includes(w))) return;
    (by.get(s.category) || by.get(null)).push(i);
  });
  const out = [];
  let shown = 0;
  for (const c of order) {
    const rows = by.get(c);
    if (!rows.length) continue;
    shown += rows.length;
    const name = c || (shown === rows.length && !order.some((o) => o && by.get(o).length) ? null : "Other");
    const shut = !words.length && state.shut.has(name);
    if (name)
      out.push(
        h("button.cat" + (shut ? ".shut" : ""), {
          type: "button",
          "aria-expanded": shut ? "false" : "true",
          html: `${CHEVRON}<span></span>`,
          onclick: () => {
            if (state.shut.has(name)) state.shut.delete(name);
            else state.shut.add(name);
            list();
          },
        }),
      );
    if (name) out[out.length - 1].children[1].textContent = name;
    if (shut) continue;
    for (const i of rows)
      out.push(
        set.sliders[i].zap
          ? toggle(i)
          : h("div.sl" + (two ? "" : ".one"), null, h("span.cut", null, set.sliders[i].shown), two && track(i, false), track(i, true)),
      );
  }
  return { rows: out, shown, two };
}

function list() {
  const el = document.getElementById("list");
  if (!el || !state.set) return;
  const { rows, shown, two } = sliderRows();
  const top = el.querySelector(".rows")?.scrollTop || 0;
  el.replaceChildren(
    h(
      "div.sl.head" + (two ? "" : ".one"),
      null,
      h("span", null, `${shown} ${shown === 1 ? "slider" : "sliders"}`),
      two && h("span", null, "Low weight"),
      h("span", null, two ? "High weight" : ""),
    ),
    h("div.rows", null, rows.length ? rows : h("div.sub.quiet", { style: "padding: 8px 12px" }, "Nothing found")),
  );
  el.querySelector(".rows").scrollTop = top;
}

function setWeight(heavy) {
  state.heavy = heavy;
  for (const b of document.querySelectorAll(".seg button")) b.classList.toggle("on", (b.dataset.heavy === "1") === heavy);
  reshape();
}

function foot() {
  const el = document.getElementById("foot");
  if (!el) return;
  const working = Boolean(state.busy);
  const ready = state.set && state.shapes.length && !working;
  el.replaceChildren(
    state.problem
      ? h("span.sub.warn.cut", null, state.problem)
      : state.built !== null
        ? h("span.sub.cut", null, `${state.built} ${state.built === 1 ? "file" : "files"} built into ${state.output}`)
        : state.note
          ? h("span.sub.cut", null, state.note)
          : state.busy
          ? h("span.sub.cut", { id: "status" }, state.busy)
          : h(
              "button.btn.ghost.small",
              {
                type: "button",
                id: "status",
                title: "The mod builds go into",
                onclick: () =>
                  rename(
                    "Build into",
                    state.output,
                    (to) => {
                      const name = to.trim();
                      if (!name || name.length > 80 || /^[.]|[\\/:*?"<>|]/.test(name)) throw new Error('A mod\'s name, without \\ / : * ? " < > |');
                      state.output = name;
                      remembered.set("output", name);
                      foot();
                    },
                    "Done",
                  ),
              },
              `Into ${state.output}`,
            ),
    h("span.grow"),
    h(
      "button.check.inline" + (state.morphs ? ".on" : ""),
      {
        type: "button",
        role: "checkbox",
        "aria-checked": state.morphs ? "true" : "false",
        title: "Also write the file RaceMenu needs to reshape this in the game",
        onclick: () => {
          state.morphs = !state.morphs;
          remembered.set("morphs", state.morphs ? "1" : "0");
          state.built = null;
          foot();
        },
      },
      h("span.box"),
      h("span", null, "RaceMenu morphs"),
    ),
    h(
      "button.btn.ghost",
      { type: "button", disabled: !ready, title: "Build into a folder you pick, outside the mods", onclick: buildToFolder },
      "To a folder…",
    ),
    h("button.btn", { type: "button", disabled: !ready, onclick: several }, "Build several…"),
    state.built !== null && !state.problem
      ? h("button.btn.primary", { type: "button", onclick: () => host.request("apply.review").catch(() => {}) }, "Apply")
      : h("button.btn.primary", { type: "button", disabled: !ready, onclick: build }, "Build"),
  );
}

let canvas = null;

/** The switch between the two tools, at the start of either's top row. */
function modes() {
  const go = (mode) => () => {
    if (state.mode === mode) return;
    state.mode = mode;
    remembered.set("mode", mode);
    // Each tool draws every shape in its own way to begin with, and the
    // project may no longer be the set it was opened from.
    view?.look({ wire: false, painted: true });
    if (view) {
      state.tool = null;
      view.brush = null;
      view.reach(null);
      canvas.style.cursor = "";
    }
    try {
      showShapes(false);
    } catch (e) {
      state.problem = e.message;
    }
    render();
  };
  return h(
    "div.seg",
    { role: "group", "aria-label": "Tool" },
    h("button" + (state.mode === "bodyslide" ? ".on" : ""), { type: "button", onclick: go("bodyslide") }, "BodySlide"),
    h("button" + (state.mode === "studio" ? ".on" : ""), { type: "button", onclick: go("studio") }, "Outfit Studio"),
  );
}

function outfitPick(label) {
  return pickButton(label, cur()?.name || "", (b) =>
    menu(
      b,
      state.catalog.sets
        .filter((s) => label === "Project" || !state.group || s.groups.includes(state.group))
        .map((s) => ({ name: s.name, note: s.groups[0], value: s, on: s === state.set })),
      (s) => leaving(() => choose(s)),
      label === "Project" ? "Search projects" : "Search outfits",
    ),
  );
}

/** Skyrim's numbers for the parts of a body an armour covers, as the game's editor names them. */
const PARTS = {
  30: "Head", 31: "Hair", 32: "Body", 33: "Hands", 34: "Forearms", 35: "Amulet", 36: "Ring", 37: "Feet", 38: "Calves",
  39: "Shield", 40: "Tail", 41: "Long hair", 42: "Circlet", 43: "Ears", 130: "Head, cut off", 131: "Hair, cut off",
  141: "Long hair, cut off", 142: "Circlet, cut off", 143: "Ears, cut off", 150: "Head, cut off", 230: "Neck, cut off",
};

/** Outfit Studio: draw only one part of the picked shape, or with null all of it again. */
function showPart(index) {
  const s = state.shapes[state.picked];
  state.part = index;
  if (!s) return;
  if (index !== null && s.parts[index]) {
    // What is drawn is what a brush can touch: its triangles, and of the
    // shape's vertices only theirs.
    s.shown = s.parts[index].triangles;
    s.live = new Uint8Array(s.base.length / 3);
    for (const v of s.shown) s.live[v] = 1;
    s.zapped = null;
    view.cut(state.picked, s.shown);
  } else {
    s.live = null;
    s.zapped = null;
    reshape();
  }
}

const EYE =
  '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/></svg>';
const EYE_OFF =
  '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 3l18 18"/><path d="M10.6 5.1A10.6 10.6 0 0 1 12 5c6.500 0 10 7 10 7a17 17 0 0 1-3.200 4"/><path d="M6.200 6.200A17 17 0 0 0 2 12s3.500 7 10 7c1.600 0 3-.4 4.300-1"/></svg>';

/** Outfit Studio's left list: the shapes, or the picked shape's bones. */
function studioSide() {
  const el = document.getElementById("side");
  if (!el) return;
  const picked = state.shapes[state.picked];
  const tab = (id, label) =>
    h("button" + (state.side === id ? ".on" : ""), { type: "button", onclick: () => ((state.side = id), studioSide()) }, label);
  const hide = (i, hidden) => {
    state.shapes[i].hidden = hidden;
    view.hide(i, hidden);
  };
  // The skeleton shows with the list of bones.
  const bonesShown = state.side === "bones" && picked && !picked.hidden;
  view.bones(bonesShown ? picked.places : null, bonesShown ? picked.bones.indexOf(state.bone) : -1);
  if (state.side !== "parts" && state.part !== null) showPart(null);
  const rows =
    state.side === "parts"
      ? (picked?.parts || []).map((part, i) =>
          h(
            "div.shape" + (state.part === i ? ".on" : ""),
            {
              tabIndex: 0,
              onclick: () => {
                showPart(state.part === i ? null : i);
                studioSide();
              },
              oncontextmenu: (e) => {
                e.preventDefault();
                if (!state.work) return;
                const masked = [];
                picked.mask.forEach((m, v) => m >= 0.5 && masked.push(v));
                const toPart = (to, number) => {
                  const [work] = core.ask({ op: "change", change: { do: "toPart", shape: picked.name, to, part: number } }, new Uint8Array(Uint32Array.from(masked).buffer));
                  showPart(null);
                  taken(work);
                };
                menu(
                  e.currentTarget,
                  [
                    { name: "Change part…", value: "change" },
                    masked.length > 0 && { name: "Move masked triangles here", value: "here", rule: true },
                    masked.length > 0 && { name: "Move masked triangles to a new part…", value: "new" },
                    picked.parts.length > 1 && { name: "Remove part…", value: "remove", danger: true, rule: true },
                  ].filter(Boolean),
                  (what) => {
                    if (what === "here") {
                      try {
                        toPart(i, 0);
                      } catch (err) {
                        state.problem = err.message;
                        render();
                      }
                      return;
                    }
                    if (what === "remove") {
                      // Its triangles go to another part: which, where there is a choice.
                      const others = picked.parts.map((o, j) => ({ name: `Into ${o.part}${PARTS[o.part] ? ` · ${PARTS[o.part]}` : ""}`, value: j })).filter((o) => o.value !== i);
                      const into = (j) => {
                        showPart(null);
                        attempt({ do: "removePart", shape: picked.name, index: i, into: j });
                      };
                      return others.length === 1 ? into(others[0].value) : menu(document.querySelector("#side .side-head"), others, into);
                    }
                    if (what === "new") {
                      return rename(
                        "New part",
                        "32",
                        (to) => {
                          const number = Number(to);
                          if (!Number.isInteger(number) || number < 0 || number > 65535) throw new Error("A part is a whole number, 30 to 61 for most armour.");
                          toPart(null, number);
                        },
                        "Add",
                      );
                    }
                    partNumber();
                  },
                );
                const partNumber = () =>
                  rename(
                    "Body part",
                    String(part.part),
                    (to) => {
                      const number = Number(to);
                      if (!Number.isInteger(number) || number < 0 || number > 65535) throw new Error("A part is a whole number, 30 to 61 for most armour.");
                      change({ do: "bodyPart", shape: picked.name, index: i, part: number });
                    },
                    "Done",
                  );
              },
            },
            h("span.nm.cut", null, `${part.part}${PARTS[part.part] ? ` · ${PARTS[part.part]}` : ""}`),
            h("span.sub.quiet", null, (part.triangles.length / 3).toLocaleString()),
          ),
        )
      : state.side === "bones"
      ? [...(picked?.bones || []), ...[...new Set([...state.shapes.flatMap((o) => o.bones), ...(state.tool === "weights" ? state.bones : [])])].filter((b) => !picked?.bones.includes(b)).sort()].map((b) =>
          h(
            "div.shape" + (state.tool === "weights" && b === state.bone ? ".on" : "") + (picked?.bones.includes(b) ? "" : ".off"),
            {
              onclick: () => {
                if (state.tool !== "weights") return;
                state.bone = b;
                heat();
                render();
              },
              oncontextmenu: (e) => {
                e.preventDefault();
                if (!state.work || !picked?.bones.includes(b)) return;
                menu(e.currentTarget, [{ name: `Remove from ${picked.name}`, value: 1, danger: true }], () => attempt({ do: "dropBone", shape: picked.name, bone: b }));
              },
            },
            h("span.nm.cut", null, b || "Unnamed"),
          ),
        )
      : state.shapes.map((sh, i) =>
          h(
            "div.shape" + (i === state.picked ? ".on" : "") + (sh.hidden ? ".off" : ""),
            {
              role: "option",
              "aria-selected": i === state.picked ? "true" : "false",
              tabIndex: 0,
              onclick: () => {
                if (state.heat) view.mask(state.picked, state.shapes[state.picked].mask);
                state.picked = i;
                view.look({ picked: i });
                heat();
                studioSide();
              },
              oncontextmenu: (e) => {
                e.preventDefault();
                const others = state.shapes.some((o, j) => j !== i && !o.hidden);
                // A shape follows a body: the one marked as the reference, or any of a few.
                const marked = state.shapes.filter((o) => o.reference && o !== sh);
                const bodies = sh.reference ? [] : marked.length ? marked : state.shapes.filter((o) => o !== sh).slice(0, 4);
                menu(
                  e.currentTarget,
                  [
                    state.work && { name: "Rename…", value: "rename" },
                    state.work && { name: "Duplicate…", value: "duplicate" },
                    state.work && { name: "Move, turn, scale…", value: "transform" },
                    state.work && !sh.reference && { name: "Mirror", value: "mirror" },
                    state.work && !sh.reference && { name: "Refine", value: "refine" },
                    state.work && !sh.reference && { name: "Weld doubled points", value: "weld" },
                    state.work && !sh.reference && state.shapes.some((o) => o !== sh && !o.reference) && { name: "Merge into…", value: "merge" },
                    state.work && !sh.reference && { name: "New shape from OBJ…", value: "fromObj" },
                    ...(state.work ? bodies.map((o) => ({ name: `Conform to ${o.name}`, value: ["conform", o.name] })) : []),
                    ...(state.work ? bodies.map((o) => ({ name: `Copy weights from ${o.name}`, value: ["weights", o.name] })) : []),
                    { name: others ? "Show only this" : "Show all", value: "only", rule: Boolean(state.work) },
                    { name: sh.hidden ? "Show" : "Hide", value: "hide" },
                    { name: sh.ghost ? "Solid" : "Ghost", value: "ghost" },
                    state.work && sh.textures.length > 0 && { name: "Textures…", value: "textures" },
                    state.work && { name: "Flip texture left to right", value: "flipU" },
                    state.work && { name: "Flip texture top to bottom", value: "flipV" },
                    { name: "Export OBJ…", value: "export", rule: true },
                    state.work && { name: "Import OBJ…", value: "import" },
                    state.work && sh.reference && { name: "Conform all to this", value: "conformAll" },
                    state.work && sh.mask.some((m) => m >= 0.5) && { name: "Separate masked points…", value: "part", rule: true },
                    state.work && sh.mask.some((m) => m >= 0.5) && { name: "Delete masked points", value: "cut", danger: true },
                    state.work && state.shapes.length > 1 && { name: "Delete", value: "delete", danger: true, rule: !sh.mask.some((m) => m >= 0.5) },
                  ].filter(Boolean),
                  (what) => {
                    if (what === "rename") {
                      return rename("Rename shape", sh.name, (to) => change({ do: "renameShape", from: sh.name, to }));
                    }
                    if (what === "duplicate") {
                      return rename("Duplicate shape", `${sh.name} copy`, (to) => change({ do: "duplicate", name: sh.name, to }), "Duplicate");
                    }
                    if (what === "transform") return transform(sh);
                    if (what === "mirror") return attempt({ do: "mirror", shape: sh.name });
                    if (what === "refine") return attempt({ do: "refine", shape: sh.name });
                    if (what === "weld") return attempt({ do: "weld", shape: sh.name });
                    if (what === "fromObj") return shapeFromObj(sh);
                    if (what === "merge") {
                      // Into which: it is drawn as that one is, and goes by its name.
                      return menu(
                        document.querySelector("#side .side-head"),
                        state.shapes.filter((o) => o !== sh && !o.reference).map((o) => ({ name: `Into ${o.name}`, value: o.name })),
                        (into) => attempt({ do: "merge", into, from: sh.name }),
                      );
                    }
                    if (what === "ghost") {
                      sh.ghost = !sh.ghost;
                      view.ghost(i, sh.ghost);
                      return studioSide();
                    }
                    if (what === "conformAll") return attempt({ do: "conformAll", body: sh.name });
                    if (what === "textures") return texturesOf(sh);
                    if (what === "part") {
                      const parted = [];
                      sh.mask.forEach((m, v) => m >= 0.5 && parted.push(v));
                      return rename(
                        "Separate masked points",
                        `${sh.name} part`,
                        (to) => {
                          const [work] = core.ask({ op: "change", change: { do: "separate", shape: sh.name, to } }, new Uint8Array(Uint32Array.from(parted).buffer));
                          sh.mask.fill(0);
                          taken(work);
                        },
                        "Separate",
                      );
                    }
                    if (what === "cut") {
                      const gone = [];
                      sh.mask.forEach((m, v) => m >= 0.5 && gone.push(v));
                      try {
                        const [work] = core.ask({ op: "change", change: { do: "deleteVertices", shape: sh.name } }, new Uint8Array(Uint32Array.from(gone).buffer));
                        taken(work);
                      } catch (e) {
                        state.problem = e.message;
                        render();
                      }
                      return;
                    }
                    if (what === "flipU" || what === "flipV") return attempt({ do: "flipUv", shape: sh.name, across: what === "flipU" });
                    if (what === "export") return exportObj(sh);
                    if (what === "import") return importObj(sh);
                    if (what === "delete") return attempt({ do: "deleteShape", name: sh.name });
                    if (Array.isArray(what)) {
                      // What a mask covers keeps the sliders, or the bones, it has.
                      const spared = [];
                      sh.mask.forEach((m, v) => m >= 0.5 && spared.push(v));
                      try {
                        const [work] = core.ask({ op: "change", change: { do: what[0], shape: sh.name, body: what[1] } }, new Uint8Array(Uint32Array.from(spared).buffer));
                        taken(work);
                      } catch (e) {
                        state.problem = e.message;
                        render();
                      }
                      return;
                    }
                    if (what === "hide") hide(i, !sh.hidden);
                    else state.shapes.forEach((_, j) => hide(j, others ? j !== i : false));
                    studioSide();
                  },
                );
              },
            },
            h("button.eye", {
              type: "button",
              "aria-label": sh.hidden ? `Show ${sh.name}` : `Hide ${sh.name}`,
              html: sh.hidden ? EYE_OFF : EYE,
              onclick: (e) => {
                e.stopPropagation();
                hide(i, !sh.hidden);
                studioSide();
              },
            }),
            h("span.nm.cut", null, sh.name),
            sh.reference && h("span.tag", null, "Reference"),
            h("span.sub.quiet", null, (sh.base.length / 3).toLocaleString()),
          ),
        );
  const scrolled = el.querySelector(".rows")?.scrollTop || 0;
  el.replaceChildren(
    h(
      "div.side-head",
      null,
      h("div.seg.plain", { role: "group", "aria-label": "List" }, tab("shapes", "Shapes"), tab("bones", "Bones"), picked?.parts.length > 0 && tab("parts", "Parts")),
      h("span.grow"),
      state.work &&
        (state.side === "bones"
          ? h("button.btn.ghost.small", { type: "button", onclick: (e) => addBone(e.currentTarget) }, "Add bone…")
          : h("button.btn.ghost.small", { type: "button", onclick: (e) => reference(e.currentTarget) }, "Reference…")),
    ),
    h("div.rows", { role: state.side === "shapes" ? "listbox" : null }, rows.length ? rows : h("div.sub.quiet", { style: "padding: 8px 12px" }, "No bones")),
    picked &&
      h(
        "div.sub.quiet.side-foot",
        null,
        `${picked.name} · ${(picked.base.length / 3).toLocaleString()} points · ${(picked.shown.length / 3).toLocaleString()} triangles · ${picked.bones.length} bones`,
      ),
  );
  el.querySelector(".rows").scrollTop = scrolled;
}

/** Outfit Studio's sliders: one value each, to see what a slider does to the outfit. */
function studioSliders() {
  const el = document.getElementById("list");
  if (!el || !state.set) return;
  const words = state.search.toLowerCase().split(/\s+/).filter(Boolean);
  const rows = [];
  // What can be done to a slider, from its row.
  const more = (sl) =>
    state.work && {
      oncontextmenu: (e) => {
        e.preventDefault();
        menu(
          e.currentTarget,
          [
            { name: "Rename…", value: "rename" },
            { name: "Settings…", value: "settings" },
            state.shapes[state.picked] && { name: `Clear for ${state.shapes[state.picked].name}`, value: "clear", rule: true },
            state.shapes[state.picked] && { name: "Mask what it moves", value: "mask" },
            { name: "Invert", value: "negate" },
            { name: "Export data…", value: "out", rule: true },
            { name: "Import data…", value: "in" },
            { name: "Delete", value: "delete", danger: true, rule: true },
          ].filter(Boolean),
          (what) => {
            const on = state.shapes[state.picked];
            if (what === "clear") return attempt({ do: "clearSlider", slider: sl.name, shape: on.name });
            if (what === "negate") return attempt({ do: "negateSlider", slider: sl.name });
            if (what === "out" || what === "in") return sliderFile(sl, what === "in");
            if (what === "mask") {
              // Covered: the vertices of the picked shape this slider moves.
              const at = cur().sliders.findIndex((x) => x.name === sl.name);
              for (const d of on.sliders.filter((x) => x.slider === at)) d.vertices.forEach((v) => (on.mask[v] = 1));
              if (!state.heat) view.mask(state.picked, on.mask);
              return render();
            }
            if (what === "rename") rename("Rename slider", sl.name, (to) => change({ do: "renameSlider", from: sl.name, to }, [sl.name, to.trim()]));
            else if (what === "settings") sliderSettings(sl);
            else attempt({ do: "deleteSlider", name: sl.name });
          },
        );
      },
    };
  cur().sliders.forEach((sl, i) => {
    if (sl.uv || !words.every((w) => sl.name.toLowerCase().includes(w))) return;
    if (sl.zap) {
      const on = state.at[i] !== 0;
      rows.push(
        h(
          "button.check" + (on ? ".on" : ""),
          {
            type: "button",
            role: "checkbox",
            "aria-checked": on ? "true" : "false",
            ...more(sl),
            onclick: () => {
              state.at[i] = on ? 0 : 100;
              studioSliders();
              reshape();
              studioSide();
            },
          },
          h("span.box"),
          h("span.nm.cut", null, sl.name),
        ),
      );
      return;
    }
    const out = h("output", null, String(Math.round(state.at[i])));
    const input = h("input", { type: "range", min: 0, max: 100, step: 1, value: state.at[i], "aria-label": sl.name });
    const fill = () => input.style.setProperty("--at", `${input.value}%`);
    fill();
    input.addEventListener("input", () => {
      state.at[i] = Number(input.value);
      out.textContent = input.value;
      fill();
      reshape();
    });
    input.addEventListener("dblclick", () => {
      input.value = 0;
      input.dispatchEvent(new Event("input"));
    });
    rows.push(h("div.sl.one", more(sl) || null, h("span.cut", null, sl.name), h("span.tr", null, input, out)));
  });
  const top = el.querySelector(".rows")?.scrollTop || 0;
  el.replaceChildren(
    h(
      "div.sl.one.head",
      null,
      h("span", null, `${rows.length} ${rows.length === 1 ? "slider" : "sliders"}`),
      state.work
        ? h(
            "span",
            { style: "display: flex; justify-content: flex-end; gap: 2px" },
            cur().sliders.length > 0 &&
              h(
                "button.btn.ghost.small",
                { type: "button", title: "The outfit as it stands becomes the shape every slider starts from", onclick: () => setBase() },
                "Set base shape",
              ),
            h(
            "button.btn.ghost.small",
            {
              type: "button",
              onclick: (e) => {
                const on = state.shapes[state.picked];
                const masked = [];
                on?.mask.forEach((m, v) => m >= 0.5 && masked.push(v));
                menu(
                  e.currentTarget,
                  [
                    { name: "Slider…", value: "slider" },
                    state.at.some((v, i) => v !== 0 && !cur().sliders[i]?.zap) && { name: "Slider of what is shown…", value: "combine" },
                    masked.length > 0 && { name: "Zap of what is masked…", value: "zap" },
                    { name: "Sliders from a morph file…", value: "in", rule: true },
                    cur().sliders.length > 0 && { name: "Export morph file…", value: "out" },
                  ].filter(Boolean),
                  (what) =>
                    what === "slider"
                      ? newSlider()
                      : what === "combine"
                        ? rename(
                            "New slider",
                            "",
                            (name) => {
                              const [work] = core.ask({ op: "change", change: { do: "combine", name, values: state.at } });
                              // The others go back to nothing and the new one stands in full: the outfit looks as it did.
                              state.at = state.at.map((v, i) => (cur().sliders[i].zap ? v : 0));
                              taken(work);
                              state.at[work.sliders.findIndex((sl) => sl.name === name.trim())] = 100;
                              reshape();
                              render();
                            },
                            "Add",
                          )
                      : what === "in" || what === "out"
                        ? morphFile(what === "in")
                        : rename(
                          "New zap",
                          "",
                          (name) => {
                            const [work] = core.ask({ op: "change", change: { do: "addZap", name, shape: on.name } }, new Uint8Array(Uint32Array.from(masked).buffer));
                            on.mask.fill(0);
                            taken(work);
                          },
                          "Add",
                        ),
                );
              },
            },
            "New…",
            ),
          )
        : h("span", null, "Shown at"),
    ),
    h("div.rows", null, rows.length ? rows : h("div.sub.quiet", { style: "padding: 8px 12px" }, "Nothing found")),
  );
  el.querySelector(".rows").scrollTop = top;
}

/** Outfit Studio: a new slider, which the brushes then shape. */
function newSlider() {
  rename(
    "New slider",
    "",
    (name) => {
      change({ do: "addSlider", name });
      // What the brushes do next is this slider, shown in full.
      const at = state.work.sliders.findIndex((sl) => sl.name === name.trim());
      state.editing = at;
      state.at[at] = 100;
      render();
    },
    "Add",
  );
}

// ── Brushes ────────────────────────────────────────────────────────────────

const BRUSHES = [
  ["inflate", "Inflate"],
  ["deflate", "Deflate"],
  ["move", "Move"],
  ["smooth", "Smooth"],
  ["erase", "Erase"],
  ["mask", "Mask"],
  ["weights", "Weights"],
  ["colour", "Colour"],
];

/** How far the brush reaches, in the mesh's units. */
function radius() {
  return view.size * (0.005 + (state.size / 100) ** 2 * 0.25);
}

/** The stroke under way: what it started from and where the brush last was. */
let stroke = null;

/** The pointer over the view with a brush in hand (see `View.pointers`). */
function brushing(what, ray) {
  const s = state.shapes[state.picked];
  if (what === "out" || !s || s.hidden || !state.tool) {
    if (!stroke) view.reach(null);
    return false;
  }
  if (what === "up") return endStroke();
  const r = radius();
  if (what === "drag" && stroke?.tool === "move") {
    // The middle of the brush follows the pointer, across the view.
    const { origin, dir, back } = ray;
    const along = (k) => stroke.at[k] - origin[k];
    const t = (along(0) * back[0] + along(1) * back[1] + along(2) * back[2]) / (dir[0] * back[0] + dir[1] * back[1] + dir[2] * back[2]);
    const by = [0, 1, 2].map((k) => origin[k] + dir[k] * t - stroke.at[k]);
    brush.carry(s.now, stroke.from, stroke.reached, by);
    if (stroke.other) brush.carry(s.now, stroke.otherFrom, stroke.other, brush.across(by));
    view.reach({ point: stroke.at.map((c, k) => c + by[k]), normal: stroke.normal, radius: r });
    return redraw(s);
  }
  const found = brush.hit(ray.origin, ray.dir, s.now, s.shown);
  if (!found) {
    if (!stroke) view.reach(null);
    return false;
  }
  view.reach({ point: found.point, normal: found.normal, radius: r });
  if (what === "over") return false;
  if (state.tool === "colour") {
    if (!s.colors.length) return false;
    const reached = alive(s, brush.spared(brush.reach(found.point, r, s.now), s.mask));
    if (ray.alt) {
      // Alt picks up the colour under the middle of the brush.
      if (what === "down" && reached.vertices.length) {
        let best = 0;
        reached.weights.forEach((w, k) => w > reached.weights[best] && (best = k));
        const at = reached.vertices[best] * 4;
        state.colour = `#${[0, 1, 2].map((c) => s.colors[at + c].toString(16).padStart(2, "0")).join("")}`;
        state.alpha = Math.round((s.colors[at + 3] / 255) * 100);
        remembered.set("brush.colour", state.colour);
        render();
      }
      return false;
    }
    if (what === "down") stroke = { tool: "colour", shape: s, before: new Uint8Array(s.colors), amount: 1 };
    const to = [1, 3, 5].map((c) => parseInt(state.colour.slice(c, c + 2), 16)).concat(Math.round((state.alpha / 100) * 255));
    const strength = state.strength / 100;
    reached.vertices.forEach((v, k) => {
      const by = Math.min(1, strength * reached.weights[k] * 1.5);
      for (let c = 0; c < 4; c++) s.colors[v * 4 + c] = Math.round(s.colors[v * 4 + c] + (to[c] - s.colors[v * 4 + c]) * by);
    });
    view.colours(state.picked, s.colors);
    return true;
  }
  if (state.tool === "weights") {
    if (!state.heat || !state.bone) return false;
    if (what === "down") {
      stroke = { tool: "weights", shape: s, before: new Float32Array(state.heat), amount: 1 };
      // Both sides: the same stroke across the body, for the bone's twin there.
      const other = brush.twin(state.bone);
      if (state.mirror && other !== state.bone) {
        try {
          const [, numbers] = core.ask({ op: "boneWeights", shape: s.name, bone: other });
          stroke.twin = { bone: other, heat: new Float32Array(numbers.slice(0)) };
          stroke.twin.before = new Float32Array(stroke.twin.heat);
        } catch {
          // Without its twin it paints the one side.
        }
      }
    }
    const far = stroke.last ? Math.hypot(...found.point.map((c, k) => c - stroke.last[k])) : Infinity;
    if (far < r * (state.spacing / 100)) return true;
    stroke.last = found.point;
    const reached = alive(s, brush.spared(brush.reach(found.point, r, s.now), s.mask));
    const by = (ray.alt ? -1 : 1) * (state.strength / 100) * 0.35;
    reached.vertices.forEach((v, k) => {
      state.heat[v] = Math.max(0, Math.min(1, state.heat[v] + by * reached.weights[k]));
    });
    if (stroke.twin) {
      const there = alive(s, brush.spared(brush.reach(brush.across(found.point), r, s.now), s.mask));
      there.vertices.forEach((v, k) => {
        stroke.twin.heat[v] = Math.max(0, Math.min(1, stroke.twin.heat[v] + by * there.weights[k]));
      });
    }
    view.mask(state.picked, state.heat);
    return true;
  }
  if (state.tool === "mask") {
    // Not a change to the outfit: what the other brushes are to leave alone.
    if (what === "down") stroke = { tool: "mask", shape: s, before: s.now, amount: 1 };
    for (const point of state.mirror ? [found.point, brush.across(found.point)] : [found.point]) {
      brush.cover(s.mask, alive(s, brush.reach(point, r, s.now)), ray.alt);
    }
    view.mask(state.picked, s.mask);
    return true;
  }
  if (what === "down") {
    const k = state.editing;
    const amount = k < 0 ? 1 : applied(cur().sliders[k], state.at[k]);
    // A slider standing at nothing shows nothing of what a stroke would do to it.
    if (amount === 0) return false;
    stroke = { tool: state.tool, shape: s, before: new Float32Array(s.now), amount, last: null, near: null };
    if (stroke.tool === "erase") {
      // How far the slider has each vertex from where it starts, to take back.
      const list = s.sliders.find((d) => d.slider === k);
      if (k < 0 || !list) {
        stroke = null;
        state.problem = k < 0 ? "Erase takes back what a slider does: choose the slider under Changes." : null;
        if (state.problem) render();
        return false;
      }
      stroke.left = new Float32Array(s.now.length);
      list.vertices.forEach((v, i) => {
        for (let c = 0; c < 3; c++) stroke.left[v * 3 + c] = list.offsets[i * 3 + c] * amount;
      });
    }
    if (stroke.tool === "move") {
      const pick = (p) => {
        const reached = within(s, brush.reach(p, r, s.now), p === found.point ? found.corners : null);
        const from = new Float32Array(reached.vertices.length * 3);
        reached.vertices.forEach((v, i) => from.set(s.now.subarray(v * 3, v * 3 + 3), i * 3));
        return [reached, from];
      };
      stroke.at = found.point;
      stroke.normal = found.normal;
      [stroke.reached, stroke.from] = pick(found.point);
      // Both sides only where they are apart: across the middle they are one hold.
      if (state.mirror && Math.abs(found.point[0]) > r) [stroke.other, stroke.otherFrom] = pick(brush.across(found.point));
      return true;
    }
  }
  // A dab each time the brush has gone a quarter of its reach.
  const far = stroke.last ? Math.hypot(...found.point.map((c, k) => c - stroke.last[k])) : Infinity;
  if (far < r * (state.spacing / 100)) return true;
  stroke.last = found.point;
  const strength = state.strength / 100;
  for (const point of state.mirror ? [found.point, brush.across(found.point)] : [found.point]) {
    const reached = within(s, brush.reach(point, r, s.now), point === found.point ? found.corners : null);
    if (stroke.tool === "erase") {
      brush.erase(s.now, stroke.left, reached, strength);
    } else if (stroke.tool === "smooth") {
      stroke.near = near(s);
      // Vertices at one place go to one place: each to its group's first.
      brush.smooth(s.now, s.weld, stroke.near, reached, strength * 0.6);
    } else {
      brush.inflate(s.now, s.normals, reached, (stroke.tool === "deflate" ? -1 : 1) * r * 0.06 * strength);
    }
  }
  redraw(s);
  return true;
}

/**
 * What a brush reaches of a shape, less what its mask covers, and with
 * `corners` (the triangle under the brush) only what is joined to them when
 * that is asked for.
 */
function within(s, reached, corners) {
  // With one part of the shape seen alone, the rest of it is out of reach.
  if (s.live) {
    const keep = [];
    reached.vertices.forEach((v, k) => s.live[v] && keep.push(k));
    reached = { vertices: Uint32Array.from(keep, (k) => reached.vertices[k]), weights: Float32Array.from(keep, (k) => reached.weights[k]) };
  }
  // A wider focus lifts the edge of the brush toward its middle.
  const power = 0.25 + (1 - state.focus / 100) * 1.5;
  for (let k = 0; k < reached.weights.length; k++) reached.weights[k] = Math.sqrt(reached.weights[k]) ** (power * 2);
  if (state.connected && corners) {
    stroke.near = near(s);
    reached = brush.joined(reached, s.weld, stroke.near, corners);
  }
  return brush.spared(reached, s.mask);
}

/** What a brush reaches of a shape, less what is out of sight while one part of it is seen alone. */
function alive(s, reached) {
  if (!s.live) return reached;
  const keep = [];
  reached.vertices.forEach((v, k) => s.live[v] && keep.push(k));
  return { vertices: Uint32Array.from(keep, (k) => reached.vertices[k]), weights: Float32Array.from(keep, (k) => reached.weights[k]) };
}

/** Which vertices of a shape its triangles join, worked out once for the triangles it now shows. */
function near(s) {
  if (s.nearOf !== s.shown) {
    s.near = brush.neighbours(s.shown, s.weld);
    s.nearOf = s.shown;
  }
  return s.near;
}

function redraw(s) {
  shade(s.now, s.shown, s.weld, s.normals, s.sum);
  view.move(state.picked, s.now, s.normals);
  return true;
}

/** The stroke is over: what it moved becomes a change to the project. */
function endStroke() {
  const done = stroke;
  stroke = null;
  if (!done) return false;
  if (done.tool === "mask") {
    render();
    return false;
  }
  if (done.tool === "colour") {
    const s = done.shape;
    const changed = [];
    for (let v = 0; v < s.colors.length / 4; v++) {
      if ([0, 1, 2, 3].some((c) => s.colors[v * 4 + c] !== done.before[v * 4 + c])) changed.push(v);
    }
    if (changed.length) {
      const bytes = new Uint8Array(changed.length * 8);
      const data = new DataView(bytes.buffer);
      changed.forEach((v, i) => {
        data.setUint32(i * 8, v, true);
        bytes.set(s.colors.subarray(v * 4, v * 4 + 4), i * 8 + 4);
      });
      try {
        [state.work] = core.ask({ op: "change", change: { do: "colour", shape: s.name } }, bytes);
        state.problem = null;
        state.saved = null;
        state.exported = null;
      } catch (e) {
        state.problem = e.message;
        s.colors.set(done.before);
        view.colours(state.picked, s.colors);
      }
    }
    settled();
    return false;
  }
  if (done.tool === "weights") {
    // What changed, as each vertex's new weight for the bone.
    // Each vertex whose weight changed, with its new weight: for the bone,
    // and after those for its twin, as one change to take back as one.
    const changed = (now, before) => {
      const rows = [];
      now.forEach((w, v) => w !== before[v] && rows.push([v, w]));
      return rows;
    };
    const first = changed(state.heat, done.before);
    const second = done.twin ? changed(done.twin.heat, done.twin.before) : [];
    if (first.length + second.length) {
      const bytes = new Uint8Array((first.length + second.length) * 8);
      const data = new DataView(bytes.buffer);
      [...first, ...second].forEach(([v, w], i) => {
        data.setUint32(i * 8, v, true);
        data.setFloat32(i * 8 + 4, w, true);
      });
      try {
        const change = { do: "paint", shape: done.shape.name, bone: state.bone, twin: second.length ? done.twin.bone : null, first: first.length };
        [state.work] = core.ask({ op: "change", change }, bytes);
        state.problem = null;
        state.saved = null;
        state.exported = null;
      } catch (e) {
        state.problem = e.message;
      }
    }
    heat();
    settled();
    return false;
  }
  moved(done.shape, done.before, done.amount);
  settled();
  return false;
}

/**
 * After a stroke: what of the screen a stroke can change is brought up to
 * date, and no more. The lists are as they were, so they are left standing.
 */
function settled() {
  const bar = (label) => [...document.querySelectorAll(".bar button")].find((b) => b.textContent === label);
  const said = document.querySelector(".bar .warn, .bar .sub");
  if (state.problem || said || !bar("Undo")) return render();
  bar("Undo").disabled = !state.work.undo;
  bar("Redo").disabled = !state.work.redo;
}

/**
 * A shape's vertices now stand where `s.now` has them, having stood at
 * `before`: make that a change to the project, to the shape itself or to
 * the slider the brushes change, which stood at `amount`.
 */
function moved(s, before, amount) {
  const k = state.editing;
  // What a slider does is what was seen to move, over how far the slider stood.
  const { bytes, vertices, offsets } = brush.moved(before, s.now, 1 / amount);
  if (!vertices.length) return;
  try {
    const [work] = core.ask({ op: "change", change: { do: "stroke", shape: s.name, slider: k < 0 ? null : cur().sliders[k].name } }, bytes);
    state.work = work;
    if (k < 0) {
      for (let i = 0; i < vertices.length; i++) for (let c = 0; c < 3; c++) s.base[vertices[i] * 3 + c] += offsets[i * 3 + c];
    } else {
      // The slider's list for this shape, with the stroke added to it.
      const list = new Map();
      const had = s.sliders.find((d) => d.slider === k);
      if (had) had.vertices.forEach((v, i) => list.set(v, [had.offsets[i * 3], had.offsets[i * 3 + 1], had.offsets[i * 3 + 2]]));
      vertices.forEach((v, i) => {
        const o = list.get(v) || [0, 0, 0];
        list.set(v, [o[0] + offsets[i * 3], o[1] + offsets[i * 3 + 1], o[2] + offsets[i * 3 + 2]]);
      });
      const made = { slider: k, vertices: Uint32Array.from(list.keys()), offsets: Float32Array.from([...list.values()].flat()) };
      if (had) s.sliders[s.sliders.indexOf(had)] = made;
      else s.sliders.push(made);
    }
    state.problem = null;
    state.saved = null;
    state.exported = null;
  } catch (e) {
    state.problem = e.message;
    s.now.set(before);
    shade(s.now, s.shown, s.weld, s.normals, s.sum);
    view.move(state.shapes.indexOf(s), s.now, s.normals);
  }
}

/**
 * Outfit Studio: a new shape made of an OBJ file's mesh, in the likeness of
 * a shape of the project: drawn with what that is drawn with, and following
 * the bones its nearest vertices follow.
 */
async function shapeFromObj(like) {
  try {
    const picked = await host.request("disk.open", { extensions: ["obj"] });
    if (!picked) return;
    const made = obj.mesh(new TextDecoder().decode(picked.bytes));
    const nv = made.positions.length / 3;
    const nt = made.triangles.length / 3;
    // How many vertices and triangles, then the places, the texture places, the triangles.
    const bytes = new Uint8Array(8 + nv * 20 + nt * 12);
    const data = new DataView(bytes.buffer);
    data.setUint32(0, nv, true);
    data.setUint32(4, nt, true);
    made.positions.forEach((c, i) => data.setFloat32(8 + i * 4, c, true));
    made.uvs.forEach((c, i) => data.setFloat32(8 + nv * 12 + i * 4, c, true));
    made.triangles.forEach((v, i) => data.setUint32(8 + nv * 20 + i * 4, v, true));
    rename(
      "New shape",
      picked.name.replace(/\.obj$/i, ""),
      (name) => {
        const [work] = core.ask({ op: "change", change: { do: "shapeFrom", like: like.name, name } }, bytes);
        taken(work);
      },
      "Add",
    );
  } catch (e) {
    state.problem = e.message;
    render();
  }
}

/** Outfit Studio: a shape as it stands, as an OBJ file saved where the person says. */
async function exportObj(sh) {
  const text = obj.write([{ name: sh.name, positions: sh.now, uvs: sh.uvs, triangles: sh.shown }]);
  try {
    await host.request("disk.save", { name: `${sh.name}.obj`, bytes: new TextEncoder().encode(text).buffer });
  } catch (e) {
    state.problem = e.message;
    render();
  }
}

/**
 * Outfit Studio: move a shape's vertices to where an OBJ file has them: the
 * shape as exported, changed in another program and brought back. It becomes
 * a change to the shape, or to the slider the brushes change.
 */
async function importObj(sh) {
  try {
    const picked = await host.request("disk.open", { extensions: ["obj"] });
    if (!picked) return;
    const to = obj.positions(new TextDecoder().decode(picked.bytes));
    if (to.length !== sh.now.length) {
      const points = (n) => `${n.toLocaleString()} ${n === 1 ? "point" : "points"}`;
      throw new Error(`${picked.name} has ${points(to.length / 3)}, and ${sh.name} ${points(sh.now.length / 3)}.`);
    }
    const k = state.editing;
    const amount = k < 0 ? 1 : applied(cur().sliders[k], state.at[k]);
    if (amount === 0) throw new Error(`Move ${cur().sliders[k].name} off nothing first.`);
    const before = new Float32Array(sh.now);
    sh.now.set(to);
    state.problem = null;
    moved(sh, before, amount);
    shade(sh.now, sh.shown, sh.weld, sh.normals, sh.sum);
    view.move(state.shapes.indexOf(sh), sh.now, sh.normals);
  } catch (e) {
    state.problem = e.message;
  }
  render();
}

/** The brushes, under the view, and with one in hand its size, its strength and what it changes. */
function brushBar() {
  const tool = (id, label) =>
    h("button" + (state.tool === id ? ".on" : ""), { type: "button", "aria-pressed": state.tool === id ? "true" : "false", onclick: () => hold(id) }, label);
  const knob = (label, key) => {
    const input = h("input", { type: "range", min: 1, max: 100, step: 1, value: state[key], "aria-label": label });
    const fill = () => input.style.setProperty("--at", `${input.value}%`);
    fill();
    input.addEventListener("input", () => {
      state[key] = Number(input.value);
      remembered.set(`brush.${key}`, input.value);
      fill();
    });
    return h("label.knob", null, h("span", null, label), input);
  };
  const [mirror] = option("Both sides", state.mirror);
  mirror.classList.add("inline");
  mirror.addEventListener("click", () => {
    state.mirror = !state.mirror;
    remembered.set("brush.mirror", state.mirror ? "1" : "0");
  });
  const [connected] = option("Connected", state.connected);
  connected.classList.add("inline");
  connected.title = "Only what is joined to what is under the brush";
  connected.addEventListener("click", () => {
    state.connected = !state.connected;
    remembered.set("brush.connected", state.connected ? "1" : "0");
  });
  const sliders = cur()
    .sliders.map((sl, i) => ({ name: sl.name, value: i, on: i === state.editing, skip: sl.zap || sl.uv }))
    .filter((it) => !it.skip);
  const target = pickButton("Changes", state.editing < 0 ? "The shape" : cur().sliders[state.editing].name, (b) =>
    menu(
      b,
      [{ name: "The shape", value: -1, on: state.editing < 0 }, ...sliders],
      (i) => {
        state.editing = i;
        // A slider is changed where its effect shows: at nothing it would show none.
        if (i >= 0 && applied(cur().sliders[i], state.at[i]) === 0) state.at[i] = cur().sliders[i].invert ? 0 : 100;
        render();
        reshape();
      },
      "Search sliders",
      true,
    ),
  );
  return h(
    "div.tools",
    null,
    state.tool &&
      h(
        "div.brush",
        null,
        knob("Size", "size"),
        state.tool !== "mask" && state.tool !== "move" && knob("Strength", "strength"),
        state.tool !== "mask" && state.tool !== "weights" && state.tool !== "colour" && knob("Focus", "focus"),
        state.tool !== "mask" && state.tool !== "move" && state.tool !== "colour" && knob("Spacing", "spacing"),
        state.tool === "colour" &&
          h(
            "label.knob",
            null,
            h("span", null, "Colour"),
            h("input.swatch", {
              type: "color",
              value: state.colour,
              "aria-label": "Colour",
              oninput: (e) => {
                state.colour = e.target.value;
                remembered.set("brush.colour", state.colour);
              },
            }),
          ),
        state.tool === "colour" && knob("Alpha", "alpha"),
        state.tool !== "colour" && (state.tool !== "weights" || (state.bone && brush.twin(state.bone) !== state.bone)) && mirror,
        state.tool !== "weights" && state.tool !== "mask" && state.tool !== "colour" && connected,
        state.tool === "weights" &&
          state.heat &&
          h(
            "button.btn.ghost.small",
            {
              type: "button",
              title: "Cover what follows this bone",
              onclick: () => {
                const sh = state.shapes[state.picked];
                state.heat.forEach((w, v) => w > 0 && (sh.mask[v] = 1));
                render();
              },
            },
            "Mask it",
          ),
        ...(state.tool === "weights" && state.bone
          ? state.shapes
              .filter((o, i) => i !== state.picked && o.bones.includes(state.bone))
              .slice(0, 1)
              .map((o) =>
                h(
                  "button.btn.ghost.small",
                  {
                    type: "button",
                    title: `Follow ${state.bone} as ${o.name} under it does; what a mask covers is left`,
                    onclick: () => {
                      const sh = state.shapes[state.picked];
                      const spared = [];
                      sh.mask.forEach((m, v) => m >= 0.5 && spared.push(v));
                      try {
                        const [work] = core.ask({ op: "change", change: { do: "copyBone", shape: sh.name, body: o.name, bone: state.bone } }, new Uint8Array(Uint32Array.from(spared).buffer));
                        taken(work);
                      } catch (e) {
                        state.problem = e.message;
                        render();
                      }
                    },
                  },
                  `Copy from ${o.name}`,
                ),
              )
          : []),
        state.tool === "mask" ? h("span.sub.quiet", null, "Alt uncovers") : state.tool === "weights" ? h("span.sub.quiet.cut", null, `${state.bone || "No bone"} · Alt takes away`) : state.tool === "colour" ? h("span.sub.quiet", null, "Alt picks a colour up") : target,
        state.shapes[state.picked]?.mask.some((m) => m > 0) &&
          h(
            "button.btn.ghost.small",
            {
              type: "button",
              onclick: () => {
                const sh = state.shapes[state.picked];
                sh.mask.fill(0);
                view.mask(state.picked, sh.mask);
                render();
              },
            },
            "Clear mask",
          ),
        state.tool === "mask" &&
          h(
            "button.btn.ghost.small",
            {
              type: "button",
              onclick: () => {
                const sh = state.shapes[state.picked];
                sh.mask.forEach((m, v) => (sh.mask[v] = 1 - m));
                view.mask(state.picked, sh.mask);
                render();
              },
            },
            "Invert mask",
          ),
        ...(state.tool === "mask" && state.shapes[state.picked]?.mask.some((m) => m > 0)
          ? [false, true].map((less) =>
              h(
                "button.btn.ghost.small",
                {
                  type: "button",
                  onclick: () => {
                    const sh = state.shapes[state.picked];
                    sh.mask.set(brush.spread(sh.mask, sh.weld, near(sh), less));
                    view.mask(state.picked, sh.mask);
                  },
                },
                less ? "Shrink" : "Grow",
              ),
            )
          : []),
      ),
    h("div.seg", { role: "group", "aria-label": "Tool" }, tool(null, "Orbit"), ...BRUSHES.map(([id, label]) => tool(id, label))),
  );
}

/**
 * Show how much the picked shape follows the bone being painted, as the
 * project has it; or, with another brush in hand, its mask again.
 */
function heat() {
  const s = state.shapes[state.picked];
  if (!s || !view) return;
  state.heat = null;
  if (state.tool === "weights" && state.work) {
    // The bone that was picked stays picked, though this shape does not
    // follow it yet: painting is how it comes to. Only with none picked,
    // or one no shape here has, is the shape's first taken.
    if (!state.bone || !(state.bones.includes(state.bone) || state.shapes.some((o) => o.bones.includes(state.bone)))) state.bone = s.bones[0] || null;
    try {
      const [, numbers] = core.ask({ op: "boneWeights", shape: s.name, bone: state.bone || "" });
      state.heat = new Float32Array(numbers.slice(0));
    } catch (e) {
      state.problem = e.message;
    }
  }
  if (state.tool === "colour" && !s.colors.length) state.problem = `${s.name} carries no colours to paint.`;
  view.look({ heat: Boolean(state.heat), colour: state.tool === "colour" && s.colors.length > 0 });
  view.mask(state.picked, state.heat || s.mask);
}

/** How the view is drawn: how wide it sees, and how brightly it is lit. */
function lens(anchor) {
  const box = anchor.getBoundingClientRect();
  const knob = (label, key, min, max, apply) => {
    const input = h("input", { type: "range", min, max, step: 1, value: state[key], "aria-label": label });
    const fill = () => input.style.setProperty("--at", `${((input.value - min) / (max - min)) * 100}%`);
    fill();
    input.addEventListener("input", () => {
      state[key] = Number(input.value);
      remembered.set(`view.${key}`, input.value);
      fill();
      apply();
    });
    return h("label.knob.wide", null, h("span", null, label), input);
  };
  const panel = h(
    "div.menu.lens",
    { role: "dialog", "aria-label": "View", style: `left: ${Math.max(8, box.left)}px; top: ${box.bottom + 6}px` },
    knob("Field of view", "fov", 15, 80, () => view.lens({ fov: state.fov })),
    knob("Light", "light", 40, 220, () => view.lens({ bright: state.light / 100 })),
  );
  const scrim = h("div.scrim", { onpointerdown: (e) => e.target === scrim && close(scrim) }, panel);
  document.body.append(scrim);
}

/** Take a brush in hand, or with null put it down. */
function hold(tool) {
  state.tool = tool;
  // The weight brush paints one bone: the list shows which.
  if (tool === "weights") state.side = "bones";
  heat();
  view.brush = tool ? brushing : null;
  view.reach(null);
  canvas.style.cursor = tool ? "crosshair" : "";
  render();
}

/** Take a description of the project after a change, keeping each slider where it stood. */
function taken(work, renamed, light) {
  const stood = new Map(state.work.sliders.map((sl, i) => [sl.name, state.at[i]]));
  if (renamed) stood.set(renamed[1], stood.get(renamed[0]));
  const edited = state.work.sliders[state.editing]?.name;
  state.work = work;
  state.editing = work.sliders.findIndex((sl) => sl.name === (renamed && edited === renamed[0] ? renamed[1] : edited));
  state.at = work.sliders.map((sl) => stood.get(sl.name) ?? (sl.zap ? sl.big : 0));
  state.problem = null;
  // A change to a name or to where a build goes moves nothing: the shapes
  // stand as they are.
  if (!light) showShapes(false);
  render();
}

/** Change the project. Throws what the change could not do. */
function change(what, renamed) {
  const [work] = core.ask({ op: "change", change: what });
  taken(work, renamed, what.do === "renameSlider" || what.do === "output");
}

/** A change with nothing to ask first: what goes wrong is said under the view. */
function attempt(what) {
  try {
    change(what);
  } catch (e) {
    state.problem = e.message;
    render();
  }
}

function undo(again) {
  if (!state.work || !(again ? state.work.redo : state.work.undo)) return;
  try {
    const [work] = core.ask({ op: "undo", again });
    taken(work);
  } catch (e) {
    state.problem = e.message;
    render();
  }
}

/** Outfit Studio: bring a body in from another project, as what the outfit is fitted to. */
function reference(anchor) {
  menu(
    anchor,
    state.catalog.sets.map((s) => ({ name: s.name, note: s.groups[0], value: s })),
    async (set) => {
      busy("Reading");
      try {
        await prepare(set);
        state.busy = null;
        change({ do: "reference", set: set.name });
      } catch (e) {
        state.busy = null;
        state.problem = e.message;
        render();
      }
    },
    "Search bodies",
  );
}

/** The game's skeletons: `{ path, bytes, names }` each, read once. */
let skeletons = null;

/**
 * Outfit Studio: bring a bone into the mesh from the game's skeleton, to
 * paint a shape onto. It is then the bone the weight brush paints.
 */
async function addBone(anchor) {
  try {
    if (!skeletons) {
      busy("Reading");
      const list = await host.request("files.list", { folder: "Data/meshes/actors/character" }).catch(() => []);
      // The woman's first: it has the bones a body's physics hangs on.
      const found = list
        .filter((f) => /(^|\/)skeleton[^/]*\.nif$/i.test(f.path))
        .sort((a, b) => /female/i.test(b.path) - /female/i.test(a.path) || a.path.length - b.path.length)
        .slice(0, 4);
      skeletons = [];
      for (const f of found) {
        const bytes = await host.request("files.read", { path: f.path });
        const [names] = core.ask({ op: "skeleton" }, new Uint8Array(bytes));
        skeletons.push({ path: f.path, bytes, names });
      }
      state.busy = null;
      render();
      anchor = document.querySelector("#side .side-head .btn") || anchor;
    }
    const have = new Set(state.bones);
    const items = [];
    for (const sk of skeletons) {
      for (const name of sk.names) {
        if (have.has(name) || items.some((it) => it.name === name)) continue;
        items.push({ name, note: sk.path.split("/").pop(), value: { name, sk } });
      }
    }
    if (!items.length) throw new Error(skeletons.length ? "The mesh has every bone of the game's skeleton." : "No mod, nor the game, has a skeleton to take a bone from.");
    items.sort((a, b) => a.name.localeCompare(b.name));
    menu(
      anchor,
      items,
      ({ name, sk }) => {
        try {
          const [work] = core.ask({ op: "change", change: { do: "addBone", bone: name } }, new Uint8Array(sk.bytes));
          state.bone = name;
          state.tool = "weights";
          view.brush = brushing;
          canvas.style.cursor = "crosshair";
          taken(work);
        } catch (e) {
          state.problem = e.message;
          render();
        }
      },
      "Search bones",
    );
  } catch (e) {
    state.busy = null;
    state.problem = e.message;
    render();
  }
}

/** Every mesh the mods have, listed once: `{ name, note, value }` for the menu. */
let meshes = null;

/** Outfit Studio: begin a project from a mesh of the mods'. */
async function newProject(anchor) {
  if (!meshes) {
    busy("Reading");
    const list = await host.request("files.list", { folder: "Data/meshes" }).catch(() => []);
    meshes = list
      .filter((f) => /\.nif$/i.test(f.path))
      .map((f) => {
        const rel = f.path.replace(/^Data\//i, "");
        const cut = rel.lastIndexOf("/");
        return { name: rel.slice(cut + 1), note: rel.slice(0, cut), value: f.path };
      })
      .sort((a, b) => a.name.localeCompare(b.name));
    state.busy = null;
    render();
    anchor = document.getElementById("new") || anchor;
  }
  menu(
    anchor,
    meshes,
    async (path) => {
      busy("Opening");
      try {
        const bytes = await host.request("files.read", { path });
        const rel = path.replace(/^Data\//i, "");
        const name = rel.split("/").pop().replace(/\.nif$/i, "").replace(/_[01]$/, "");
        const [work] = core.ask({ op: "new", name, path: rel }, new Uint8Array(bytes));
        state.work = work;
        state.at = [];
        state.editing = -1;
        state.picked = 0;
        state.shapes = [];
        state.problem = null;
        state.saved = null;
        state.exported = null;
        state.busy = null;
        showShapes(true);
      } catch (e) {
        state.busy = null;
        state.problem = e.message;
      }
      render();
    },
    "Search meshes",
  );
}

/** Ask before letting go of changes that are not saved, then do `then`. */
function leaving(then) {
  if (!state.work?.changed) return then();
  const el = sheet("Unsaved changes", null, h("span.sub", null, `${state.work.name} has changes that are not saved.`), [
    h("span.grow"),
    h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"),
    h("button.btn", { type: "button", onclick: () => (close(el), then()) }, "Discard"),
  ]);
}

/** Outfit Studio: the outfit as it stands becomes the base shape, and the sliders go back to nothing. */
function setBase() {
  try {
    const [work] = core.ask({ op: "change", change: { do: "setBase", values: state.at } });
    state.at = work.sliders.map((sl) => (sl.zap ? state.at[work.sliders.indexOf(sl)] : 0));
    taken(work);
  } catch (e) {
    state.problem = e.message;
    render();
  }
}

/** A sheet asking for one name. `done` throws what is wrong with it. */
function rename(title, from, done, doing = "Rename") {
  const name = h("input.input", { type: "text", value: from, "aria-label": "Name", maxLength: 120, spellcheck: false });
  const go = h("button.btn.primary", { type: "button" }, doing);
  const said = h("span.sub.warn.grow.cut");
  const run = () => {
    try {
      done(name.value);
      close(el);
    } catch (e) {
      said.textContent = e.message;
    }
  };
  go.addEventListener("click", run);
  name.addEventListener("keydown", (e) => e.key === "Enter" && run());
  const el = sheet(title, null, h("label.field", null, h("span", null, "Name"), name), [
    said,
    h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"),
    go,
  ]);
  name.focus();
  name.select();
}

/** A switch in a sheet. Returns the element and a way to read it. */
function option(label, on) {
  const el = h(
    "button.check" + (on ? ".on" : ""),
    {
      type: "button",
      role: "checkbox",
      "aria-checked": on ? "true" : "false",
      onclick: () => {
        on = !on;
        el.classList.toggle("on", on);
        el.setAttribute("aria-checked", on ? "true" : "false");
      },
    },
    h("span.box"),
    h("span.nm.cut", null, label),
  );
  return [el, () => on];
}

/** Outfit Studio: move, turn and scale a whole shape. */
function transform(sh) {
  const number = (label, value) => {
    const input = h("input.input", { type: "number", step: "any", value: String(value), "aria-label": label });
    return [h("label.field", null, h("span", null, label), input), input];
  };
  const row = (what, value) => ["X", "Y", "Z"].map((axis) => number(`${what} ${axis}`, value));
  const move = row("Move", 0);
  const turn = row("Turn", 0);
  const [scaleField, scale] = number("Scale, percent", 100);
  const go = h("button.btn.primary", { type: "button" }, "Done");
  const said = h("span.sub.warn.grow.cut");
  go.addEventListener("click", () => {
    const read = (fields) => fields.map(([, input]) => Number(input.value));
    try {
      change({ do: "transform", shape: sh.name, by: read(move), scale: Number(scale.value) / 100, turn: read(turn) });
      close(el);
    } catch (e) {
      said.textContent = e.message;
    }
  });
  const el = sheet(
    sh.name,
    null,
    [h("div.pair.three", null, move.map((f) => f[0])), h("div.pair.three", null, turn.map((f) => f[0])), h("div.pair.three", null, scaleField)],
    [said, h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"), go],
  );
}

/** Outfit Studio: the texture files a shape is drawn with. */
function texturesOf(sh) {
  const SLOTS = ["Colour", "Normal", "Glow or skin tint", "Height", "Environment", "Environment mask", "Inner layer", "Specular or backlight"];
  const inputs = sh.textures.map((t, i) => {
    const input = h("input.input", { type: "text", value: t, "aria-label": SLOTS[i] || `Slot ${i + 1}`, spellcheck: false, maxLength: 260 });
    return [h("label.field", null, h("span", null, SLOTS[i] || `Slot ${i + 1}`), input), input];
  });
  const go = h("button.btn.primary", { type: "button" }, "Done");
  const said = h("span.sub.warn.grow.cut");
  go.addEventListener("click", () => {
    try {
      pictures.clear();
      change({ do: "textures", shape: sh.name, textures: inputs.map(([, input]) => input.value.trim()) });
      close(el);
    } catch (e) {
      said.textContent = e.message;
    }
  });
  const el = sheet(
    `${sh.name} textures`,
    null,
    h("div.fields", null, inputs.map((f) => f[0])),
    [said, h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"), go],
  );
}

/** Outfit Studio: every slider out as a morph file, or sliders in from one. */
async function morphFile(into) {
  try {
    if (into) {
      const picked = await host.request("disk.open", { extensions: ["tri"] });
      if (!picked) return;
      const [work] = core.ask({ op: "change", change: { do: "importMorphs" } }, new Uint8Array(picked.bytes));
      return taken(work);
    }
    const [made, bytes] = core.ask({ op: "morphs" });
    await host.request("disk.save", { name: made.name, bytes: bytes.slice(0) });
  } catch (e) {
    state.problem = e.message;
    render();
  }
}

/** Outfit Studio: what a slider does, out to a slider data file or in from one. */
async function sliderFile(sl, into) {
  try {
    if (into) {
      const picked = await host.request("disk.open", { extensions: ["osd"] });
      if (!picked) return;
      const [work] = core.ask({ op: "change", change: { do: "importSlider", slider: sl.name } }, new Uint8Array(picked.bytes));
      return taken(work);
    }
    const [made, bytes] = core.ask({ op: "sliderData", slider: sl.name });
    await host.request("disk.save", { name: made.name, bytes: bytes.slice(0) });
  } catch (e) {
    state.problem = e.message;
    render();
  }
}

/** Outfit Studio: what a slider starts at and how it behaves. */
function sliderSettings(sl) {
  const number = (label, value) => {
    const input = h("input.input", { type: "number", min: -500, max: 500, step: 1, value: String(value), "aria-label": label });
    return [h("label.field", null, h("span", null, label), input), input];
  };
  const [smallField, small] = number("Low weight", sl.small);
  const [bigField, big] = number("High weight", sl.big);
  const [invert, inverted] = option("Inverted", sl.invert);
  const [hide, hidden] = option("Hidden in BodySlide", sl.hidden);
  const [clamp, clamped] = option("Kept between 0 and 100", sl.clamp);
  const [zap, zapped] = option("Removes what it marks", sl.zap);
  const go = h("button.btn.primary", { type: "button" }, "Done");
  const said = h("span.sub.warn.grow.cut");
  go.addEventListener("click", () => {
    const lo = Number(small.value);
    const hi = Number(big.value);
    if (small.value === "" || big.value === "" || !Number.isFinite(lo) || !Number.isFinite(hi)) return;
    try {
      change({ do: "settings", name: sl.name, small: lo, big: hi, invert: inverted(), hidden: hidden(), zap: zapped(), uv: sl.uv && !zapped(), clamp: clamped() });
      close(el);
    } catch (e) {
      said.textContent = e.message;
    }
  });
  const el = sheet(sl.name, null, [h("div.pair", null, smallField, bigField), h("div.checks", null, invert, hide, clamp, zap)], [
    said,
    h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"),
    go,
  ]);
}

/** Outfit Studio: save the project, into the output mod, as a set BodySlide can build. */
function saveProject() {
  const work = state.work;
  const field = (label, value) => {
    const input = h("input.input", { type: "text", value, "aria-label": label, maxLength: 160, spellcheck: false });
    return [h("label.field", null, h("span", null, label), input), input];
  };
  // A project of the mods' is left as it is: this one is saved beside it.
  const own = state.kept.has(`slidersets/${work.name.toLowerCase()}.osp`) || !state.catalog.sets.some((x) => x.name === work.name);
  const [nameField, name] = field("Name", own ? work.name : `${work.name} (mine)`);
  const [folderField, folder] = field("Builds into", work.outputFolder);
  const [fileField, file] = field("Mesh name", work.outputFile);
  const [weights, weighted] = option("Two meshes, low and high weight", work.weights);
  const go = h("button.btn.primary", { type: "button" }, "Save");
  const said = h("span.sub.warn.grow.cut");
  const run = async () => {
    const wanted = name.value.trim();
    if (!wanted || !file.value.trim()) return;
    go.disabled = true;
    try {
      if (folder.value !== work.outputFolder || file.value !== work.outputFile || weighted() !== work.weights) {
        change({ do: "output", folder: folder.value, file: file.value, weights: weighted() });
      }
      const [saved, bytes] = core.ask({ op: "save", name: wanted });
      const files = saved.outputs.map((o) => ({ path: `${TOOLS}/${o.path}`, bytes: bytes.slice(o.at, o.at + o.length) }));
      for (const [i, o] of saved.outputs.entries()) state.kept.set(o.path.toLowerCase(), files[i].bytes);
      await write(files);
      // It is a set like the others from here on, and the one being worked on.
      [state.catalog] = core.ask({ op: "load" });
      state.set = state.catalog.sets.find((x) => x.name === saved.name) || state.set;
      remembered.set(`set.${state.game?.id}`, state.set.name);
      const fits = presetsFor(state.set);
      state.preset = fits.find((x) => x.name === state.preset?.name) || fits[0] || null;
      Object.assign(state, presetValues(state.set, state.preset));
      state.work = { ...state.work, name: saved.name, changed: false };
      state.saved = saved.name;
      state.exported = null;
      state.built = null;
      close(el);
      render();
    } catch (e) {
      said.textContent = e.message;
      go.disabled = false;
    }
  };
  go.addEventListener("click", run);
  name.addEventListener("keydown", (e) => e.key === "Enter" && run());
  // The project as one file to hand to someone else: laid out as a mod is.
  const pack = h(
    "button.btn.ghost",
    {
      type: "button",
      title: "The project as one file to share, which installs as a mod",
      onclick: async () => {
        try {
          const [made, bytes] = core.ask({ op: "pack" });
          await host.request("disk.save", { name: made.name, bytes: bytes.slice(0) });
        } catch (e) {
          said.textContent = e.message;
        }
      },
    },
    "Pack as a file…",
  );
  const el = sheet("Save project", null, [nameField, h("div.pair", null, folderField, fileField), h("div.checks", null, weights)], [
    pack,
    said,
    h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"),
    go,
  ]);
  name.focus();
  name.select();
}

/** Outfit Studio: write the outfit as it stands as one mesh, into the output mod. */
function exportMesh() {
  const path = h("input.input", { type: "text", value: `${cur().output}.nif`, "aria-label": "Where", spellcheck: false });
  const go = h("button.btn.primary", { type: "button" }, "Export");
  const said = h("span.sub.warn.grow.cut");
  const run = async () => {
    const wanted = path.value.trim();
    if (!wanted || /[:*?"<>|]/.test(wanted)) {
      said.textContent = wanted ? 'Leave out : * ? " < > |' : "";
      return;
    }
    go.disabled = true;
    try {
      const [made, bytes] = core.ask({ op: "export", set: state.set.name, values: state.at, path: wanted, work: Boolean(state.work) });
      await write([{ path: `Data/${made.path}`, bytes }]);
      state.exported = made.path.split("/").pop();
      state.saved = null;
      close(el);
      render();
    } catch (e) {
      said.textContent = e.message;
      go.disabled = false;
    }
  };
  go.addEventListener("click", run);
  path.addEventListener("keydown", (e) => e.key === "Enter" && run());
  const el = sheet("Export mesh", null, h("label.field", null, h("span", null, "Where, under the game's data folder"), path), [
    said,
    h("button.btn.ghost", { type: "button", onclick: () => close(el) }, "Cancel"),
    go,
  ]);
  path.focus();
}

function renderStudio() {
  const flag = (label, key) => {
    const b = h(
      "button" + (view.looks[key] ? ".on" : ""),
      {
        type: "button",
        "aria-pressed": view.looks[key] ? "true" : "false",
        onclick: () => {
          view.look({ [key]: !view.looks[key] });
          b.classList.toggle("on", view.looks[key]);
          b.setAttribute("aria-pressed", view.looks[key] ? "true" : "false");
        },
      },
      label,
    );
    return b;
  };
  const search = h("input", {
    type: "text",
    placeholder: "Search sliders",
    "aria-label": "Search sliders",
    value: state.search,
    oninput: () => ((state.search = search.value), studioSliders()),
  });
  view.look({ picked: state.picked });
  const sideAt = document.querySelector("#side .rows")?.scrollTop || 0;
  root.replaceChildren(
    h(
      "div.bar",
      null,
      modes(),
      outfitPick("Project"),
      h("button.btn.ghost", { type: "button", id: "new", onclick: (e) => leaving(() => newProject(e.currentTarget)) }, "New…"),
      h("span.grow"),
      state.work && h("button.btn.ghost", { type: "button", disabled: !state.work.undo, onclick: () => undo(false) }, "Undo"),
      state.work && h("button.btn.ghost", { type: "button", disabled: !state.work.redo, onclick: () => undo(true) }, "Redo"),
      h("button.btn.ghost", { type: "button", disabled: !state.shapes.length, onclick: exportMesh }, "Export mesh…"),
      state.work && h("button.btn", { type: "button", onclick: saveProject }, "Save project…"),
      h("label.search", { style: "flex: 0 1 160px; min-width: 120px", html: GLASS }, search),
    ),
    h(
      "div.main.studio",
      null,
      h("div.card.list", { id: "side" }),
      h(
        "div.view",
        null,
        canvas,
        h(
          "div.looks",
          null,
          h(
            "div.seg",
            { role: "group", "aria-label": "How it is drawn" },
            flag("Textures", "painted"),
            flag("Wireframe", "wire"),
            h("button", { type: "button", "aria-haspopup": "dialog", onclick: (e) => lens(e.currentTarget) }, "View"),
          ),
        ),
        state.work && state.shapes.length > 0 && brushBar(),
      ),
      h("div.card.list", { id: "list" }),
    ),
    ...(state.problem
      ? [h("div.bar", null, h("span.sub.warn.cut", null, state.problem))]
      : state.saved
        ? [
            h(
              "div.bar",
              null,
              h("span.sub.cut", null, `${state.saved} saved into ${state.output}`),
              h("span.grow"),
              h("button.btn.primary", { type: "button", onclick: () => host.request("apply.review").catch(() => {}) }, "Apply"),
            ),
          ]
        : state.exported
        ? [
            h(
              "div.bar",
              null,
              h("span.sub.cut", null, `${state.exported} exported into ${state.output}`),
              h("span.grow"),
              h("button.btn.primary", { type: "button", onclick: () => host.request("apply.review").catch(() => {}) }, "Apply"),
            ),
          ]
        : []),
  );
  const studio = root.querySelector(".main.studio");
  splits(studio, [
    { el: studio.querySelector("#side"), edge: "right", key: "side", least: 160 },
    { el: studio.querySelector("#list"), edge: "left", key: "sliders", least: 200 },
  ]);
  studioSide();
  const side = document.querySelector("#side .rows");
  if (side) side.scrollTop = sideAt;
  studioSliders();
}

function render() {
  if (!state.catalog.sets.length) {
    root.replaceChildren(
      h(
        "div.empty",
        null,
        state.busy
          ? h("span", { id: "status" }, state.busy)
          : [h("strong", null, "No bodies or outfits"), h("span", null, state.problem || "Install a body mod that comes with BodySlide files.")],
      ),
    );
    return;
  }
  if (!canvas) {
    canvas = h("canvas", { "aria-label": "Preview" });
    view = new View(canvas);
    view.lens({ fov: state.fov, bright: state.light / 100 });
  }
  if (state.mode === "studio") return renderStudio();
  const set = state.set;
  const fits = set ? presetsFor(set) : [];
  const points = state.shapes.reduce((n, s) => n + s.base.length / 3, 0);
  const search = h("input", {
    type: "text",
    placeholder: "Search sliders",
    "aria-label": "Search sliders",
    value: state.search,
    oninput: () => ((state.search = search.value), list()),
  });
  root.replaceChildren(
    h(
      "div.bar",
      null,
      modes(),
      state.catalog.groups.length > 1 &&
        pickButton("Group", state.group || "All", (b) =>
          menu(
            b,
            [
              { name: "All", value: null, on: !state.group },
              ...state.catalog.groups.map((g) => ({ name: g, value: g, on: g === state.group })),
              set && { name: "Add this outfit to a group…", value: addToGroup, rule: true },
              set && state.group && set.groups.includes(state.group) && { name: `Take this outfit out of ${state.group}`, value: outOfGroup },
            ].filter(Boolean),
            (g) => {
              if (g === addToGroup) return addToGroup();
              if (g === outOfGroup) return outOfGroup();
              state.group = g;
              render();
            },
            "Search groups",
          ),
        ),
      outfitPick("Outfit"),
      fits.length > 0 &&
        pickButton("Preset", state.preset?.name || "None", (b) =>
          menu(
            b,
            fits.map((p) => ({ name: p.name, value: p, on: p === state.preset })),
            (p) => {
              state.preset = p;
              Object.assign(state, presetValues(set, p));
              state.built = null;
              remembered.set(`preset.${state.game?.id}`, p.name);
              render();
              reshape();
            },
            "Search presets",
          ),
        ),
      h("button.btn.ghost", { type: "button", disabled: !set, onclick: savePreset }, "Save preset…"),
      set &&
        h(
          "button.btn.ghost",
          {
            type: "button",
            title: "Presets and projects saved here",
            onclick: (e) =>
              menu(
                e.currentTarget,
                [
                  state.preset && { name: `Delete preset ${state.preset.name}`, value: "preset", danger: true },
                  { name: `Delete project ${set.name}`, value: "project", danger: true },
                ].filter(Boolean),
                (what) => remove(what),
              ),
          },
          "Delete…",
        ),
      h("span.grow"),
      h("label.search", { style: "flex: 0 1 200px; min-width: 120px", html: GLASS }, search),
    ),
    h(
      "div.main",
      null,
      h("div.card.list", { id: "list" }),
      h(
        "div.view",
        null,
        canvas,
        points > 0 && h("span.sub.quiet.note", null, `${points.toLocaleString()} points`),
        set?.weights &&
          h(
            "div.weights",
            null,
            h(
              "div.seg",
              { role: "group", "aria-label": "Weight shown" },
              h("button" + (state.heavy ? "" : ".on"), { type: "button", "data-heavy": "0", onclick: () => setWeight(false) }, "Low"),
              h("button" + (state.heavy ? ".on" : ""), { type: "button", "data-heavy": "1", onclick: () => setWeight(true) }, "High"),
            ),
          ),
      ),
    ),
    h("div.bar", { id: "foot" }),
  );
  const main = root.querySelector(".main");
  splits(main, [{ el: main.querySelector(".view"), edge: "left", key: "view", least: 240 }]);
  list();
  foot();
}

/** Sets that were on screen when the engine stopped. */
const stopped = new Set();

// ── Start ──────────────────────────────────────────────────────────────────

async function start() {
  state.busy = "Opening";
  render();
  try {
    const hello = await host.request("hello");
    state.game = hello.game;
    host.wear(hello.theme);
    host.listen((event, data) => event === "theme" && host.wear(data));
    host.request("fonts").then(async (fonts) => {
      for (const f of fonts || []) {
        const face = new FontFace(f.family, f.bytes, { weight: f.weight, style: f.style });
        document.fonts.add(await face.load());
      }
    }, () => {});
    await core.start("bodyslide.wasm");
    // Started afresh, it is given the catalogue again, and the set on screen.
    core.onRestart(async () => {
      const said = "That stopped the add-on's engine, which has been started again.";
      const name = state.set?.name;
      try {
        await readCatalog();
        // Once: a set that stops it every time is not opened in a ring.
        const again = !stopped.has(name) && state.catalog.sets.find((s) => s.name === name);
        stopped.add(name);
        state.work = null;
        if (again) await choose(again, state.preset?.name);
        else state.shapes = [];
      } catch {
        // Said below either way.
      }
      state.problem = said;
      render();
    });
    busy("Reading");
    await readCatalog();
  } catch (e) {
    state.problem = e.message;
  }
  state.busy = null;
  const sets = state.catalog.sets;
  if (!sets.length) return render();
  const last = remembered.get(`set.${state.game?.id}`);
  const first = sets.find((s) => s.name === last) || sets.find((s) => /\bbody\b/i.test(s.name)) || sets[0];
  await choose(first, remembered.get(`preset.${state.game?.id}`));
}

// Escape shuts the menu or sheet on top, wherever the keys are going.
addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  const open = [...document.querySelectorAll(".scrim")].pop();
  if (open) close(open);
});

// Outfit Studio: Q puts the brush down, 1 and up take one in hand in the
// order they are shown, the square brackets size it, W and T change how the
// outfit is drawn.
addEventListener("keydown", (e) => {
  if (state.mode !== "studio" || !state.work || e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.target.closest?.("input, textarea, .scrim") || document.querySelector(".scrim")) return;
  const key = e.key.toLowerCase();
  const tool = BRUSHES[Number(key) - 1];
  if (key === "q") hold(null);
  else if (tool) hold(tool[0]);
  else if (key === "[" || key === "]") {
    state.size = Math.max(1, Math.min(100, state.size + (key === "[" ? -5 : 5)));
    remembered.set("brush.size", String(state.size));
    render();
  } else if (key === "f") view.frame(state.shapes.filter((sh) => !sh.hidden).map((sh) => sh.now));
  else if (key === "x") {
    state.mirror = !state.mirror;
    remembered.set("brush.mirror", state.mirror ? "1" : "0");
    render();
  } else if (key === "h" || key === "g") {
    // The picked shape hidden, or drawn as a ghost; again to bring it back.
    const sh = state.shapes[state.picked];
    if (!sh) return;
    if (key === "h") {
      sh.hidden = !sh.hidden;
      view.hide(state.picked, sh.hidden);
    } else {
      sh.ghost = !sh.ghost;
      view.ghost(state.picked, sh.ghost);
    }
    render();
  } else if (key === "-" || key === "=" || key === "+") {
    state.strength = Math.max(1, Math.min(100, state.strength + (key === "-" ? -5 : 5)));
    remembered.set("brush.strength", String(state.strength));
    render();
  } else if (key === "w") view.look({ wire: !view.looks.wire }), render();
  else if (key === "t") view.look({ painted: !view.looks.painted }), render();
  else return;
  e.preventDefault();
});

// Outfit Studio: Ctrl+S saves the project and Ctrl+E exports the mesh.
addEventListener("keydown", (e) => {
  if (state.mode !== "studio" || !state.work || !(e.ctrlKey || e.metaKey) || document.querySelector(".scrim")) return;
  const key = e.key.toLowerCase();
  if (key === "s") saveProject();
  else if (key === "e") exportMesh();
  else return;
  e.preventDefault();
});

// Outfit Studio: a change is taken back and put back from the keyboard too.
addEventListener("keydown", (e) => {
  if (state.mode !== "studio" || !(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== "z") return;
  if (e.target.closest?.("input, textarea, .scrim")) return;
  e.preventDefault();
  undo(e.shiftKey);
});

start();
