import assert from "node:assert/strict";
import test from "node:test";
import * as face from "../ui/face.js";

// Every value here is made up.

/** A look as the module gives one, made by hand. */
const look = () => ({
  headParts: [
    { plugin: "Base.esm", id: 0x1234 },
    { plugin: "Hair.esp", id: 0x801 },
    { plugin: "Gone.esp", id: 0x123 },
  ],
  faceMorphs: [0, -1, 0.5, null],
  faceParts: [0, -1, 2, 0],
  hairRgb: [51 / 255, 60 / 255, 70 / 255],
  skinTone: null,
  tints: [{ mask: 7, colour: [10, 20, 30, 96], strength: 96 / 255 }],
  weight: 37,
  custom: [
    ["Lip_Width", 0.25],
    ["Face_Width", -0.4],
    ["Zeroed", 0],
  ],
});

/** `presetLook`'s `parts`: the face, a hair, a part whose plugin isn't in the save, and one whose plugin is but isn't installed. */
const parts = [
  { plugin: "Base.esm", id: 0x1234, name: "Face", kind: 1, loaded: true, installed: true },
  { plugin: "Hair.esp", id: 0x801, name: "Long", kind: 3, loaded: true, installed: true },
  { plugin: "Gone.esp", id: 0x123, name: null, kind: null, loaded: false, installed: false },
  { plugin: "Away.esp", id: 0x456, name: null, kind: null, loaded: true, installed: false },
];

test("a slider's value sits on the track in hundredths, and comes back as it went", () => {
  assert.equal(face.toPosition(0.27), 27);
  assert.equal(face.toPosition(-0.4), -40);
  assert.equal(face.toPosition(NaN), 0);
  assert.equal(face.fromPosition(-40), -0.4);
  // A value read from a preset as a float widened to a double lands on its hundredth.
  assert.equal(face.toPosition(Math.fround(0.27)), 27);
  for (let p = -150; p <= 150; p++) assert.equal(face.toPosition(face.fromPosition(p)), p);
});

test("the sign is not decided here: below 0 is the low end as the module names it", () => {
  // A negative value stays negative on the track and back; which morph it
  // turns on is the module's `below`, shown as it is given.
  assert.equal(face.fromPosition(face.toPosition(-0.4)), -0.4);
  assert.equal(face.ends("MadeNarrow", "MadeWide"), "Below 0: MadeNarrow. Above 0: MadeWide.");
  assert.equal(face.ends(null, "MadeUp"), "Below 0: nothing. Above 0: MadeUp.");
});

test("a track is wide enough for where it is", () => {
  assert.deepEqual(face.range(27), { min: -100, max: 100 });
  assert.deepEqual(face.range(250), { min: -100, max: 250 });
  assert.deepEqual(face.range(-180), { min: -180, max: 100 });
  assert.deepEqual(face.range(3, 0, 5), { min: 0, max: 5 });
});

test("a value shows as hundredths, a choice as itself or None, and one JSON couldn't hold as kept", () => {
  assert.equal(face.shown(0.27), "27");
  assert.equal(face.shown(-1, true), "None");
  assert.equal(face.shown(4, true), "4");
  assert.equal(face.shown(null), "Kept");
});

test("a face preset's track goes as far as the race offers, or where it is", () => {
  assert.equal(face.presetTop([1, 2, 5], 2), 5);
  assert.equal(face.presetTop([1, 2, 5], 12), 12);
  assert.equal(face.presetTop([], 0), 31);
});

test("named sliders are set where the look has them, added after, and the look they came from is left as it was", () => {
  const l = look();
  const set = face.setCustom(l, "lip_width", 0.5);
  assert.deepEqual(set.custom[0], ["Lip_Width", 0.5]);
  assert.equal(set.custom.length, 3);
  const added = face.setCustom(l, "New", -0.2);
  assert.deepEqual(added.custom[3], ["New", -0.2]);
  const off = face.setCustom(l, "Face_Width", 0);
  assert.equal(off.custom.length, 3);
  assert.deepEqual(off.custom[1], ["Face_Width", 0]);
  assert.deepEqual(l, look());
  assert.equal(face.moved(l), 2);
  assert.equal(face.moved(off), 1);
});

