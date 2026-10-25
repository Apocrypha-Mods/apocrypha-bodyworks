import assert from "node:assert/strict";
import test from "node:test";
import * as keys from "../ui/keys.js";

const codes = { "!": "Digit1", "@": "Digit2", "#": "Digit3", $: "Digit4", "%": "Digit5", " ": "Space" };
const codeOf = (key) => codes[key] || (/^\d$/.test(key) ? `Digit${key}` : /^[a-z]$/i.test(key) ? `Key${key.toUpperCase()}` : key);
const press = (key, mods = {}) => ({ key, code: codeOf(key), ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods });
const ctrl = { ctrlKey: true };
const shift = { shiftKey: true };
const alt = { altKey: true };
const open = { work: true, typing: false, sheet: false, side: "shapes", tool: null };
const action = (set, screen, key, mods, at) => keys.action(set, screen, press(key, mods), { ...open, ...at });

test("a remembered choice names the original's set only when it says so", () => {
  assert.deepEqual(keys.SETS, [["bodyworks", "Bodyworks"], ["outfit", "Outfit Studio"]]);
  assert.equal(keys.chosen("outfit"), "outfit");
  for (const other of ["bodyworks", null, undefined, "", "Outfit", "outfit studio"]) assert.equal(keys.chosen(other), "bodyworks");
});

test("Bodyworks' digits take the brushes in the order they were always shown", () => {
  assert.deepEqual(
    keys.BRUSHES.map(([id]) => id),
    ["inflate", "deflate", "move", "smooth", "erase", "mask", "weights", "colour", "edge", "place", "pose"],
  );
  for (let n = 1; n <= 9; n++) assert.deepEqual(action("bodyworks", "studio", String(n)), { do: "hold", tool: keys.BRUSHES[n - 1][0] }, `key ${n}`);
  assert.equal(action("bodyworks", "studio", "0"), null);
});

// [keys, held, what it does] in Bodyworks' own set, as the handlers it replaced had them.
const OWN = [
  ["q", {}, { do: "hold", tool: null }],
  ["Q", shift, { do: "hold", tool: null }],
  ["[", {}, { do: "size", by: -5 }],
  ["]", {}, { do: "size", by: 5 }],
  ["-", {}, { do: "strength", by: -5 }],
  ["=", {}, { do: "strength", by: 5 }],
  ["+", shift, { do: "strength", by: 5 }],
  ["f", {}, { do: "frame" }],
  ["x", {}, { do: "mirror" }],
  ["h", {}, { do: "hide" }],
  ["g", {}, { do: "ghost" }],
  ["w", {}, { do: "wire" }],
  ["t", {}, { do: "textures" }],
  ["s", ctrl, { do: "saveProject" }],
  ["s", { metaKey: true }, { do: "saveProject" }],
  ["S", { ctrlKey: true, shiftKey: true }, { do: "saveProject" }],
  ["s", { ctrlKey: true, altKey: true }, { do: "saveProject" }],
  ["e", ctrl, { do: "exportMesh" }],
  ["e", { metaKey: true }, { do: "exportMesh" }],
  ["z", ctrl, { do: "undo", again: false }],
  ["z", { metaKey: true }, { do: "undo", again: false }],
  ["Z", { ctrlKey: true, shiftKey: true }, { do: "undo", again: true }],
  ["Z", { metaKey: true, shiftKey: true }, { do: "undo", again: true }],
];

test("each of Bodyworks' own keys does what it always did", () => {
  for (const [key, mods, want] of OWN) assert.deepEqual(action("bodyworks", "studio", key, mods), want, `${JSON.stringify(mods)} ${key}`);
});

