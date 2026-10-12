// The mod manager, as the add-on sees it. Everything the add-on can do
// outside its own frame is one of these requests; the window that holds the
// frame answers them and can refuse any.
//
//   hello                          { api, game: { id, name }, theme }
//   files.list  { folder }         [{ path, size }]   files the enabled mods put under it
//   files.read  { path }           ArrayBuffer
//   files.picture { path, longest } ArrayBuffer       a texture, as a PNG no longer than `longest` a side
//   output.write { name, files }   { files }          files: [{ path, bytes }], into the mod `name`
//   output.remove { name, paths }  { removed }        files this add-on built into the mod `name`, taken out again
//   disk.save   { name, bytes }    { saved }          a file, where the person says; false if they close the dialog
//   disk.open   { extensions }     { name, bytes }    one file the person picks, or null
//   apply.review                   opens the mod manager's own review of what Apply will do
//
// Paths are relative to the game folder, with forward slashes, in any case.

let next = 1;
const waiting = new Map();
const listeners = new Set();

window.addEventListener("message", (e) => {
  const m = e.data;
  if (e.source !== window.parent || !m || m.apocAddon !== 1) return;
  if (m.event) {
    for (const f of listeners) f(m.event, m.data);
    return;
  }
  const w = waiting.get(m.id);
  if (!w) return;
  waiting.delete(m.id);
  if ("error" in m) w.reject(new Error(m.error));
  else w.resolve(m.ok);
});

export function request(op, args = {}, transfer = []) {
  return new Promise((resolve, reject) => {
    const id = next++;
    waiting.set(id, { resolve, reject });
    window.parent.postMessage({ apocAddon: 1, id, op, args }, "*", transfer);
  });
}

/** Hear what the mod manager says unasked: `theme` when its look changes. */
export function listen(f) {
  listeners.add(f);
}

/** Take the mod manager's colours and type as this page's own. */
export function wear(theme) {
  if (!theme) return;
  const root = document.documentElement;
  root.dataset.theme = theme.dark ? "dark" : "light";
  for (const [k, v] of Object.entries(theme.vars || {})) root.style.setProperty(`--${k}`, v);
}