test("a named slider the preset hadn't got is sent only while it does something, and reset takes it out", () => {
  const read = look();
  const touched = face.setCustom(face.setCustom(read, "New", 0.3), "New", 0);
  // At 0 again it is left out of what is sent: the preset gets no entry for it.
  assert.deepEqual(face.outgoing(touched, read).custom, read.custom);
  assert.equal(face.changed(touched, read), false);
  // One the preset has keeps its entry at 0.
  const zero = face.setCustom(read, "Lip_Width", 0);
  assert.deepEqual(face.outgoing(zero, read).custom[0], ["Lip_Width", 0]);
  assert.equal(face.changed(zero, read), true);
  // Reset: the preset's value back, or one it hadn't got taken out.
  const moved = face.setCustom(face.setCustom(read, "New", 0.3), "Lip_Width", 0.9);
  assert.deepEqual(face.resetCustom(moved, read, "lip_width").custom[0], ["Lip_Width", 0.25]);
  assert.deepEqual(
    face.resetCustom(moved, read, "New").custom.map(([n]) => n),
    ["Lip_Width", "Face_Width", "Zeroed"],
  );
});

test("the race's sliders take the look's values; the look's others are listed apart", () => {
  const sliders = [
    { name: "lip_width", kind: "slider", below: "LipThin", above: "LipWide" },
    { name: "EyeShape", kind: "preset", count: 4 },
  ];
  const { race, other } = face.sliderRows(look(), sliders);
  assert.deepEqual(
    race.map((s) => [s.name, s.value]),
    [
      ["lip_width", 0.25],
      ["EyeShape", 0],
    ],
  );
  assert.deepEqual(
    other.map((s) => [s.name, s.kind, s.value]),
    [
      ["Face_Width", "unknown", -0.4],
      ["Zeroed", "unknown", 0],
    ],
  );
});

test("colours go between #rrggbb, bytes and 0 to 1 without drifting", () => {
  assert.equal(face.hexOfUnit([51 / 255, 60 / 255, 70 / 255]), "#333c46");
  assert.deepEqual(face.bytesOfHex("#333c46"), [51, 60, 70]);
  assert.deepEqual(face.unitOfHex("#ff0080"), [1, 0, 128 / 255]);
  assert.equal(face.hexOfBytes([10, 20, 30, 96]), "#0a141e");
  assert.equal(face.hexOfUnit([2, -1, 0.5]), "#ff0080");
  for (let b = 0; b < 256; b++) {
    const hex = face.hexOfBytes([b, 255 - b, b]);
    assert.equal(face.hexOfUnit(face.unitOfHex(hex)), hex);
  }
});

test("a part is picked into its kind's place, and parts of no known kind are left alone", () => {
  const l = look();
  const hair = { plugin: "Other.esp", id: 0x900, kind: 3 };
  const known = [...parts, hair];
  assert.deepEqual(face.partOf(l, known, 3), { plugin: "Hair.esp", id: 0x801 });
  const picked = face.setPart(l, known, 3, hair);
  assert.deepEqual(picked.headParts, [
    { plugin: "Base.esm", id: 0x1234 },
    { plugin: "Other.esp", id: 0x900 },
    { plugin: "Gone.esp", id: 0x123 },
  ]);
  // A kind the look hasn't got goes after the rest; none takes it out.
  const eyes = face.setPart(l, known, 2, { plugin: "Eyes.esp", id: 1 });
  assert.deepEqual(eyes.headParts.at(-1), { plugin: "Eyes.esp", id: 1 });
  assert.deepEqual(face.setPart(l, known, 3, null).headParts, [l.headParts[0], l.headParts[2]]);
  assert.equal(face.partOf(l, known, 4), null);
});

test("parts of no known kind are listed with why they aren't drawn, and kept or taken out", () => {
  const l = look();
  const listed = face.unplaced(parts);
  assert.deepEqual(listed.map(face.formText), ["Gone.esp|000123", "Away.esp|000456"]);
  assert.deepEqual(listed.map(face.whyNotDrawn), ["Not in this save's plugins", "Not installed"]);
  assert.equal(face.whyNotDrawn({ loaded: true, installed: true, kind: null }), "Not a head part");
  const out = face.keepPart(l, listed[0], false);
  assert.equal(out.headParts.length, 2);
  assert.equal(face.keepPart(out, listed[0], false), out);
  assert.deepEqual(face.keepPart(out, listed[0], true).headParts.at(-1), { plugin: "Gone.esp", id: 0x123 });
});

