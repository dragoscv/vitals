// Social, store and promo art as HTML templates, rendered by Playwright so the
// tagline is set in the embedded Inter (correct ș ț ă î â) rather than in
// whatever font librsvg finds. Copy comes from BRAND.md (marketing line) and
// the store listings — nothing here is new wording.
import { readFileSync } from 'node:fs';
import { colour } from '../../mark.mjs';
import { at, lockupSvg, fontFace, FONT } from './common.mjs';

export const COPY = {
  en: {
    tagline: 'See what your computer is actually doing.',
    sub: 'Free, open-source system monitor and task manager for Windows.',
    url: 'vitals.dragoscatalin.ro',
  },
  ro: {
    tagline: 'Vezi ce face de fapt calculatorul tău.',
    sub: 'Monitor de sistem și manager de activități gratuit și open source pentru Windows.',
    url: 'vitals.dragoscatalin.ro',
  },
};

/**
 * Every social/store image. `layout`:
 *   text   — lockup, tagline, sub, url (OG, GitHub, Play feature graphic)
 *   split  — text left, a real screenshot right (X, promo)
 *   banner — one row, wide and short (LinkedIn)
 *   hero   — no tagline, lockup + screenshot (Microsoft Store hero: the Store overlays its own text)
 * `scale` renders at deviceScaleFactor for the 4K hero.
 */
export const SOCIAL = [
  {
    name: 'og-1200x630-en',
    w: 1200,
    h: 630,
    lang: 'en',
    layout: 'text',
    use: 'Open Graph (site, link previews)',
  },
  {
    name: 'og-1200x630-ro',
    w: 1200,
    h: 630,
    lang: 'ro',
    layout: 'text',
    use: 'Open Graph, Romanian pages',
  },
  {
    name: 'x-1600x900-en',
    w: 1600,
    h: 900,
    lang: 'en',
    layout: 'split',
    use: 'X / Twitter post image',
  },
  {
    name: 'x-1600x900-ro',
    w: 1600,
    h: 900,
    lang: 'ro',
    layout: 'split',
    use: 'X / Twitter post image, RO',
  },
  {
    name: 'github-social-1280x640',
    w: 1280,
    h: 640,
    lang: 'en',
    layout: 'text',
    use: 'GitHub repository social preview',
  },
  {
    name: 'linkedin-banner-1584x396',
    w: 1584,
    h: 396,
    lang: 'en',
    layout: 'banner',
    use: 'LinkedIn page banner',
  },
  {
    name: 'promo-1920x1080-en',
    w: 1920,
    h: 1080,
    lang: 'en',
    layout: 'split',
    use: 'YouTube thumbnail / store promo',
  },
  {
    name: 'promo-1920x1080-ro',
    w: 1920,
    h: 1080,
    lang: 'ro',
    layout: 'split',
    use: 'YouTube thumbnail / store promo, RO',
  },
  {
    name: 'play-feature-1024x500-en',
    w: 1024,
    h: 500,
    lang: 'en',
    layout: 'text',
    use: 'Google Play feature graphic',
  },
  {
    name: 'play-feature-1024x500-ro',
    w: 1024,
    h: 500,
    lang: 'ro',
    layout: 'text',
    use: 'Google Play feature graphic, RO',
  },
  {
    name: 'store-hero-1920x1080',
    w: 1920,
    h: 1080,
    lang: 'en',
    layout: 'hero',
    use: 'Microsoft Store hero (16:9)',
  },
  {
    name: 'store-hero-3840x2160',
    w: 1920,
    h: 1080,
    scale: 2,
    lang: 'en',
    layout: 'hero',
    use: 'Microsoft Store hero, 4K',
  },
];

const shot = (n) =>
  `data:image/png;base64,${readFileSync(at('packaging/msix/listing/screenshots', n)).toString('base64')}`;

