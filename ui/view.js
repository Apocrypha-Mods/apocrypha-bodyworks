// The 3D view: the shapes of one set, turned with the pointer. It draws only
// when something changed.

const VS = `#version 300 es
in vec3 p; in vec3 n;
uniform mat4 mvp; uniform mat3 nm;
out vec3 vn;
void main() { vn = nm * n; gl_Position = mvp * vec4(p, 1.0); }`;

const FS = `#version 300 es
precision highp float;
in vec3 vn; out vec4 o;
uniform vec3 tint;
void main() {
  vec3 n = normalize(vn);
  float key = max(dot(n, normalize(vec3(0.35, 0.55, 0.75))), 0.0);
  float fill = max(dot(n, normalize(vec3(-0.6, 0.1, 0.4))), 0.0);
  float rim = pow(1.0 - max(n.z, 0.0), 3.0);
  vec3 c = tint * (0.22 + 0.72 * key + 0.2 * fill) + vec3(0.12) * rim;
  o = vec4(pow(c, vec3(1.0 / 2.2)), 1.0);
}`;

/** A little to one side of straight on. */
const FRONT = Math.PI - 0.45;

function compile(gl, type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
  return s;
}

function mul(a, b) {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++)
    for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      o[c * 4 + r] = s;
    }
  return o;
}

export class View {
  constructor(canvas) {
    this.canvas = canvas;
    const gl = (this.gl = canvas.getContext("webgl2", { antialias: true, alpha: true, premultipliedAlpha: false }));
    if (!gl) throw new Error("This computer cannot draw 3D here.");
    const prog = (this.prog = gl.createProgram());
    gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VS));
    gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FS));
    gl.bindAttribLocation(prog, 0, "p");
    gl.bindAttribLocation(prog, 1, "n");
    gl.linkProgram(prog);
    this.uMvp = gl.getUniformLocation(prog, "mvp");
    this.uNm = gl.getUniformLocation(prog, "nm");
    this.uTint = gl.getUniformLocation(prog, "tint");
    this.shapes = [];
    this.turn = FRONT;
    this.tilt = 0.08;
    this.far = 1;
    this.at = [0, 0, 0];
    this.size = 1;
    this.wide = 1;
    this.tall = 1;
    this.tint = [0.78, 0.6, 0.5];
    this.queued = false;
    this.pointers();
    new ResizeObserver(() => this.draw()).observe(canvas);
  }

  pointers() {
    const c = this.canvas;
    let held = null;
    c.addEventListener("pointerdown", (e) => {
      held = { x: e.clientX, y: e.clientY, pan: e.button !== 0 || e.shiftKey };
      c.setPointerCapture(e.pointerId);
    });
    c.addEventListener("pointermove", (e) => {
      if (!held) return;
      const dx = e.clientX - held.x;
      const dy = e.clientY - held.y;
      held.x = e.clientX;
      held.y = e.clientY;
      if (held.pan) {
        const k = (this.size * this.far) / c.clientHeight;
        this.at[0] -= Math.cos(this.turn) * dx * k;
        this.at[1] -= Math.sin(this.turn) * dx * k;
        this.at[2] += dy * k;
      } else {
        this.turn -= dx * 0.008;
        this.tilt = Math.max(-1.5, Math.min(1.5, this.tilt + dy * 0.008));
      }
      this.draw();
    });
    const up = () => (held = null);
    c.addEventListener("pointerup", up);
    c.addEventListener("pointercancel", up);
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        this.far = Math.max(0.15, Math.min(4, this.far * Math.exp(e.deltaY * 0.0012)));
        this.draw();
      },
      { passive: false },
    );
    c.addEventListener("dblclick", () => this.frame());
  }

  /** Replace what is shown. Each shape: `{ positions, triangles }`. */
  show(shapes) {
    const gl = this.gl;
    for (const s of this.shapes) {
      gl.deleteBuffer(s.pb);
      gl.deleteBuffer(s.nb);
      gl.deleteBuffer(s.ib);
      gl.deleteVertexArray(s.vao);
    }
    this.shapes = shapes.map((s) => {
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      const pb = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, pb);
      gl.bufferData(gl.ARRAY_BUFFER, s.positions, gl.DYNAMIC_DRAW);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
      const nb = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, nb);
      gl.bufferData(gl.ARRAY_BUFFER, s.positions.byteLength, gl.DYNAMIC_DRAW);
      gl.enableVertexAttribArray(1);
      gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 0, 0);
      const ib = gl.createBuffer();
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ib);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, s.triangles, gl.STATIC_DRAW);
      gl.bindVertexArray(null);
      return { vao, pb, nb, ib, count: s.triangles.length, shown: true };
    });
  }

  /** New positions and normals for shape `i`. */
  move(i, positions, normals) {
    const gl = this.gl;
    const s = this.shapes[i];
    if (!s) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, s.pb);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, positions);
    gl.bindBuffer(gl.ARRAY_BUFFER, s.nb);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, normals);
    this.draw();
  }

  /** Look at everything in `positions` (a list of arrays), from the front. */
  frame(positions = this.framed) {
    if (!positions) return;
    this.framed = positions;
    const lo = [Infinity, Infinity, Infinity];
    const hi = [-Infinity, -Infinity, -Infinity];
    for (const p of positions)
      for (let i = 0; i < p.length; i++) {
        const k = i % 3;
        if (p[i] < lo[k]) lo[k] = p[i];
        if (p[i] > hi[k]) hi[k] = p[i];
      }
    if (!isFinite(lo[0])) return;
    this.at = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
    this.size = Math.max(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], 1e-3);
    this.wide = Math.max(hi[0] - lo[0], hi[1] - lo[1], 1e-3);
    this.tall = Math.max(hi[2] - lo[2], 1e-3);
    this.turn = FRONT;
    this.tilt = 0.08;
    this.far = 1;
    this.draw();
  }

  draw() {
    if (this.queued) return;
    this.queued = true;
    requestAnimationFrame(() => {
      this.queued = false;
      this.paint();
    });
  }

  paint() {
    const gl = this.gl;
    const c = this.canvas;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(1, Math.round(c.clientWidth * dpr));
    const h = Math.max(1, Math.round(c.clientHeight * dpr));
    if (c.width !== w || c.height !== h) {
      c.width = w;
      c.height = h;
    }
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    if (!this.shapes.length) return;

    // The games' meshes stand along Z and face +Y. The eye circles Z, far
    // enough that the whole of it fits the canvas either way.
    const fov = 0.6;
    const f = 1 / Math.tan(fov / 2);
    const d = Math.max(this.tall * f, (this.wide * f) / (w / h)) * 0.56 * this.far;
    const near = d * 0.05;
    const farP = d * 4 + this.size;
    const proj = new Float32Array([
      f / (w / h), 0, 0, 0,
      0, f, 0, 0,
      0, 0, (farP + near) / (near - farP), -1,
      0, 0, (2 * farP * near) / (near - farP), 0,
    ]);
    const ct = Math.cos(this.turn), st = Math.sin(this.turn);
    const cp = Math.cos(this.tilt), sp = Math.sin(this.tilt);
    // Camera axes in the mesh's space: right, up, and back toward the eye.
    const right = [ct, st, 0];
    const back = [st * cp, -ct * cp, sp];
    const up = [-st * sp, ct * sp, cp];
    const eye = [this.at[0] + back[0] * d, this.at[1] + back[1] * d, this.at[2] + back[2] * d];
    const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    const view = new Float32Array([
      right[0], up[0], back[0], 0,
      right[1], up[1], back[1], 0,
      right[2], up[2], back[2], 0,
      -dot(right, eye), -dot(up, eye), -dot(back, eye), 1,
    ]);
    gl.useProgram(this.prog);
    gl.uniformMatrix4fv(this.uMvp, false, mul(proj, view));
    gl.uniformMatrix3fv(this.uNm, false, new Float32Array([
      right[0], up[0], back[0],
      right[1], up[1], back[1],
      right[2], up[2], back[2],
    ]));
    gl.uniform3fv(this.uTint, this.tint);
    for (const s of this.shapes) {
      if (!s.shown) continue;
      gl.bindVertexArray(s.vao);
      gl.drawElements(gl.TRIANGLES, s.count, gl.UNSIGNED_INT, 0);
    }
    gl.bindVertexArray(null);
  }
}

