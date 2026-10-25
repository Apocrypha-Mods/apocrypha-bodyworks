// Which key does what. Two sets: Bodyworks' own, and the one the original
// two programs have, for hands that learnt them there. Nothing here touches
// the page: it is told the key and what is on screen, and names an action
// for `app.js` to run. The original's set is as its user documentation gives
// it (the wiki's Keyboard Shortcuts, Brushes and Tools pages, and its
// changelog where that is newer); the program itself was never looked into.

/** The brushes, in the order they are shown; Bodyworks' digits take them in this order. */
export const BRUSHES = [
  ["inflate", "Inflate"],
  ["deflate", "Deflate"],
  ["move", "Move"],
  ["smooth", "Smooth"],
  ["erase", "Erase"],
  ["mask", "Mask"],
  ["weights", "Weights"],
  ["colour", "Colour"],
  ["edge", "Edges"],
  ["place", "Place"],
  ["pose", "Pose"],
];

/** The two sets, by the name they are remembered under, and as they are shown. */
export const SETS = [
  ["bodyworks", "Bodyworks"],
  ["outfit", "Outfit Studio"],
];

/** The set a remembered choice names: Bodyworks' own unless it is the original's. */
export function chosen(remembered) {
  return remembered === "outfit" ? "outfit" : "bodyworks";
}

/**
 * What a key does, as `{ do, … }`, or null for nothing.
 *
 * `set` is "bodyworks" or "outfit"; `screen` is "studio" or "bodyslide".
 * `e` is the key event, or as much of one: `key`, `code`, `ctrlKey`,
 * `metaKey`, `shiftKey`, `altKey`. `at` is what is on screen: `work` (a
 * project is open), `typing` (the key went to a field or an open sheet),
 * `sheet` (a menu or sheet is open), `side` (the tab the list beside the
 * view shows) and `tool` (the brush in hand, or null).
 *
 * Escape is not here: it shuts the menu or sheet on top in either set.
 */
export function action(set, screen, e, at = {}) {
  return set === "outfit" ? original(screen, e, at) : own(screen, e, at);
}

/** Bodyworks' own: Outfit Studio only, and as it always was. */
function own(screen, e, at) {
  if (screen !== "studio" || typeof e.key !== "string") return null;
  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  // Undo and redo, whether a menu is open or not, but not into a field.
  if (ctrl && key === "z") return at.typing ? null : { do: "undo", again: Boolean(e.shiftKey) };
  if (ctrl) {
    if (!at.work || at.sheet) return null;
    if (key === "s") return { do: "saveProject" };
    if (key === "e") return { do: "exportMesh" };
    return null;
  }
  if (e.altKey || !at.work || at.typing || at.sheet) return null;
  // Q puts the brush down, 1 and up take one in hand in the order they are
  // shown, the square brackets size it, W and T change how the outfit is drawn.
  if (key === "q") return { do: "hold", tool: null };
  if (/^[1-9]$/.test(key) && BRUSHES[Number(key) - 1]) return { do: "hold", tool: BRUSHES[Number(key) - 1][0] };
  if (key === "[" || key === "]") return { do: "size", by: key === "[" ? -5 : 5 };
  if (key === "-" || key === "=" || key === "+") return { do: "strength", by: key === "-" ? -5 : 5 };
  if (key === "f") return { do: "frame" };
  if (key === "x") return { do: "mirror" };
  if (key === "h") return { do: "hide" };
  if (key === "g") return { do: "ghost" };
  if (key === "w") return { do: "wire" };
  if (key === "t") return { do: "textures" };
  return null;
}

/** The original's, by screen. Keys it has not got do nothing. */
function original(screen, e, at) {
  if (typeof e.key !== "string" || at.typing || at.sheet) return null;
  return screen === "studio" ? studio(e, at) : bodyslide(e);
}

/** The digits' brushes, 0 the select tool: the Keyboard Shortcuts page gives 0 to 6, Brushes.md 7 and 8. */
const DIGITS = { 0: null, 1: "mask", 2: "inflate", 3: "deflate", 4: "move", 5: "smooth", 6: "weights", 7: "colour", 8: "colour" };

/** Shift and a digit, for the view: by the key's place, as Shift changes what the key says. */
const SIDES = { Digit1: "front", Digit2: "back", Digit3: "left", Digit4: "right" };

