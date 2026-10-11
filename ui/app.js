// The BodySlide screen: pick a body or outfit, pick a preset, move its
// sliders with the shape following, and build it into a mod.

import * as host from "./host.js";
import * as core from "./core.js";
import { View, welded, shade } from "./view.js";

const TOOLS = "Data/CalienteTools/BodySlide";
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

const remembered = {
  get(key) {
    try {
      return localStorage.getItem(`bodyslide.${key}`);
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(`bodyslide.${key}`, value);
    } catch {
      // A frame with no storage of its own remembers nothing; that is fine.
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
  busy: null,
  problem: null,
};

let view = null;
const root = document.getElementById("app");

// ── Files ──────────────────────────────────────────────────────────────────

const under = (path) => path.slice(TOOLS.length + 1);

/** Hand the module the file the mods have at `rel` (under the BodySlide folder). */
async function give(rel) {
  const real = state.paths.get(rel.replace(/\\/g, "/").toLowerCase());
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

/** Move every shape to where the sliders put it, and show it. */
function reshape() {
  const values = state.heavy ? state.big : state.small;
  state.shapes.forEach((s, i) => {
    s.now.set(s.base);
    for (const d of s.sliders) {
      const amount = applied(state.set.sliders[d.slider], values[d.slider]);
      if (amount === 0) continue;
      const { vertices, offsets } = d;
      for (let k = 0; k < vertices.length; k++) {
        const v = vertices[k] * 3;
        s.now[v] += offsets[k * 3] * amount;
        s.now[v + 1] += offsets[k * 3 + 1] * amount;
        s.now[v + 2] += offsets[k * 3 + 2] * amount;
      }
    }
    shade(s.now, s.triangles, s.weld, s.normals, s.sum);
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

async function choose(set, presetName) {
  state.set = set;
  state.built = null;
  state.problem = null;
  state.shapes = [];
  remembered.set(`set.${state.game?.id}`, set.name);
  busy("Opening");
  try {
    const missing = await prepare(set);
    const [preview, numbers] = core.ask({ op: "preview", set: set.name });
    const f32 = new Float32Array(numbers);
    const u32 = new Uint32Array(numbers);
    state.shapes = preview.shapes.map((s) => {
      const base = f32.slice(s.positions[0], s.positions[0] + s.positions[1]);
      return {
        name: s.name,
        base,
        now: new Float32Array(base),
        normals: new Float32Array(base.length),
        sum: new Float32Array(base.length),
        weld: welded(base),
        triangles: u32.slice(s.triangles[0], s.triangles[0] + s.triangles[1]),
        sliders: s.sliders.map((d) => ({
          slider: d.slider,
          vertices: u32.subarray(d.vertices[0], d.vertices[0] + d.vertices[1]),
          offsets: f32.subarray(d.offsets[0], d.offsets[0] + d.offsets[1]),
        })),
      };
    });
    if (!state.shapes.length) {
      state.problem = missing.length ? `${missing[0].split("/").pop()} is missing.` : "This set has no shapes to show.";
    }
    const fits = presetsFor(set);
    state.preset = fits.find((p) => p.name === presetName) || fits.find((p) => p.name === state.preset?.name) || fits[0] || null;
    Object.assign(state, presetValues(set, state.preset));
  } catch (e) {
    state.problem = e.message;
  }
  state.busy = null;
  render();
  if (state.shapes.length) {
    view.show(state.shapes.map((s) => ({ positions: s.base, triangles: s.triangles })));
    reshape();
    view.frame(state.shapes.map((s) => s.base));
  } else view.show([]);
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
    { name: OUTPUT, files },
    files.map((f) => f.bytes),
  );
}

async function build() {
  busy("Building");
  try {
    const [built, bytes] = core.ask({ op: "build", set: state.set.name, small: state.small, big: state.big });
    const files = filesOf(built, bytes);
    await write(files);
    state.built = files.length;
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
function menu(anchor, items, onPick, placeholder) {
  const box = anchor.getBoundingClientRect();
  const rows = h("div.rows");
  const fill = (query) => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const shown = items.filter((it) => words.every((w) => `${it.name} ${it.note || ""}`.toLowerCase().includes(w)));
    rows.replaceChildren(
      ...shown.slice(0, 400).map((it) =>
        h(
          "button" + (it.on ? ".on" : ""),
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
      ),
    );
    if (!shown.length) rows.append(h("div.sub.quiet", { style: "padding: 8px 10px" }, "Nothing found"));
  };
  const input = h("input", { type: "text", placeholder, "aria-label": placeholder, oninput: () => fill(input.value) });
  const panel = h(
    "div.menu",
    { role: "dialog", style: `left: ${Math.max(8, Math.min(box.left, innerWidth - 328))}px; top: ${box.bottom + 6}px` },
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
      await host.request("output.write", { name: OUTPUT, files: [{ path: `${TOOLS}/${rel}`, bytes }] }, [bytes]);
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

function sliderRows() {
  const set = state.set;
  const two = set.weights;
  const words = state.search.toLowerCase().split(/\s+/).filter(Boolean);
  const order = [...state.catalog.categories.map((c) => c.name), null];
  const by = new Map(order.map((c) => [c, []]));
  set.sliders.forEach((s, i) => {
    if (s.hidden || s.zap || s.uv) return;
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
      out.push(h("div.sl" + (two ? "" : ".one"), null, h("span.cut", null, set.sliders[i].shown), two && track(i, false), track(i, true)));
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
        ? h("span.sub.cut", null, `${state.built} ${state.built === 1 ? "file" : "files"} built into ${OUTPUT}`)
        : h("span.sub.cut", { id: "status" }, state.busy || `Into ${OUTPUT}`),
    h("span.grow"),
    h("button.btn", { type: "button", disabled: !ready, onclick: several }, "Build several…"),
    state.built !== null && !state.problem
      ? h("button.btn.primary", { type: "button", onclick: () => host.request("apply.review").catch(() => {}) }, "Apply")
      : h("button.btn.primary", { type: "button", disabled: !ready, onclick: build }, "Build"),
  );
}

let canvas = null;

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
  }
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
      pickButton("Outfit", set?.name || "", (b) =>
        menu(
          b,
          state.catalog.sets.map((s) => ({ name: s.name, note: s.groups[0], value: s, on: s === set })),
          (s) => choose(s),
          "Search outfits",
        ),
      ),
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
      h("span.grow"),
      h("label.search", { style: "flex: 0 1 220px", html: GLASS }, search),
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
  list();
  foot();
}

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

start();
