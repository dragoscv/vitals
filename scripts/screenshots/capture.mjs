// Captures the Store and website screenshots from the real release build.
//
// Why CDP against the real exe and not the Vite page in a browser: the
// browser has no Tauri host, so every reading would be the no-host fallback
// and the Store rejects anything that is not a direct capture of the app
// (certification 10.1.1.3, 2026-10-14).
//
// Usage (Windows, after `tauri build --no-bundle`):
//   node scripts/screenshots/capture.mjs <exe> <outDir>
// The exe must have been built with the debugging port in the window's
// `additionalBrowserArgs` (run.ps1 does this); the viewport is pinned to
// 1920x1080 with Emulation.setDeviceMetricsOverride so the output does not
// depend on the machine's display.
import { execSync, spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [exe, outDir] = process.argv.slice(2);
if (!exe || !outDir) throw new Error('usage: capture.mjs <exe> <outDir>');
mkdirSync(outDir, { recursive: true });

const PORT = 9333;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.log('[capture]', ...a);

const app = spawn(exe, [], { stdio: 'ignore', detached: false });
let exited = null;
app.on('exit', (code) => {
  exited = code;
});

async function target() {
  for (let i = 0; i < 45; i++) {
    try {
      const list = await (
        await fetch(`http://127.0.0.1:${PORT}/json`, { signal: AbortSignal.timeout(3000) })
      ).json();
      const t = list.find((x) => x.type === 'page' && !/hud|mobile/.test(x.url));
      if (t) return t;
    } catch {
      // Not listening yet.
    }
    await sleep(1000);
  }
  // Say why, so a headless or session-0 failure is not a mystery.
  console.log('[capture] app exit code:', exited);
  for (const cmd of [
    'tasklist /v /fi "imagename eq vitals-desktop.exe"',
    'tasklist /fi "imagename eq msedgewebview2.exe"',
    'netstat -ano -p tcp | findstr LISTENING',
  ]) {
    try {
      console.log(`$ ${cmd}\n${execSync(cmd, { encoding: 'utf8' })}`);
    } catch (e) {
      console.log(`$ ${cmd} failed: ${e.message}`);
    }
  }
  throw new Error('no WebView2 page on the debugging port after 45 s');
}

const t = await target();
log('page', t.url);
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((r, j) => {
  ws.onopen = r;
  ws.onerror = j;
});
let seq = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  }
};
const send = (method, params = {}) =>
  new Promise((res) => {
    const n = ++seq;
    pending.set(n, res);
    ws.send(JSON.stringify({ id: n, method, params }));
  });
