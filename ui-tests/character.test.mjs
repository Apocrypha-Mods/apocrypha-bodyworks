// The character screen loaded as it is, with a stand-in for what app.js
// gives it and for the little of the page it touches: every value made up.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { characterScreen } from "../ui/character.js";

const read = (f) => readFileSync(new URL(`../ui/${f}`, import.meta.url), "utf8");

test("app.js gives the character screen everything it asks of it", () => {
  const app = read("app.js");
  const at = app.indexOf("characterScreen({");
  const block = app.slice(at, app.indexOf("\n});", at));
  const given = new Set([...block.matchAll(/^ {2}([A-Za-z]+)[,:]/gm)].map((m) => m[1]));
  const used = new Set([...read("character.js").matchAll(/\bctx\.([A-Za-z]+)/g)].map((m) => m[1]));
  assert.ok(used.size > 10, "the screen's uses were found");
  for (const name of used) assert.ok(given.has(name), `ctx.${name} is used but not given`);
});

/** An element as far as the screen uses one. */
function fake(spec = "div", props = null, kids = []) {
  return {
    spec,
    props: props || {},
    kids: kids.flat(Infinity).filter((k) => k !== null && k !== undefined && k !== false && k !== ""),
    children: [{ textContent: "" }, { textContent: "" }],
    style: { setProperty() {} },
    classList: { toggle() {} },
    textContent: "",
    append(...k) {
      this.kids.push(...k);
    },
    replaceChildren(...k) {
      this.kids = k;
    },
    addEventListener() {},
    querySelector: () => fake(),
    focus() {},
  };
}

/** Every element under `el`, itself first. */
function* all(el) {
  if (!el || typeof el !== "object") return;
  yield el;
  for (const k of el.kids || []) yield* all(k);
}

const text = (el) => (el.kids || []).filter((k) => typeof k === "string").join("");
const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setImmediate(r));
};

const SAVES = [
  { name: "one.ess", character: "First" },
  { name: "two.ess", character: "Second" },
];

const look = () => ({ headParts: [], faceMorphs: [0], faceParts: [0, -1, 0, 0], hairRgb: null, skinTone: null, tints: [], weight: 40, custom: [] });

/** The screen with a stand-in app and module. `lookFor(args, save)` answers `presetLook`; `badSave` can't be read. */
function harness({ work = null, lookFor = null, badSave = null } = {}) {
  const root = fake();
  globalThis.document = { getElementById: (id) => (id === "app" ? root : null) };
  const state = { mode: "character", work, busy: null, shapes: [], output: "Out", into: null };
  const log = { leaving: [], fromSave: [], readSave: [], looks: [], shown: 0 };
  let save = null;
  const answer = (args) => ({
    files: [],
    parts: [],
    choices: {},
    look: look(),
    race: args.race ? { ...args.race, name: "Asked" } : { plugin: "Base.esm", id: 1, name: "Own" },
    female: args.female ?? true,
    races: [],
    masks: [],
    offered: [[], [], [], []],
    sliders: [],
    faceSliders: [],
    notes: [],
    kept: [],
  });
  const core = {
    ask(req) {
      if (req.op === "presetOpen") return [{ name: req.name, kept: [] }];
      if (req.op === "presetLook") {
        log.looks.push({ ...req, save });
        if (lookFor) lookFor(req, save);
        return [answer(req)];
      }
      if (req.op === "presetCharacter") return [{ preset: true, changed: false, notes: [], missing: [], faceTint: null }];
      throw new Error(`not stood in for: ${req.op}`);
    },
  };
  const host = {
    async request(op) {
      if (op === "files.list") return [{ path: "Data/SKSE/Plugins/CharGen/Presets/Made.jslot" }];
      if (op === "files.read") return new ArrayBuffer(2);
      if (op === "saves.list") return SAVES;
      throw new Error(`not stood in for: ${op}`);
    },
  };
  let screen;
  const ctx = {
    h: (spec, props, ...kids) => fake(spec, props, kids),
    state,
    host,
    core,
    view: () => null,
    canvas: () => fake("canvas"),
    modes: () => fake(),
    menu: (anchor, items, onPick) => (log.menu = { items, onPick }),
    sheet: () => fake(),
    close() {},
    rename() {},
    pickButton: (label, value, onclick) => Object.assign(fake("button.pick"), { label, value, onclick }),
    splits() {},
    busy: (t) => (state.busy = t),
    render: () => screen.render(),
    leaving: (then) => log.leaving.push(then),
    showShapes: () => log.shown++,
    chooseTarget() {},
    write: async () => {},
    fromSave: (opts) => log.fromSave.push(opts),
    readSave: async (sv) => {
      log.readSave.push(sv.name);
      if (sv.name === badSave) throw new Error("That save couldn't be read.");
      save = sv.name;
    },
    inTurns: (items, n, f) => Promise.all(items.map(f)),
    forgetTints() {},
    tintsMade: () => 1,
    GLASS: "",
    CHEVRON: "",
  };
  screen = characterScreen(ctx);
  const find = (pred) => [...all(root)].find(pred);
  const pick = (label) => find((el) => el.label === label);
  /** Open the preset from the mods' list, through whichever button offers it. */
  const open = async () => {
    const button = find((el) => text(el) === "Open preset…") || pick("Preset");
    const onclick = button.props.onclick || button.onclick;
    onclick({ currentTarget: fake() });
    await settle();
    log.menu.onPick("Data/SKSE/Plugins/CharGen/Presets/Made.jslot");
    await settle();
  };
  return { screen, state, log, find, pick, open };
}

