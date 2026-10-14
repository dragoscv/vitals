// Bodymovin (Lottie) JSON for each motion in motion.mjs, sampled from the same
// pose function the CSS keyframes describe, so lottie-web / lottie-android
// play exactly what the animated SVG and the videos show.
//
// Deliberately primitive: shape layers with a rounded rect + linear gradient
// fill, an open path + stroke + trim path, and layer transforms. No
// expressions, mattes, precomps or effects — the subset every Lottie player
// renders identically.
import { tile, colour, variantFor } from '../../mark.mjs';
import { MOTIONS, glyphCentre } from './motion.mjs';

const FR = 60;
const W = 512;
const K = W / 48; // grid unit → composition pixel

const rgb = (hex) => {
  const n = Number.parseInt(hex.slice(1), 16);
  return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255].map(
    (x) => Math.round(x * 1000) / 1000,
  );
};
const r3 = (x) => Math.round(x * 1000) / 1000;
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Animated property from per-frame samples. Linear interpolation between
 * samples (60 fps, so the curve is reproduced to within a frame); interior
 * points of flat runs are dropped to keep the file small.
 */
function prop(samples) {
  if (samples.every((s) => same(s, samples[0]))) return { a: 0, k: samples[0] };
  const kf = [];
  samples.forEach((s, i) => {
    const prev = samples[i - 1];
    const next = samples[i + 1];
    if (i > 0 && i < samples.length - 1 && same(prev, s) && same(next, s)) return;
    const dims = s.length;
    kf.push({
      t: i,
      s,
      i: { x: Array(dims).fill(1), y: Array(dims).fill(1) },
      o: { x: Array(dims).fill(0), y: Array(dims).fill(0) },
    });
  });
  return { a: 1, k: kf };
}

const staticTr = () => ({
  ty: 'tr',
  p: { a: 0, k: [0, 0] },
  a: { a: 0, k: [0, 0] },
  s: { a: 0, k: [100, 100] },
  r: { a: 0, k: 0 },
  o: { a: 0, k: 100 },
  sk: { a: 0, k: 0 },
  sa: { a: 0, k: 0 },
});

function layer(ind, nm, shapes, ks, op) {
  return { ddd: 0, ind, ty: 4, nm, sr: 1, ks, ao: 0, shapes, ip: 0, op, st: 0, bm: 0 };
}