/** The faint sparkline in the background: the mark's own gesture, at landscape scale. */
const spark = (w, h) => {
  const y = (f) => (h * f).toFixed(1);
  const x = (f) => (w * f).toFixed(1);
  return `<svg class="spark" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none" aria-hidden="true"><path d="M0 ${y(0.72)}H${x(0.22)}L${x(0.28)} ${y(0.8)}L${x(0.44)} ${y(0.46)}L${x(0.5)} ${y(0.55)}L${x(0.64)} ${y(0.36)}L${x(0.75)} ${y(0.42)}L${w} ${y(0.27)}" fill="none" stroke="${colour.signal.hex}" stroke-opacity="0.16" stroke-width="${Math.max(3, h / 105)}" stroke-linecap="round" stroke-linejoin="round" vector-effect="non-scaling-stroke"/></svg>`;
};

export function socialHtml({ w, h, lang, layout }) {
  const c = COPY[lang];
  const u = h / 100; // 1 % of height, the type scale's unit
  const base = `<!doctype html><html lang="${lang}"><meta charset="utf-8"><style>${fontFace()}
*{box-sizing:border-box;margin:0}
html,body{width:${w}px;height:${h}px;overflow:hidden}
body{font-family:${FONT},sans-serif;color:${colour.paper.hex};background:radial-gradient(120% 140% at 20% 10%,${colour.nightRaised.hex} 0%,${colour.night.hex} 62%);position:relative;-webkit-font-smoothing:antialiased}
.spark{position:absolute;inset:0;width:100%;height:100%}
.wrap{position:absolute;inset:0;display:flex;align-items:center}
.tag{font-weight:650;letter-spacing:-0.02em;line-height:1.08;text-wrap:balance}
.sub{font-weight:420;opacity:.74;line-height:1.3;text-wrap:balance}
.url{font-weight:560;color:${colour.signal.hex}}
.frame{border-radius:${1.6 * u}px;overflow:hidden;box-shadow:0 ${2 * u}px ${6 * u}px rgba(0,0,0,.45),0 0 0 1px rgba(255,255,255,.08)}
.frame img{display:block;width:100%;height:auto}
</style><body>${spark(w, h)}`;
  if (layout === 'text') {
    return `${base}<div class="wrap" style="padding:0 ${w * 0.08}px"><div>
<div style="margin-bottom:${7 * u}px">${lockupSvg(colour.paper.hex, 15 * u)}</div>
<p class="tag" style="font-size:${8.6 * u}px;max-width:${w * 0.84}px">${c.tagline}</p>
<p class="sub" style="font-size:${4.6 * u}px;margin-top:${3 * u}px;max-width:${w * 0.8}px">${c.sub}</p>
<p class="url" style="font-size:${4 * u}px;margin-top:${6 * u}px">${c.url}</p></div></div>`;
  }
  if (layout === 'banner') {
    // LinkedIn crops the left third under the avatar on some views: keep it to the right.
    return `${base}<div class="wrap" style="justify-content:flex-end;gap:${6 * u}px;padding:0 ${w * 0.06}px">
${lockupSvg(colour.paper.hex, 22 * u)}<div style="border-left:1px solid rgba(255,255,255,.18);padding-left:${6 * u}px">
<p class="tag" style="font-size:${10 * u}px">${c.tagline}</p><p class="url" style="font-size:${6.5 * u}px;margin-top:${3 * u}px">${c.url}</p></div></div>`;
  }
  if (layout === 'split') {
    return `${base}<div class="wrap" style="padding:0 0 0 ${w * 0.06}px;gap:${w * 0.04}px">
<div style="flex:0 0 ${w * 0.36}px">
<div style="margin-bottom:${6 * u}px">${lockupSvg(colour.paper.hex, 10 * u)}</div>
<p class="tag" style="font-size:${7 * u}px">${c.tagline}</p>
<p class="sub" style="font-size:${3.2 * u}px;margin-top:${3 * u}px">${c.sub}</p>
<p class="url" style="font-size:${3 * u}px;margin-top:${5 * u}px">${c.url}</p></div>
<div class="frame" style="flex:0 0 ${w * 0.62}px;transform:translateX(${w * 0.02}px)"><img src="${shot('01-dashboard-dark.png')}" alt=""></div></div>`;
  }
  // hero
  return `${base}<div class="wrap" style="flex-direction:column;justify-content:center;gap:${6 * u}px">
${lockupSvg(colour.paper.hex, 11 * u)}<div class="frame" style="width:${w * 0.62}px"><img src="${shot('01-dashboard-dark.png')}" alt=""></div></div>`;
}
