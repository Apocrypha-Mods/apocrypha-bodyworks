// A RaceMenu preset's face as the character screen edits it: which parts it
// picks, how its numbers sit on the sliders, its colours, and what it keeps
// as read. Nothing here touches the page, so node can test it.
//
// A look is never changed in place: each change here makes a new one, its
// lists copied, so the look as read (and the one before a change) stays as
// it was. A look is as the module gives it (`presetLook`'s `look`) and takes it
// back (`presetCharacter`, `presetWrite`): `headParts` ({ plugin, id }),
// `faceMorphs`, `faceParts`, `hairRgb` and `skinTone` (0 to 1),
// `tints` ({ mask, colour: [r, g, b, a] bytes, strength 0 to 1 }), `weight`
// and `custom` ([name, value]). A number JSON can't hold comes as null and
// goes back as null, which the module reads as the preset's own.
//
// Which morph a slider turns on below nought and above it is not decided
// here: the module says it (`faceSliders`, and each RaceMenu slider's
// `below` and `above`), from the one place each rule is kept.

/** The head parts a look picks, by the `PNAM` of their record, in the order shown. UBE's lashes are facial hair (`PNAM` 4). */
export const SLOTS = [
  { kind: 3, label: "Hair" },
  { kind: 2, label: "Eyes" },
  { kind: 6, label: "Brows" },
  { kind: 4, label: "Facial hair" },
  { kind: 5, label: "Scars" },
];

/** The game's face sliders, in the order a look holds them; the 19th is kept as read. */
export const FACE_SLIDERS = [
  "Nose length",
  "Nose height",
  "Jaw height",
  "Jaw width",
  "Jaw forward",
  "Cheek height",
  "Cheek width",
  "Eye height",
  "Eye spacing",
  "Brow height",
  "Brow width",
  "Brow forward",
  "Mouth height",
  "Mouth forward",
  "Chin width",
  "Chin length",
  "Bite",
  "Eye depth",
];

/** The face presets, by their place in a look; the second is unused and kept as read. */
export const FACE_PRESETS = [
  { at: 0, label: "Nose" },
  { at: 2, label: "Eyes" },
  { at: 3, label: "Lips" },
];

/** What the screen keeps as read, by the key the preset has it under, with what each count is of. */
export const KEPT = {
  sculpt: ["Sculpting", "mesh", "meshes"],
  overrides: ["Overlays", "entry", "entries"],
  transforms: ["Bone scales", "entry", "entries"],
  faceTextures: ["Face textures", "entry", "entries"],
  bodyMorphs: ["Body morphs", "morph", "morphs"],
};

/** The kept sections a preset has (`presetOpen`'s `kept`), as rows: `{ label, count }` with the count in words. */
export function keptRows(kept) {
  return (kept || [])
    .filter((k) => KEPT[k.key])
    .map((k) => {
      const [label, one, many] = KEPT[k.key];
      return { key: k.key, label, count: `${k.count} ${k.count === 1 ? one : many}` };
    });
}

// ── Sliders ─────────────────────────────────────────────────────────────────

/** A slider's value where the track has it: hundredths, so -1 to 1 is -100 to 100. */
export function toPosition(value) {
  return Number.isFinite(value) ? Math.round(value * 100) : 0;
}

/** A track's position as the value a look holds. */
export function fromPosition(position) {
  return position / 100;
}

/** The track's ends: `low` to `high`, wide enough for where it is. */
export function range(position, low = -100, high = 100) {
  return { min: Math.min(low, position), max: Math.max(high, position) };
}

/** What a track shows of a value: hundredths as a whole number, or for a choice the choice, "None" below 0. */
export function shown(value, choice = false) {
  if (value === null || value === undefined || !Number.isFinite(value)) return "Kept";
  if (choice) return value < 0 ? "None" : String(Math.round(value));
  return String(toPosition(value));
}