test("a makeup layer's colour keeps its strength and its strength its colour", () => {
  const l = look();
  const coloured = face.setLayer(l, 7, { hex: "#ff0000" });
  assert.deepEqual(coloured.tints[0], { mask: 7, colour: [255, 0, 0, 96], strength: 96 / 255 });
  const stronger = face.setLayer(coloured, 7, { strength: 0.5 });
  assert.deepEqual(stronger.tints[0].colour, [255, 0, 0, 128]);
  assert.equal(stronger.tints[0].strength, 0.5);
  assert.deepEqual(l.tints, look().tints);
  assert.equal(face.layerOf(l, 7).mask, 7);
  assert.equal(face.layerOf(l, 9), null);
});

test("a layer taken up starts from the colour the preset keeps for it, and put back at nothing is not sent", () => {
  const read = look();
  const stored = [0xab, 0xcd, 0xef];
  const up = face.setLayer(read, 9, { strength: 0.2 }, stored);
  assert.deepEqual(face.layerOf(up, 9), { mask: 9, colour: [0xab, 0xcd, 0xef, 51], strength: 0.2 });
  // 0, then 20, then 0 again: what is sent is the look as read.
  const back = face.setLayer(up, 9, { strength: 0 }, stored);
  assert.deepEqual(face.outgoing(back, read), read);
  assert.equal(face.changed(back, read), false);
  // With no colour kept for it, white.
  assert.deepEqual(face.setLayer(read, 9, { strength: 2 }).tints[1], { mask: 9, colour: [255, 255, 255, 255], strength: 1 });
  // A layer the preset had, at nothing, is sent at nothing: the module writes it as not in use.
  const off = face.setLayer(read, 7, { strength: 0 });
  assert.deepEqual(face.outgoing(off, read).tints, [{ mask: 7, colour: [10, 20, 30, 0], strength: 0 }]);
});

test("the skin tone is its mask's layer at full strength, and the tone that gives", () => {
  const l = look();
  const toned = face.setSkinTone(l, 5, "#c08060");
  assert.deepEqual(face.layerOf(toned, 5), { mask: 5, colour: [0xc0, 0x80, 0x60, 255], strength: 1 });
  assert.deepEqual(toned.skinTone, [0xc0 / 255, 0x80 / 255, 0x60 / 255]);
  assert.equal(face.skinHex(toned), "#c08060");
  // No skin tone mask: the tone alone.
  const alone = face.setSkinTone(l, null, "#c08060");
  assert.deepEqual(alone.tints, l.tints);
  assert.equal(face.skinHex(l), "#7f7f7f");
});

test("what is kept as read is listed by what the preset has, and nothing else", () => {
  const rows = face.keptRows([
    { key: "sculpt", count: 3 },
    { key: "bodyMorphs", count: 12 },
    { key: "transforms", count: 1 },
    { key: "somethingNew", count: 2 },
  ]);
  assert.deepEqual(rows, [
    { key: "sculpt", label: "Sculpting", count: "3 meshes" },
    { key: "bodyMorphs", label: "Body morphs", count: "12 morphs" },
    { key: "transforms", label: "Bone scales", count: "1 entry" },
  ]);
  assert.deepEqual(face.keptRows(undefined), []);
});