test("a Studio project with changes not saved is asked about once; Cancel holds the preview until the next open", async () => {
  const t = harness({ work: { changed: true } });
  t.screen.render();
  await t.open();
  // No save yet: one is asked for, and read.
  assert.equal(t.log.fromSave.length, 1);
  await t.log.fromSave[0].open(SAVES[0]);
  await settle();
  assert.equal(t.log.leaving.length, 1, "asked once");
  // Cancelled (nothing called back): drawn again and again, not asked again.
  t.screen.render();
  await settle();
  t.screen.render();
  await settle();
  assert.equal(t.log.leaving.length, 1, "not asked again");
  assert.equal(t.log.shown, 0, "nothing built over the project");
  assert.match(text(t.find((el) => el.props.id === "view-note")), /waits/);
  // Opened again: asked again; let go, the preview is built.
  await t.open();
  await settle();
  assert.equal(t.log.leaving.length, 2, "asked again on the next open");
  t.log.leaving[1]();
  await settle();
  assert.equal(t.log.shown, 1);
  assert.equal(t.state.work.preset, true);
  t.screen.render();
  await settle();
  assert.equal(t.log.leaving.length, 2);
  assert.equal(text(t.find((el) => el.props.id === "view-note")), "Race and sex are only for the preview");
});

test("a save that can't be read leaves the last one, as the race it was shown as", async () => {
  const t = harness({ badSave: "two.ess" });
  t.screen.render();
  await t.open();
  await t.log.fromSave[0].open(SAVES[0]);
  await settle();
  assert.equal(t.pick("Load order").value, "First");
  t.pick("Load order").onclick();
  await settle();
  await assert.rejects(t.log.fromSave[1].open(SAVES[1]), /couldn't be read/);
  await settle();
  assert.deepEqual(t.log.readSave, ["one.ess", "two.ess", "one.ess"]);
  assert.equal(t.pick("Load order").value, "First");
  const last = t.log.looks.at(-1);
  assert.deepEqual([last.save, last.race, last.female], ["one.ess", { plugin: "Base.esm", id: 1 }, true]);
});

test("a save whose character's race can't be shown is read as the race shown before", async () => {
  const t = harness({
    lookFor: (req, save) => {
      if (save === "two.ess" && !req.race) throw new Error("There's no race Gone.esp|000001.");
    },
  });
  t.screen.render();
  await t.open();
  await t.log.fromSave[0].open(SAVES[0]);
  await settle();
  t.pick("Load order").onclick();
  await settle();
  await t.log.fromSave[1].open(SAVES[1]);
  await settle();
  assert.deepEqual(t.log.readSave, ["one.ess", "two.ess"]);
  assert.equal(t.pick("Load order").value, "Second");
  assert.equal(t.pick("Race").value, "Asked");
});

test("a save whose race fails both ways leaves the last save read again", async () => {
  const t = harness({
    lookFor: (req, save) => {
      if (save === "two.ess") throw new Error("The save names no race for its character.");
    },
  });
  t.screen.render();
  await t.open();
  await t.log.fromSave[0].open(SAVES[0]);
  await settle();
  t.pick("Load order").onclick();
  await settle();
  await assert.rejects(t.log.fromSave[1].open(SAVES[1]), /names no race/);
  await settle();
  assert.deepEqual(t.log.readSave, ["one.ess", "two.ess", "one.ess"]);
  assert.equal(t.pick("Load order").value, "First");
});