/** A slider's tooltip: which morph it turns on each side of nought, as the module has it. */
export function ends(below, above) {
  const side = (where, morph) => (morph ? `${where} 0: ${morph}.` : `${where} 0: nothing.`);
  return `${side("Below", below)} ${side("Above", above)}`;
}

/** The highest choice a face preset's track offers: the race's highest, or 31 when it says none, and never below where it is. */
export function presetTop(offered, value) {
  const top = offered && offered.length ? Math.max(...offered) : 31;
  return Math.max(top, Number.isFinite(value) ? value : 0);
}

/** A look's named slider: its value, or undefined when the look has none of that name. Names compare without case. */
export function customValue(look, name) {
  const lower = name.toLowerCase();
  const found = look.custom.find(([n]) => n.toLowerCase() === lower);
  return found ? found[1] : undefined;
}

/**
 * The look with named slider `name` at `value`: set where the look has it,
 * else added after the rest. A slider is never taken out, as the preset
 * keeps every one it has: at 0 it does nothing.
 */
export function setCustom(look, name, value) {
  if (!look.custom) return look;
  const lower = name.toLowerCase();
  const at = look.custom.findIndex(([n]) => n.toLowerCase() === lower);
  const custom = look.custom.map((c) => [...c]);
  if (at >= 0) custom[at][1] = value;
  else custom.push([name, value]);
  return { ...look, custom };
}

/** The look with named slider `name` back as the preset has it: its value, or, one the preset hadn't got, taken out. */
export function resetCustom(look, read, name) {
  const was = customValue(read, name);
  if (was !== undefined) return setCustom(look, name, was);
  const lower = name.toLowerCase();
  return { ...look, custom: look.custom.filter(([n]) => n.toLowerCase() !== lower).map((c) => [...c]) };
}

/** How many of a look's named sliders do something. */
export function moved(look) {
  return look.custom.filter(([, v]) => v !== null && v !== 0).length;
}

/**
 * The rows of the slider list: the race's RaceMenu sliders with the look's
 * values (those the look hasn't got at 0), then the look's own the race's
 * slider files don't have. `sliders` is `presetLook`'s.
 */
export function sliderRows(look, sliders) {
  const known = new Set(sliders.map((s) => s.name.toLowerCase()));
  const race = sliders.map((s) => ({ ...s, value: customValue(look, s.name) ?? 0 }));
  const other = look.custom
    .filter(([n]) => !known.has(n.toLowerCase()))
    .map(([name, value]) => ({ name, kind: "unknown", value }));
  return { race, other };
}

/**
 * The look read again as another race or sex (`fresh`), with what was
 * changed on the one before (`was`, read as `read`) carried over: the head
 * parts, sliders, presets, hair colour and weight. Tint layers and the skin
 * tone are the new read's: a layer is named by its race's mask number, so
 * one of the last race means nothing to this one.
 */
export function carry(was, read, fresh, { tints = false } = {}) {
  const kept = copy(fresh);
  const keys = ["headParts", "faceMorphs", "faceParts", "hairRgb", "weight", "custom", ...(tints ? ["tints", "skinTone"] : [])];
  for (const k of keys) if (!same(was[k], read[k])) kept[k] = copy(was[k]);
  return kept;
}

/** A deep copy of what JSON holds. */
export function copy(v) {
  return v === undefined ? v : JSON.parse(JSON.stringify(v));
}

/** Whether two things JSON holds are alike, whatever the order of their keys. */
export function same(a, b) {
  const plain = (v) => (Array.isArray(v) ? v.map(plain) : v && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, plain(v[k])])) : v);
  return JSON.stringify(plain(a)) === JSON.stringify(plain(b));
}

/** The look with face slider `i` at `value`. */
export function setMorph(look, i, value) {
  const faceMorphs = [...look.faceMorphs];
  faceMorphs[i] = value;
  return { ...look, faceMorphs };
}

/** The look with face preset `at` at `value` (-1 none). */
export function setFacePart(look, at, value) {
  const faceParts = [...look.faceParts];
  faceParts[at] = value;
  return { ...look, faceParts };
}

