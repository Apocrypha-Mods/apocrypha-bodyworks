// The character screen: a RaceMenu preset's face, opened from the mods or
// from disk, read with the plugins of a save, changed with the same pickers,
// sliders and colours as the other screens, shown in the same view, and
// saved as a preset into a mod or to disk.
//
// It is laid out as Outfit Studio is: a list on each side of the view, the
// head and makeup on the left and the sliders on the right, and it builds
// as BodySlide does, from the bar at the foot. Everything it draws with
// comes from `app.js` (`characterScreen`'s argument); the face's rules are
// in `face.js`. A preset holds no race, so the race and sex are only those
// it is shown as, and it holds no load order, so a save's is taken
// (`crates/app/src/preset.rs`).

import * as face from "./face.js";

/** Where RaceMenu keeps its presets, under the game's folder. */
const PRESETS = "Data/SKSE/Plugins/CharGen/Presets";
/** A value no preset's path can be: the menu's row for one from disk. */
const FROM_DISK = "\0disk";

export function characterScreen(ctx) {
  const { h, state } = ctx;
  const cs = {
    /** `presetOpen`'s answer: the preset's name and what it keeps as read. */
    opened: null,
    /** The preset's bytes, to open again with another save's plugins. */
    bytes: null,
    /** `presetLook`'s answer: the look as read, and everything picked from. */
    answer: null,
    /** The look as changed here. */
    look: null,
    /** The save whose plugins the preset is read and saved with. */
    save: null,
    side: "head",
    search: "",
    shut: new Set(["Not in use"]),
    /** What the last build left out: its notes, and the files it didn't find. */
    built: null,
    /** The last save: the file's name, where it went, and what was written. */
    saved: null,
    problem: null,
    /** Counts builds: only the last one asked for is shown. */
    turn: 0,
    /** Whether the screen, drawn next, builds the character again: "frame" to frame it in the view as well. */
    stale: null,
    /** Every head part met so far, by plugin and id: its name and kind, so a part picked as one sex is still known as the other. */
    known: new Map(),
    /** A note that stays until the preset is read again: what a change of race or save let go. */
    note: null,
    /** A Studio project whose changes the person let go, for the preview to take its place. */
    letGo: null,
    /** Asked whether to let such a project go, and not told to: the preview waits, unasked, until a preset or save is opened again. */
    waiting: false,
  };

  /** Forget everything, saying why: the engine was started again, and holds nothing. */
  function reset(why = null) {
    const lost = why && cs.opened ? `${why} The preset and its changes went with it.` : why;
    Object.assign(cs, { opened: null, bytes: null, answer: null, look: null, save: null, built: null, saved: null, problem: lost, note: null, letGo: null, waiting: false });
    cs.known.clear();
    cs.turn++;
  }

  /** Whether building the preset's character would let go of a Studio project with changes not saved. */
  function blocks() {
    return Boolean(state.work?.changed && !state.work.preset && state.work !== cs.letGo);
  }

  /** Whether showing this screen would let go of such a project: only once there is a preset to show. */
  function replaces() {
    return Boolean(cs.answer) && blocks();
  }

  /** The person let go of the Studio project's changes: the preview may take its place. */
  function letGo() {
    cs.letGo = state.work;
  }

  /** The save went to another mod, or the look changed: Save again, not Apply. */
  function unsaved() {
    if (!cs.saved) return;
    cs.saved = null;
    foot();
  }

  const clone = face.copy;

  /** The look as changed, which is no longer the one saved. */
  function touch(look) {
    cs.look = look;
    unsaved();
  }

  /** The look changed, and the character built again. */
  function edit(look) {
    touch(look);
    build();
  }

  /** The look as the module is sent it. */
  const sent = () => face.outgoing(cs.look, cs.answer.look);

  /** Ask before letting go of changes to the preset not saved, then do `then`: the Studio's sheet for it. */
  function discarding(then) {
    if (!cs.answer || cs.saved || !face.changed(cs.look, cs.answer.look)) return then();
    const el = ctx.sheet("Unsaved changes", null, h("span.sub", null, `${cs.opened.name} has changes that are not saved.`), [
      h("span.grow"),
      h("button.btn.ghost", { type: "button", onclick: () => ctx.close(el) }, "Cancel"),
      h("button.btn", { type: "button", onclick: () => (ctx.close(el), then()) }, "Discard"),
    ]);
  }

  /** What the view says over the preview: that it waits for the Studio project, or that race and sex are only for it. */
  const WAITS = "The preview waits until the Studio project's changes are saved or let go";
  const PREVIEW_ONLY = "Race and sex are only for the preview";
  function viewNote() {
    const el = document.getElementById("view-note");
    if (!el) return;
    el.textContent = cs.waiting ? WAITS : PREVIEW_ONLY;
    el.classList.toggle("warn", cs.waiting);
  }

  /** Hand the module the files it asked for, as the mods have them; one they haven't got is left out. */
  async function fetchFiles(files) {
    await ctx.inTurns(files, 8, async (path) => {
      const file = await ctx.host.request("files.read", { path: `Data/${path}` }).catch(() => null);
      if (file) ctx.core.ask({ op: "characterFile", path, preset: true }, new Uint8Array(file));
    });
  }

  /** Read the preset as a race and sex (none given: as before, or the save's character's), fetching what it asks for. */
  async function readLook(args = {}) {
    let [a] = ctx.core.ask({ op: "presetLook", ...args });
    for (let i = 0; a.files.length && i < 64; i++) {
      ctx.busy("Reading files");
      await fetchFiles(a.files);
      [a] = ctx.core.ask({ op: "presetLook" });
    }
    cs.answer = a;
    const key = (p) => `${p.plugin.toLowerCase()}|${p.id}`;
    for (const p of a.parts) if (!cs.known.has(key(p)) || p.kind !== null) cs.known.set(key(p), p);
    for (const [kind, list] of Object.entries(a.choices)) for (const c of list) cs.known.set(key(c), { ...c, kind: Number(kind), loaded: true });
  }

  /** Build the character with the look as it stands, and show it. */
  async function build(frame = false, asked = false) {
    // The preview becomes the project: a Studio project with changes not
    // saved is asked about first, once. Not let go, the preview waits,
    // unasked, until a preset or a save is opened again.
    if (!asked && blocks()) {
      if (cs.waiting) return viewNote();
      cs.waiting = true;
      viewNote();
      return ctx.leaving(() => {
        cs.waiting = false;
        letGo();
        viewNote();
        build(frame, true);
      });
    }
    cs.waiting = false;
    viewNote();
    const turn = ++cs.turn;
    state.busy = "Building";
    foot();
    try {
      let [a] = ctx.core.ask({ op: "presetCharacter", look: sent() });
      // Files a picked part needs are fetched first.
      for (let i = 0; !a.preset && i < 8; i++) {
        await fetchFiles(a.files);
        if (turn !== cs.turn) return;
        [a] = ctx.core.ask({ op: "presetCharacter", look: sent() });
      }
      if (turn !== cs.turn) return;
      ctx.forgetTints();
      state.face = a.faceTint ? { ...a.faceTint, made: ctx.tintsMade() } : null;
      state.work = a;
      state.character = null;
      state.at = [];
      state.editing = -1;
      state.picked = 0;
      state.shapes = [];
      cs.built = { notes: a.notes || [], missing: a.missing || [] };
      cs.problem = null;
      state.busy = null;
      ctx.showShapes(frame);
      foot();
      ctx.view()?.draw();
    } catch (e) {
      if (turn !== cs.turn) return;
      cs.problem = e.message;
      state.busy = null;
      foot();
    }
  }

  /** The preset as a file's bytes with the look as it stands, and what was written. */
  function written() {
    const [answer, bytes] = ctx.core.ask({ op: "presetWrite", look: sent() });
    return { notes: answer.notes, bytes };
  }

  // ── Opening ──────────────────────────────────────────────────────────────

  /** The presets the mods have, and one from disk, to open. */
  async function openMenu(anchor) {
    const list = await ctx.host.request("files.list", { folder: PRESETS }).catch(() => []);
    const presets = list
      .filter((f) => /\.jslot$/i.test(f.path))
      .map((f) => ({ name: f.path.split("/").pop().replace(/\.jslot$/i, ""), value: f.path }))
      .sort((a, b) => a.name.localeCompare(b.name));
    ctx.menu(
      document.getElementById("preset") || anchor,
      [{ name: "From disk…", value: FROM_DISK }, ...presets.map((p, i) => ({ ...p, rule: i === 0 }))],
      (path) =>
        discarding(async () => {
          try {
            if (path === FROM_DISK) {
              const file = await ctx.host.request("disk.open", { extensions: ["jslot"] });
              if (file) await open(file.name.replace(/\.jslot$/i, ""), new Uint8Array(file.bytes));
            } else {
              await open(path.split("/").pop().replace(/\.jslot$/i, ""), new Uint8Array(await ctx.host.request("files.read", { path })));
            }
          } catch (e) {
            fail(e);
          }
        }),
      "Search presets",
    );
  }

  /** Open a preset's bytes; it is read with the save's plugins, a save being asked for first. */
  async function open(name, bytes) {
    [cs.opened] = ctx.core.ask({ op: "presetOpen", name }, bytes);
    cs.bytes = bytes;
    cs.answer = null;
    cs.look = null;
    cs.saved = null;
    cs.problem = null;
    cs.note = null;
    // Opened again: a Studio project with changes not saved is asked about again.
    cs.waiting = false;
    if (!cs.save) {
      ctx.render();
      return pickSave();
    }
    await shown();
  }

  /** The preset read with the save's plugins, as the save's character's race and sex, and shown. */
  async function shown(args = {}) {
    await readLook(args);
    cs.look = clone(cs.answer.look);
    state.busy = null;
    cs.stale = "frame";
    ctx.render();
  }

  /** What a change of race or save says, until the preset is read again. */
  const LET_GO = "Changes to the makeup and skin tone were let go: tint layers belong to the race.";

  /** Pick the save whose plugins the preset is read and saved with. */
  async function pickSave() {
    const saves = await ctx.host.request("saves.list").catch(() => null);
    if (!saves?.length) {
      cs.problem = "There are no saves to take the load order from. A preset is read with the plugins of a save.";
      return ctx.render();
    }
    ctx.fromSave({
      title: "Load order from a save",
      note: "The preset is read and saved with this save's plugins, in its order, and shown as its character's race and sex to begin with.",
      weighs: false,
      saves,
      open: (sv) => switchSave(sv),
    });
  }

  /**
   * Read the preset with save `sv`'s plugins, what was changed carried
   * over. As that save's character's race and sex; failing that, as the
   * race and sex it was shown as; failing that too, the last save is read
   * again. Throws what went wrong, for the save sheet to say.
   */
  async function switchSave(sv) {
    const was = cs.look;
    const read = cs.answer?.look;
    const before = cs.save;
    const as = cs.answer && { race: { plugin: cs.answer.race.plugin, id: cs.answer.race.id }, female: cs.answer.female };
    try {
      await ctx.readSave(sv, true);
      cs.known.clear();
      cs.waiting = false;
      if (cs.opened) {
        try {
          await reread({}, was, read, as);
        } catch (e) {
          // A race the new plugins haven't got, or a save that names none: as before.
          if (!as) throw e;
          await reread(as, was, read, as);
        }
      }
      cs.save = sv;
      state.busy = null;
      ctx.render();
    } catch (e) {
      await restore(before, as, was, read);
      throw e;
    }
  }

  /**
   * The preset read again from the file as it came, with the plugins read
   * last, and what was changed on `was` (read as `read`) carried over: the
   * makeup and skin tone too when it is shown as the race and sex it was
   * (`as`), and the screen says when they were let go.
   */
  async function reread(args, was, read, as = null) {
    [cs.opened] = ctx.core.ask({ op: "presetOpen", name: cs.opened.name }, cs.bytes);
    await readLook(args);
    const a = cs.answer;
    const sameRace = Boolean(as) && a.race.plugin.toLowerCase() === as.race.plugin.toLowerCase() && a.race.id === as.race.id && a.female === as.female;
    cs.look = was ? face.carry(was, read, a.look, { tints: sameRace }) : clone(a.look);
    cs.note = was && !sameRace && face.tintsChanged(was, read) ? LET_GO : null;
    cs.saved = null;
    state.busy = null;
    cs.stale = "frame";
    ctx.render();
  }

  /**
   * A save that couldn't be read may have taken the place of the last one
   * before failing: the last one is read again, and the preset as it was
   * shown. If that fails too, the preset is shown as needing a save.
   */
  async function restore(before, as, was, read) {
    if (!before || !cs.opened || !as) {
      // Nothing was shown before: still no save.
      Object.assign(cs, { answer: null, look: null, save: null, built: null, saved: null });
      state.busy = null;
      return ctx.render();
    }
    try {
      await ctx.readSave(before, true);
      cs.known.clear();
      await reread(as, was, read, as);
      cs.save = before;
      ctx.render();
    } catch (e) {
      Object.assign(cs, { answer: null, look: null, save: null, built: null, saved: null });
      cs.problem = `Neither save could be read, so the preset's changes were let go: ${e.message}`;
      state.busy = null;
      ctx.render();
    }
  }

  /** Show the preset as another race or sex. Tint layers belong to the race, so changes to them are let go; the rest stay. */
  async function previewAs(args) {
    const was = cs.look;
    const read = cs.answer.look;
    try {
      await readLook(args);
      cs.look = face.carry(was, read, cs.answer.look);
      cs.note = face.tintsChanged(was, read) ? LET_GO : null;
      cs.saved = null;
      state.busy = null;
      ctx.render();
      foot();
      await build(false);
    } catch (e) {
      fail(e);
    }
  }

  function fail(e) {
    cs.problem = e.message;
    state.busy = null;
    ctx.render();
  }

  // ── Saving ───────────────────────────────────────────────────────────────

  /** Save the preset as it stands, into the mod builds go into, or with `toDisk` where the person says. */
  function save(toDisk) {
    ctx.rename(
      toDisk ? "Save preset to disk" : "Save preset",
      // Into the mods, not under the preset's own name, which a mod of that name would hide or be hidden by.
      toDisk ? cs.opened.name : face.savedName(cs.opened.name),
      (to) => {
        const name = to.trim();
        const wrong = face.nameProblem(name);
        if (wrong) throw new Error(wrong);
        const file = `${name}.jslot`;
        if (toDisk) return put(file, true);
        // A preset of the name the mods already have is asked about first.
        ctx.host.request("files.list", { folder: PRESETS }).then(
          (files) => (face.hasFile(files, file) ? replacing(file, () => put(file, false)) : put(file, false)),
          // Not known: asked, never written over unasked.
          () => replacing(file, () => put(file, false), true),
        );
      },
      "Save",
    );
  }

  /** Ask before a preset the mods have is replaced, as before changes are let go. */
  function replacing(file, then, unknown = false) {
    const said = unknown
      ? `The mods' presets couldn't be listed, so it isn't known whether one is named ${file}. Saving puts this one in its place if there is.`
      : `The mods already have ${file}. Saving puts this one in its place.`;
    const el = ctx.sheet(unknown ? "Save preset" : "Replace preset", null, h("span.sub", null, said), [
      h("span.grow"),
      h("button.btn.ghost", { type: "button", onclick: () => ctx.close(el) }, "Cancel"),
      h("button.btn", { type: "button", onclick: () => (ctx.close(el), then()) }, unknown ? "Save" : "Replace"),
    ]);
  }

  /** Write the preset as `file`, into the mod builds go into, or with `toDisk` where the person says. */
  async function put(file, toDisk) {
    try {
      const { notes, bytes } = written();
      let into = null;
      if (toDisk) {
        const r = await ctx.host.request("disk.save", { name: file, bytes });
        if (!r?.saved) return;
      } else {
        await ctx.write([{ path: `${PRESETS}/${file}`, bytes }]);
        into = state.into?.name || state.output;
      }
      cs.saved = { file, into, notes };
      cs.problem = null;
    } catch (e) {
      cs.problem = e.message;
    }
    foot();
  }

  /** A sheet of notes, each group under its heading. */
  function notesSheet(title, groups) {
    const shown = groups.filter(([, lines]) => lines.length);
    const el = ctx.sheet(
      title,
      null,
      h(
        "div.rows.notes",
        null,
        shown.length
          ? shown.flatMap(([heading, lines]) => [heading && h("div.cat", null, heading), ...lines.map((l) => h("div.sub.note-line", null, l))]).filter(Boolean)
          : h("div.sub.quiet.note-line", null, "Nothing to note."),
      ),
      [h("span.grow"), h("button.btn.primary", { type: "button", onclick: () => ctx.close(el) }, "Done")],
    );
  }

  // ── The screen ───────────────────────────────────────────────────────────

  /** A track like the other screens' sliders: hundredths, or choices, a number beside it. */
  function track(label, title, position, { min, max }, text, { input, change, reset }) {
    const out = h("output", null, text(position));
    const el = h("input", { type: "range", min, max, step: 1, value: position, "aria-label": label });
    const paint = () => el.style.setProperty("--at", `${((el.value - el.min) / (el.max - el.min)) * 100}%`);
    paint();
    el.addEventListener("input", () => {
      out.textContent = text(Number(el.value));
      paint();
      input(Number(el.value));
    });
    el.addEventListener("change", change);
    // Back to as the preset has it.
    el.addEventListener("dblclick", reset);
    return h("div.sl.one", { title }, h("span.cut", null, label), h("span.tr", null, el, out));
  }

  /** A category's heading, open or shut. */
  function cat(name, shut, again) {
    const b = h("button.cat" + (shut ? ".shut" : ""), {
      type: "button",
      "aria-expanded": shut ? "false" : "true",
      html: `${ctx.CHEVRON}<span></span>`,
      onclick: () => {
        if (cs.shut.has(name)) cs.shut.delete(name);
        else cs.shut.add(name);
        again();
      },
    });
    b.children[1].textContent = name;
    return b;
  }

  /** A colour, as the material sheet picks one: the swatch fills its place. */
  function swatch(label, hex, onchange) {
    const el = h("input.input.swatch", { type: "color", value: hex, "aria-label": label });
    el.addEventListener("change", () => onchange(el.value));
    return el;
  }

  /** Keep a list's place while it is drawn again. */
  function again(id, fill) {
    const el = document.getElementById(id);
    if (!el) return;
    const top = el.querySelector(".rows")?.scrollTop || 0;
    fill(el);
    const rows = el.querySelector(".rows");
    if (rows) rows.scrollTop = top;
  }

  /** The right-hand list: the game's face sliders and presets, then RaceMenu's. */
  function sliders() {
    again("list", (el) => {
      const a = cs.answer;
      const look = cs.look;
      const words = cs.search.toLowerCase().split(/\s+/).filter(Boolean);
      const fits = (name) => words.every((w) => name.toLowerCase().includes(w));
      const out = [];
      const group = (name, rows) => {
        if (!rows.length) return;
        const shut = !words.length && cs.shut.has(name);
        out.push(cat(name, shut, sliders));
        if (!shut) out.push(...rows);
      };
      const changed = () => build();
      group(
        "Face",
        face.FACE_SLIDERS.map((label, i) => {
          if (i >= look.faceMorphs.length || !fits(label)) return null;
          const v = look.faceMorphs[i];
          if (v === null) return kept(label);
          const p = face.toPosition(v);
          return track(label, face.ends(...(a.faceSliders[i] || [])), p, face.range(p), (x) => String(x), {
            input: (x) => touch(face.setMorph(cs.look, i, face.fromPosition(x))),
            change: changed,
            reset: () => (edit(face.setMorph(cs.look, i, cs.answer.look.faceMorphs[i])), sliders()),
          });
        }).filter(Boolean),
      );
      group(
        "Face presets",
        face.FACE_PRESETS.map(({ at, label }) => {
          if (at >= look.faceParts.length || !fits(label)) return null;
          const v = look.faceParts[at];
          return track(label, "", v, { min: -1, max: face.presetTop(a.offered[at], v) }, (x) => face.shown(x, true), {
            input: (x) => touch(face.setFacePart(cs.look, at, x)),
            change: changed,
            reset: () => (edit(face.setFacePart(cs.look, at, cs.answer.look.faceParts[at])), sliders()),
          });
        }).filter(Boolean),
      );
      const { race, other } = face.sliderRows(look, a.sliders);
      const named = (s) => {
        if (s.kind === "headPart" || s.value === null) return kept(s.name, s.kind === "headPart" ? "Picks a head part in the game, which isn't drawn here" : "");
        const choice = s.kind === "preset";
        const p = choice ? Math.round(s.value) : face.toPosition(s.value);
        const title = s.kind === "slider" ? face.ends(s.below, s.above) : s.kind === "unknown" ? "Not one of this race's sliders, so it isn't drawn" : "";
        return track(s.name, title, p, choice ? face.range(p, 0, s.count) : face.range(p), (x) => String(x), {
          input: (x) => touch(face.setCustom(cs.look, s.name, choice ? x : face.fromPosition(x))),
          change: () => (sliders(), build()),
          // Back as the preset has it; one it hadn't got is taken out again.
          reset: () => (edit(face.resetCustom(cs.look, cs.answer.look, s.name)), sliders()),
        });
      };
      group("RaceMenu", race.filter((s) => fits(s.name)).map(named));
      group("Not for this race", other.filter((s) => fits(s.name)).map(named));
      el.replaceChildren(
        h("div.sl.head.one", null, h("span", null, `${race.length} RaceMenu ${race.length === 1 ? "slider" : "sliders"}`), h("span", null, `${face.moved(look)} moved`)),
        h("div.rows", null, out.length ? out : h("div.sub.quiet", { style: "padding: 8px 12px" }, "Nothing found")),
      );
    });
  }

  /** A row for a value kept as the preset has it, not changed here. */
  function kept(label, title = "") {
    return h("div.sl.one", { title }, h("span.cut", null, label), h("span.sub.quiet", null, "Kept"));
  }

  /** Every part known by its form: the look's as read, and every one there was to pick. */
  function known() {
    return [...cs.known.values()];
  }

  /** The left-hand list: the head (parts, colours, weight, what is kept) or the makeup. */
  function side() {
    again("side", (el) => {
      const tab = (id, label) => h("button" + (cs.side === id ? ".on" : ""), { type: "button", onclick: () => ((cs.side = id), side()) }, label);
      const out = cs.side === "makeup" ? makeup() : head();
      el.replaceChildren(
        h("div.side-head", null, h("div.seg.plain", { role: "group", "aria-label": "Show" }, tab("head", "Head"), tab("makeup", "Makeup"))),
        h("div.rows", null, out),
      );
    });
  }

  function head() {
    const a = cs.answer;
    const look = cs.look;
    const all = known();
    const nameOf = (f) => all.find((p) => p.plugin.toLowerCase() === f.plugin.toLowerCase() && p.id === f.id)?.name || face.formText(f);
    const out = [];
    const group = (name, rows) => {
      const shut = cs.shut.has(name);
      out.push(cat(name, shut, side));
      if (!shut) out.push(...rows);
    };
    group(
      "Head parts",
      face.SLOTS.map((slot) => {
        const now = face.partOf(look, all, slot.kind);
        // With none of a kind, the race's own part of that kind is worn, as the preview draws it.
        const b = ctx.pickButton(slot.label, now ? nameOf(now) : "Race default", (b) =>
          ctx.menu(
            b,
            [
              { name: "Race default", value: null, on: !now },
              ...(a.choices[slot.kind] || []).map((c) => ({ name: c.name, note: c.plugin, value: c, on: Boolean(now) && now.plugin === c.plugin && now.id === c.id })),
            ].filter(Boolean),
            (form) => {
              edit(face.setPart(cs.look, all, slot.kind, form));
              side();
            },
            `Search ${slot.label.toLowerCase()}`,
          ),
        );
        return h("div.part", null, b);
      }),
    );
    const unplaced = face.unplaced(a.parts);
    if (unplaced.length)
      group("Not drawn", [
        h("div.sub.quiet.note-line", null, "Kept in the preset while ticked. Their kind isn't known, so a part picked above doesn't take their place: untick any it replaces."),
        ...unplaced.map((p) => {
          const on = look.headParts.some((q) => q.plugin === p.plugin && q.id === p.id);
          return h(
            "button.check" + (on ? ".on" : ""),
            {
              type: "button",
              role: "checkbox",
              "aria-checked": on ? "true" : "false",
              title: `${face.formText(p)}: ${on ? "kept in the preset, and not drawn" : "taken out of the preset"}`,
              onclick: () => {
                edit(face.keepPart(cs.look, p, !on));
                side();
              },
            },
            h("span.box"),
            // The plugin over why it isn't drawn, as a save's row has its place under its name.
            h("span.grow.cut", null, h("span.nm.cut", null, p.plugin), h("span.sub.quiet.cut", null, face.whyNotDrawn(p))),
          );
        }),
      ]);
    const skinMask = a.masks.find((m) => m.kind === face.SKIN_TONE)?.index ?? null;
    group("Colours", [
      h(
        "div.sl.one",
        { title: "Drawn as the game draws a hair colour record's; not checked in the game" },
        h("span.cut", null, "Hair colour"),
        swatch("Hair colour", look.hairRgb ? face.hexOfUnit(look.hairRgb) : "#000000", (hex) => edit({ ...cs.look, hairRgb: face.unitOfHex(hex) })),
      ),
      h(
        "div.sl.one",
        null,
        h("span.cut", null, "Skin tone"),
        swatch("Skin tone", face.skinHex(look), (hex) => edit(face.setSkinTone(cs.look, skinMask, hex))),
      ),
    ]);
    const w = Math.round(look.weight);
    group("Body", [
      track("Weight", "", w, { min: 0, max: 100 }, (x) => String(x), {
        input: (x) => touch({ ...cs.look, weight: x }),
        change: () => build(),
        reset: () => (edit({ ...cs.look, weight: cs.answer.look.weight }), side()),
      }),
    ]);
    const kept = face.keptRows(cs.opened?.kept);
    if (kept.length)
      group("Kept as read", [
        h("div.sub.quiet.note-line", null, "Saved as the preset has them, and not drawn here."),
        ...kept.map((k) => h("div.shape", null, h("span.nm.cut", null, k.label), h("span.sub.quiet", null, k.count))),
      ]);
    return out;
  }

  function makeup() {
    const look = cs.look;
    const seen = new Set();
    // A look names a layer by its mask's number: masks that share one are one row.
    const masks = cs.answer.masks.filter((m) => m.kind !== face.SKIN_TONE && !seen.has(m.index) && seen.add(m.index));
    const row = (m) => {
      const layer = face.layerOf(look, m.index);
      const strength = layer ? Math.round(layer.strength * 100) : 0;
      const out = h("output", null, String(strength));
      const el = h("input", { type: "range", min: 0, max: 100, step: 1, value: strength, "aria-label": `${face.maskName(m.texture)}, strength` });
      const paint = () => el.style.setProperty("--at", `${el.value}%`);
      paint();
      el.addEventListener("input", () => {
        out.textContent = el.value;
        paint();
        touch(face.setLayer(cs.look, m.index, { strength: Number(el.value) / 100 }, m.stored));
      });
      el.addEventListener("change", () => (side(), build()));
      return h(
        "div.sl.tint",
        { title: m.texture },
        h("span.cut", null, face.maskName(m.texture)),
        // A layer not in use shows the colour the preset keeps for it.
        swatch(`${face.maskName(m.texture)}, colour`, layer ? face.hexOfBytes(layer.colour) : m.stored ? face.hexOfBytes(m.stored) : "#ffffff", (hex) =>
          edit(face.setLayer(cs.look, m.index, { hex }, m.stored)),
        ),
        h("span.tr", null, el, out),
      );
    };
    const using = masks.filter((m) => (face.layerOf(look, m.index)?.strength || 0) > 0);
    const rest = masks.filter((m) => !using.includes(m));
    const out = [];
    for (const [name, list] of [
      ["In use", using],
      ["Not in use", rest],
    ]) {
      if (!list.length) continue;
      const shut = cs.shut.has(name);
      out.push(cat(name, shut, side));
      if (!shut) out.push(...list.map(row));
    }
    return out.length ? out : [h("div.sub.quiet", { style: "padding: 8px 12px" }, "This race has no makeup layers.")];
  }

  /** The bar at the foot: what happened, where saves go, and saving. */
  function foot() {
    const el = document.getElementById("foot");
    if (!el || state.mode !== "character") return;
    const ready = Boolean(cs.answer) && !state.busy;
    const read = cs.answer?.notes || [];
    const preview = [...(cs.built?.missing || []).map((m) => `Not found: ${m}`), ...(cs.built?.notes || [])];
    const count = read.length + preview.length;
    const parts = [
      cs.problem
        ? h("span.sub.warn.cut", null, cs.problem)
        : state.busy
          ? h("span.sub.cut", { id: "busy" }, state.busy)
          : cs.saved
            ? h("span.sub.cut", null, cs.saved.into ? `Saved ${cs.saved.file} into ${cs.saved.into}` : `Saved ${cs.saved.file}`)
            : cs.note
              ? h("span.sub.warn.cut", null, cs.note)
              : count
              ? h("span.sub.cut", null, `${count} ${count === 1 ? "note" : "notes"} on what couldn't be used or shown`)
              : "",
      cs.saved
        ? h("button.btn.ghost", { type: "button", onclick: () => notesSheet("What was written", [[null, cs.saved.notes.length ? cs.saved.notes : ["Nothing changed."]]]) }, "What was written…")
        : count > 0 &&
            h(
              "button.btn.ghost",
              {
                type: "button",
                onclick: () =>
                  notesSheet("Notes", [
                    ["This screen", cs.note ? [cs.note] : []],
                    ["The preset", read],
                    ["The preview", preview],
                  ]),
              },
              "Notes…",
            ),
      h("span.grow"),
      (() => {
        const b = ctx.pickButton("Into", state.into ? state.into.name : state.output, ctx.chooseTarget);
        b.id = "status";
        b.title = "The mod presets are saved into";
        b.disabled = Boolean(state.busy);
        return b;
      })(),
      h("button.btn.ghost", { type: "button", disabled: !ready, title: "Save the preset where you pick, outside the mods", onclick: () => save(true) }, "To disk…"),
      cs.saved?.into
        ? h("button.btn.primary", { type: "button", onclick: () => ctx.host.request("apply.review").catch(() => {}) }, "Apply")
        : h("button.btn.primary", { type: "button", disabled: !ready, onclick: () => save(false) }, "Save"),
    ];
    el.replaceChildren(...parts.filter(Boolean));
  }

  function render() {
    const root = document.getElementById("app");
    const a = cs.answer;
    const search = h("input", {
      type: "text",
      placeholder: "Search sliders",
      "aria-label": "Search sliders",
      value: cs.search,
      oninput: () => ((cs.search = search.value), sliders()),
    });
    const preset = ctx.pickButton("Preset", cs.opened?.name || "None", openMenu);
    preset.id = "preset";
    const sex = (female, label) =>
      h("button" + (a?.female === female ? ".on" : ""), { type: "button", onclick: () => a && a.female !== female && previewAs({ female }) }, label);
    root.replaceChildren(
      h(
        "div.bar",
        null,
        ctx.modes(),
        preset,
        a &&
          ctx.pickButton("Race", a.race.name || face.formText(a.race), (b) =>
            ctx.menu(
              b,
              a.races.map((r) => ({ name: r.name, note: r.plugin, value: r, on: r.plugin === a.race.plugin && r.id === a.race.id })),
              (r) => previewAs({ race: { plugin: r.plugin, id: r.id } }),
              "Search races",
            ),
          ),
        a && h("div.seg", { role: "group", "aria-label": "Sex" }, sex(true, "Female"), sex(false, "Male")),
        cs.save &&
          (() => {
            const b = ctx.pickButton("Load order", cs.save.character || cs.save.name, () => pickSave());
            b.title = "The save whose plugins the preset is read and saved with";
            return b;
          })(),
        h("span.grow"),
        a && h("label.search", { style: "flex: 0 1 160px; min-width: 120px", html: ctx.GLASS }, search),
      ),
      a
        ? h(
            "div.main.studio",
            null,
            h("div.card.list", { id: "side" }),
            h("div.view", null, ctx.canvas(), h("span.sub.quiet.note" + (cs.waiting ? ".warn" : ""), { id: "view-note" }, cs.waiting ? WAITS : PREVIEW_ONLY)),
            h("div.card.list", { id: "list" }),
          )
        : h(
            "div.empty",
            null,
            state.busy
              ? h("span", { id: "busy" }, state.busy)
              : cs.opened
                ? [
                    h("strong", null, "No save picked"),
                    h("span", null, cs.problem || "A preset is read and saved with the plugins of a save."),
                    h("button.btn.primary", { type: "button", style: "margin-top: 12px", onclick: () => pickSave() }, "Pick a save…"),
                  ]
                : [
                    h("strong", null, "No preset open"),
                    h("span", null, cs.problem || "Open a RaceMenu preset to change the face it gives."),
                    h("button.btn.primary", { type: "button", style: "margin-top: 12px", onclick: (e) => openMenu(e.currentTarget) }, "Open preset…"),
                  ],
          ),
      h("div.bar", { id: "foot" }),
    );
    if (a) {
      const main = root.querySelector(".main.studio");
      ctx.splits(main, [
        { el: main.querySelector("#side"), edge: "right", key: "side", least: 160 },
        { el: main.querySelector("#list"), edge: "left", key: "sliders", least: 200 },
      ]);
      side();
      sliders();
      // Just read, or come back to from another screen where another
      // project was opened: the preset's character is the project again.
      if (!state.busy && (cs.stale || !state.work?.preset)) {
        const frame = cs.stale === "frame" || !state.shapes.length;
        cs.stale = null;
        queueMicrotask(() => build(frame));
      }
    }
    foot();
  }

  return { render, foot, reset, replaces, letGo, unsaved };
}
