// Shared pieces for the kit: paths, the static mark and lockup as inline SVG,
// the embedded Inter face, and the Playwright handle.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { glyph, tile, colour, variantFor } from '../../mark.mjs';

export const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
export const at = (...p) => join(root, ...p);
export const write = (p, data) => {
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, data);
};
export const brand = JSON.parse(readFileSync(at('brand', 'brand.json'), 'utf8'));
export const wordmark = JSON.parse(readFileSync(at('brand', 'wordmark.json'), 'utf8'));

let gid = 0;
const gradient = (id) =>
  `<linearGradient id="${id}" x1="0.2" y1="0" x2="0.8" y2="1"><stop offset="0" stop-color="${colour.tileTop.hex}"/><stop offset="1" stop-color="${colour.tileBottom.hex}"/></linearGradient>`;

/** Static mark (tile + line) at a rendered pixel size, optical variant included. */
export function markSvg(px, { bare = false, line = colour.glyph.hex, extra = '' } = {}) {
  const v = variantFor(px);
  const id = `kg${gid++}`;
  const plate = bare
    ? ''
    : `<defs>${gradient(id)}</defs><rect x="${tile.x}" y="${tile.y}" width="${tile.size}" height="${tile.size}" rx="${tile.radius}" fill="url(#${id})"/>`;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="${px}" height="${px}" aria-hidden="true">${plate}<path d="${v.d}" fill="none" stroke="${bare ? 'currentColor' : line}" stroke-width="${v.stroke}" stroke-linecap="round" stroke-linejoin="round"/>${extra}</svg>`;
}

/** Mark + outlined wordmark, `h` px tall; the font is never needed. */
export function lockupSvg(ink, h) {
  const scale = 20 / wordmark.capHeight;
  const wmW = wordmark.width * scale;
  const w = Math.ceil(48 + 8 + wmW + 3);
  const id = `kg${gid++}`;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} 48" height="${h}" width="${(h * w) / 48}" role="img" aria-label="Vitals"><defs>${gradient(id)}</defs><rect x="3" y="3" width="42" height="42" rx="10" fill="url(#${id})"/><path d="${glyph.master.d}" fill="none" stroke="#fff" stroke-width="${glyph.master.stroke}" stroke-linecap="round" stroke-linejoin="round"/><path transform="translate(56 34) scale(${scale})" fill="${ink}" d="${wordmark.d}"/></svg>`;
}

/** Inter Variable (OFL-1.1, brand/OFL-Inter.txt), embedded so every render is identical. */
export const FONT = 'InterKit';
export const fontFace = () =>
  `@font-face{font-family:${FONT};src:url(data:font/woff2;base64,${readFileSync(at('brand/fonts/InterVariable.woff2')).toString('base64')}) format('woff2');font-weight:100 900;font-display:block}`;

/** Chromium from @playwright/test as installed for apps/desktop (no new dependency). */
export async function launch() {
  const req = createRequire(at('apps/desktop/package.json'));
  const { chromium } = req('@playwright/test');
  return chromium.launch();
}

export const { colour: C } = { colour };