/**
 * The look as the module is sent it: a makeup layer the preset had no layer
 * of, at no strength again, is left out, so the preset keeps that entry as
 * it was; and so is a named slider the preset hadn't got, at 0 again.
 */
export function outgoing(look, read) {
  const had = new Set(read.tints.map((t) => t.mask));
  const named = new Set(read.custom.map(([n]) => n.toLowerCase()));
  return copy({
    ...look,
    tints: look.tints.filter((t) => had.has(t.mask) || t.strength > 0),
    custom: look.custom.filter(([n, v]) => named.has(n.toLowerCase()) || v !== 0),
  });
}

/** Whether the look, as it would be sent, differs from the one read. */
export function changed(look, read) {
  return !same(outgoing(look, read), read);
}

/** Whether a look's tint layers or skin tone were changed from the look it was read as. */
export function tintsChanged(was, read) {
  return JSON.stringify([was.tints, was.skinTone]) !== JSON.stringify([read.tints, read.skinTone]);
}

// ── Colours ─────────────────────────────────────────────────────────────────

const byte = (v) => Math.round(Math.max(0, Math.min(1, v)) * 255);

/** Red, green and blue from 0 to 1 as "#rrggbb", each to the nearest of 255 steps. */
export function hexOfUnit(rgb) {
  return "#" + rgb.map((v) => byte(v).toString(16).padStart(2, "0")).join("");
}

/** "#rrggbb" as bytes. */
export function bytesOfHex(hex) {
  return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
}

/** "#rrggbb" as red, green and blue from 0 to 1. */
export function unitOfHex(hex) {
  return bytesOfHex(hex).map((b) => b / 255);
}

/** Bytes as "#rrggbb". */
export function hexOfBytes(rgb) {
  return "#" + rgb.slice(0, 3).map((b) => b.toString(16).padStart(2, "0")).join("");
}

// ── Head parts ──────────────────────────────────────────────────────────────

const sameForm = (a, b) => a.plugin.toLowerCase() === b.plugin.toLowerCase() && a.id === b.id;

/**
 * The part a look has for a slot, from `presetLook`'s `parts` (each with
 * its record's `kind`, null when its plugin isn't loaded), or null.
 */
export function partOf(look, parts, kind) {
  return look.headParts.find((p) => parts.some((q) => sameForm(p, q) && q.kind === kind)) || null;
}

/**
 * The look with `form` in place of its part of `kind` (every part of that
 * kind, where the preset had more than one), or with none of that kind
 * when `form` is null. A new kind goes after the rest. Parts whose plugin
 * isn't loaded are left alone: their kind isn't known.
 */
export function setPart(look, parts, kind, form) {
  const ofKind = (p) => parts.some((q) => sameForm(p, q) && q.kind === kind);
  const at = look.headParts.findIndex(ofKind);
  const headParts = [];
  look.headParts.forEach((p, i) => {
    if (!ofKind(p)) headParts.push(p);
    else if (i === at && form) headParts.push({ plugin: form.plugin, id: form.id });
  });
  if (at < 0 && form) headParts.push({ plugin: form.plugin, id: form.id });
  return { ...look, headParts };
}

/** The look's parts as read that are of no known kind, so in no slot (`presetLook`'s `parts`), whether still in the look or not. */
export function unplaced(parts) {
  return parts.filter((p) => p.kind === null);
}

/** Why a part of no known kind isn't drawn, as is true of it: its plugin isn't in the save's, isn't installed, or holds no such head part. */
export function whyNotDrawn(p) {
  if (!p.loaded) return "Not in this save's plugins";
  if (!p.installed) return "Not installed";
  return "Not a head part";
}

/** The look with a part whose plugin isn't loaded kept (`keep`) or taken out. */
export function keepPart(look, part, keep) {
  const has = look.headParts.some((p) => sameForm(p, part));
  if (keep === has) return look;
  return { ...look, headParts: keep ? [...look.headParts, { plugin: part.plugin, id: part.id }] : look.headParts.filter((p) => !sameForm(p, part)) };
}

