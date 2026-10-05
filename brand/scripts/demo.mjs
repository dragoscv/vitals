#!/usr/bin/env node
// Renders brand/dist/demo.html: the chosen mark at every size and context, the
// logo motion set live (with a replay button), the storyboard and the palette.
// Run after build.mjs: node brand/scripts/demo.mjs
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { glyph, tile, colour, variantFor } from '../mark.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const css = readFileSync(join(root, 'packages/ui/src/styles/brand.css'), 'utf8');
const lockupLight = readFileSync(join(root, 'brand/logo/lockup-light.svg'), 'utf8');
const lockupDark = readFileSync(join(root, 'brand/logo/lockup-dark.svg'), 'utf8');
let n = 0;

function mark(px, state = 'idle', { bare = false, interactive = false } = {}) {
  const v = variantFor(px);
  const id = `g${n++}`;
  const plate = bare
    ? ''
    : `<defs><linearGradient id="${id}" x1="0.2" y1="0" x2="0.8" y2="1"><stop offset="0" stop-color="${colour.tileTop.hex}"/><stop offset="1" stop-color="${colour.tileBottom.hex}"/></linearGradient></defs><rect class="vm-tile" x="${tile.x}" y="${tile.y}" width="${tile.size}" height="${tile.size}" rx="${tile.radius}" fill="url(#${id})"/>`;
  const s = bare ? 'currentColor' : '#fff';
  return `<svg class="vitals-mark" data-state="${state}" ${interactive ? 'data-interactive' : ''} viewBox="0 0 48 48" width="${px}" height="${px}">${plate}<g class="vm-glyph"><path class="vm-line" d="${v.d}" pathLength="1" fill="none" stroke="${s}" stroke-width="${v.stroke}" stroke-linecap="round" stroke-linejoin="round"/><path class="vm-trace" d="${v.d}" pathLength="1" fill="none" stroke="${bare ? 'currentColor' : colour.signal.hex}" stroke-width="${v.stroke}" stroke-linecap="round" stroke-linejoin="round"/></g></svg>`;
}

const neighbours = ['#0078D4', '#F25022', '#2BA640', '#E8A33D', '#5E5E5E', '#1DB954'];
const taskbar = (px, bg) =>
  `<div class="taskbar" style="background:${bg}">${neighbours
    .slice(0, 3)
    .map((c) => `<i style="width:${px}px;height:${px}px;background:${c}"></i>`)
    .join('')}${mark(px)}${neighbours
    .slice(3)
    .map((c) => `<i style="width:${px}px;height:${px}px;background:${c}"></i>`)
    .join('')}</div>`;

const story = [0, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9, 1]
  .map((t) => {
    const tileT = Math.min(1, t / 0.55);
    const drawT = Math.max(0, Math.min(1, (t - 0.3) / 0.7));
    const ease = (x) => 1 - (1 - x) ** 3;
    return `<figure><svg viewBox="0 0 48 48" width="72" height="72"><rect x="3" y="3" width="42" height="42" rx="10" fill="${colour.tileTop.hex}" opacity="${ease(tileT)}" transform="translate(24 24) scale(${0.9 + 0.1 * ease(tileT)}) translate(-24 -24)"/><path d="${glyph.master.d}" pathLength="1" stroke-dasharray="1" stroke-dashoffset="${1 - ease(drawT)}" fill="none" stroke="#fff" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"/></svg><figcaption>${Math.round(t * 900)} ms</figcaption></figure>`;
  })
  .join('');