test("the original's keys do nothing in Bodyworks' own set", () => {
  const theirs = [
    ["l", {}],
    ["e", {}],
    ["c", {}],
    ["v", {}],
    ["a", {}],
    ["d", {}],
    ["b", {}],
    ["B", shift],
    ["R", shift],
    ["!", shift],
    ["%", shift],
    ["Delete", {}],
    ["Delete", shift],
    ["Delete", ctrl],
    ["F2", {}],
    ["Tab", {}],
    [" ", {}],
    [" ", ctrl],
    ["y", ctrl],
    ["n", ctrl],
    ["o", ctrl],
    ["w", ctrl],
    ["a", ctrl],
    ["i", ctrl],
    ["q", alt],
    ["1", alt],
    ["Escape", {}],
  ];
  for (const [key, mods] of theirs) assert.equal(action("bodyworks", "studio", key, mods), null, `${JSON.stringify(mods)} ${key}`);
  for (const [key, mods] of OWN) assert.equal(action("bodyworks", "bodyslide", key, mods), null, `BodySlide: ${JSON.stringify(mods)} ${key}`);
});

test("Bodyworks' keys keep out of fields, sheets, and a studio with nothing open", () => {
  const plain = OWN.filter(([, mods]) => !mods.ctrlKey && !mods.metaKey);
  for (const at of [{ typing: true }, { sheet: true }, { work: false }]) {
    for (const [key, mods] of plain) assert.equal(action("bodyworks", "studio", key, mods, at), null, `${JSON.stringify(at)} ${key}`);
  }
  // Saving and exporting want a project and no sheet, but go from a field as they always did.
  for (const at of [{ sheet: true }, { work: false }]) {
    assert.equal(action("bodyworks", "studio", "s", ctrl, at), null);
    assert.equal(action("bodyworks", "studio", "e", ctrl, at), null);
  }
  assert.deepEqual(action("bodyworks", "studio", "s", ctrl, { typing: true }), { do: "saveProject" });
  // Undo never goes into a field, but needs neither a project nor the sheets shut.
  assert.equal(action("bodyworks", "studio", "z", ctrl, { typing: true }), null);
  assert.deepEqual(action("bodyworks", "studio", "z", ctrl, { sheet: true }), { do: "undo", again: false });
  assert.deepEqual(action("bodyworks", "studio", "z", ctrl, { work: false }), { do: "undo", again: false });
  assert.equal(keys.action("bodyworks", "studio", { ctrlKey: true }, open), null);
});

// [keys, held, on screen, what it does] in the original's set, as its documentation lists them.
const STUDIO = [
  ["n", ctrl, {}, { do: "newProject" }],
  ["o", ctrl, {}, { do: "openProject" }],
  ["w", ctrl, {}, { do: "closeProject" }],
  ["s", ctrl, {}, { do: "saveProject" }],
  ["S", { ctrlKey: true, shiftKey: true }, {}, { do: "saveProject" }],
  ["e", ctrl, {}, { do: "exportMesh" }],
  ["Delete", {}, {}, { do: "deleteShape" }],
  ["F2", {}, {}, { do: "renameShape" }],
  ["Tab", {}, {}, { do: "sliderSettings" }],
  ["Delete", ctrl, {}, { do: "deleteSlider" }],
  [" ", ctrl, {}, { do: "editSlider" }],
  ["0", {}, {}, { do: "hold", tool: null }],
  ["1", {}, {}, { do: "hold", tool: "mask" }],
  ["2", {}, {}, { do: "hold", tool: "inflate" }],
  ["3", {}, {}, { do: "hold", tool: "deflate" }],
  ["4", {}, {}, { do: "hold", tool: "move" }],
  ["5", {}, {}, { do: "hold", tool: "smooth" }],
  ["6", {}, { side: "bones" }, { do: "hold", tool: "weights" }],
  ["7", {}, {}, { do: "hold", tool: "colour" }],
  ["8", {}, {}, { do: "hold", tool: "colour" }],
  [" ", {}, {}, { do: "brushSettings" }],
  ["a", ctrl, {}, { do: "clearMask" }],
  ["i", ctrl, {}, { do: "invertMask" }],
  ["a", {}, {}, { do: "spreadMask", less: true }],
  ["d", {}, {}, { do: "spreadMask", less: false }],
  ["z", ctrl, {}, { do: "undo", again: false }],
  ["y", ctrl, {}, { do: "undo", again: true }],
  ["x", {}, {}, { do: "mirror" }],
  ["c", {}, {}, { do: "connected" }],
  ["f", {}, {}, { do: "hold", tool: "place" }],
  ["v", {}, { tool: "colour" }, { do: "pickColour" }],
  ["Delete", shift, {}, { do: "deleteVertices" }],
  ["S", shift, {}, { do: "separate" }],
  ["w", {}, {}, { do: "wire" }],
  ["t", {}, {}, { do: "textures" }],
  ["l", {}, {}, { do: "lighting" }],
  ["e", {}, {}, { do: "hide" }],
  ["g", {}, {}, { do: "grid" }],
  ["!", shift, {}, { do: "face", side: "front" }],
  ["@", shift, {}, { do: "face", side: "back" }],
  ["#", shift, {}, { do: "face", side: "left" }],
  ["$", shift, {}, { do: "face", side: "right" }],
  ["%", shift, {}, { do: "perspective" }],
  ["R", shift, {}, { do: "pivot" }],
  ["B", shift, {}, { do: "bones" }],
];

