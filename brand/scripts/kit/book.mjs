// brand/book/index.html — the brand book as one self-contained page. Every
// fact on it is read from mark.mjs, brand.json, contrast-pairs.json or
// BRAND.md's voice table; nothing is restated by hand.
import { readFileSync } from 'node:fs';
import { glyph, tile, colour, motion } from '../../mark.mjs';
import { at, brand, markSvg, lockupSvg, fontFace, FONT } from './common.mjs';
import { MOTIONS, animatedSvg, EASE, OVERSHOOT, SWEEP } from './motion.mjs';
import { contrast, hex, threshold } from './colour.mjs';

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** The voice table straight out of BRAND.md, so the book cannot drift from it. */
function voiceRows() {
  const md = readFileSync(at('brand/BRAND.md'), 'utf8');
  const start = md.indexOf('| Context');
  const block = md.slice(start, md.indexOf('\n\n', start)).split('\n').slice(2);
  return block.map((r) =>
    r
      .split('|')
      .slice(1, -1)
      .map((c) => c.trim()),
  );
}

const PALETTE = [
  ['Fern tile top', colour.tileTop.oklch, 'Icon tile, gradient start'],
  ['Fern tile bottom', colour.tileBottom.oklch, 'Icon tile, gradient end; mark on light'],
  ['UI accent, light', 'oklch(0.55 0.13 152)', 'Buttons, focus, toggles (light theme)'],
  ['UI accent, dark', 'oklch(0.72 0.11 152)', 'Buttons, focus, toggles (dark theme)'],
  ['Signal lime', colour.signal.oklch, 'The "all clear" moment, on dark only'],
  ['Night', colour.night.oklch, 'Splash, OG, store art, TV'],
  ['Night raised', colour.nightRaised.oklch, 'Glow and raised surfaces on night'],
];

