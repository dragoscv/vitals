// The kit's motion set, derived from brand/mark.mjs so the files and the app
// (packages/ui/src/styles/brand.css) speak the same vocabulary:
//   intro  → tile settles (scale 0.9 → 1), line draws          (reveal)
//   success→ glyph lifts 1.5 units on the overshoot curve      (pulse)
//   thinking → a lime highlight travels the dimmed line        (thinking)
//   error  → three horizontal nudges, shape not colour         (alert)
// plus a bare loader (the line draws in and out) for inline spinners.
//
// Every motion is described twice from the same constants: as CSS keyframes
// (animated SVG, and the source the videos are rendered from) and as a pure
// function of time (sampled into Lottie keyframes). Only transform, opacity
// and stroke-dashoffset animate — the same compositor-only rule as the app.
import { glyph, tile, colour, motion as m, variantFor } from '../../mark.mjs';

/** CSS cubic-bezier as a function of progress, solved by Newton + bisection. */
export function bezier(x1, y1, x2, y2) {
  const cx = 3 * x1;
  const bx = 3 * (x2 - x1) - cx;
  const ax = 1 - cx - bx;
  const cy = 3 * y1;
  const by = 3 * (y2 - y1) - cy;
  const ay = 1 - cy - by;
  const sx = (t) => ((ax * t + bx) * t + cx) * t;
  const sy = (t) => ((ay * t + by) * t + cy) * t;
  const dx = (t) => (3 * ax * t + 2 * bx) * t + cx;
  return (x) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let t = x;
    for (let i = 0; i < 8; i++) {
      const e = sx(t) - x;
      if (Math.abs(e) < 1e-6) return sy(t);
      const d = dx(t);
      if (Math.abs(d) < 1e-6) break;
      t -= e / d;
    }
    let lo = 0;
    let hi = 1;
    t = x;
    while (hi - lo > 1e-6) {
      if (sx(t) < x) lo = t;
      else hi = t;
      t = (lo + hi) / 2;
    }
    return sy(t);
  };
}

const parseBezier = (s) => bezier(...s.match(/[\d.]+/g).map(Number));
export const EASE = m.ease;
export const OVERSHOOT = m.easeOvershoot;
/** The app's thinking curve (brand.css vm-trace). */
export const SWEEP = 'cubic-bezier(0.45, 0, 0.55, 1)';
const ease = parseBezier(EASE);
const overshoot = parseBezier(OVERSHOOT);
const sweep = parseBezier(SWEEP);

const clamp01 = (x) => Math.max(0, Math.min(1, x));
const lerp = (a, b, t) => a + (b - a) * t;
/** Progress of t through [start, start + dur], 0 before and 1 after. */
const span = (t, start, dur) => clamp01((t - start) / dur);

// Derived timings — identical arithmetic to build.mjs's brandCss().
const INTRO_TILE = Math.round(m.intro.ms * 0.55);
const INTRO_DRAW = Math.round(m.intro.ms * 0.7);
const INTRO_DELAY = Math.round(m.intro.ms * 0.3);
const LIFT_Y = 1.5;
const LIFT_S = 1.06;
const SEG = m.thinking.segment;

/** Lift keyframes 0 / 45 / 100 % with the curve applied per segment, as CSS does. */
function lift(p) {
  if (p <= 0 || p >= 1) return { y: 0, s: 1 };
  if (p < 0.45) {
    const k = overshoot(p / 0.45);
    return { y: lerp(0, -LIFT_Y, k), s: lerp(1, LIFT_S, k) };
  }
  const k = overshoot((p - 0.45) / 0.55);
  return { y: lerp(-LIFT_Y, 0, k), s: lerp(LIFT_S, 1, k) };
}

/** Head position of the travelling highlight → visible [start, end] of the path. */
function trace(p) {
  const head = p * (1 + SEG);
  return { start: clamp01(head - SEG), end: clamp01(head) };
}

const IDLE = Object.freeze({
  tileOpacity: 1,
  tileScale: 1,
  lineOpacity: 1,
  lineStart: 0,
  lineEnd: 1,
  traceOpacity: 0,
  traceStart: 0,
  traceEnd: 0,
  glyphX: 0,
  glyphY: 0,
  glyphScale: 1,
});

const REVEAL_LIFT_AT = m.intro.ms;
const PULSE_MS = 2400;
const ALERT_MS = 2200;
const LOADER_MS = m.thinking.ms;

/**
 * Each motion: CSS body for a scope prefix `P` (class selector) and keyframe
 * prefix `K`, plus `at(ms)` returning the pose. `periodMs` is the loop length;
 * `renderMs` is how long a video/GIF of it runs.
 */
