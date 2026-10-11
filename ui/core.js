// The add-on's WebAssembly module: one function, a request as JSON and
// sometimes a file's bytes, an answer as JSON and sometimes bytes after it.

let wasm;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

export async function start(url) {
  const bytes = await (await fetch(url)).arrayBuffer();
  wasm = (await WebAssembly.instantiate(bytes, {})).instance.exports;
}

function put(bytes) {
  const at = wasm.alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, at, bytes.length).set(bytes);
  return at;
}

/** Ask the module. Returns `[answer, bytes]`; throws what it refuses with. */
export function ask(request, data) {
  const req = encoder.encode(JSON.stringify(request));
  const reqAt = put(req);
  const bytes = data ? (data instanceof Uint8Array ? data : new Uint8Array(data)) : null;
  const dataAt = bytes && bytes.length ? put(bytes) : 0;
  const packed = wasm.call(reqAt, req.length, dataAt, bytes ? bytes.length : 0);
  wasm.dealloc(reqAt, req.length);
  if (dataAt) wasm.dealloc(dataAt, bytes.length);
  const at = Number(packed >> 32n);
  const length = Number(packed & 0xffffffffn);
  const all = new Uint8Array(wasm.memory.buffer, at, length);
  const jsonLength = new DataView(wasm.memory.buffer, at, 4).getUint32(0, true);
  const answer = JSON.parse(decoder.decode(all.subarray(4, 4 + jsonLength)));
  // Copied out, so it outlives the module's memory and starts on a boundary.
  const rest = all.slice(4 + jsonLength).buffer;
  wasm.dealloc(at, length);
  if ("error" in answer) throw new Error(answer.error);
  return [answer.ok, rest];
}