const states = ['intro', 'thinking', 'success', 'error'];
const html = `<!doctype html><meta charset="utf-8"><title>Vitals — brand</title>
<style>
${css}
body{margin:0;padding:28px;font:13px/1.45 'Segoe UI Variable Text','Segoe UI',sans-serif;background:#eef3ef;color:#0f1a14}
h1{font-size:22px;margin:0 0 4px}h2{font-size:14px;margin:22px 0 8px}p{margin:0 0 8px;max-width:72ch}
.row{display:flex;gap:14px;flex-wrap:wrap;align-items:stretch}
.pane{display:flex;gap:16px;align-items:center;padding:16px;border-radius:14px;background:#fff}
.pane.dark{background:#141816;color:#e6efe8}.col{flex-direction:column;align-items:flex-start;gap:8px}
.taskbar{display:flex;gap:8px;align-items:center;padding:6px 10px;border-radius:8px}.taskbar i{display:inline-block;border-radius:6px}
figure{margin:0;text-align:center}figcaption{font-size:11px;opacity:.7}
.sw{display:flex;gap:8px}.sw div{width:96px}.sw b{display:block;height:44px;border-radius:8px;margin-bottom:4px}
button{font:inherit;padding:6px 12px;border-radius:8px;border:1px solid #c9d6cd;background:#fff;cursor:pointer}
.lockup svg{height:48px;width:auto}
</style>
<h1>Vitals — "All clear"</h1>
<p>A calm line that dips and rises past where it started: a V, a sparkline and a tick at once. Fern tile, white line, signal-lime for the "all clear" moment.</p>
<h2>Sizes (optical variants: micro ≤ 20 px, small ≤ 48 px, master above)</h2>
<div class="row"><div class="pane dark">${[16, 20, 24, 32, 48, 64, 128].map((s) => mark(s)).join('')}${mark(256)}</div><div class="pane">${[16, 20, 24, 32, 48, 64, 128].map((s) => mark(s)).join('')}</div></div>
<h2>In context</h2>
<div class="row">
<div class="pane col dark"><small>Windows taskbar, dark · 24 / 36 px</small>${taskbar(24, '#202020')}${taskbar(36, '#202020')}</div>
<div class="pane col"><small>Windows taskbar, light</small>${taskbar(24, '#f3f3f3')}${taskbar(36, '#f3f3f3')}</div>
<div class="pane col"><small>Android mask (66 dp safe circle)</small><svg width="96" height="96" viewBox="0 0 108 108"><defs><clipPath id="c"><circle cx="54" cy="54" r="54"/></clipPath><linearGradient id="ab" x1="0.28" y1="0" x2="0.72" y2="1"><stop offset="0" stop-color="${colour.tileTop.hex}"/><stop offset="1" stop-color="${colour.tileBottom.hex}"/></linearGradient></defs><g clip-path="url(#c)"><rect width="108" height="108" fill="url(#ab)"/><g transform="translate(15.6 15.6) scale(1.6)"><path d="${glyph.master.d}" fill="none" stroke="#fff" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"/></g></g><circle cx="54" cy="54" r="33" fill="none" stroke="#fff" stroke-opacity=".35" stroke-dasharray="2 2"/></svg></div>
<div class="pane col"><small>Mono · inverse · squint</small><div style="display:flex;gap:12px;align-items:center"><span style="color:#111">${mark(48, 'idle', { bare: true })}</span><span style="color:${colour.tileBottom.hex}">${mark(48, 'idle', { bare: true })}</span><span style="filter:blur(2px)">${mark(48)}</span></div></div>
</div>
<h2>Lockups</h2>
<div class="row"><div class="pane lockup">${lockupLight}</div><div class="pane dark lockup">${lockupDark}</div></div>
<h2>Motion — live <button onclick="replay()">Replay</button></h2>
<p>intro 900 ms (tile settles, line draws) · thinking 1.6 s loop (a highlight travels the line) · success 360 ms (redraw + lift) · error 180 ms (horizontal nudge, readable without colour) · hover 140 ms (1 px lift). Reduced motion: every state shows its final frame.</p>
<div class="row" id="live">${states.map((s) => `<div class="pane col"><small>${s}</small>${mark(96, s)}</div>`).join('')}<div class="pane col"><small>hover / press</small>${mark(96, 'idle', { interactive: true })}</div><div class="pane col dark"><small>thinking on dark, 32 px</small>${mark(32, 'thinking')}</div></div>
<h2>Intro storyboard</h2>
<div class="row"><div class="pane">${story}</div></div>
<h2>Palette</h2>
<div class="pane sw">${[
  ['Fern tile top', colour.tileTop.hex, colour.tileTop.oklch],
  ['Fern tile bottom', colour.tileBottom.hex, colour.tileBottom.oklch],
  ['UI accent (light)', '#22864a', 'oklch(0.55 0.13 152)'],
  ['UI accent (dark)', '#6cb882', 'oklch(0.72 0.11 152)'],
  ['Signal lime', colour.signal.hex, colour.signal.oklch],
  ['Night', colour.night.hex, colour.night.oklch],
]
  .map(
    ([n2, h, o]) => `<div><b style="background:${h}"></b>${n2}<br><small>${h} · ${o}</small></div>`,
  )
  .join('')}</div>
<script>
function replay(){document.querySelectorAll('#live .vitals-mark').forEach(m=>{const s=m.dataset.state;m.dataset.state='idle';void m.getBoundingClientRect();requestAnimationFrame(()=>{m.dataset.state=s})})}
setInterval(()=>{document.querySelectorAll('#live [data-state=success],#live [data-state=error]').forEach(m=>{const s=m.dataset.state;m.dataset.state='idle';requestAnimationFrame(()=>requestAnimationFrame(()=>{m.dataset.state=s}))})},2200);
</script>`;

mkdirSync(join(root, 'brand/dist'), { recursive: true });
writeFileSync(join(root, 'brand/dist/demo.html'), html);
console.log('wrote brand/dist/demo.html');