/** Build the Lottie document for one motion id. */
export function lottieFor(id) {
  const mo = MOTIONS[id];
  const op = Math.round((mo.periodMs * FR) / 1000);
  const frames = Array.from({ length: op + 1 }, (_, f) => mo.at((f * 1000) / FR));
  const v = variantFor(W);
  const { pts, cx, cy } = glyphCentre(v);
  const lineColour = mo.bare ? colour.tileBottom.hex : colour.glyph.hex;

  const pathShape = {
    ty: 'sh',
    nm: 'All clear',
    ks: {
      a: 0,
      k: { c: false, v: pts, i: pts.map(() => [0, 0]), o: pts.map(() => [0, 0]) },
    },
  };
  const stroke = (hex, w) => ({
    ty: 'st',
    c: { a: 0, k: [...rgb(hex), 1] },
    o: { a: 0, k: 100 },
    w: { a: 0, k: w },
    lc: 2,
    lj: 2,
    ml: 4,
  });
  const trim = (startKey, endKey) => ({
    ty: 'tm',
    s: prop(frames.map((p) => [r3(p[startKey] * 100)])),
    e: prop(frames.map((p) => [r3(p[endKey] * 100)])),
    o: { a: 0, k: 0 },
    m: 1,
  });
  // Glyph transform about its fill-box centre, as CSS transform-box: fill-box does.
  const glyphKs = (opacityKey) => ({
    o: prop(frames.map((p) => [r3(p[opacityKey] * 100)])),
    r: { a: 0, k: 0 },
    a: { a: 0, k: [cx, cy, 0] },
    p: prop(frames.map((p) => [r3((cx + p.glyphX) * K), r3((cy + p.glyphY) * K), 0])),
    s: prop(frames.map((p) => [r3(p.glyphScale * K * 100), r3(p.glyphScale * K * 100), 100])),
  });

  const layers = [];
  if (!mo.bare) {
    layers.push(
      layer(
        1,
        'trace',
        [
          {
            ty: 'gr',
            nm: 'trace',
            it: [
              pathShape,
              trim('traceStart', 'traceEnd'),
              stroke(colour.signal.hex, v.stroke),
              staticTr(),
            ],
          },
        ],
        glyphKs('traceOpacity'),
        op + 1,
      ),
    );
  }
  layers.push(
    layer(
      layers.length + 1,
      'line',
      [
        {
          ty: 'gr',
          nm: 'line',
          it: [pathShape, trim('lineStart', 'lineEnd'), stroke(lineColour, v.stroke), staticTr()],
        },
      ],
      glyphKs('lineOpacity'),
      op + 1,
    ),
  );
  if (!mo.bare) {
    const c = tile.x + tile.size / 2;
    layers.push(
      layer(
        layers.length + 1,
        'tile',
        [
          {
            ty: 'gr',
            nm: 'tile',
            it: [
              {
                ty: 'rc',
                d: 1,
                p: { a: 0, k: [c, c] },
                s: { a: 0, k: [tile.size, tile.size] },
                r: { a: 0, k: tile.radius },
              },
              {
                ty: 'gf',
                o: { a: 0, k: 100 },
                r: 1,
                t: 1,
                // SVG gradient x1=.2 y1=0 x2=.8 y2=1 over the tile's bounding box.
                s: { a: 0, k: [tile.x + 0.2 * tile.size, tile.y] },
                e: { a: 0, k: [tile.x + 0.8 * tile.size, tile.y + tile.size] },
                g: {
                  p: 2,
                  k: { a: 0, k: [0, ...rgb(colour.tileTop.hex), 1, ...rgb(colour.tileBottom.hex)] },
                },
              },
              staticTr(),
            ],
          },
        ],
        {
          o: prop(frames.map((p) => [r3(p.tileOpacity * 100)])),
          r: { a: 0, k: 0 },
          a: { a: 0, k: [c, c, 0] },
          p: { a: 0, k: [c * K, c * K, 0] },
          s: prop(frames.map((p) => [r3(p.tileScale * K * 100), r3(p.tileScale * K * 100), 100])),
        },
        op + 1,
      ),
    );
  }
  return {
    v: '5.7.4',
    fr: FR,
    ip: 0,
    op,
    w: W,
    h: W,
    nm: `Vitals — ${mo.title}`,
    ddd: 0,
    assets: [],
    meta: { g: 'brand/scripts/kit.mjs from brand/mark.mjs', loop: mo.loop },
    layers,
  };
}

/** Structural check: the fields every Lottie player requires. Throws on failure. */
export function validateLottie(doc, name) {
  const need = ['v', 'fr', 'ip', 'op', 'w', 'h', 'layers'];
  for (const k of need) if (!(k in doc)) throw new Error(`${name}: missing ${k}`);
  if (!(doc.op > doc.ip)) throw new Error(`${name}: op must exceed ip`);
  if (!Array.isArray(doc.layers) || doc.layers.length === 0) throw new Error(`${name}: no layers`);
  for (const l of doc.layers) {
    for (const k of ['ty', 'ks', 'ip', 'op', 'shapes'])
      if (!(k in l)) throw new Error(`${name}: layer ${l.nm} missing ${k}`);
    const walk = (it) => {
      for (const s of it) {
        if (s.ty === 'gr') {
          if (s.it.at(-1)?.ty !== 'tr') throw new Error(`${name}: group ${s.nm} lacks a final tr`);
          walk(s.it);
        }
      }
    };
    walk(l.shapes);
    for (const p of Object.values(l.ks)) {
      if (p.a === 1 && p.k.some((kf, i) => i > 0 && kf.t <= p.k[i - 1].t))
        throw new Error(`${name}: keyframes out of order in ${l.nm}`);
    }
  }
  return true;
}
