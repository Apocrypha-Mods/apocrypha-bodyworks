// The 3D view: the shapes of one set, turned with the pointer. It draws only
// when something changed.

const VS = `#version 300 es
in vec3 p; in vec3 n; in vec2 t; in float m; in vec4 k;
uniform mat4 mvp; uniform mat3 nm;
out vec3 vn; out vec2 vt; out float vm; out vec4 vk;
void main() { vn = nm * n; vt = t; vm = m; vk = k; gl_Position = mvp * vec4(p, 1.0); gl_PointSize = 7.0; }`;

const FS = `#version 300 es
precision highp float;
in vec3 vn; in vec2 vt; in float vm; in vec4 vk; out vec4 o;
uniform vec3 tint; uniform vec3 dye; uniform sampler2D skin; uniform float painted; uniform vec4 line; uniform float heat; uniform float alpha; uniform float cut; uniform float see; uniform float coloured; uniform float bright; uniform float lit;
void main() {
  // The lines of a wireframe are one flat colour.
  if (line.a > 0.0) { o = line; return; }
  vec3 n = normalize(vn);
  // The texture where the shape has one, the plain tint where it has not;
  // a texture the game colours (hair by its colour) is coloured so too.
  vec4 t = texture(skin, vt);
  // A see-through shape (hair, brows, lashes) is cut away where its texture's
  // alpha is under the threshold, as the game does; only with its texture on.
  if (painted > 0.5 && t.a < cut) discard;
  vec3 base = mix(tint, pow(min(t.rgb * dye, vec3(1.0)), vec3(2.2)), painted);
  // With the colour brush in hand, a shape is drawn in the colours its vertices carry.
  if (coloured > 0.5) base = pow(vk.rgb, vec3(2.2));
  float key = max(dot(n, normalize(vec3(0.35, 0.55, 0.75))), 0.0);
  float fill = max(dot(n, normalize(vec3(-0.6, 0.1, 0.4))), 0.0);
  float rim = pow(1.0 - max(n.z, 0.0), 3.0);
  // A painted shape is lit harder: a skin texture is darker than the tint.
  float light = (0.22 + 0.72 * key + 0.2 * fill) * mix(1.0, 1.9, painted) * bright;
  // Unlit, every face is as bright as a face turned to the light half way.
  if (lit < 0.5) { light = 0.6 * mix(1.0, 1.9, painted) * bright; rim = 0.0; }
  vec3 c = base * light + vec3(0.12) * rim * mix(1.0, 0.35, painted);
  // What a mask covers is drawn dark: the brushes leave it alone.
  float m = clamp(vm, 0.0, 1.0);
  if (heat > 0.5) {
    // How much a vertex follows a bone: blue for a little, through green, to red for all of it.
    vec3 ramp = m < 0.5 ? mix(vec3(0.1, 0.3, 1.0), vec3(0.1, 0.9, 0.2), m * 2.0) : mix(vec3(0.1, 0.9, 0.2), vec3(1.0, 0.15, 0.1), m * 2.0 - 2.0 + 1.0);
    c = mix(c, ramp * (0.35 + 0.65 * light / mix(1.0, 1.9, painted)), m > 0.0 ? 0.85 : 0.0);
  } else {
    c *= mix(1.0, 0.3, m);
  }
  // One laid over what is behind it lets that through by its alpha.
  o = vec4(pow(c, vec3(1.0 / 2.2)), alpha * mix(1.0, t.a, see * painted));
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
    gl.bindAttribLocation(prog, 2, "t");
    gl.bindAttribLocation(prog, 3, "m");
    gl.bindAttribLocation(prog, 4, "k");
    gl.linkProgram(prog);
    this.uMvp = gl.getUniformLocation(prog, "mvp");
    this.uNm = gl.getUniformLocation(prog, "nm");
    this.uTint = gl.getUniformLocation(prog, "tint");
    this.uDye = gl.getUniformLocation(prog, "dye");
    this.uPainted = gl.getUniformLocation(prog, "painted");
    this.uLine = gl.getUniformLocation(prog, "line");
    this.uHeat = gl.getUniformLocation(prog, "heat");
    this.uAlpha = gl.getUniformLocation(prog, "alpha");
    this.uCut = gl.getUniformLocation(prog, "cut");
    this.uSee = gl.getUniformLocation(prog, "see");
    this.uColoured = gl.getUniformLocation(prog, "coloured");
    this.uBright = gl.getUniformLocation(prog, "bright");
    this.uLit = gl.getUniformLocation(prog, "lit");
    /** How wide the eye sees, in radians, and how brightly the shapes are lit (1 is as designed). */
    this.fov = 0.6;
    this.bright = 1;
    /** How shapes are drawn: with their textures, with their edges, and which one is picked out. */
    this.looks = { painted: true, wire: false, picked: -1, heat: false, colour: false, lit: true, grid: false };
    /** Whether the eye sees straight on, with nothing smaller for being further off. */
    this.flat = false;
    /** What a press on the view starts (see `keys.press`): the main button brushes, or turns; any other, or Shift, moves the view. */
    this.press = (e) => ({ brush: e.button === 0 && !e.shiftKey, then: e.button !== 0 || e.shiftKey ? "pan" : "turn" });
    /** Told of the wheel first, when set: answers whether it took it, or it zooms. */
    this.wheel = null;
    /** Where on a shape a ray first meets one, when set: for a press that makes that what the view turns about. */
    this.pick = null;
    /** Where the pointer last was over the view: `{ clientX, clientY }`, or null. */
    this.pointer = null;
    this.shapes = [];
    /** Textures already on the card, by what the caller names them: a shape shown again wears its own without a second upload. */
    this.skins = new Map();
    this.turn = FRONT;
    this.tilt = 0.08;
    this.far = 1;
    this.at = [0, 0, 0];
    this.size = 1;
    this.wide = 1;
    this.tall = 1;
    this.tint = [0.78, 0.6, 0.5];
    this.queued = false;
    /** A brush, when one is in hand: told of the pointer over the view (see `pointers`). */
    this.brush = null;
    /** The brush's reach, drawn on the shape: `{ point, normal, radius }`, or null. */
    this.ring = null;
    /** Bones to mark in the view: `{ places: Float32Array, picked }`, or null. */
    this.marks = null;
    this.ringVao = gl.createVertexArray();
    this.ringBuf = gl.createBuffer();
    gl.bindVertexArray(this.ringVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.ringBuf);
    gl.bufferData(gl.ARRAY_BUFFER, 64 * 3 * 4, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    this.gridVao = gl.createVertexArray();
    this.gridBuf = gl.createBuffer();
    gl.bindVertexArray(this.gridVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridBuf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    this.pointers();
    new ResizeObserver(() => this.draw()).observe(canvas);
    // Come back to, from a system dialog or another window, the view paints
    // again: what it had drawn may be gone, and a frame asked for while it
    // was covered may never have come.
    const again = () => {
      this.queued = false;
      this.draw();
    };
    window.addEventListener("focus", again);
    document.addEventListener("visibilitychange", again);
  }

  pointers() {
    const c = this.canvas;
    let held = null;
    // With a brush in hand, the main button on the shape is a stroke, and
    // anywhere else, or any other button, still turns and moves the view.
    // The brush is told "over" as the pointer passes, "down" (and answers
    // whether it took the stroke), "drag", "up" and "out".
    c.addEventListener("pointerdown", (e) => {
      c.setPointerCapture(e.pointerId);
      const press = this.press(e);
      if (this.brush && press.brush && this.brush("down", this.ray(e))) {
        held = { stroke: true };
        return;
      }
      if (press.then === "pivot") {
        const at = this.pick?.(this.ray(e));
        if (at) this.turnAbout(at);
      }
      held = { x: e.clientX, y: e.clientY, pan: press.then === "pan" };
    });
    c.addEventListener("pointerleave", () => {
      this.pointer = null;
      if (!held) this.brush?.("out");
    });
    c.addEventListener("pointermove", (e) => {
      this.pointer = { clientX: e.clientX, clientY: e.clientY };
      if (!held) return void this.brush?.("over", this.ray(e));
      if (held.stroke) return void this.brush("drag", this.ray(e));
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
    const up = () => {
      if (held?.stroke) this.brush?.("up");
      held = null;
    };
    c.addEventListener("pointerup", up);
    c.addEventListener("pointercancel", up);
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        if (this.wheel?.(e)) return;
        this.far = Math.max(0.15, Math.min(4, this.far * Math.exp(e.deltaY * 0.0012)));
        this.draw();
      },
      { passive: false },
    );
    c.addEventListener("dblclick", () => this.frame());
  }

  /** Replace what is shown. Each shape: `{ positions, triangles, uvs }`. */
  show(shapes) {
    const gl = this.gl;
    for (const s of this.shapes) {
      gl.deleteBuffer(s.tb);
      if (s.eb) gl.deleteBuffer(s.eb);
      gl.deleteBuffer(s.pb);
      gl.deleteBuffer(s.mb);
      if (s.cb) gl.deleteBuffer(s.cb);
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
      const tb = gl.createBuffer();
      if (s.uvs && s.uvs.length === (s.positions.length / 3) * 2) {
        gl.bindBuffer(gl.ARRAY_BUFFER, tb);
        gl.bufferData(gl.ARRAY_BUFFER, s.uvs, gl.STATIC_DRAW);
        gl.enableVertexAttribArray(2);
        gl.vertexAttribPointer(2, 2, gl.FLOAT, false, 0, 0);
      }
      const mb = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, mb);
      gl.bufferData(gl.ARRAY_BUFFER, s.mask || new Float32Array(s.positions.length / 3), gl.DYNAMIC_DRAW);
      gl.enableVertexAttribArray(3);
      gl.vertexAttribPointer(3, 1, gl.FLOAT, false, 0, 0);
      // Colours only for a shape that carries them: the others read white.
      let cb = null;
      if (s.colors && s.colors.length === (s.positions.length / 3) * 4) {
        cb = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, cb);
        gl.bufferData(gl.ARRAY_BUFFER, s.colors, gl.DYNAMIC_DRAW);
        gl.enableVertexAttribArray(4);
        gl.vertexAttribPointer(4, 4, gl.UNSIGNED_BYTE, true, 0, 0);
      } else gl.vertexAttrib4f(4, 1, 1, 1, 1);
      const ib = gl.createBuffer();
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ib);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, s.triangles, gl.STATIC_DRAW);
      gl.bindVertexArray(null);
      return { vao, pb, nb, tb, mb, cb, ib, count: s.triangles.length, shown: true, skin: null, kept: s.triangles, edges: 0, eb: null, alpha: s.alpha || null };
    });
  }

  /** Change how shapes are drawn (see `looks`). */
  look(changes) {
    Object.assign(this.looks, changes);
    this.draw();
  }

  /** How much of each vertex of shape `i` a mask covers: nothing (0) to all of it (1). */
  mask(i, covered) {
    const gl = this.gl;
    const s = this.shapes[i];
    if (!s) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, s.mb);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, covered);
    this.draw();
  }

  /** The colours the vertices of shape `i` carry: red, green, blue and how solid, a byte each. */
  colours(i, bytes) {
    const gl = this.gl;
    const s = this.shapes[i];
    if (!s || !s.cb) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, s.cb);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, bytes);
    this.draw();
  }

  /** Draw shape `i` as a ghost, seen through, or solid again. */
  ghost(i, on) {
    if (this.shapes[i]) this.shapes[i].ghost = on;
    this.draw();
  }

  /** Leave shape `i` out, or put it back. */
  hide(i, hidden) {
    if (this.shapes[i]) this.shapes[i].shown = !hidden;
    this.draw();
  }

  /**
   * Paint shape `i` with a picture (an ImageBitmap), laid by its texture
   * coordinates. `key` names the picture: one already sent under that name
   * is used again as it is.
   */
  /** Colour shape `i`'s texture as the game does (`[r, g, b]`, or null for as it is). */
  dye_with(i, dye) {
    const s = this.shapes[i];
    if (!s) return;
    s.dye = dye;
    this.draw();
  }

  paint_with(i, picture, key) {
    const gl = this.gl;
    const s = this.shapes[i];
    if (!s) return;
    let skin = key === undefined ? null : this.skins.get(key);
    if (!skin) {
      skin = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, skin);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, picture);
      gl.generateMipmap(gl.TEXTURE_2D);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
      if (key !== undefined) this.skins.set(key, skin);
    }
    s.skin = skin;
    this.draw();
  }

  /** Let go of every texture sent: for when another set is opened. */
  forget() {
    for (const skin of this.skins.values()) this.gl.deleteTexture(skin);
    this.skins.clear();
    for (const s of this.shapes) s.skin = null;
  }

  /** Draw only these triangles of shape `i`. */
  cut(i, triangles) {
    const gl = this.gl;
    const s = this.shapes[i];
    if (!s) return;
    gl.bindVertexArray(s.vao);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, triangles, gl.DYNAMIC_DRAW);
    gl.bindVertexArray(null);
    s.count = triangles.length;
    s.kept = triangles;
    s.edges = 0;
    this.draw();
  }

  /** The edges of shape `s`'s triangles, as pairs of vertices, made when first drawn. */
  edges(s) {
    const gl = this.gl;
    if (!s.eb) s.eb = gl.createBuffer();
    if (!s.edges) {
      const t = s.kept;
      const lines = new Uint32Array(t.length * 2);
      for (let i = 0; i < t.length; i += 3) {
        lines.set([t[i], t[i + 1], t[i + 1], t[i + 2], t[i + 2], t[i]], i * 2);
      }
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.eb);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, lines, gl.DYNAMIC_DRAW);
      s.edges = lines.length;
    }
    return s.eb;
  }

  /** New positions and normals for shape `i`. */
  /** Lay shape `i`'s texture by other places: two numbers a vertex. */
  place(i, uvs) {
    const gl = this.gl;
    const s = this.shapes[i];
    if (!s || !s.tb) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, s.tb);
    gl.bufferData(gl.ARRAY_BUFFER, uvs, gl.DYNAMIC_DRAW);
    this.draw();
  }

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
    // Where the floor is, under the middle: for the grid.
    this.low = lo[2];
    this.middle = [this.at[0], this.at[1]];
    this.turn = FRONT;
    this.tilt = 0.08;
    this.far = 1;
    this.draw();
  }

  /** Where the eye is and which way it looks, for a canvas `aspect` times as wide as tall. */
  camera(aspect) {
    // The games' meshes stand along Z and face +Y. The eye circles Z, far
    // enough that the whole of it fits the canvas either way.
    const fov = this.fov;
    const f = 1 / Math.tan(fov / 2);
    const d = Math.max(this.tall * f, (this.wide * f) / aspect) * 0.56 * this.far;
    const near = d * 0.05;
    const farP = d * 4 + this.size;
    // Seen flat, as much of it fits as fits where it stands seen in depth.
    const proj = this.flat
      ? new Float32Array([
          f / (d * aspect), 0, 0, 0,
          0, f / d, 0, 0,
          0, 0, 2 / (near - farP), 0,
          0, 0, (farP + near) / (near - farP), 1,
        ])
      : new Float32Array([
          f / aspect, 0, 0, 0,
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
    return { f, d, proj, right, up, back, eye };
  }

  /** The line from the eye through where a pointer event is, in the mesh's space: `{ origin, dir, back }`. */
  ray(e) {
    const box = this.canvas.getBoundingClientRect();
    const aspect = box.width / Math.max(1, box.height);
    const { f, d, right, up, back, eye } = this.camera(aspect);
    const x = (((e.clientX - box.left) / box.width) * 2 - 1) * (aspect / f);
    const y = (1 - ((e.clientY - box.top) / box.height) * 2) / f;
    if (this.flat) {
      // Seen flat, every ray goes straight in, from where the pointer is.
      const origin = [0, 1, 2].map((k) => eye[k] + (right[k] * x + up[k] * y) * d);
      return { origin, dir: back.map((c) => -c), back, alt: e.altKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey, shiftKey: e.shiftKey, altKey: e.altKey };
    }
    const dir = [0, 1, 2].map((k) => right[k] * x + up[k] * y - back[k]);
    const l = Math.hypot(dir[0], dir[1], dir[2]);
    return { origin: eye, dir: dir.map((c) => c / l), back, alt: e.altKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey, shiftKey: e.shiftKey, altKey: e.altKey };
  }

  /** How wide the eye sees (`fov`, in degrees) and how brightly the shapes are lit (1 is as designed). */
  lens({ fov, bright }) {
    if (fov) this.fov = (Math.max(10, Math.min(90, fov)) * Math.PI) / 180;
    if (bright) this.bright = Math.max(0.2, Math.min(3, bright));
    this.draw();
  }

  /** Look from straight in front, behind, the left or the right: "front", "back", "left" or "right". */
  face(side) {
    // The meshes face +Y with their left toward -X; FRONT is a little to one side of this.
    this.turn = { front: Math.PI, back: 0, left: -Math.PI / 2, right: Math.PI / 2 }[side] ?? this.turn;
    this.tilt = 0;
    this.draw();
  }

  /** Turn about `point` from here on: it becomes the middle of the view. */
  turnAbout(point) {
    this.at = [point[0], point[1], point[2]];
    this.draw();
  }

  /** Mark where bones are (x, y, z each), one of them picked out; or with null mark none. */
  bones(places, picked = -1) {
    this.marks = places && places.length ? { places, picked } : null;
    this.draw();
  }

  /** A grid on the floor the shapes stand on, a tenth of their height a square. */
  floor() {
    const gl = this.gl;
    const span = Math.max(this.size, 1e-3) * 1.5;
    const step = span / 15;
    const ground = this.low ?? this.at[2] - this.tall / 2;
    const [cx, cy] = this.middle || this.at;
    const lines = [];
    for (let i = -15; i <= 15; i++) {
      lines.push(cx + i * step, cy - span, ground, cx + i * step, cy + span, ground);
      lines.push(cx - span, cy + i * step, ground, cx + span, cy + i * step, ground);
    }
    gl.bindVertexArray(this.gridVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridBuf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(lines), gl.DYNAMIC_DRAW);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.uniform4f(this.uLine, 0.5, 0.52, 0.56, 0.35);
    gl.drawArrays(gl.LINES, 0, lines.length / 3);
    gl.disable(gl.BLEND);
  }

  /** Show the brush's reach on the shape, or with null stop showing it. */
  reach(ring) {
    this.ring = ring;
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

    const { proj, right, up, back, eye } = this.camera(w / h);
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
    const { painted, wire, picked } = this.looks;
    // Faces sit a little back, so the edges drawn over them are not buried.
    gl.enable(gl.POLYGON_OFFSET_FILL);
    gl.polygonOffset(1, 1);
    gl.uniform4f(this.uLine, 0, 0, 0, 0);
    const face = (s, i) => {
      gl.bindVertexArray(s.vao);
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ib);
      const skinned = painted && s.skin;
      // With `heat`, the picked shape's covering is read as weights.
      gl.uniform1f(this.uHeat, this.looks.heat && picked === i ? 1 : 0);
      gl.uniform1f(this.uColoured, this.looks.colour && picked === i && s.cb ? 1 : 0);
      gl.uniform1f(this.uPainted, skinned ? 1 : 0);
      // The picked shape keeps the tint; the others step back from it.
      const dim = picked >= 0 && picked !== i ? 0.62 : 1;
      gl.uniform3f(this.uTint, this.tint[0] * dim, this.tint[1] * dim, this.tint[2] * dim);
      const dye = s.dye || [1, 1, 1];
      gl.uniform3f(this.uDye, dye[0], dye[1], dye[2]);
      if (skinned) gl.bindTexture(gl.TEXTURE_2D, s.skin);
      gl.drawElements(gl.TRIANGLES, s.count, gl.UNSIGNED_INT, 0);
    };
    // A see-through shape after the solid ones, as the game draws it: cut
    // below its threshold, and laid over them where it blends. One that
    // only blends keeps the depth of what it lies on.
    const clear = (s, i) => {
      const { blend, test, threshold } = s.alpha;
      gl.uniform1f(this.uCut, test ? Math.max(threshold, 1) / 255 : 1 / 255);
      gl.uniform1f(this.uSee, blend ? 1 : 0);
      if (blend) {
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      }
      if (blend && !test) gl.depthMask(false);
      face(s, i);
      gl.depthMask(true);
      gl.disable(gl.BLEND);
      gl.uniform1f(this.uCut, 0);
      gl.uniform1f(this.uSee, 0);
    };
    gl.uniform1f(this.uAlpha, 1);
    gl.uniform1f(this.uBright, this.bright);
    gl.uniform1f(this.uLit, this.looks.lit ? 1 : 0);
    gl.uniform1f(this.uCut, 0);
    gl.uniform1f(this.uSee, 0);
    this.shapes.forEach((s, i) => s.shown && !s.ghost && !s.alpha && face(s, i));
    this.shapes.forEach((s, i) => s.shown && !s.ghost && s.alpha && clear(s, i));
    // Ghosts after everything solid, seen through and hiding nothing.
    if (this.shapes.some((s) => s.shown && s.ghost)) {
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.depthMask(false);
      gl.uniform1f(this.uAlpha, 0.42);
      this.shapes.forEach((s, i) => s.shown && s.ghost && face(s, i));
      gl.depthMask(true);
      gl.disable(gl.BLEND);
      gl.uniform1f(this.uAlpha, 1);
    }
    gl.disable(gl.POLYGON_OFFSET_FILL);
    if (wire) {
      this.shapes.forEach((s, i) => {
        if (!s.shown) return;
        gl.bindVertexArray(s.vao);
        gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.edges(s));
        const a = picked < 0 || picked === i ? 0.55 : 0.22;
        gl.uniform4f(this.uLine, 0.04, 0.05, 0.07, a);
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        gl.drawElements(gl.LINES, s.edges, gl.UNSIGNED_INT, 0);
        gl.disable(gl.BLEND);
        // The vertex array remembers its index buffer: give it back its faces.
        gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, s.ib);
      });
    }
    if (this.looks.grid) this.floor();
    if (this.marks) {
      // A dot where each bone is, over everything; the picked one stands out.
      const { places, picked } = this.marks;
      gl.bindVertexArray(this.ringVao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.ringBuf);
      gl.bufferData(gl.ARRAY_BUFFER, places, gl.DYNAMIC_DRAW);
      gl.disable(gl.DEPTH_TEST);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.uniform4f(this.uLine, 1, 1, 1, 0.55);
      gl.drawArrays(gl.POINTS, 0, places.length / 3);
      if (picked >= 0 && picked < places.length / 3) {
        gl.uniform4f(this.uLine, 1, 0.75, 0.2, 1);
        gl.drawArrays(gl.POINTS, picked, 1);
      }
      gl.disable(gl.BLEND);
      gl.enable(gl.DEPTH_TEST);
      gl.bufferData(gl.ARRAY_BUFFER, 64 * 3 * 4, gl.DYNAMIC_DRAW);
    }
    if (this.ring) {
      // A circle lying on the shape where the brush is, drawn over everything.
      const { point, normal: n, radius } = this.ring;
      const side = Math.abs(n[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0];
      let a = [n[1] * side[2] - n[2] * side[1], n[2] * side[0] - n[0] * side[2], n[0] * side[1] - n[1] * side[0]];
      const l = Math.hypot(a[0], a[1], a[2]) || 1;
      a = a.map((c) => c / l);
      const b = [n[1] * a[2] - n[2] * a[1], n[2] * a[0] - n[0] * a[2], n[0] * a[1] - n[1] * a[0]];
      const round = new Float32Array(64 * 3);
      for (let i = 0; i < 64; i++) {
        const t = (i / 64) * Math.PI * 2;
        for (let k = 0; k < 3; k++) round[i * 3 + k] = point[k] + (a[k] * Math.cos(t) + b[k] * Math.sin(t)) * radius;
      }
      gl.bindVertexArray(this.ringVao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.ringBuf);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, round);
      gl.disable(gl.DEPTH_TEST);
      gl.uniform4f(this.uLine, 1, 1, 1, 0.9);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.drawArrays(gl.LINE_LOOP, 0, 64);
      gl.disable(gl.BLEND);
      gl.enable(gl.DEPTH_TEST);
    }
    gl.bindVertexArray(null);
  }
}

/** For each vertex, the first vertex at the same place: seams light as one surface. */
export function welded(positions) {
  const count = positions.length / 3;
  // Places are told apart to a two-thousandth of a unit; each is looked up
  // by a number made of its three, and checked, since two places can make
  // the same number.
  const q = new Int32Array(positions.length);
  for (let i = 0; i < positions.length; i++) q[i] = Math.round(positions[i] * 2000);
  const first = new Map();
  const out = new Uint32Array(count);
  const next = new Int32Array(count).fill(-1);
  for (let i = 0; i < count; i++) {
    const x = q[i * 3], y = q[i * 3 + 1], z = q[i * 3 + 2];
    const key = (Math.imul(x, 73856093) ^ Math.imul(y, 19349663) ^ Math.imul(z, 83492791)) | 0;
    let at = first.get(key);
    let found = -1;
    for (let j = at === undefined ? -1 : at; j >= 0; j = next[j]) {
      if (q[j * 3] === x && q[j * 3 + 1] === y && q[j * 3 + 2] === z) {
        found = j;
        break;
      }
    }
    if (found >= 0) out[i] = found;
    else {
      out[i] = i;
      // Chained under its number, oldest last, so the first at a place is the one found.
      next[i] = at === undefined ? -1 : at;
      first.set(key, i);
    }
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