function studio(e, at) {
  const ctrl = e.ctrlKey || e.metaKey;
  const { shiftKey: shift, altKey: alt } = e;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (ctrl) {
    if (alt) return null; // Ctrl+Alt+E, an export with the reference in it, is not here.
    if (shift) return key === "s" ? { do: "saveProject" } : null; // Save As: the save sheet asks the name.
    if (key === "n") return { do: "newProject" };
    if (key === "o") return { do: "openProject" };
    if (key === "w") return { do: "closeProject" };
    if (key === "s") return { do: "saveProject" };
    if (key === "e") return { do: "exportMesh" };
    if (key === "a") return { do: "clearMask" };
    if (key === "i") return { do: "invertMask" };
    if (key === "z") return { do: "undo", again: false };
    if (key === "y") return { do: "undo", again: true };
    if (key === "Delete") return { do: "deleteSlider" };
    if (key === " ") return { do: "editSlider" };
    return null;
  }
  if (alt) return null;
  if (shift) {
    if (SIDES[e.code]) return { do: "face", side: SIDES[e.code] };
    if (e.code === "Digit5") return { do: "perspective" };
    if (key === "r") return { do: "pivot" };
    if (key === "b") return { do: "bones" };
    if (key === "Delete") return { do: "deleteVertices" };
    if (key === "s") return { do: "separate" };
    return null;
  }
  if (key in DIGITS) {
    // The weight brush is the bones tab's; in the original the colour and
    // alpha brushes are its Colors tab's, which Bodyworks has not got: its
    // one colour brush paints both, wherever the list is.
    if (key === "6" && at.side !== "bones") return null;
    return { do: "hold", tool: DIGITS[key] };
  }
  if (key === "Delete") return { do: "deleteShape" };
  if (key === "F2") return { do: "renameShape" };
  if (key === "Tab") return { do: "sliderSettings" };
  if (key === " ") return { do: "brushSettings" };
  if (key === "a" || key === "d") return { do: "spreadMask", less: key === "a" };
  if (key === "x") return { do: "mirror" };
  if (key === "c") return { do: "connected" };
  // The original's transform tool has handles; Bodyworks' Place brush drags the shape.
  if (key === "f") return { do: "hold", tool: "place" };
  // V picks a colour up in the Colors tab: here, with the colour brush in hand.
  if (key === "v") return at.tool === "colour" ? { do: "pickColour" } : null;
  if (key === "w") return { do: "wire" };
  if (key === "t") return { do: "textures" };
  if (key === "l") return { do: "lighting" };
  // "Visibility of the selected shape(s)": the picked one hidden, or shown again.
  if (key === "e") return { do: "hide" };
  if (key === "g") return { do: "grid" };
  return null;
}

function bodyslide(e) {
  const ctrl = e.ctrlKey || e.metaKey;
  const { shiftKey: shift, altKey: alt } = e;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (shift) return null;
  if (ctrl && alt) return key === "s" ? { do: "savePreset", asNew: true } : null;
  if (alt) return null;
  if (key === "PageUp" || key === "PageDown") return { do: ctrl ? "outfit" : "preset", by: key === "PageUp" ? -1 : 1 };
  if (ctrl) {
    if (key === "a") return { do: "clearFilters" };
    if (key === "o") return { do: "toStudio" };
    if (key === "s") return { do: "savePreset", asNew: false };
    if (key === "g") return { do: "groups" };
    return null;
  }
  if (key === "F5") return { do: "refresh" };
  if (key === "w") return { do: "wire" };
  if (key === "t") return { do: "textures" };
  if (key === "l") return { do: "lighting" };
  return null;
}

/**
 * What the wheel over the view does in Outfit Studio, beyond going nearer
 * and further: `held` is whether S is held down. Null leaves it to zoom.
 */
export function wheel(set, e, held) {
  if (set !== "outfit") return null;
  const by = e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0;
  if (!by) return null;
  // Hold S and the wheel sizes the brush; Ctrl and it, the slider edited (CL 5.4.0).
  if (held && !(e.ctrlKey || e.metaKey)) return { do: "size", by: -by * 5 };
  if (e.ctrlKey || e.metaKey) return { do: "nextSlider", by };
  return null;
}

/**
 * What a press on the view starts: whether a brush in hand is offered it
 * first, and what it does if not, "turn", "pan", or "pivot" (the point
 * pressed becomes what the view turns about, and the drag turns it).
 */
export function press(set, e) {
  if (set !== "outfit") {
    // Bodyworks: the main button brushes, or turns the view off the shape; any other, or Shift, moves it.
    const pan = e.button !== 0 || Boolean(e.shiftKey);
    return { brush: e.button === 0 && !e.shiftKey, then: pan ? "pan" : "turn" };
  }
  // The original (CL 2.1): the left button paints, a right drag turns,
  // the middle button or Shift and a right drag pans; Alt and a right drag
  // from a vertex turns about it (CL 5.4.0).
  if (e.button === 0) return { brush: true, then: "turn" };
  if (e.button === 1) return { brush: false, then: "pan" };
  return { brush: false, then: e.shiftKey ? "pan" : e.altKey ? "pivot" : "turn" };
}

/** Brushes the held keys work on; the edge, place and pose tools are not brushed. */
const BRUSHED = new Set(["inflate", "deflate", "move", "smooth", "erase", "mask", "weights", "colour"]);

/**
 * The brush a stroke is made with, for the keys held as it starts:
 * `{ tool, alt, smooth }`. `alt` is the brush's other way (the mask
 * uncovers, the weight brush takes away, the colour brush picks a colour
 * up); `smooth` has the weight brush smooth the weights.
 */
export function stroke(set, tool, e) {
  const alt = Boolean(e.altKey);
  if (set !== "outfit" || !BRUSHED.has(tool)) return { tool, alt, smooth: false };
  const ctrl = e.ctrlKey || e.metaKey;
  // With any brush, Ctrl masks and Ctrl+Alt unmasks.
  if (ctrl) return { tool: "mask", alt, smooth: false };
  // Alt is a brush's opposite: inflate deflates, and deflate inflates.
  if (alt && tool === "inflate") return { tool: "deflate", alt: false, smooth: false };
  if (alt && tool === "deflate") return { tool: "inflate", alt: false, smooth: false };
  return { tool, alt, smooth: tool === "weights" && Boolean(e.shiftKey) };
}
