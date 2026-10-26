// Runs the add-on outside the mod manager, for working on it: serves `ui/`
// and the built module, and a page that stands in for the mod manager,
// answering the frame from a folder laid out like a game's.
//
//   node tools/dev.cjs <game folder> [port]
//
// What the add-on builds lands in `scratch/dev-output/`. `SAVES=<folder>`
// names a folder of saves to stand in for the game's.
const http = require("http");
const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const game = path.resolve(process.argv[2] || path.join(root, "scratch/samples/cbbe/@required"));
const port = Number(process.argv[3] || 5290);
const wasm = path.join(root, "target/wasm32-unknown-unknown/release/apoc_bodyslide_app.wasm");
const out = path.join(root, "scratch/dev-output");
const saves = process.env.SAVES ? path.resolve(process.env.SAVES) : null;
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".wasm": "application/wasm" };

/** What a save's header says of it, and where its screenshot is, or null. */
function saveHeader(file) {
  const fd = fs.openSync(file, "r");
  const b = Buffer.alloc(64 * 1024);
  const got = fs.readSync(fd, b, 0, b.length, 0);
  fs.closeSync(fd);
  if (got < 30 || b.toString("latin1", 0, 13) !== "TESV_SAVEGAME") return null;
  let at = 17;
  const u32 = () => ((at += 4), b.readUInt32LE(at - 4));
  const str = () => {
    const n = b.readUInt16LE(at);
    at += 2 + n;
    return b.toString("utf8", at - n, at);
  };
  const version = u32();
  u32();
  const character = str();
  const level = u32();
  const location = str();
  const playtime = str();
  const race = str();
  at += 2 + 8;
  const filetime = b.readBigUInt64LE(at);
  at += 8;
  const width = u32();
  const height = u32();
  if (version >= 12) at += 2;
  return { character, level, location, playtime, race, savedAt: Number(filetime / 10000000n - 11644473600n), width, height, depth: version >= 12 ? 4 : 3, shot: at };
}

/** RGBA or RGB pixels as a PNG. */
function png(width, height, depth, pixels) {
  const zlib = require("zlib");
  const crc = (buf) => {
    let c = ~0;
    for (const x of buf) {
      c ^= x;
      for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
    }
    return ~c >>> 0;
  };
  const chunk = (kind, data) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(data.length);
    head.write(kind, 4, "latin1");
    const tail = Buffer.alloc(4);
    tail.writeUInt32BE(crc(Buffer.concat([head.subarray(4), data])));
    return Buffer.concat([head, data, tail]);
  };
  const rows = Buffer.alloc((width * depth + 1) * height);
  for (let y = 0; y < height; y++) pixels.copy(rows, y * (width * depth + 1) + 1, y * width * depth, (y + 1) * width * depth);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width);
  ihdr.writeUInt32BE(height, 4);
  ihdr.set([8, depth === 4 ? 6 : 2, 0, 0, 0], 8);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(rows)), chunk("IEND", Buffer.alloc(0))]);
}

/** A save in the stand-in's folder by its file name, or null. */
function saveFile(name) {
  if (!saves || !name || name.includes("/") || name.includes("\\")) return null;
  const file = path.join(saves, name);
  return fs.existsSync(file) ? file : null;
}

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
  "files.part": async (a) => { const r = await fetch("/part?at=" + a.at + "&length=" + a.length + "&path=" + encodeURIComponent(a.path)); if (!r.ok) throw new Error("No such file."); const all = await r.arrayBuffer(); return { size: Number(new DataView(all).getBigUint64(0, true)), bytes: all.slice(8) }; },
  "saves.list": async () => (await fetch("/saves")).json(),
  "saves.picture": async (a) => { const r = await fetch("/save-picture?name=" + encodeURIComponent(a.name)); if (!r.ok) throw new Error("That save has no picture."); return r.arrayBuffer(); },
  "saves.read": async (a) => { const r = await fetch("/save?name=" + encodeURIComponent(a.name)); if (!r.ok) throw new Error("There is no such save."); return r.arrayBuffer(); },
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
    frame.contentWindow.postMessage({ apocAddon: 1, id: m.id, ok }, "*", ok instanceof ArrayBuffer ? [ok] : ok && ok.bytes instanceof ArrayBuffer ? [ok.bytes] : []);
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
    if (url.pathname === "/part") {
      const file = find(url.searchParams.get("path"));
      if (!file) return send(404, "missing");
      const at = Number(url.searchParams.get("at"));
      const length = Math.min(Number(url.searchParams.get("length")), 64 * 1024 * 1024);
      const size = fs.statSync(file).size;
      const piece = Buffer.alloc(Math.max(0, Math.min(length, size - at)));
      if (piece.length) {
        const fd = fs.openSync(file, "r");
        fs.readSync(fd, piece, 0, piece.length, at);
        fs.closeSync(fd);
      }
      const head = Buffer.alloc(8);
      head.writeBigUInt64LE(BigInt(size));
      return send(200, Buffer.concat([head, piece]), "application/octet-stream");
    }
    if (url.pathname === "/saves") {
      const list = (saves ? fs.readdirSync(saves) : [])
        .filter((n) => /\.ess$/i.test(n))
        .map((name) => {
          const h = saveHeader(path.join(saves, name));
          return h && { name, character: h.character, level: h.level, location: h.location, race: h.race, playtime: h.playtime, savedAt: h.savedAt, screenshot: h.width > 0 };
        })
        .filter(Boolean)
        .sort((a, b) => b.savedAt - a.savedAt || a.name.localeCompare(b.name));
      return send(200, JSON.stringify(list), "application/json");
    }
    if (url.pathname === "/save" || url.pathname === "/save-picture") {
      const file = saveFile(url.searchParams.get("name"));
      if (!file) return send(404, "missing");
      if (url.pathname === "/save") return send(200, fs.readFileSync(file), "application/octet-stream");
      const h = saveHeader(file);
      if (!h || !h.width) return send(404, "no picture");
      const pixels = Buffer.alloc(h.width * h.height * h.depth);
      const fd = fs.openSync(file, "r");
      fs.readSync(fd, pixels, 0, pixels.length, h.shot);
      fs.closeSync(fd);
      return send(200, png(h.width, h.height, h.depth, pixels), "image/png");
    }
    if (url.pathname === "/picture") {
      // The mod manager decodes textures itself; here ImageMagick stands in.
      const file = find(url.searchParams.get("path"));
      if (!file) return send(404, "missing");
      const longest = Number(url.searchParams.get("longest")) || 2048;
      // Alpha kept, as the mod manager keeps it.
      require("child_process").execFile("magick", [`dds:${file}[0]`, "-resize", `${longest}x${longest}>`, "png:-"], { encoding: "buffer", maxBuffer: 1 << 28 }, (err, out) =>
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
