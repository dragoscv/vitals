#!/usr/bin/env node
// Generates every brand asset from brand/mark.mjs. Run: node brand/scripts/build.mjs
//
// Outputs (all overwritten; never edit them by hand):
//   brand/logo/*.svg                        master, optical sizes, mono, inverse, wordmark, lockups, construction
//   brand/icons/msix/*                 MSIX AppList targetsize + unplated + lightunplated, tiles, StoreLogo
//   brand/icons/play-512.png           Google Play listing icon
//   apps/desktop/src-tauri/icons/*          Tauri bundle icons + icon.ico (16–256, 32 first)
//   apps/site/public/*                      favicon.svg/.ico, apple-touch, manifest icons, og.svg/.png, logo
//   apps/android/**/res/drawable/*.xml      adaptive fg/mono, status icon, TV banner, Wear fg/bg
//   apps/tizen/public/icon.png              Tizen launcher icon
//   packages/ui/src/styles/brand.css        mark keyframes (intro, thinking, success, error, hover, morph)
//   packages/ui/src/components/brandGeometry.ts  the geometry the React BrandMark renders
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';
import { GRID, glyph, variantFor, tile, colour, motion } from '../mark.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const at = (...p) => join(root, ...p);
const write = (p, data) => {
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, data);
};
const wordmark = JSON.parse(readFileSync(at('brand', 'wordmark.json'), 'utf8'));

// ---------- SVG builders ----------
const gradient = (id) =>
  `<linearGradient id="${id}" x1="0.2" y1="0" x2="0.8" y2="1"><stop offset="0" stop-color="${colour.tileTop.hex}"/><stop offset="1" stop-color="${colour.tileBottom.hex}"/></linearGradient>`;
const line = (v, stroke) =>
  `<path d="${v.d}" fill="none" stroke="${stroke}" stroke-width="${v.stroke}" stroke-linecap="round" stroke-linejoin="round"/>`;

/** The app icon: tile + glyph, optical variant chosen for the target pixel size. */
function iconSvg(px, { plate = true, glyphColour = colour.glyph.hex, inset = tile } = {}) {
  const v = variantFor(px);
  const body = plate
    ? `<defs>${gradient('t')}</defs><rect x="${inset.x}" y="${inset.y}" width="${inset.size}" height="${inset.size}" rx="${inset.radius}" fill="url(#t)"/>
  <rect x="${inset.x + 0.5}" y="${inset.y + 0.5}" width="${inset.size - 1}" height="${inset.size - 1}" rx="${inset.radius - 0.5}" fill="none" stroke="#ffffff" stroke-opacity="0.14"/>`
    : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="0 0 ${GRID} ${GRID}">${body}${line(v, glyphColour)}</svg>`;
}

/** Full-bleed square (Play, apple-touch, maskable): the tile fills the canvas, glyph scaled to the safe zone. */
function bleedSvg(px, scale) {
  const v = variantFor(px);
  const t = (1 - scale) * 24;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="0 0 48 48"><defs>${gradient('t')}</defs><rect width="48" height="48" fill="url(#t)"/><g transform="translate(${t} ${t}) scale(${scale})">${line(v, colour.glyph.hex)}</g></svg>`;
}

const WM_SCALE = 20 / wordmark.capHeight; // cap height = 20 grid units
const WM_WIDTH = wordmark.width * WM_SCALE;
const wordmarkPath = (fill, x, baseline) =>
  `<path transform="translate(${x} ${baseline}) scale(${WM_SCALE})" fill="${fill}" d="${wordmark.d}"/>`;