test("read as another race, edits are carried but tint layers are the new read's, and nothing is shared", () => {
  const read = look();
  const was = face.setSkinTone(face.setCustom({ ...read, weight: 60 }, "Lip_Width", 0.9), 5, "#102030");
  const fresh = { ...look(), tints: [{ mask: 3, colour: [1, 2, 3, 4], strength: 4 / 255 }], skinTone: [0.5, 0.5, 0.5] };
  const carried = face.carry(was, read, fresh);
  assert.equal(carried.weight, 60);
  assert.deepEqual(carried.custom[0], ["Lip_Width", 0.9]);
  assert.deepEqual(carried.tints, fresh.tints);
  assert.deepEqual(carried.skinTone, [0.5, 0.5, 0.5]);
  assert.equal(face.tintsChanged(was, read), true);
  assert.equal(face.tintsChanged(read, look()), false);
  // Unchanged fields are the fresh read's values, in lists of their own.
  const plain = face.carry(read, read, fresh);
  assert.deepEqual(plain.custom, fresh.custom);
  for (const k of ["custom", "faceMorphs", "faceParts", "headParts", "tints"]) assert.notEqual(plain[k], fresh[k], k);
  // Kept with `tints`, as when the same save is read again.
  assert.deepEqual(face.carry(was, read, fresh, { tints: true }).tints, was.tints);
});

test("open, show as Male, edit, show as Female: the edit stays, and reset goes back to the file's value", () => {
  // The screen's own steps: the look as read, a copy of it to change, and
  // each race or sex read afresh with the changes carried over.
  const file = look();
  let answer = face.copy(file);
  let edited = face.copy(answer);
  const asSex = () => {
    const fresh = face.copy(file);
    const carried = face.carry(edited, answer, fresh);
    answer = fresh;
    edited = carried;
  };
  asSex(); // Male
  edited = face.setFacePart(face.setMorph(edited, 0, 0.4), 0, 3);
  // The look as read is not touched by the edit.
  assert.equal(answer.faceMorphs[0], 0);
  assert.equal(answer.faceParts[0], 0);
  asSex(); // Female
  assert.equal(edited.faceMorphs[0], 0.4);
  assert.equal(edited.faceParts[0], 3);
  edited = face.setFacePart(face.setMorph(edited, 0, answer.faceMorphs[0]), 0, answer.faceParts[0]);
  assert.equal(edited.faceMorphs[0], file.faceMorphs[0]);
  assert.equal(edited.faceParts[0], file.faceParts[0]);
  assert.equal(face.changed(edited, answer), false);
});

test("a look's lists are copied, never changed in place", () => {
  const l = look();
  const m = face.setMorph(l, 1, 0.3);
  const p = face.setFacePart(l, 2, 5);
  assert.deepEqual(l, look());
  assert.equal(m.faceMorphs[1], 0.3);
  assert.equal(p.faceParts[2], 5);
  assert.notEqual(m.faceMorphs, l.faceMorphs);
  assert.equal(face.same({ a: 1, b: [1, { c: 2, d: 3 }] }, { b: [1, { d: 3, c: 2 }], a: 1 }), true);
  assert.equal(face.same({ a: 1 }, { a: 2 }), false);
});

test("a preset's name is one Windows takes, and a changed one isn't saved under its own to begin with", () => {
  assert.equal(face.nameProblem("Mine"), null);
  assert.equal(face.nameProblem("Console"), null);
  assert.equal(face.nameProblem("COM0"), null);
  for (const bad of ["", "a/b", "a|b", ".a", "a.", "a ", "a\u0001b", "a\u007fb", "CON", "con", "nul.jslot", "Com1", "LPT9.txt", "aux.", "x".repeat(81)]) {
    assert.notEqual(face.nameProblem(bad), null, JSON.stringify(bad));
  }
  assert.equal(face.savedName("Mine"), "Mine (edited)");
  const files = [{ path: "Data/SKSE/Plugins/CharGen/Presets/Mine (edited).jslot" }];
  assert.equal(face.hasFile(files, "mine (EDITED).jslot"), true);
  assert.equal(face.hasFile(files, "Mine.jslot"), false);
  assert.equal(face.hasFile(null, "Mine.jslot"), false);
});

test("the edited fields are the look the module takes; values it couldn't read go back as null", () => {
  const l = look();
  const sent = face.outgoing(face.setCustom(l, "Lip_Width", 0.5), l);
  assert.deepEqual(Object.keys(sent).sort(), ["custom", "faceMorphs", "faceParts", "hairRgb", "headParts", "skinTone", "tints", "weight"]);
  assert.equal(sent.faceMorphs[3], null);
  assert.equal(face.FACE_SLIDERS.length, 18);
  assert.deepEqual(
    face.FACE_PRESETS.map((p) => p.at),
    [0, 2, 3],
  );
});