export const MOTIONS = {
  reveal: {
    title: 'Logo reveal',
    says: 'The tile settles, the line draws, the tick lifts once: all clear.',
    loop: false,
    periodMs: 1600,
    renderMs: 2200,
    css: (P, K) => `
${P} .t { animation: ${K}tile ${INTRO_TILE}ms ${EASE} both; }
${P} .l { animation: ${K}draw ${INTRO_DRAW}ms ${EASE} ${INTRO_DELAY}ms both; }
${P} .g { animation: ${K}lift ${m.success.ms}ms ${OVERSHOOT} ${REVEAL_LIFT_AT}ms both; }
@keyframes ${K}tile { from { opacity: 0; transform: scale(${m.intro.tileFrom}); } to { opacity: 1; transform: scale(1); } }
@keyframes ${K}draw { from { stroke-dashoffset: 1; } to { stroke-dashoffset: 0; } }
@keyframes ${K}lift { 0% { transform: translateY(0) scale(1); } 45% { transform: translateY(-${LIFT_Y}px) scale(${LIFT_S}); } 100% { transform: translateY(0) scale(1); } }`,
    at: (t) => {
      const kt = ease(span(t, 0, INTRO_TILE));
      const kd = ease(span(t, INTRO_DELAY, INTRO_DRAW));
      const l = lift(span(t, REVEAL_LIFT_AT, m.success.ms));
      return {
        ...IDLE,
        tileOpacity: kt,
        tileScale: lerp(m.intro.tileFrom, 1, kt),
        lineEnd: kd,
        glyphY: l.y,
        glyphScale: l.s,
      };
    },
  },
  pulse: {
    title: 'Pulse (idle loop)',
    says: 'A periodic check-in: the glyph lifts and a lime highlight passes once, then rests.',
    loop: true,
    periodMs: PULSE_MS,
    renderMs: PULSE_MS,
    css: (P, K) => {
      const a = ((0.45 * m.success.ms) / PULSE_MS) * 100;
      const b = (m.success.ms / PULSE_MS) * 100;
      const c = (m.thinking.ms / PULSE_MS) * 100;
      return `
${P} .g { animation: ${K}lift ${PULSE_MS}ms ${OVERSHOOT} infinite; }
${P} .r { stroke-dasharray: ${SEG} ${1 + SEG}; animation: ${K}trace ${PULSE_MS}ms ${SWEEP} infinite; }
@keyframes ${K}lift { 0% { transform: translateY(0) scale(1); } ${a.toFixed(3)}% { transform: translateY(-${LIFT_Y}px) scale(${LIFT_S}); } ${b.toFixed(3)}%, 100% { transform: translateY(0) scale(1); } }
@keyframes ${K}trace { 0% { opacity: 1; stroke-dashoffset: ${SEG}; } ${c.toFixed(3)}% { opacity: 1; stroke-dashoffset: -1; } ${(c + 0.01).toFixed(3)}%, 100% { opacity: 0; stroke-dashoffset: -1; } }`;
    },
    at: (t) => {
      const p = t % PULSE_MS;
      const l = lift(span(p, 0, m.success.ms));
      const on = p < m.thinking.ms;
      const tr = trace(sweep(span(p, 0, m.thinking.ms)));
      return {
        ...IDLE,
        glyphY: l.y,
        glyphScale: l.s,
        traceOpacity: on ? 1 : 0,
        traceStart: tr.start,
        traceEnd: tr.end,
      };
    },
  },
  thinking: {
    title: 'Thinking',
    says: 'Working: a lime highlight travels the dimmed line. The app uses it for scans and benchmarks.',
    loop: true,
    periodMs: m.thinking.ms,
    renderMs: m.thinking.ms * 2,
    css: (P, K) => `
${P} .l { opacity: 0.55; }
${P} .r { opacity: 1; stroke-dasharray: ${SEG} ${1 + SEG}; animation: ${K}trace ${m.thinking.ms}ms ${SWEEP} infinite; }
@keyframes ${K}trace { from { stroke-dashoffset: ${SEG}; } to { stroke-dashoffset: -1; } }`,
    reduced: (P) => `${P} .r { stroke-dasharray: 1; stroke-dashoffset: 0; opacity: 0.9; }`,
    at: (t) => {
      const tr = trace(sweep((t % m.thinking.ms) / m.thinking.ms));
      return {
        ...IDLE,
        lineOpacity: 0.55,
        traceOpacity: 1,
        traceStart: tr.start,
        traceEnd: tr.end,
      };
    },
  },
  loader: {
    title: 'Loader',
    says: 'The bare line draws in and out — an inline spinner in currentColor.',
    loop: true,
    bare: true,
    periodMs: LOADER_MS,
    renderMs: LOADER_MS * 2,
    css: (P, K) => `
${P} .l { stroke-dasharray: 1 1; animation: ${K}load ${LOADER_MS}ms ${SWEEP} infinite; }
@keyframes ${K}load { 0% { stroke-dashoffset: 1; } 50% { stroke-dashoffset: 0; } 100% { stroke-dashoffset: -1; } }`,
    at: (t) => {
      const p = (t % LOADER_MS) / LOADER_MS;
      const a = p < 0.5 ? sweep(p / 0.5) : 1;
      const b = p < 0.5 ? 0 : sweep((p - 0.5) / 0.5);
      return { ...IDLE, tileOpacity: 0, lineStart: b, lineEnd: a };
    },
  },
  alert: {
    title: 'Alert',
    says: 'Three short horizontal nudges, then rest. Readable without colour.',
    loop: true,
    periodMs: ALERT_MS,
    renderMs: ALERT_MS,
    css: (P, K) => {
      const f = (x) => ((x * m.error.ms) / ALERT_MS) * 100;
      return `
${P} .g { animation: ${K}nudge ${ALERT_MS}ms linear infinite; }
@keyframes ${K}nudge { 0%, ${f(1).toFixed(3)}%, 100% { transform: translateX(0); } ${f(0.17).toFixed(3)}% { transform: translateX(-${m.error.px}px); } ${f(0.5).toFixed(3)}% { transform: translateX(${m.error.px}px); } ${f(0.83).toFixed(3)}% { transform: translateX(-${m.error.px / 2}px); } }`;
    },
    at: (t) => {
      const p = (t % ALERT_MS) / m.error.ms;
      const pts = [
        [0, 0],
        [0.17, -m.error.px],
        [0.5, m.error.px],
        [0.83, -m.error.px / 2],
        [1, 0],
      ];
      let x = 0;
      for (let i = 1; i < pts.length; i++) {
        const [p0, x0] = pts[i - 1];
        const [p1, x1] = pts[i];
        if (p >= p0 && p <= p1) x = lerp(x0, x1, (p - p0) / (p1 - p0));
      }
      return { ...IDLE, glyphX: x };
    },
  },
};