function lockupSvg(ink) {
  const w = Math.ceil(48 + 8 + WM_WIDTH + 3);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} 48" width="${w * 4}" height="192" role="img" aria-label="Vitals"><defs>${gradient('t')}</defs>
  <rect x="3" y="3" width="42" height="42" rx="10" fill="url(#t)"/>${line(glyph.master, colour.glyph.hex)}${wordmarkPath(ink, 56, 34)}</svg>`;
}

function constructionSvg() {
  let grid = '';
  for (let i = 0; i <= GRID; i += 3) {
    const major = i % 12 === 0;
    grid += `<path d="M${i} 0V48M0 ${i}H48" stroke="${major ? '#7a8a80' : '#c7d1cb'}" stroke-width="${major ? 0.12 : 0.06}"/>`;
  }
  const nodes = glyph.master.d.match(/[\d.]+\s*[\d.]+|H[\d.]+/g);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="-4 -4 56 64" width="672" height="768" font-family="Segoe UI, sans-serif">
  <rect x="-4" y="-4" width="56" height="64" fill="#fbfdfb"/>${grid}
  <rect x="3" y="3" width="42" height="42" rx="10" fill="${colour.tileTop.hex}" fill-opacity="0.12" stroke="${colour.tileTop.hex}" stroke-width="0.15"/>
  <circle cx="24" cy="24" r="16" fill="none" stroke="#c0392b" stroke-width="0.1" stroke-dasharray="0.6 0.4"/>
  ${line(glyph.master, colour.tileBottom.hex).replace('/>', ' stroke-opacity="0.85"/>')}
  <path d="${glyph.master.d}" fill="none" stroke="#c0392b" stroke-width="0.12"/>
  <text x="0" y="51.5" font-size="1.6" fill="#33443a">48-unit grid · tile 42 @ (3,3) r10 · stroke 4.5 (master) / 5 (small) / 6 (micro, pixel-snapped)</text>
  <text x="0" y="54" font-size="1.6" fill="#33443a">nodes ${nodes ? nodes.join(' · ') : ''} · glyph centred on the 16-unit optical circle</text>
  <text x="0" y="56.5" font-size="1.6" fill="#33443a">clear space = 6 units (one stroke + node) on every side · minimum 16 px (micro variant)</text></svg>`;
}