const BODYSLIDE = [
  ["F5", {}, { do: "refresh" }],
  ["PageUp", {}, { do: "preset", by: -1 }],
  ["PageDown", {}, { do: "preset", by: 1 }],
  ["PageUp", ctrl, { do: "outfit", by: -1 }],
  ["PageDown", ctrl, { do: "outfit", by: 1 }],
  ["a", ctrl, { do: "clearFilters" }],
  ["o", ctrl, { do: "toStudio" }],
  ["s", ctrl, { do: "savePreset", asNew: false }],
  ["s", { ctrlKey: true, altKey: true }, { do: "savePreset", asNew: true }],
  ["g", ctrl, { do: "groups" }],
  ["w", {}, { do: "wire" }],
  ["t", {}, { do: "textures" }],
  ["l", {}, { do: "lighting" }],
];

test("each key the original documents does what it says, in Outfit Studio and in BodySlide", () => {
  for (const [key, mods, at, want] of STUDIO) {
    assert.deepEqual(action("outfit", "studio", key, mods, at), want, `Outfit Studio: ${JSON.stringify(mods)} ${key}`);
    // The Command key is taken for Ctrl.
    if (mods.ctrlKey) assert.deepEqual(action("outfit", "studio", key, { ...mods, ctrlKey: false, metaKey: true }, at), want, `Outfit Studio: Meta ${key}`);
  }
  for (const [key, mods, want] of BODYSLIDE) {
    assert.deepEqual(action("outfit", "bodyslide", key, mods), want, `BodySlide: ${JSON.stringify(mods)} ${key}`);
    if (mods.ctrlKey) assert.deepEqual(action("outfit", "bodyslide", key, { ...mods, ctrlKey: false, metaKey: true }), want, `BodySlide: Meta ${key}`);
  }
  // A project is opened or begun with none open.
  assert.deepEqual(action("outfit", "studio", "n", ctrl, { work: false }), { do: "newProject" });
  assert.deepEqual(action("outfit", "studio", "o", ctrl, { work: false }), { do: "openProject" });
});

test("keys the original has not got, or Bodyworks has not, do nothing in the original's set", () => {
  const none = [
    ["q", {}],
    ["b", {}],
    ["h", {}],
    ["9", {}],
    ["[", {}],
    ["]", {}],
    ["-", {}],
    ["=", {}],
    ["F5", {}],
    ["Escape", {}],
    ["e", { ctrlKey: true, altKey: true }],
    ["A", { ctrlKey: true, shiftKey: true }],
    ["Z", { ctrlKey: true, shiftKey: true }],
    ["N", shift],
    ["Q", shift],
    ["^", { shiftKey: true, code: "Digit6" }],
    ["p", ctrl],
    ["w", alt],
    ["1", alt],
    ["Delete", alt],
  ];
  for (const [key, mods] of none) assert.equal(action("outfit", "studio", key, mods), null, `Outfit Studio: ${JSON.stringify(mods)} ${key}`);
  for (const [key, mods] of [["p", ctrl], ["e", {}], ["1", {}], ["z", ctrl], ["W", shift], ["w", alt], ["F5", ctrl], ["Delete", {}]]) {
    assert.equal(action("outfit", "bodyslide", key, mods), null, `BodySlide: ${JSON.stringify(mods)} ${key}`);
  }
});