/** For each vertex, the first vertex at the same place: seams light as one surface. */
export function welded(positions) {
  const count = positions.length / 3;
  const first = new Map();
  const out = new Uint32Array(count);
  for (let i = 0; i < count; i++) {
    const key = `${Math.round(positions[i * 3] * 2000)},${Math.round(positions[i * 3 + 1] * 2000)},${Math.round(positions[i * 3 + 2] * 2000)}`;
    const seen = first.get(key);
    if (seen === undefined) {
      first.set(key, i);
      out[i] = i;
    } else out[i] = seen;
  }
  return out;
}

/** Smooth normals for `positions`, into `normals`; `sum` is scratch of the same size. */
export function shade(positions, triangles, weld, normals, sum) {
  sum.fill(0);
  for (let t = 0; t < triangles.length; t += 3) {
    const a = triangles[t] * 3, b = triangles[t + 1] * 3, c = triangles[t + 2] * 3;
    const ux = positions[b] - positions[a], uy = positions[b + 1] - positions[a + 1], uz = positions[b + 2] - positions[a + 2];
    const vx = positions[c] - positions[a], vy = positions[c + 1] - positions[a + 1], vz = positions[c + 2] - positions[a + 2];
    const nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx;
    for (const v of [triangles[t], triangles[t + 1], triangles[t + 2]]) {
      const g = weld[v] * 3;
      sum[g] += nx;
      sum[g + 1] += ny;
      sum[g + 2] += nz;
    }
  }
  for (let i = 0; i < weld.length; i++) {
    const g = weld[i] * 3;
    const x = sum[g], y = sum[g + 1], z = sum[g + 2];
    const l = Math.hypot(x, y, z) || 1;
    normals[i * 3] = x / l;
    normals[i * 3 + 1] = y / l;
    normals[i * 3 + 2] = z / l;
  }
}