export function bookHtml({ downloads }) {
  const pairs = JSON.parse(readFileSync(at('brand/contrast-pairs.json'), 'utf8')).pairs;
  const rows = pairs.map((p) => {
    const r = contrast(p.fg, p.bg);
    const need = threshold(p.use);
    return { ...p, r, need, ok: r >= need };
  });
  const fails = rows.filter((r) => !r.ok).length;

  const sizes = [16, 20, 24, 32, 48, 64, 128];
  const motionRows = Object.entries(MOTIONS)
    .map(
      ([id, mo]) =>
        `<figure class="mo"><div class="mo-stage${mo.bare ? ' bare' : ''}">${animatedSvg(id, { px: 120 })}</div><figcaption><b>${mo.title}</b><span>${esc(mo.says)}</span><code>${(mo.periodMs / 1000).toFixed(1)} s${mo.loop ? ' loop' : ', once'}</code></figcaption></figure>`,
    )
    .join('');

  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Vitals brand book ${brand.version}</title>
<style>${fontFace()}
:root{--ink:${colour.ink.hex};--paper:#ffffff;--muted:#4a5a50;--line:#d5e0d8;--fern:${colour.tileBottom.hex};--night:${colour.night.hex};--signal:${colour.signal.hex}}
*{box-sizing:border-box}
body{margin:0;font:16px/1.55 ${FONT},system-ui,sans-serif;color:var(--ink);background:${colour.paper.hex};-webkit-font-smoothing:antialiased}
a{color:var(--fern);text-underline-offset:2px}a:focus-visible{outline:2px solid var(--fern);outline-offset:2px;border-radius:2px}
header{background:var(--night);color:${colour.paper.hex};padding:72px 8vw 64px}
header p{max-width:60ch;opacity:.8;margin:18px 0 0;font-size:18px}
header .ver{font-size:13px;opacity:.7;margin-top:28px;letter-spacing:.02em}
main{padding:0 8vw 80px;max-width:1280px}
section{padding:56px 0 8px;border-bottom:1px solid var(--line);break-inside:avoid-page}
h2{font-size:28px;letter-spacing:-.015em;margin:0 0 6px}h3{font-size:17px;margin:28px 0 8px}
p{max-width:72ch;margin:0 0 12px}.muted{color:var(--muted)}
.row{display:flex;gap:16px;flex-wrap:wrap;align-items:stretch}
.card{background:#fff;border:1px solid var(--line);border-radius:14px;padding:20px;display:flex;flex-direction:column;gap:10px;align-items:flex-start}
.card.dark{background:var(--night);border-color:var(--night);color:${colour.paper.hex}}
.card small{font-size:12px;color:var(--muted)}.card.dark small{color:#b9c9be}
.sizes{display:flex;gap:18px;align-items:flex-end}.sizes figure{margin:0;text-align:center;font-size:12px}
.clear{position:relative;padding:36px;background:repeating-linear-gradient(45deg,#e7efe9 0 6px,#f6faf7 6px 12px);border-radius:12px}
.clear .box{background:#fff;outline:1px dashed var(--fern)}
.dont{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:14px}
.dont figure{margin:0;background:#fff;border:1px solid var(--line);border-radius:12px;padding:14px;text-align:center;font-size:13px}
.dont figure::after{content:"✕ Don't";display:block;color:#b3261e;font-weight:600;margin-top:6px}
.sw{display:grid;grid-template-columns:repeat(auto-fill,minmax(170px,1fr));gap:14px}
.sw div{border:1px solid var(--line);border-radius:12px;overflow:hidden;background:#fff;font-size:13px}
.sw b{display:block;height:72px}.sw p{margin:0;padding:10px 12px}
table{border-collapse:collapse;width:100%;font-size:14px;margin:8px 0 16px}
th,td{text-align:left;padding:8px 10px;border-bottom:1px solid var(--line);vertical-align:top}th{font-weight:600;background:#e9f1eb}
td.num{font-variant-numeric:tabular-nums;white-space:nowrap}
.pass{color:#1d6b3a;font-weight:600}.fail{color:#b3261e;font-weight:600}
.chip{display:inline-block;width:14px;height:14px;border-radius:4px;vertical-align:-2px;margin-right:6px;border:1px solid rgba(0,0,0,.15)}
.type .big{font-size:56px;font-weight:650;letter-spacing:-.02em;line-height:1.05}
.type .mid{font-size:24px;font-weight:600}.type .ro{font-size:20px}
.mo{margin:0;background:#fff;border:1px solid var(--line);border-radius:14px;padding:16px;width:260px}
.mo-stage{height:150px;display:grid;place-items:center;background:var(--night);border-radius:10px}
.mo-stage.bare{background:#fff;border:1px solid var(--line)}
.mo figcaption{display:flex;flex-direction:column;gap:4px;margin-top:10px;font-size:13px}
.mo figcaption span{color:var(--muted)}
code{font-family:ui-monospace,Consolas,monospace;font-size:13px;background:#e9f1eb;border-radius:4px;padding:1px 5px}
ul.dl{columns:2;padding-left:18px}ul.dl li{break-inside:avoid;margin-bottom:4px}
@media (max-width:760px){ul.dl{columns:1}}
@media print{header{padding:40px 32px}main{padding:0 32px 32px}section{padding-top:28px}.mo{width:30%}a{color:var(--ink)}}
</style></head><body>
<header>${lockupSvg(colour.paper.hex, 72)}
<p>${esc(brand.concept.meaning)}</p>
<p class="ver">Brand book · version ${brand.version} · source of truth <code style="background:rgba(255,255,255,.1);color:inherit">brand/mark.mjs</code> · generated by <code style="background:rgba(255,255,255,.1);color:inherit">brand/scripts/kit.mjs</code></p></header>
<main>

<section id="logo"><h2>Logo</h2>
<p>The mark is a tile and one line. The lockup adds the outlined Inter SemiBold wordmark, so no surface needs the font. Write the name "Vitals" — capital V, never "VITALS" or "vitals" in prose.</p>
<div class="row">
<div class="card">${lockupSvg(colour.ink.hex, 64)}<small>Lockup on light · logo/lockup-light.svg</small></div>
<div class="card dark">${lockupSvg(colour.paper.hex, 64)}<small>Lockup on dark · logo/lockup-dark.svg</small></div>
<div class="card">${markSvg(96)}<small>Mark · logo/mark.svg</small></div>
<div class="card" style="color:var(--ink)">${markSvg(96, { bare: true })}<small>Mono (currentColor) · logo/mark-mono.svg</small></div>
</div>
<h3>Optical sizes</h3>
<p>One stroke cannot serve 16 px and 512 px. <b>master</b> ≥ 64 px (stroke ${glyph.master.stroke}) · <b>small</b> 24–48 px (stroke ${glyph.small.stroke}) · <b>micro</b> 16–20 px (stroke ${glyph.micro.stroke}, every node on the pixel grid). Use the file made for the size.</p>
<div class="row"><div class="card"><div class="sizes">${sizes.map((s) => `<figure>${markSvg(s)}<figcaption>${s}</figcaption></figure>`).join('')}</div></div>
<div class="card dark"><div class="sizes">${sizes.map((s) => `<figure>${markSvg(s)}<figcaption>${s}</figcaption></figure>`).join('')}</div></div></div>
</section>

<section id="space"><h2>Clear space and minimum size</h2>
<p>Keep 6 grid units (⅛ of the mark) clear on every side — one stroke plus a node. Nothing else enters that area: no text, no edge, no other logo. Minimum size is 16 px for the mark (micro variant) and 24 px tall for the lockup.</p>
<div class="row"><div class="clear"><div class="box">${markSvg(144)}</div></div>
<div class="clear"><div class="box" style="padding:18px">${lockupSvg(colour.ink.hex, 48)}</div></div></div>
<p class="muted" style="margin-top:10px">The hatched band is the clear space at this size (6 of 48 units). Tile ${tile.size} × ${tile.size} at (${tile.x}, ${tile.y}), radius ${tile.radius}, on a 48-unit grid.</p>
</section>

<section id="misuse"><h2>Misuse</h2>
<div class="dont">
<figure>${markSvg(64, { line: '#f2c94c' })}<br>Recolour the line</figure>
<figure><div style="display:inline-block;transform:rotate(-14deg)">${markSvg(64)}</div><br>Rotate</figure>
<figure><div style="display:inline-block;transform:scaleX(1.35)">${markSvg(64)}</div><br>Stretch</figure>
<figure><div style="display:inline-block;filter:drop-shadow(0 6px 4px rgba(0,0,0,.55))">${markSvg(64)}</div><br>Add shadows</figure>
<figure><svg viewBox="0 0 48 48" width="64" height="64"><circle cx="24" cy="24" r="21" fill="${colour.tileTop.hex}"/><path d="${glyph.master.d}" fill="none" stroke="#fff" stroke-width="${glyph.master.stroke}" stroke-linecap="round" stroke-linejoin="round"/></svg><br>Put it on a circle</figure>
<figure><svg viewBox="0 0 48 48" width="64" height="64"><rect x="3" y="3" width="42" height="42" rx="10" fill="${colour.tileTop.hex}"/><path d="M8 26h8l3-8 5 16 4-12 3 4h9" fill="none" stroke="#fff" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"/></svg><br>Use a heartbeat zig-zag</figure>
<figure>${markSvg(64, { extra: '<rect x="3" y="3" width="42" height="42" rx="10" fill="none" stroke="#000" stroke-width="2"/>' })}<br>Outline the tile</figure>
<figure><div style="display:inline-block;background:${colour.tileTop.hex};padding:6px;border-radius:10px">${markSvg(52)}</div><br>Place on low-contrast fern</figure>
</div></section>

<section id="colour"><h2>Colour</h2>
<p>Authored in OKLCH; hex is the sRGB fallback, computed from the OKLCH value. Signal lime is for the "all clear" moment on dark only — never text on a light background. Metric colours in the app (CPU, memory, …) are categorical and do not follow the brand.</p>
<div class="sw">${PALETTE.map(([n, o, use]) => `<div><b style="background:${o}"></b><p><strong>${n}</strong><br><code>${hex(o)}</code><br><code>${o}</code><br><span class="muted">${use}</span></p></div>`).join('')}</div>
<h3>Contrast — every shipped pairing (WCAG 2.2 AA)</h3>
<p>From <code>brand/contrast-pairs.json</code>, ratios computed here from the same OKLCH strings. Body text needs 4.5:1; large text and UI parts 3:1. Result: <b class="${fails ? 'fail' : 'pass'}">${rows.length - fails} of ${rows.length} pass</b>.</p>
<table><thead><tr><th>Pair</th><th>Foreground</th><th>Background</th><th>Use</th><th>Ratio</th><th>Needs</th><th></th></tr></thead><tbody>
${rows.map((r) => `<tr><td>${esc(r.name)}</td><td><span class="chip" style="background:${r.fg}"></span><code>${hex(r.fg)}</code></td><td><span class="chip" style="background:${r.bg}"></span><code>${hex(r.bg)}</code></td><td>${r.use}</td><td class="num">${r.r.toFixed(2)}:1</td><td class="num">${r.need}:1</td><td class="${r.ok ? 'pass' : 'fail'}">${r.ok ? 'Pass' : 'Fail'}</td></tr>`).join('')}
</tbody></table></section>

<section id="type" class="type"><h2>Typography</h2>
<p>UI: Segoe UI Variable on Windows (system font), Inter Variable as the fallback and for brand art, JetBrains Mono for figures. Tabular numerals always. The wordmark is Inter SemiBold, outlined; tracking −1 %. Inter is licensed under the SIL Open Font Licence 1.1 (<code>brand/OFL-Inter.txt</code>).</p>
<div class="card" style="gap:14px">
<div class="big">See what your computer is actually doing.</div>
<div class="mid">Vezi ce face de fapt calculatorul tău.</div>
<div class="ro">Romanian check: Ă Â Î Ș Ț ă â î ș ț — „ghilimele” « » – — €</div>
<div class="muted" style="font-variant-numeric:tabular-nums">Tabular figures: 0123456789 · 12.4 GB · 37 % · 1 024 MB</div></div>
<p class="muted" style="margin-top:10px">Romanian uses comma-below ș ț, never cedilla ş ţ. Glyph coverage verified ${brand.fonts[0].verifiedGlyphs.date}: ${esc(brand.fonts[0].verifiedGlyphs.result)}.</p>
</section>

<section id="motion"><h2>Motion</h2>
<p>The kit uses the app's motion vocabulary (<code>packages/ui/src/styles/brand.css</code>, generated from the same source): the tile settles, the line draws, a highlight travels the line while working, the glyph lifts on success, and an alert is a horizontal nudge — shape, not colour. Only transform, opacity and stroke-dashoffset animate.</p>
<div class="row">${motionRows}</div>
<h3>Timing and easing tokens</h3>
<table><thead><tr><th>Token</th><th>Value</th><th>Used for</th></tr></thead><tbody>
<tr><td><code>logo.ease</code></td><td><code>${EASE}</code></td><td>A long, calm settle: intro, morph, hover</td></tr>
<tr><td><code>logo.easeOvershoot</code></td><td><code>${OVERSHOOT}</code></td><td>The success lift and the pulse</td></tr>
<tr><td><code>logo.sweep</code></td><td><code>${SWEEP}</code></td><td>The thinking highlight and the loader</td></tr>
<tr><td><code>intro</code></td><td>${motion.intro.ms} ms, tile from scale ${motion.intro.tileFrom}</td><td>Reveal; splash</td></tr>
<tr><td><code>thinking</code></td><td>${motion.thinking.ms} ms loop, segment ${motion.thinking.segment}</td><td>Scans, benchmarks, loaders</td></tr>
<tr><td><code>success</code></td><td>${motion.success.ms} ms</td><td>Lift at the end of the reveal; pulse</td></tr>
<tr><td><code>error</code></td><td>${motion.error.ms} ms, ±${motion.error.px} units</td><td>Alert</td></tr>
<tr><td><code>hover</code></td><td>${motion.hover.ms} ms, ${motion.hover.lift} unit lift</td><td>Interactive marks</td></tr>
<tr><td><code>morph</code></td><td>${motion.morph.ms} ms (View Transitions, <code>${motion.morph.name}</code>)</td><td>Splash mark → title bar</td></tr>
</tbody></table>
<h3>Principles</h3>
<p><b>Calm, not alarmist.</b> Nothing flashes or blinks; the loudest motion is a 2-unit nudge. <b>One idea per motion.</b> Each state says exactly one thing. <b>Reduced motion is a first-class state.</b> Every file honours <code>prefers-reduced-motion: reduce</code> and shows its final frame; "thinking" keeps a static trace over a dimmed line so working is still visible.</p>
</section>

<section id="voice"><h2>Voice and tone</h2>
<p>Calm, precise, helpful, fast and light, playful only in moments. Short sentences, everyday words ("memory in use", not "working set"), British spelling in English, correct diacritics in Romanian. An unmeasured number is shown as "—", never "0".</p>
<table><thead><tr><th>Context</th><th>English</th><th>Română</th></tr></thead><tbody>
${voiceRows()
  .map((r) => `<tr>${r.map((c) => `<td>${esc(c)}</td>`).join('')}</tr>`)
  .join('')}
</tbody></table></section>

<section id="downloads" style="border-bottom:0"><h2>Downloads</h2>
<p>Everything below lives in <code>brand/</code> and is regenerated by <code>node brand/scripts/kit.mjs</code>. The press kit ZIP bundles it.</p>
${downloads.map(([title, files]) => `<h3>${esc(title)}</h3><ul class="dl">${files.map((p) => `<li><a href="../../${p}">${p.replace(/^brand\//, '')}</a></li>`).join('')}</ul>`).join('')}
</section>
</main></body></html>`;
}