test("the original's keys keep out of fields and sheets", () => {
  for (const at of [{ typing: true }, { sheet: true }]) {
    for (const [key, mods, own, want] of STUDIO) {
      assert.notEqual(action("outfit", "studio", key, mods, own), null, `${key} with nothing in the way`);
      assert.equal(action("outfit", "studio", key, mods, { ...own, ...at }), null, `${JSON.stringify(at)} ${key} ${want.do}`);
    }
    for (const [key, mods] of BODYSLIDE) assert.equal(action("outfit", "bodyslide", key, mods, at), null, `BodySlide: ${JSON.stringify(at)} ${key}`);
  }
  assert.equal(keys.action("outfit", "studio", { code: "KeyW" }, open), null);
});

test("the weight brush's digit is the bones tab's, and V picks a colour only with the colour brush", () => {
  assert.deepEqual(action("outfit", "studio", "6", {}, { side: "bones" }), { do: "hold", tool: "weights" });
  for (const side of ["shapes", "sliders", undefined]) assert.equal(action("outfit", "studio", "6", {}, { side }), null, `the ${side} tab`);
  assert.deepEqual(action("outfit", "studio", "v", {}, { tool: "colour" }), { do: "pickColour" });
  for (const tool of [null, "mask", "weights"]) assert.equal(action("outfit", "studio", "v", {}, { tool }), null, `with ${tool}`);
  // The colour brush is the colour brush whichever tab is shown.
  assert.deepEqual(action("outfit", "studio", "7", {}, { side: "bones" }), { do: "hold", tool: "colour" });
});

test("Shift and a digit face the view by where the key is, whatever it says", () => {
  // A British keyboard's Shift+2 says ", a French one's 1 is & unshifted and 1 shifted.
  assert.deepEqual(keys.action("outfit", "studio", { key: '"', code: "Digit2", shiftKey: true }, open), { do: "face", side: "back" });
  assert.deepEqual(keys.action("outfit", "studio", { key: "1", code: "Digit1", shiftKey: true }, open), { do: "face", side: "front" });
  assert.deepEqual(keys.action("outfit", "studio", { key: "5", code: "Digit5", shiftKey: true }, open), { do: "perspective" });
  assert.equal(keys.action("outfit", "studio", { key: "!", code: "Numpad1", shiftKey: true }, open), null);
  // Without Shift the same key is a brush.
  assert.deepEqual(keys.action("outfit", "studio", { key: "1", code: "Digit1" }, open), { do: "hold", tool: "mask" });
});

test("the wheel sizes the brush with S held and steps the sliders with Ctrl, only in the original's set", () => {
  const roll = (deltaY, mods = {}) => ({ deltaY, ctrlKey: false, metaKey: false, ...mods });
  assert.deepEqual(keys.wheel("outfit", roll(100), true), { do: "size", by: -5 });
  assert.deepEqual(keys.wheel("outfit", roll(-3), true), { do: "size", by: 5 });
  assert.deepEqual(keys.wheel("outfit", roll(100, ctrl), false), { do: "nextSlider", by: 1 });
  assert.deepEqual(keys.wheel("outfit", roll(-100, { metaKey: true }), false), { do: "nextSlider", by: -1 });
  assert.deepEqual(keys.wheel("outfit", roll(100, ctrl), true), { do: "nextSlider", by: 1 });
  assert.equal(keys.wheel("outfit", roll(100), false), null);
  assert.equal(keys.wheel("outfit", roll(0), true), null);
  assert.equal(keys.wheel("outfit", roll(0, ctrl), false), null);
  for (const [e, held] of [[roll(100), true], [roll(100, ctrl), false]]) assert.equal(keys.wheel("bodyworks", e, held), null);
});

