// A face's tint, laid as the game lays it, and the face's colour texture
// made from it. Nothing here touches the page: images in and out are
// `{ width, height, data }` with `data` RGBA bytes, so node can test it.
//
// The rule was found from the game's own baked tint textures: for every
// Skyrim.esm NPC with tint layers (2,292 of them), the race's masks laid
// this way come within the few levels DXT5 costs of the baked texture
// (mean 1.8, 1.1, 2.1 levels per channel; the 41 USSEP re-baked ones, which
// are not compressed, within 0.3). It starts with the skin tone; each layer
// in the order given is a straight lerp toward its colour by the mask's red
// times the layer's strength, with no gamma. Laying the layers in `TINI`
// order instead is worse for every NPC where the two differ (172); laying
// them in linear light is twice as far off. Every vanilla mask is the
// texture's size, so how a mask of another size is scaled was not seen: it
// is scaled bilinearly here.

/** A mask's coverage (its red) at `width` × `height`, scaled bilinearly where its size differs. */
export function coverage(mask, width, height) {
  const out = new Float32Array(width * height);
  const { width: mw, height: mh, data } = mask;
  if (mw === width && mh === height) {
    for (let i = 0; i < out.length; i++) out[i] = data[i * 4] / 255;
    return out;
  }
  const sx = mw / width;
  const sy = mh / height;
  for (let y = 0; y < height; y++) {
    const fy = Math.min(mh - 1, Math.max(0, (y + 0.5) * sy - 0.5));
    const y0 = Math.floor(fy);
    const y1 = Math.min(mh - 1, y0 + 1);
    const ty = fy - y0;
    for (let x = 0; x < width; x++) {
      const fx = Math.min(mw - 1, Math.max(0, (x + 0.5) * sx - 0.5));
      const x0 = Math.floor(fx);
      const x1 = Math.min(mw - 1, x0 + 1);
      const tx = fx - x0;
      const a = data[(y0 * mw + x0) * 4] * (1 - tx) + data[(y0 * mw + x1) * 4] * tx;
      const b = data[(y1 * mw + x0) * 4] * (1 - tx) + data[(y1 * mw + x1) * 4] * tx;
      out[y * width + x] = (a * (1 - ty) + b * ty) / 255;
    }
  }
  return out;
}

/**
 * The tint texture: `base` (the skin tone, `[r, g, b]` 0 to 1) with each of
 * `layers` (`{ mask, colour: [r, g, b] 0 to 1, strength 0 to 1 }`) laid over
 * it in turn. A layer without a mask, or of no strength, is skipped. RGBA bytes, opaque.
 */
export function compose(base, layers, width, height) {
  const n = width * height;
  const r = new Float32Array(n).fill(base[0] * 255);
  const g = new Float32Array(n).fill(base[1] * 255);
  const b = new Float32Array(n).fill(base[2] * 255);
  for (const layer of layers) {
    if (!layer.mask || !(layer.strength > 0)) continue;
    const cover = coverage(layer.mask, width, height);
    const s = Math.min(1, layer.strength);
    const [cr, cg, cb] = layer.colour.map((c) => c * 255);
    for (let i = 0; i < n; i++) {
      const a = cover[i] * s;
      // Most of a mask is empty: nothing to lay there.
      if (a === 0) continue;
      r[i] += (cr - r[i]) * a;
      g[i] += (cg - g[i]) * a;
      b[i] += (cb - b[i]) * a;
    }
  }
  const out = new Uint8ClampedArray(n * 4);
  for (let i = 0; i < n; i++) {
    out[i * 4] = r[i];
    out[i * 4 + 1] = g[i];
    out[i * 4 + 2] = b[i];
    out[i * 4 + 3] = 255;
  }
  return out;
}

/**
 * The face's colour texture: the head's texture times twice the tint,
 * clamped, the head's alpha kept. The tint is scaled to the head's size.
 * This was chosen by eye: there is no oracle for how the game draws a face
 * with its tint, as the game's shaders are not read.
 */
export function face(head, tint) {
  const { width, height } = head;
  const n = width * height;
  const same = tint.width === width && tint.height === height;
  const ch = same ? null : [0, 1, 2].map((k) => coverage(channel(tint, k), width, height));
  const out = new Uint8ClampedArray(n * 4);
  const d = head.data;
  for (let i = 0; i < n; i++) {
    for (let k = 0; k < 3; k++) {
      const t = same ? tint.data[i * 4 + k] / 255 : ch[k][i];
      out[i * 4 + k] = d[i * 4 + k] * 2 * t;
    }
    out[i * 4 + 3] = d[i * 4 + 3];
  }
  return { width, height, data: out };
}

/** Channel `k` of an image moved into red, for scaling it as a mask. */
function channel(image, k) {
  const data = new Uint8ClampedArray(image.data.length);
  for (let i = 0; i < data.length; i += 4) data[i] = image.data[i + k];
  return { width: image.width, height: image.height, data };
}
