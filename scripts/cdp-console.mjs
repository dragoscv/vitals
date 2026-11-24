/*
 * Captures the webview's console through the WebView2 DevTools protocol, so a
 * React warning that only exists in the console (and only in a real window)
 * arrives with its component stack instead of being guessed at.
 *
 * A duplicate React key naming "Vitals" was reported from the console and could
 * not be reproduced: this is the tool that says so, and the reason the answer
 * is "not reproducible" rather than "we think it is fine". Two properties make
 * it usable as evidence, and both are missing from a plain look at the console:
 *
 *   - The subscription is proved live. A line this script emits itself has to
 *     come back before the run is reported, because "nothing was printed" and
 *     "the subscription was dead" look identical otherwise — a false negative
 *     is what ends an investigation early, and it reads like a clean result.
 *   - Every click is proved by the title it produced. A click that missed looks
 *     exactly like a page with nothing to report.
 *
 * The port belongs to the app, not to this script: a window started without
 * WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS has no port, and a window started with
 * it closes the port again on the next restart.
 *
 * Usage:
 *   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9333 --remote-allow-origins=*'
 *   Start-Process "$env:LOCALAPPDATA\Programs\Vitals\vitals-desktop.exe"
 *   node scripts/cdp-console.mjs 20 --reload 112,82 283,227
 *
 * Arguments: [seconds to listen] [--reload] [clickX,clickY ...]
 */
const seconds = Number(process.argv[2] ?? 20);
const rest = process.argv.slice(3);
const reload = rest.includes('--reload');
const clicks = rest.filter((s) => s !== '--reload').map((s) => s.split(',').map(Number));
const PORT = process.env.CDP_PORT ?? '9333';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function target() {
  for (let i = 0; i < 20; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page;
    } catch {
      /* the port is not up yet */
    }
    await sleep(500);
  }
  throw new Error(`no page target on 127.0.0.1:${PORT}`);
}

const describe = (arg) => {
  if (arg === null) return 'null';
  if (typeof arg.value === 'string') return JSON.stringify(arg.value);
  if (arg.value !== undefined) return JSON.stringify(arg.value);
  return arg.description ?? arg.type;
};

const page = await target();
console.log(`target ${page.title} ${page.url}`);

const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const send = (method, params = {}) => ws.send(JSON.stringify({ id: ++id, method, params }));

const seen = [];
let lastTitle = '';
let nextId = 0;
const titles = new Map();
ws.addEventListener('message', (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id !== undefined && titles.has(msg.id)) {
    const value = msg.result?.result?.value;
    if (typeof value === 'string') {
      lastTitle = value;
      console.log(`      title now: ${value}`);
    }
    titles.delete(msg.id);
  }
  if (msg.method === 'Runtime.consoleAPICalled') {
    const text = msg.params.args.map(describe).join(' ');
    const frames = (msg.params.stackTrace?.callFrames ?? [])
      .slice(0, 12)
      .map((f) => `      at ${f.functionName || '<anonymous>'} (${f.url}:${f.lineNumber + 1})`)
      .join('\n');
    const line = `[${msg.params.type}] ${text}${frames ? `\n${frames}` : ''}`;
    seen.push(line);
    console.log(line);
  }
  if (msg.method === 'Runtime.exceptionThrown') {
    const line = `[exception] ${msg.params.exceptionDetails.text} ${msg.params.exceptionDetails.exception?.description ?? ''}`;
    seen.push(line);
    console.log(line);
  }
});

ws.addEventListener('open', () => {
  send('Runtime.enable');
  send('Log.enable');
  send('Page.enable');
  if (reload) setTimeout(() => send('Page.reload', { ignoreCache: false }), 200);
});

await sleep(1500);
// Prove the channel works: a console line we emit ourselves has to come back.
// Without this, "no console output" cannot be told apart from "the subscription
// is dead", which is exactly the kind of false negative that ends an
// investigation early.
send('Runtime.evaluate', { expression: "console.log('cdp-probe')" });
await sleep(1200);
console.log(
  seen.some((line) => line.includes('cdp-probe'))
    ? 'channel: OK (our own line came back)'
    : 'channel: DEAD (our own line did not come back)',
);
for (const [x, y] of clicks) {
  console.log(`click ${x},${y}`);
  send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
  send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
  await sleep(1200);
  // A click that missed would look exactly like a page with nothing to report,
  // so every click is proved by the title it produced.
  const probe = ++nextId + 1000;
  titles.set(probe, true);
  ws.send(
    JSON.stringify({
      id: probe,
      method: 'Runtime.evaluate',
      params: { expression: 'document.title', returnByValue: true },
    }),
  );
  await sleep(1300);
}

await sleep(seconds * 1000);
console.log(`--- ${seen.length} console line(s) captured ---`);
if (lastTitle !== '') console.log(`last title: ${lastTitle}`);
ws.close();