/** Favicon SVG with its own dark-mode colours, as the web spec recommends. */
const faviconSvg = () =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><style>.t{fill:${colour.tileTop.hex}}@media (prefers-color-scheme:dark){.t{fill:#2f9a58}}</style><rect class="t" x="3" y="3" width="42" height="42" rx="10"/>${line(glyph.small, '#ffffff')}</svg>`;

function ogSvg() {
  const spark = 'M0 430H260L330 500L520 280L600 340L760 220L900 260L1200 170';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630" width="1200" height="630" role="img" aria-labelledby="t d">
  <title id="t">Vitals</title><desc id="d">See what your computer is actually doing. A free, open-source system monitor and task manager for Windows.</desc>
  <defs>${gradient('tg')}<linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${colour.night.hex}"/><stop offset="1" stop-color="${colour.nightRaised.hex}"/></linearGradient></defs>
  <rect width="1200" height="630" fill="url(#bg)"/>
  <path d="${spark}" fill="none" stroke="${colour.signal.hex}" stroke-opacity="0.18" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>
  <g transform="translate(96 120) scale(3)"><rect x="3" y="3" width="42" height="42" rx="10" fill="url(#tg)"/>${line(glyph.master, '#ffffff')}</g>
  ${wordmarkPath(colour.paper.hex, 266, 216).replace(`scale(${WM_SCALE})`, `scale(${WM_SCALE * 3.6})`)}
  <text x="96" y="380" font-family="Segoe UI Variable Display, Segoe UI, Inter, sans-serif" font-size="46" font-weight="600" fill="${colour.paper.hex}">See what your computer is actually doing.</text>
  <text x="96" y="440" font-family="Segoe UI Variable Text, Segoe UI, Inter, sans-serif" font-size="30" fill="${colour.paper.hex}" fill-opacity="0.72">Free, open-source system monitor and task manager for Windows.</text>
  <text x="96" y="540" font-family="Segoe UI Variable Text, Segoe UI, Inter, sans-serif" font-size="26" fill="${colour.signal.hex}">vitals.dragoscatalin.ro</text></svg>`;
}

// ---------- raster + ICO ----------
// The SVG already declares width = px; render at 4× and downsample for clean anti-aliasing.
const png = (svg, px) => sharp(Buffer.from(svg), { density: 288 }).resize(px, px).png().toBuffer();

/** PNG-in-ICO (Vista+). Tauri wants the 32 px entry first. */
function ico(entries) {
  const head = Buffer.alloc(6 + 16 * entries.length);
  head.writeUInt16LE(0, 0);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(entries.length, 4);
  let offset = head.length;
  entries.forEach(({ px, data }, i) => {
    const o = 6 + i * 16;
    head.writeUInt8(px >= 256 ? 0 : px, o);
    head.writeUInt8(px >= 256 ? 0 : px, o + 1);
    head.writeUInt16LE(1, o + 4);
    head.writeUInt16LE(32, o + 6);
    head.writeUInt32LE(data.length, o + 8);
    head.writeUInt32LE(offset, o + 12);
    offset += data.length;
  });
  return Buffer.concat([head, ...entries.map((e) => e.data)]);
}

// ---------- Android vectors ----------
/** Maps the 48 grid into the 108 dp adaptive canvas; 1.6× keeps the stroke inside the 66 dp safe circle. */
const A_SCALE = 1.6;
const A_T = 54 - 24 * A_SCALE;
const androidLine = (
  v,
  colourHex,
  extra = '',
) => `    <group android:translateX="${A_T}" android:translateY="${A_T}" android:scaleX="${A_SCALE}" android:scaleY="${A_SCALE}">
        <path
            android:name="line"
            android:fillColor="#00000000"
            android:strokeColor="${colourHex}"
            android:strokeWidth="${v.stroke}"
            android:strokeLineCap="round"
            android:strokeLineJoin="round"${extra}
            android:pathData="${v.d}" />
    </group>`;
const vector = (w, h, vw, vh, body, comment) => `<?xml version="1.0" encoding="utf-8"?>
<!-- ${comment} Generated by brand/scripts/build.mjs from brand/mark.mjs; do not edit. -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="${w}dp"
    android:height="${h}dp"
    android:viewportWidth="${vw}"
    android:viewportHeight="${vh}">
${body}
</vector>
`;
const androidBackground = () =>
  vector(
    108,
    108,
    108,
    108,
    `    <path android:pathData="M0,0h108v108h-108z">
        <aapt:attr name="android:fillColor" xmlns:aapt="http://schemas.android.com/aapt">
            <gradient android:type="linear" android:startX="30" android:startY="0" android:endX="78" android:endY="108">
                <item android:offset="0" android:color="#FF${colour.tileTop.hex.slice(1)}" />
                <item android:offset="1" android:color="#FF${colour.tileBottom.hex.slice(1)}" />
            </gradient>
        </aapt:attr>
    </path>`,
    'Adaptive icon background: the fern tile gradient, lit from the top left.',
  ).replace(
    '<vector xmlns:android',
    '<vector xmlns:aapt="http://schemas.android.com/aapt" xmlns:android',
  );

function tvBanner() {
  // 320×180 dp banner: night field, tile + outlined wordmark (no font needed on the TV).
  const s = 1.8; // tile 75.6 dp, centred vertically
  const k = 2.1; // wordmark: 2.1 grid units per unit → cap height 42 dp, width ≈ 155 dp
  const wmScale = WM_SCALE * k;
  const wmX = 40 + 42 * s + 16;
  const baseline = 90 + (20 * k) / 2;
  return vector(
    320,
    180,
    320,
    180,
    `    <path android:fillColor="${colour.night.hex}" android:pathData="M0,0h320v180h-320z" />
    <group android:translateX="${40 - 3 * s}" android:translateY="${90 - 24 * s}" android:scaleX="${s}" android:scaleY="${s}">
        <path android:fillColor="${colour.tileTop.hex}" android:pathData="M13,3h22a10,10 0,0 1,10 10v22a10,10 0,0 1,-10 10h-22a10,10 0,0 1,-10 -10v-22a10,10 0,0 1,10 -10z" />
        <path android:fillColor="#00000000" android:strokeColor="#FFFFFF" android:strokeWidth="${glyph.small.stroke}" android:strokeLineCap="round" android:strokeLineJoin="round" android:pathData="${glyph.small.d}" />
    </group>
    <group android:translateX="${wmX}" android:translateY="${baseline}" android:scaleX="${wmScale}" android:scaleY="${wmScale}">
        <path android:fillColor="${colour.paper.hex}" android:pathData="${wordmark.d}" />
    </group>`,
    'Leanback launcher banner. The wordmark is outlined Inter SemiBold (OFL), so the TV needs no font.',
  );
}

// ---------- CSS motion (one source → keyframes) ----------
function brandCss() {
  const m = motion;
  return `/*
 * The Vitals mark in motion. Generated by brand/scripts/build.mjs from
 * brand/mark.mjs — change the source and rebuild; do not edit this file.
 *
 * Every state is derived from the mark's own construction: the tile settles,
 * the line draws, a highlight travels the line while working, the tick end
 * lifts on success, and an error is a horizontal nudge (shape, not colour).
 * Only transform, opacity and stroke-dashoffset animate (compositor-only).
 * The static mark is the final frame of every animation.
 */
.vitals-mark {
  --mark-ease: ${m.ease};
  overflow: visible;
}
.vitals-mark .vm-line,
.vitals-mark .vm-trace {
  stroke-dasharray: 1;
  stroke-dashoffset: 0;
}
.vitals-mark .vm-trace {
  opacity: 0;
}
.vitals-mark .vm-tile,
.vitals-mark .vm-glyph {
  transform-box: fill-box;
  transform-origin: center;
}

/* intro — tile settles, then the line draws from its calm start to the tick */
.vitals-mark[data-state='intro'] .vm-tile {
  animation: vm-tile-in ${Math.round(m.intro.ms * 0.55)}ms var(--mark-ease) both;
}
.vitals-mark[data-state='intro'] .vm-line {
  animation: vm-draw ${Math.round(m.intro.ms * 0.7)}ms var(--mark-ease) ${Math.round(m.intro.ms * 0.3)}ms both;
}
@keyframes vm-tile-in {
  from { opacity: 0; transform: scale(${m.intro.tileFrom}); }
  to { opacity: 1; transform: scale(1); }
}
@keyframes vm-draw {
  from { stroke-dashoffset: 1; }
  to { stroke-dashoffset: 0; }
}

/* thinking — a short highlight travels the line: the product watching */
.vitals-mark[data-state='thinking'] .vm-trace {
  opacity: 1;
  stroke-dasharray: ${m.thinking.segment} ${1 + m.thinking.segment};
  animation: vm-trace ${m.thinking.ms}ms cubic-bezier(0.45, 0, 0.55, 1) infinite;
}
.vitals-mark[data-state='thinking'] .vm-line {
  opacity: 0.55;
}
@keyframes vm-trace {
  from { stroke-dashoffset: ${m.thinking.segment}; }
  to { stroke-dashoffset: ${-1}; }
}

/* success — the line re-draws quickly and the whole glyph lifts once */
.vitals-mark[data-state='success'] .vm-line {
  animation: vm-draw ${m.success.ms}ms var(--mark-ease) both;
}
.vitals-mark[data-state='success'] .vm-glyph {
  animation: vm-lift ${m.success.ms}ms ${m.easeOvershoot} both;
}
@keyframes vm-lift {
  0% { transform: translateY(0) scale(1); }
  45% { transform: translateY(-1.5px) scale(1.06); }
  100% { transform: translateY(0) scale(1); }
}

/* error — three short horizontal nudges; readable without colour */
.vitals-mark[data-state='error'] .vm-glyph {
  animation: vm-nudge ${m.error.ms}ms linear both;
}
@keyframes vm-nudge {
  0%, 100% { transform: translateX(0); }
  17% { transform: translateX(-${m.error.px}px); }
  50% { transform: translateX(${m.error.px}px); }
  83% { transform: translateX(-${m.error.px / 2}px); }
}

/* hover / press — micro, at most a pixel */
.vitals-mark[data-interactive] .vm-glyph {
  transition: transform ${m.hover.ms}ms var(--mark-ease);
}
.vitals-mark[data-interactive]:hover .vm-glyph {
  transform: translateY(-${m.hover.lift}px);
}
.vitals-mark[data-interactive]:active .vm-glyph {
  transform: translateY(0) scale(0.96);
}

/* morph — splash mark → title-bar mark, by the View Transitions API */
::view-transition-group(${m.morph.name}) {
  animation-duration: ${m.morph.ms}ms;
  animation-timing-function: var(--mark-ease, ${m.ease});
}

@media (prefers-reduced-motion: reduce) {
  .vitals-mark .vm-tile,
  .vitals-mark .vm-line,
  .vitals-mark .vm-glyph {
    animation: none !important;
    transition: none !important;
  }
  /* Working is still visible: a static trace over a dimmed line, no movement. */
  .vitals-mark[data-state='thinking'] .vm-trace {
    animation: none;
    stroke-dasharray: 1;
    opacity: 0.9;
  }
  ::view-transition-group(${m.morph.name}) {
    animation-duration: 0s;
  }
}
`;
}

function geometryTs() {
  return `// Generated by brand/scripts/build.mjs from brand/mark.mjs — do not edit.
export const MARK_GRID = ${GRID};
export const MARK_TILE = ${JSON.stringify(tile)} as const;
export const MARK_GLYPH = ${JSON.stringify(glyph, null, 2)} as const;
export const MARK_COLOUR = { top: '${colour.tileTop.hex}', bottom: '${colour.tileBottom.hex}', signal: '${colour.signal.hex}' } as const;
export const MARK_MORPH_NAME = '${motion.morph.name}';
export const MARK_INTRO_MS = ${motion.intro.ms};

export type MarkVariant = keyof typeof MARK_GLYPH;

/** The optical variant for a rendered pixel size; see brand/mark.mjs. */
export function markVariantFor(px: number): MarkVariant {
  if (px <= 20) return 'micro';
  if (px <= 48) return 'small';
  return 'master';
}
`;
}

// ---------- write everything ----------
const tasks = [];
const pngTo = (p, svg, px) => tasks.push(png(svg, px).then((b) => write(p, b)));

// Logo pack
write(at('brand/logo/mark.svg'), iconSvg(512));
write(at('brand/logo/mark-small.svg'), iconSvg(32));
write(at('brand/logo/mark-micro.svg'), iconSvg(16));
write(at('brand/logo/mark-mono.svg'), iconSvg(512, { plate: false, glyphColour: 'currentColor' }));
write(
  at('brand/logo/mark-inverse.svg'),
  iconSvg(512, { plate: false, glyphColour: colour.tileBottom.hex }),
);
write(
  at('brand/logo/wordmark.svg'),
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -21 ${Math.ceil(WM_WIDTH) + 1} 22" role="img" aria-label="Vitals">${wordmarkPath('currentColor', 0, 0)}</svg>`,
);
write(at('brand/logo/lockup-light.svg'), lockupSvg(colour.ink.hex));
write(at('brand/logo/lockup-dark.svg'), lockupSvg(colour.paper.hex));
write(at('brand/logo/construction.svg'), constructionSvg());

// Desktop (Tauri) — same filenames tauri.conf.json already lists
const tauri = (n) => at('apps/desktop/src-tauri/icons', n);
pngTo(tauri('32x32.png'), iconSvg(32), 32);
pngTo(tauri('128x128.png'), iconSvg(128), 128);
pngTo(tauri('128x128@2x.png'), iconSvg(256), 256);
pngTo(tauri('icon.png'), iconSvg(512), 512);
for (const s of [30, 44, 71, 89, 107, 142, 150, 284, 310])
  pngTo(tauri(`Square${s}x${s}Logo.png`), iconSvg(s), s);
pngTo(tauri('StoreLogo.png'), iconSvg(50), 50);
const icoSizes = [32, 16, 20, 24, 30, 36, 40, 48, 60, 64, 72, 80, 96, 256];
tasks.push(
  Promise.all(icoSizes.map(async (px) => ({ px, data: await png(iconSvg(px), px) }))).then(
    (entries) => {
      write(tauri('icon.ico'), ico(entries));
      write(
        at('apps/site/public/favicon.ico'),
        ico(entries.filter((e) => e.px === 32 || e.px === 16)),
      );
    },
  ),
);

// MSIX — AppList plated + unplated (dark taskbar) + lightunplated, and the scaled tiles
const msix = (n) => at('brand/icons/msix', n);
for (const s of [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256]) {
  pngTo(msix(`Square44x44Logo.targetsize-${s}.png`), iconSvg(s), s);
  pngTo(msix(`Square44x44Logo.targetsize-${s}_altform-unplated.png`), iconSvg(s), s);
  pngTo(msix(`Square44x44Logo.targetsize-${s}_altform-lightunplated.png`), iconSvg(s), s);
}
const scales = { 100: 1, 125: 1.25, 150: 1.5, 200: 2, 400: 4 };
for (const [scale, k] of Object.entries(scales)) {
  pngTo(
    msix(`Square44x44Logo.scale-${scale}.png`),
    iconSvg(Math.round(44 * k)),
    Math.round(44 * k),
  );
  pngTo(
    msix(`Square71x71Logo.scale-${scale}.png`),
    iconSvg(Math.round(71 * k)),
    Math.round(71 * k),
  );
  pngTo(
    msix(`Square150x150Logo.scale-${scale}.png`),
    iconSvg(Math.round(150 * k)),
    Math.round(150 * k),
  );
  pngTo(
    msix(`Square310x310Logo.scale-${scale}.png`),
    iconSvg(Math.round(310 * k)),
    Math.round(310 * k),
  );
  pngTo(msix(`StoreLogo.scale-${scale}.png`), iconSvg(Math.round(50 * k)), Math.round(50 * k));
  const w = Math.round(310 * k);
  const h = Math.round(150 * k);
  const m = Math.round(150 * k * 0.6);
  tasks.push(
    png(iconSvg(m), m).then((mark) =>
      sharp({
        create: { width: w, height: h, channels: 4, background: { r: 0, g: 0, b: 0, alpha: 0 } },
      })
        .composite([{ input: mark, gravity: 'centre' }])
        .png()
        .toBuffer()
        .then((b) => write(msix(`Wide310x150Logo.scale-${scale}.png`), b)),
    ),
  );
}
pngTo(at('brand/icons/play-512.png'), bleedSvg(512, 0.62), 512);

// Microsoft Store listing art: 9:16 poster (the main Store logo on Windows) and
// 1:1 box art. Night field, the mark centred, outlined wordmark below.
function storeArtSvg(w, h) {
  const markPx = Math.round(Math.min(w, h) * 0.42);
  const mx = (w - markPx) / 2;
  const my = h * (h > w ? 0.3 : 0.2);
  const wmScale = (markPx * 0.22) / wordmark.capHeight;
  const wmW = wordmark.width * wmScale;
  const baseline = my + markPx + markPx * 0.42;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
  <defs>${gradient('t')}<radialGradient id="glow" cx="0.5" cy="${(my + markPx / 2) / h}" r="0.6"><stop offset="0" stop-color="${colour.nightRaised.hex}"/><stop offset="1" stop-color="${colour.night.hex}"/></radialGradient></defs>
  <rect width="${w}" height="${h}" fill="url(#glow)"/>
  <g transform="translate(${mx} ${my}) scale(${markPx / 48})"><rect x="3" y="3" width="42" height="42" rx="10" fill="url(#t)"/>${line(glyph.master, '#ffffff')}</g>
  <path transform="translate(${(w - wmW) / 2} ${baseline}) scale(${wmScale})" fill="${colour.paper.hex}" d="${wordmark.d}"/></svg>`;
}
for (const [name, w, h] of [
  ['poster-720x1080', 720, 1080],
  ['poster-1440x2160', 1440, 2160],
  ['box-1080', 1080, 1080],
  ['box-2160', 2160, 2160],
]) {
  tasks.push(
    sharp(Buffer.from(storeArtSvg(w, h)))
      .png()
      .toBuffer()
      .then((b) => write(at(`brand/icons/store/${name}.png`), b)),
  );
}

// Web / site
write(at('apps/site/public/favicon.svg'), faviconSvg());
write(at('apps/site/src/assets/logo.svg'), iconSvg(64));
write(at('apps/site/public/og.svg'), ogSvg());
tasks.push(
  sharp(Buffer.from(ogSvg()))
    .png()
    .toBuffer()
    .then((b) => write(at('apps/site/public/og.png'), b)),
);
pngTo(at('apps/site/public/apple-touch-icon.png'), bleedSvg(180, 0.78), 180);
pngTo(at('apps/site/public/icon-192.png'), iconSvg(192), 192);
pngTo(at('apps/site/public/icon-512.png'), iconSvg(512), 512);
pngTo(at('apps/site/public/icon-512-maskable.png'), bleedSvg(512, 0.62), 512);
write(
  at('apps/site/public/manifest.webmanifest'),
  `${JSON.stringify(
    {
      name: 'Vitals',
      short_name: 'Vitals',
      start_url: '/',
      display: 'browser',
      background_color: colour.night.hex,
      theme_color: colour.night.hex,
      icons: [
        { src: '/icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
        { src: '/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
        { src: '/icon-512-maskable.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
      ],
    },
    null,
    2,
  )}\n`,
);

// Tizen
pngTo(at('apps/tizen/public/icon.png'), iconSvg(512), 512);

// Android (phone + TV share the foreground; Wear has its own copy of the files)
const fg = vector(
  108,
  108,
  108,
  108,
  androidLine(glyph.master, '#FFFFFF'),
  'Adaptive icon foreground: the All clear line inside the 66 dp safe zone.',
);
const mono = vector(
  108,
  108,
  108,
  108,
  androidLine(glyph.master, '#FF000000'),
  'Themed (monochrome) icon: the same silhouette; the launcher tints it.',
);
for (const app of ['app', 'tv']) {
  write(at(`apps/android/${app}/src/main/res/drawable/ic_launcher_foreground.xml`), fg);
  write(at(`apps/android/${app}/src/main/res/drawable/ic_launcher_monochrome.xml`), mono);
  write(
    at(`apps/android/${app}/src/main/res/drawable/ic_launcher_background.xml`),
    androidBackground(),
  );
}
write(at('apps/android/wear/src/main/res/drawable/ic_launcher_foreground.xml'), fg);
write(
  at('apps/android/wear/src/main/res/drawable/ic_launcher_background.xml'),
  androidBackground(),
);
write(
  at('apps/android/app/src/main/res/drawable/ic_stat_vitals.xml'),
  vector(
    24,
    24,
    48,
    48,
    `    <path android:fillColor="#00000000" android:strokeColor="#FFFFFFFF" android:strokeWidth="${glyph.micro.stroke}" android:strokeLineCap="round" android:strokeLineJoin="round" android:pathData="${glyph.micro.d}" />`,
    'Status bar and tile icon: the line alone, white on transparent as the platform requires.',
  ),
);
write(at('apps/android/tv/src/main/res/drawable/tv_banner.xml'), tvBanner());

// UI package: motion CSS + geometry for the React mark
write(at('packages/ui/src/styles/brand.css'), brandCss());
write(at('packages/ui/src/components/brandGeometry.ts'), geometryTs());

// Desktop splash: inline in index.html so it paints before any JS. The mark
// assembles (intro) and then, while start-up continues, the trace runs.
{
  const p = at('apps/desktop/index.html');
  const html = readFileSync(p, 'utf8');
  const open = '<!-- brand:splash (generated by brand/scripts/build.mjs) -->';
  const close = '<!-- /brand:splash -->';
  const v = glyph.master;
  const i = motion.intro.ms;
  const block = `${open}
      <style>
        .splash-mark { width: 64px; height: 64px; overflow: visible; }
        .splash-mark .t { transform-box: fill-box; transform-origin: center; animation: sm-tile ${Math.round(i * 0.55)}ms ${motion.ease} both; }
        .splash-mark .l { stroke-dasharray: 1; animation: sm-draw ${Math.round(i * 0.7)}ms ${motion.ease} ${Math.round(i * 0.3)}ms both; }
        .splash-mark .r { stroke-dasharray: ${motion.thinking.segment} ${1 + motion.thinking.segment}; opacity: 0; animation: sm-trace ${motion.thinking.ms}ms cubic-bezier(0.45, 0, 0.55, 1) ${i}ms infinite; }
        @keyframes sm-tile { from { opacity: 0; transform: scale(${motion.intro.tileFrom}); } }
        @keyframes sm-draw { from { stroke-dashoffset: 1; } }
        @keyframes sm-trace { from { opacity: 1; stroke-dashoffset: ${motion.thinking.segment}; } to { opacity: 1; stroke-dashoffset: -1; } }
        @media (prefers-reduced-motion: reduce) { .splash-mark * { animation: none !important; } }
      </style>
      <svg class="splash-mark" id="splash-mark" viewBox="0 0 48 48">
        <defs><linearGradient id="sg" x1="0.2" y1="0" x2="0.8" y2="1"><stop offset="0" stop-color="${colour.tileTop.hex}"/><stop offset="1" stop-color="${colour.tileBottom.hex}"/></linearGradient></defs>
        <rect class="t" x="${tile.x}" y="${tile.y}" width="${tile.size}" height="${tile.size}" rx="${tile.radius}" fill="url(#sg)"/>
        <path class="l" d="${v.d}" pathLength="1" fill="none" stroke="#fff" stroke-width="${v.stroke}" stroke-linecap="round" stroke-linejoin="round"/>
        <path class="r" d="${v.d}" pathLength="1" fill="none" stroke="${colour.signal.hex}" stroke-width="${v.stroke}" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
      ${close}`;
  const a = html.indexOf(open);
  const b = html.indexOf(close);
  if (a < 0 || b < 0) throw new Error('index.html is missing the brand:splash markers');
  write(p, html.slice(0, a) + block + html.slice(b + close.length));
}

await Promise.all(tasks);
console.log(`brand: wrote ${tasks.length} rasters + vectors, css, geometry`);