/** A form as it is written in a preset: "Plugin.esp|001234". */
export function formText(f) {
  return `${f.plugin}|${f.id.toString(16).toUpperCase().padStart(6, "0")}`;
}

// ── Tint layers ─────────────────────────────────────────────────────────────

/** `TINP` of the skin tone mask. */
export const SKIN_TONE = 6;

/** A mask's name as shown: its file's name, without the folder or `.dds`. */
export function maskName(texture) {
  return texture.split(/[\\/]/).pop().replace(/\.dds$/i, "");
}

/** The look's layer of mask `index` (its `TINI`), or null. */
export function layerOf(look, index) {
  return look.tints.find((t) => t.mask === index) || null;
}

/**
 * The look with mask `index`'s layer changed: its colour ("#rrggbb") and
 * its strength (0 to 1), each where given. A layer the look hasn't got is
 * added, with the colour the preset keeps for the mask (`stored`, bytes),
 * else white, until given one; one at no strength stays, and the
 * preset keeps its colour with no strength. The colour's fourth byte
 * follows the strength.
 */
export function setLayer(look, index, { hex, strength } = {}, stored = null) {
  const tints = look.tints.map((t) => ({ ...t, colour: [...t.colour] }));
  let t = tints.find((x) => x.mask === index);
  if (!t) {
    t = { mask: index, colour: [...(stored || [255, 255, 255]).slice(0, 3), 0], strength: 0 };
    tints.push(t);
  }
  if (hex !== undefined) t.colour = [...bytesOfHex(hex), t.colour[3]];
  // The colour's fourth byte is the strength as a preset writes it, as the module reads it.
  if (strength !== undefined) {
    t.strength = Math.max(0, Math.min(1, strength));
    t.colour[3] = Math.round(t.strength * 255);
  }
  return { ...look, tints };
}

/**
 * The look with skin tone `hex`: the skin tone mask's layer at that colour
 * and full strength, and the skin tone the same, which is what that layer
 * gives at full strength. Without a skin tone mask, the skin tone alone.
 */
export function setSkinTone(look, skinMask, hex) {
  const tone = unitOfHex(hex);
  if (skinMask === null || skinMask === undefined) return { ...look, skinTone: tone };
  const withLayer = setLayer(look, skinMask, { hex, strength: 1 });
  return { ...withLayer, skinTone: tone };
}

/** The skin tone as "#rrggbb": the look's, else mid grey, the tone with no layer. */
export function skinHex(look) {
  return hexOfUnit(look.skinTone || [127 / 255, 127 / 255, 127 / 255]);
}

// ── Saving ──────────────────────────────────────────────────────────────────

/** Names Windows keeps for devices, with or without an extension. */
const DEVICES = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i;

/** What is wrong with a preset's file name, or null. */
export function nameProblem(name) {
  if (!name) return "A preset needs a name.";
  if (name.length > 80) return "A name of 80 characters at most.";
  if (/[\\/:*?"<>|]/.test(name) || name.startsWith(".")) return 'A name, without \\ / : * ? " < > |';
  if (/[\u0000-\u001f\u007f]/.test(name)) return "A name without control characters.";
  if (/[. ]$/.test(name)) return "A name can't end in a dot or a space.";
  if (DEVICES.test(name)) return "Windows keeps that name for a device.";
  return null;
}

/** The name a changed preset is saved under to begin with: not the one it was opened as, which a mod of the same name would hide or be hidden by. */
export function savedName(name) {
  return `${name} (edited)`;
}

/** Whether `files` (`files.list`'s) has a preset of the name `file`, in any case. */
export function hasFile(files, file) {
  const lower = file.toLowerCase();
  return (files || []).some((f) => f.path.split("/").pop().toLowerCase() === lower);
}