const ev = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description);
  return r.result?.result?.value;
};
const waitFor = async (expression, ms = 30000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await ev(`!!(${expression})`)) return true;
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${expression}`);
};
// Clicks the first visible element matching a selector whose text or
// accessible name contains `text`. Text, not test ids: the app ships none,
// and the labels are what a user would click too.
const click = (selector, text) =>
  ev(`(() => {
    const want = ${JSON.stringify(text ?? '')};
    const el = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => {
      const r = e.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) return false;
      const name = (e.getAttribute('aria-label') || '') + ' ' + (e.textContent || '');
      return name.includes(want);
    });
    if (!el) return false;
    el.click();
    return true;
  })()`);
// The process table freezes its order under the pointer; park it in the corner.
const parkMouse = () => send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1910, y: 1070 });
// A real pointer click at the element's centre. Radix tabs activate on
// pointerdown, which a synthetic `el.click()` never sends.
async function mouseClick(selector, text) {
  const box = await ev(`(() => {
    const el = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => {
      const r = e.getBoundingClientRect();
      return r.width > 0 && ((e.getAttribute('aria-label') || '') + ' ' + e.textContent).includes(${JSON.stringify(text)});
    });
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  })()`);
  if (!box) throw new Error(`nothing to click: ${selector} "${text}"`);
  for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
    await send('Input.dispatchMouseEvent', {
      type,
      x: box.x,
      y: box.y,
      button: 'left',
      clickCount: 1,
    });
  }
}

async function shot(name) {
  await parkMouse();
  await sleep(600);
  const r = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  writeFileSync(join(outDir, `${name}.png`), Buffer.from(r.result.data, 'base64'));
  log('saved', name);
}

await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {
  width: 1920,
  height: 1080,
  deviceScaleFactor: 1,
  mobile: false,
});
await waitFor(`document.querySelector('[data-nav-item="dashboard"]')`, 60000);

// A fresh profile shows the welcome card; the Store shots show the app as
// it looks every day after.
await click('button', 'Dismiss the welcome card');

// Let the charts gather a minute-ish of history so they are not flat lines.
log('warming up 45 s');
await sleep(45000);

const nav = async (id) => {
  await ev(`document.querySelector('[data-nav-item="${id}"]').click()`);
  await waitFor(`document.querySelector('[data-route-visible]')`);
  await sleep(2500);
};

// Storage scan first so its result exists for both themes.
await nav('storage');
if (await click('button', 'Scan this drive')) {
  log('scanning');
  await waitFor(
    `![...document.querySelectorAll('button')].some((b) => b.textContent.includes('Stop the scan'))`,
    240000,
  ).catch(() => log('scan still running'));
  await sleep(1500);
}

// Processes: on a server session most processes fold into three or four
// groups, which hides the icons the shot is meant to show. Flat list, sorted
// by memory, largest first (checked through aria-sort, not assumed).
await nav('processes');
await mouseClick('label, button, [role="checkbox"]', 'Group by app').catch((e) => log(e.message));
await sleep(800);
await click('[role="columnheader"] button', 'Sort by Memory');
await sleep(600);
if (
  await ev(
    `[...document.querySelectorAll('[role="columnheader"]')].some((h) => h.textContent.includes('Memory') && h.getAttribute('aria-sort') === 'ascending')`,
  )
) {
  await click('[role="columnheader"] button', 'Sort by Memory');
}

for (const mode of ['dark', 'light']) {
  // `system` is the default theme; emulating the media query exercises the
  // same path a user's Windows setting does.
  await send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-color-scheme', value: mode }],
  });
  await sleep(1200);

  await nav('dashboard');
  await waitFor(`document.body.textContent.includes('All clear')`, 60000).catch(() =>
    log('health strip not "All clear"'),
  );
  await shot(`${mode}-01-dashboard`);

  await click('button', 'Why is my PC slow?');
  await waitFor(`document.querySelector('[role="dialog"]')`);
  await sleep(4000);
  await shot(`${mode}-02-diagnosis`);
  await send('Input.dispatchKeyEvent', {
    type: 'keyDown',
    key: 'Escape',
    code: 'Escape',
    windowsVirtualKeyCode: 27,
  });
  await send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key: 'Escape',
    code: 'Escape',
    windowsVirtualKeyCode: 27,
  });
  await sleep(800);

  await nav('processes');
  await sleep(3000);
  await shot(`${mode}-03-processes`);

  await nav('performance');
  await sleep(3000);
  await shot(`${mode}-04-performance`);

  await nav('storage');
  await shot(`${mode}-05-storage`);

  await click('button', 'Settings');
  await waitFor(`document.querySelector('[role="dialog"]')`);
  await mouseClick('[role="dialog"] [role="tab"]', 'Remote access');
  await waitFor(
    `[...document.querySelectorAll('[role="dialog"] [role="tab"]')].some((t) => t.getAttribute('aria-selected') === 'true' && t.textContent.includes('Remote access'))`,
    10000,
  );
  await sleep(800);
  // Inactive tab panels stay mounted, so every query is limited to visible
  // elements; the first switch in the dialog is otherwise on another tab.
  const VIS = `((sel) => [...document.querySelectorAll('[role="dialog"] ' + sel)].filter((e) => e.getBoundingClientRect().width > 0))`;
  const isOn = `(${VIS}('[role="switch"]')[0]?.getAttribute('aria-checked') === 'true')`;
  const create = `${VIS}('button').find((b) => b.textContent.trim() === 'Create')`;
  try {
    if (!(await ev(isOn))) {
      await mouseClick('[role="dialog"] [role="switch"]', '');
      await waitFor(isOn, 15000);
    }
    if (!(await ev(`!!document.querySelector('[role="dialog"] [role="img"] svg')`))) {
      await waitFor(`${create} && !${create}.disabled`, 30000);
      await ev(`${create}.click()`);
      await waitFor(`document.querySelector('[role="dialog"] [role="img"] svg')`);
    }
  } catch (error) {
    log('remote access failed:', error.message);
    log(
      'dialog text:',
      await ev(`document.querySelector('[role="dialog"]')?.innerText.slice(0, 1500)`),
    );
    await shot(`${mode}-06-remote-access-FAILED`);
    throw error;
  }
  // The QR panel sits below the fold of the settings panel.
  await ev(
    `document.querySelector('[role="dialog"] [role="img"]').scrollIntoView({ block: 'center' })`,
  );
  // The printed link carries the secret; show only its 8-character prefix,
  // as the app does everywhere after pairing. (The workspace's address is
  // private and the server is switched off again at the end.)
  await ev(
    `(() => { const c = document.querySelector('[role="dialog"] code'); if (c) c.textContent = c.textContent.replace(/(#t=)(\\S{8})\\S*/, '$1$2…'); })()`,
  );
  await sleep(1200);
  await shot(`${mode}-06-remote-access`);
  await send('Input.dispatchKeyEvent', {
    type: 'keyDown',
    key: 'Escape',
    code: 'Escape',
    windowsVirtualKeyCode: 27,
  });
  await send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key: 'Escape',
    code: 'Escape',
    windowsVirtualKeyCode: 27,
  });
  await sleep(800);
}

// Frames for the hero loop: dashboard, then the diagnosis opening.
await send('Emulation.setEmulatedMedia', {
  features: [{ name: 'prefers-color-scheme', value: 'dark' }],
});
await nav('dashboard');
mkdirSync(join(outDir, 'frames'), { recursive: true });
let f = 0;
const frame = async () => {
  const r = await send('Page.captureScreenshot', { format: 'jpeg', quality: 85 });
  writeFileSync(
    join(outDir, 'frames', `f${String(f++).padStart(4, '0')}.jpg`),
    Buffer.from(r.result.data, 'base64'),
  );
};
for (let i = 0; i < 24; i++) {
  await frame();
  await sleep(125);
}
await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1700, y: 90 });
await click('button', 'Why is my PC slow?');
for (let i = 0; i < 40; i++) {
  await frame();
  await sleep(125);
}

// Leave remote access off on the workspace.
await click('button', 'Settings');
await waitFor(`document.querySelector('[role="dialog"]')`);
await mouseClick('[role="dialog"] [role="tab"]', 'Remote access');
await sleep(800);
if (
  await ev(
    `[...document.querySelectorAll('[role="dialog"] [role="tab"]')].some((t) => t.getAttribute('aria-selected') === 'true' && t.textContent.includes('Remote access'))`,
  )
) {
  await mouseClick('[role="dialog"] [role="switch"]', '');
}
await sleep(1500);
ws.close();
app.kill();
log('done');
process.exit(0);