test("a press on the view brushes, turns or pans as each set has it", () => {
  const at = (button, mods = {}) => ({ button, shiftKey: false, altKey: false, ...mods });
  const bodyworks = [
    [at(0), { brush: true, then: "turn" }],
    [at(0, shift), { brush: false, then: "pan" }],
    [at(1), { brush: false, then: "pan" }],
    [at(2), { brush: false, then: "pan" }],
    [at(2, alt), { brush: false, then: "pan" }],
  ];
  for (const [e, want] of bodyworks) assert.deepEqual(keys.press("bodyworks", e), want, `Bodyworks: ${JSON.stringify(e)}`);
  const outfit = [
    [at(0), { brush: true, then: "turn" }],
    [at(0, shift), { brush: true, then: "turn" }],
    [at(1), { brush: false, then: "pan" }],
    [at(2), { brush: false, then: "turn" }],
    [at(2, shift), { brush: false, then: "pan" }],
    [at(2, alt), { brush: false, then: "pivot" }],
    [at(2, { shiftKey: true, altKey: true }), { brush: false, then: "pan" }],
  ];
  for (const [e, want] of outfit) assert.deepEqual(keys.press("outfit", e), want, `Outfit Studio: ${JSON.stringify(e)}`);
});

test("keys held as a stroke starts change the brush only in the original's set, and only for the brushes", () => {
  const held = (mods = {}) => ({ ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods });
  const brushed = ["inflate", "deflate", "move", "smooth", "erase", "mask", "weights", "colour"];
  for (const tool of brushed) {
    assert.deepEqual(keys.stroke("outfit", tool, held(ctrl)), { tool: "mask", alt: false, smooth: false }, `Ctrl with ${tool}`);
    assert.deepEqual(keys.stroke("outfit", tool, held({ metaKey: true })), { tool: "mask", alt: false, smooth: false }, `Meta with ${tool}`);
    assert.deepEqual(keys.stroke("outfit", tool, held({ ctrlKey: true, altKey: true })), { tool: "mask", alt: true, smooth: false }, `Ctrl+Alt with ${tool}`);
    assert.deepEqual(keys.stroke("outfit", tool, held()), { tool, alt: false, smooth: false }, `${tool} alone`);
  }
  assert.deepEqual(keys.stroke("outfit", "inflate", held(alt)), { tool: "deflate", alt: false, smooth: false });
  assert.deepEqual(keys.stroke("outfit", "deflate", held(alt)), { tool: "inflate", alt: false, smooth: false });
  for (const tool of ["mask", "weights", "colour", "move"]) assert.deepEqual(keys.stroke("outfit", tool, held(alt)), { tool, alt: true, smooth: false }, `Alt with ${tool}`);
  assert.deepEqual(keys.stroke("outfit", "weights", held(shift)), { tool: "weights", alt: false, smooth: true });
  assert.deepEqual(keys.stroke("outfit", "inflate", held(shift)), { tool: "inflate", alt: false, smooth: false });
  for (const tool of ["edge", "place", "pose", null]) {
    for (const mods of [ctrl, { ctrlKey: true, altKey: true }, shift, {}]) {
      assert.deepEqual(keys.stroke("outfit", tool, held(mods)), { tool, alt: Boolean(mods.altKey), smooth: false }, `${JSON.stringify(mods)} with ${tool}`);
    }
  }
  assert.deepEqual(keys.stroke("outfit", "pose", held(alt)), { tool: "pose", alt: true, smooth: false });
  // Bodyworks' own: the brush in hand, Alt its other way, and nothing else.
  for (const tool of [...brushed, "edge", "place", "pose", null]) {
    for (const mods of [ctrl, { ctrlKey: true, altKey: true }, shift, alt, {}]) {
      assert.deepEqual(keys.stroke("bodyworks", tool, held(mods)), { tool, alt: Boolean(mods.altKey), smooth: false }, `Bodyworks: ${JSON.stringify(mods)} with ${tool}`);
    }
  }
});