/**
 * A self-contained animated SVG of one motion. All classes, keyframes and ids
 * are prefixed with `vk-<id>` so several can be inlined into one HTML page.
 * `px` picks the optical variant (16 → micro), exactly as the app does.
 */
export function animatedSvg(id, { px = 512, line = colour.glyph.hex, title } = {}) {
  const mo = MOTIONS[id];
  const v = variantFor(px);
  const scope = `vk-${id}-${px}`;
  const P = `.${scope}`;
  const K = `${scope}-`;
  const stroke = mo.bare ? 'currentColor' : line;
  const tileEl = mo.bare
    ? ''
    : `<defs><linearGradient id="${scope}-g" x1="0.2" y1="0" x2="0.8" y2="1"><stop offset="0" stop-color="${colour.tileTop.hex}"/><stop offset="1" stop-color="${colour.tileBottom.hex}"/></linearGradient></defs><rect class="t" x="${tile.x}" y="${tile.y}" width="${tile.size}" height="${tile.size}" rx="${tile.radius}" fill="url(#${scope}-g)"/>`;
  const path = (cls, c) =>
    `<path class="${cls}" d="${v.d}" pathLength="1" fill="none" stroke="${c}" stroke-width="${v.stroke}" stroke-linecap="round" stroke-linejoin="round"/>`;
  const bareColour = mo.bare
    ? `${P} { color: ${colour.tileBottom.hex}; } @media (prefers-color-scheme: dark) { ${P} { color: #6cb882; } }`
    : '';
  const css = `${bareColour}
${P} { overflow: visible; }
${P} .t, ${P} .g { transform-box: fill-box; transform-origin: center; }
${P} .l, ${P} .r { stroke-dasharray: 1; stroke-dashoffset: 0; }
${P} .r { opacity: 0; }${mo.css(P, K)}
@media (prefers-reduced-motion: reduce) { ${P} * { animation: none !important; } ${mo.reduced ? mo.reduced(P) : ''} }
@media print { ${P} * { animation: none !important; } ${mo.reduced ? mo.reduced(P) : ''} }`;
  const label = title ?? `Vitals — ${mo.title}`;
  return `<svg xmlns="http://www.w3.org/2000/svg" class="${scope}" viewBox="0 0 48 48" width="${px}" height="${px}" role="img" aria-label="${label}"><title>${label}</title><style>${css}</style>${tileEl}<g class="g">${path('l', stroke)}${mo.bare ? '' : path('r', colour.signal.hex)}</g></svg>`;
}

/** The fill-box centre of a glyph variant (what CSS transform-origin: center resolves to). */
export function glyphCentre(v) {
  // d is "Mx yHxLx yLx y": rebuild the node list.
  const pts = [];
  const re = /([MLH])([\d.]+)(?:\s+([\d.]+))?/g;
  let mm;
  let y = 0;
  while ((mm = re.exec(v.d))) {
    if (mm[1] === 'H') pts.push([Number(mm[2]), y]);
    else {
      y = Number(mm[3]);
      pts.push([Number(mm[2]), y]);
    }
  }
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  return {
    pts,
    cx: (Math.min(...xs) + Math.max(...xs)) / 2,
    cy: (Math.min(...ys) + Math.max(...ys)) / 2,
  };
}

export { glyph, IDLE };
