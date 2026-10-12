// Runs the add-on outside the mod manager, for working on it: serves `ui/`
// and the built module, and a page that stands in for the mod manager,
// answering the frame from a folder laid out like a game's.
//
//   node tools/dev.cjs <game folder> [port]
//
// What the add-on builds lands in `scratch/dev-output/`.
const http = require("http");
const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const game = path.resolve(process.argv[2] || path.join(root, "scratch/samples/cbbe/@required"));
const port = Number(process.argv[3] || 5290);
const wasm = path.join(root, "target/wasm32-unknown-unknown/release/apoc_bodyslide_app.wasm");
const out = path.join(root, "scratch/dev-output");
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".wasm": "application/wasm" };

/** The real path for a path in any case, or null. */
function find(rel) {
  let at = game;
  for (const part of rel.split("/").filter(Boolean)) {
    if (part === "..") return null;
    let names;
    try {
      names = fs.readdirSync(at);
    } catch {
      return null;
    }
    const hit = names.find((n) => n.toLowerCase() === part.toLowerCase());
    if (!hit) return null;
    at = path.join(at, hit);
  }
  return at;
}

function walk(dir, rel, list) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, e.name);
    const stat = fs.statSync(full);
    if (stat.isDirectory()) walk(full, `${rel}/${e.name}`, list);
    else list.push({ path: `${rel}/${e.name}`, size: stat.size });
  }
}

// Which game the stand-in says it is: `GAME=fallout4` for Fallout 4.
const GAME = process.env.GAME === "fallout4" ? { id: "fallout4", name: "Fallout 4" } : { id: "skyrim-se", name: "Skyrim Special Edition" };

const HOST = `<!doctype html><meta charset="utf-8"><title>Stand-in mod manager</title>
<style>html,body{height:100%;margin:0;background:hsl(0 0% 7%)}iframe{width:100%;height:100%;border:0;display:block}</style>
<iframe id="f" src="/ui/index.html" sandbox="allow-scripts"></iframe>
<script>
const frame = document.getElementById("f");
const answers = {
  hello: async () => ({ api: 1, game: ${JSON.stringify(GAME)}, theme: { dark: true, vars: {} } }),
  fonts: async () => [],
  "files.list": async (a) => (await fetch("/list?folder=" + encodeURIComponent(a.folder))).json(),
  "files.picture": async (a) => { const r = await fetch("/picture?longest=" + (a.longest || 2048) + "&path=" + encodeURIComponent(a.path)); if (!r.ok) throw new Error("No such texture."); return r.arrayBuffer(); },
  "files.read": async (a) => { const r = await fetch("/read?path=" + encodeURIComponent(a.path)); if (!r.ok) throw new Error("No such file."); return r.arrayBuffer(); },
  "output.write": async (a) => { window.into = a.into || null; for (const f of a.files) await fetch("/write?path=" + encodeURIComponent(f.path), { method: "POST", body: f.bytes }); window.written = (window.written || []).concat(a.files.map((f) => f.path)); return { files: a.files.length }; },
  // No dialogs here: a save lands under the output folder, and an open hands back what a test put in window.toOpen.
  "disk.save": async (a) => { await fetch("/write?path=" + encodeURIComponent("disk/" + a.name), { method: "POST", body: a.bytes }); window.saved = a.name; return { saved: true }; },
  "disk.open": async () => window.toOpen || null,
  "disk.folder": async (a) => { for (const f of a.files) await fetch("/write?path=" + encodeURIComponent("folder/" + f.path), { method: "POST", body: f.bytes }); window.foldered = a.files.map((f) => f.path); return { saved: a.files.length }; },
  "mods.list": async () => ["Bodyslide Presets", "CBBE 3BA", "SkyUI", "RaceMenu", "XP32 Skeleton", "CBPC", "Faster HDT-SMP", "UBE 2.0", "Engine Fixes", "PapyrusUtil"].map((name, i) => ({ id: "mod-" + i, name })),
  "output.open": async (a) => { window.opened = a.name; return null; },
  "output.remove": async (a) => { const had = window.written || []; const gone = a.paths.filter((p) => had.includes(p)); window.written = had.filter((p) => !gone.includes(p)); window.removed = (window.removed || []).concat(gone); return { removed: gone.length }; },
  "apply.review": async () => { window.reviewed = true; },
};
addEventListener("message", async (e) => {
  const m = e.data;
  if (e.source !== frame.contentWindow || !m || m.apocAddon !== 1) return;
  try {
    const ok = await answers[m.op](m.args);
    frame.contentWindow.postMessage({ apocAddon: 1, id: m.id, ok }, "*", ok instanceof ArrayBuffer ? [ok] : []);
  } catch (err) {
    frame.contentWindow.postMessage({ apocAddon: 1, id: m.id, error: String(err.message || err) }, "*");
  }
});
</script>`;

http
  .createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    const send = (code, body, type) => {
      res.writeHead(code, { "content-type": type || "text/plain", "access-control-allow-origin": "*", "cache-control": "no-store" });
      res.end(body);
    };
    if (url.pathname === "/") return send(200, HOST, "text/html");
    if (url.pathname === "/list") {
      const rel = url.searchParams.get("folder");
      const dir = find(rel);
      const list = [];
      if (dir) walk(dir, rel, list);
      return send(200, JSON.stringify(list), "application/json");
    }
    if (url.pathname === "/read") {
      const file = find(url.searchParams.get("path"));
      return file ? send(200, fs.readFileSync(file), "application/octet-stream") : send(404, "missing");
    }
    if (url.pathname === "/picture") {
      // The mod manager decodes textures itself; here ImageMagick stands in.
      const file = find(url.searchParams.get("path"));
      if (!file) return send(404, "missing");
      const longest = Number(url.searchParams.get("longest")) || 2048;
      require("child_process").execFile("magick", [`dds:${file}[0]`, "-alpha", "off", "-resize", `${longest}x${longest}>`, "png:-"], { encoding: "buffer", maxBuffer: 1 << 28 }, (err, out) =>
        err ? send(404, "not readable") : send(200, out, "image/png"),
      );
      return;
    }
    if (url.pathname === "/write") {
      const rel = path.normalize(url.searchParams.get("path"));
      if (rel.startsWith("..") || path.isAbsolute(rel)) return send(400, "bad path");
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        const to = path.join(out, rel);
        fs.mkdirSync(path.dirname(to), { recursive: true });
        fs.writeFileSync(to, Buffer.concat(chunks));
        send(200, "ok");
      });
      return;
    }
    if (url.pathname.startsWith("/ui/")) {
      const name = url.pathname.slice(4);
      const file = name === "bodyworks.wasm" ? wasm : path.join(root, "ui", path.normalize(name));
      if (!file.startsWith(root) || !fs.existsSync(file)) return send(404, "missing");
      return send(200, fs.readFileSync(file), types[path.extname(file)]);
    }
    send(404, "missing");
  })
  .listen(port, "127.0.0.1", () => console.log(`http://127.0.0.1:${port}/  (game folder: ${game})`));
