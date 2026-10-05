// The one source of truth for the Vitals mark: geometry, colours and motion.
// Everything under brand/dist, every platform icon, the splash and the CSS
// keyframes are generated from this file by brand/scripts/build.mjs — never
// edit a generated asset by hand, change this and rebuild.
//
// The idea ("All clear"): a calm line that dips and then rises past where it
// started. It reads at once as a V, a sparkline and a tick — "your computer is
// being watched, and it is fine". Chosen at Gate B, 2026-10-05.

/** 48-unit keyline grid, Windows 11 / Fluent convention. */
export const GRID = 48;

/**
 * Optical sizes. One stroke cannot serve 16 px and 512 px: at 16 px a 4.5-unit
 * stroke is 1.5 px and smears across two pixel rows, so the micro variant snaps
 * every node to the 3-unit pixel grid (16 px → 1 px = 3 units) at a 2 px stroke.
 */
export const glyph = {
  master: { d: 'M10 25.5H17L24 33.5L38 14', stroke: 4.5 }, // >= 64 px
  small: { d: 'M10 25.5H17L24 33L38 14.5', stroke: 5 }, // 24–48 px
  micro: { d: 'M9 27H18L24 33L39 15', stroke: 6 }, // 16–20 px, pixel-snapped
};

/** Pick the optical variant for a rendered pixel size. */
export function variantFor(px) {
  if (px <= 20) return glyph.micro;
  if (px <= 48) return glyph.small;
  return glyph.master;
}

/** Tile: a Fluent-ish rounded square inset so it sits level with OS icons. */
export const tile = { x: 3, y: 3, size: 42, radius: 10 };

/**
 * Fern. Authored in OKLCH; hex is the sRGB fallback. Proven in
 * brand/contrast-pairs.json (WCAG 2.2 AA, 0 failures).
 */
export const colour = {
  tileTop: { oklch: 'oklch(0.52 0.12 152)', hex: '#227c45' },
  tileBottom: { oklch: 'oklch(0.42 0.1 152)', hex: '#135c30' },
  glyph: { oklch: 'oklch(1 0 0)', hex: '#ffffff' },
  /** Signal lime: the "all clear" moment (success, OG accents). Never body text on light. */
  signal: { oklch: 'oklch(0.9 0.15 135)', hex: '#b2f48c' },
  /** Brand night: OG image, site hero, TV banner, Android splash. */
  night: { oklch: 'oklch(0.2 0.03 155)', hex: '#0a1a10' },
  nightRaised: { oklch: 'oklch(0.26 0.04 155)', hex: '#13291b' },
  ink: { hex: '#0f1a14' },
  paper: { hex: '#f1f7f2' },
  /** App accent preset (theme.css `[data-accent='green']`). */
  accent: { hue: 152, chroma: 0.13, lightL: 0.5 },
};

/** Motion: one brand curve for the mark, everything else from the UI tokens. */
export const motion = {
  ease: 'cubic-bezier(0.22, 1, 0.36, 1)', // logo.ease — a long, calm settle
  easeOvershoot: 'cubic-bezier(0.34, 1.56, 0.64, 1)',
  intro: { ms: 900, drawFrom: 0.15, tileFrom: 0.9 }, // tile settles, line draws
  thinking: { ms: 1600, segment: 0.28 }, // a highlight travels the line: "watching"
  success: { ms: 360 }, // line re-draws fast, tick end lifts in signal lime
  error: { ms: 180, px: 2 }, // three horizontal nudges, never colour only
  hover: { ms: 140, lift: 1 }, // 1 unit up, ≤ 2 px at any size
  morph: { ms: 380, name: 'vitals-mark' }, // splash → title bar (View Transitions)
};
