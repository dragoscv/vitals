// Drives the app on the TV through Chromium DevTools, after
// `tv-deploy.ps1 -DevTools` forwarded the port to localhost:9222.
//
//   node cdp.mjs eval "<expression>"
//   node cdp.mjs keys right,right,enter,down      (arrows, enter, back, 0-9)
//   node cdp.mjs type "192.168.1.20:7331"         (into the focused field)
//   node cdp.mjs shot <file.png>
//   node cdp.mjs focus                            (what has focus now)
//
// Several commands can be chained: `node cdp.mjs keys down,enter shot a.png focus`.
// Key presses go through Input.dispatchKeyEvent, the same path as the remote.
import { writeFileSync } from 'node:fs';

const KEYS = {
  left: [37, 'ArrowLeft'],
  up: [38, 'ArrowUp'],
  right: [39, 'ArrowRight'],
  down: [40, 'ArrowDown'],
  enter: [13, 'Enter'],
  back: [10009, 'XF86Back'],
};

const list = await (await fetch('http://127.0.0.1:9222/json')).json();
const page = list.find((p) => p.type === 'page' && p.webSocketDebuggerUrl) ?? list[0];
if (!page) {
  console.log('no page');
  process.exit(1);
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener('open', r));
let next = 1;
const waiting = new Map();
ws.addEventListener('message', (m) => {
  const d = JSON.parse(m.data);
  const w = waiting.get(d.id);
  if (w) {
    waiting.delete(d.id);
    w(d);
  }
});
function send(method, params = {}) {
  const id = next++;
  ws.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve) => waiting.set(id, resolve));
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function evaluate(expression) {
  const d = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  const r = d.result?.result;
  return typeof r?.value === 'string' ? r.value : JSON.stringify(r?.value ?? d.result);
}

async function press(name) {
  const digit = /^\d$/.test(name);
  const known = KEYS[name];
  if (!digit && known === undefined) throw new Error(`unknown key ${name}`);
  const [code, key] = digit ? [48 + Number(name), name] : known;
  const base = { windowsVirtualKeyCode: code, nativeVirtualKeyCode: code, key };
  await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base });
  if (name === 'enter') await send('Input.dispatchKeyEvent', { type: 'char', text: '\r', ...base });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
  await sleep(300);
}

const FOCUS =
  "(() => { const a = document.activeElement; if (!a || a === document.body) return 'body'; return a.tagName + ' ' + (a.getAttribute('aria-label') || a.textContent || a.value || '').trim().slice(0, 80); })()";

const args = process.argv.slice(2);
const timer = setTimeout(() => {
  console.log('timeout');
  process.exit(2);
}, 60_000);
for (let i = 0; i < args.length; i++) {
  const cmd = args[i];
  if (cmd === 'eval') console.log(await evaluate(args[++i]));
  else if (cmd === 'keys') for (const k of args[++i].split(',')) await press(k.trim());
  else if (cmd === 'type') {
    await send('Input.insertText', { text: args[++i] });
    await sleep(200);
  } else if (cmd === 'wait') await sleep(Number(args[++i]));
  else if (cmd === 'focus') console.log('focus:', await evaluate(FOCUS));
  else if (cmd === 'shot') {
    const d = await send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(args[++i], Buffer.from(d.result.data, 'base64'));
    console.log('saved', args[i]);
  } else throw new Error(`unknown command ${cmd}`);
}
clearTimeout(timer);
ws.close();
process.exit(0);
