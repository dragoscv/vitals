#!/usr/bin/env node
// The Vitals brand kit: motion (SVG, Lottie, MP4, GIF, WebM), splash and
// installer art, animated UI icons, social and store images, the brand book
// (HTML + PDF) and the press kit ZIP — all generated from brand/mark.mjs and
// brand/brand.json. Run after build.mjs: node brand/scripts/kit.mjs
//
// Needs: sharp (root devDependency), Chromium via apps/desktop's
// @playwright/test, and ffmpeg on PATH for the videos (skipped with a
// message if absent; the frame sequences are kept so the one command in
// docs/brand-kit.md can encode them later).
//
// Flags: --only=motion,splash,icons,social,book,press  run a subset
//        --fps=60                                       video frame rate
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import sharp from 'sharp';
import { format, resolveConfig } from 'prettier';
import { colour } from '../mark.mjs';
import { at, root, write, brand, markSvg, lockupSvg, launch } from './kit/common.mjs';
import { MOTIONS, animatedSvg } from './kit/motion.mjs';
import { lottieFor, validateLottie } from './kit/lottie.mjs';
import { SOCIAL, socialHtml, COPY } from './kit/social.mjs';
import { fieldSvg, androidSplashSvg, androidAvd, bmp } from './kit/splash.mjs';
import { bookHtml } from './kit/book.mjs';
import { zip } from './kit/zip.mjs';

const args = Object.fromEntries(
  process.argv.slice(2).map((a) => {
    const [k, v] = a.replace(/^--/, '').split('=');
    return [k, v ?? true];
  }),
);
const only = args.only ? new Set(String(args.only).split(',')) : null;
const want = (s) => !only || only.has(s);
const FPS = Number(args.fps ?? 60);
const VERSION = brand.version;
const DAY = new Date().toISOString().slice(0, 10);
const tmp = at('.copilot-tmp', 'kit-frames');
const log = (...a) => console.log(`[kit ${new Date().toISOString().slice(11, 19)}]`, ...a);

const ffmpeg = (() => {
  const r = spawnSync('ffmpeg', ['-hide_banner', '-version'], { encoding: 'utf8' });
  return r.status === 0 ? 'ffmpeg' : null;
})();
function ff(argv) {
  const r = spawnSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y', ...argv], {
    encoding: 'utf8',
  });
  if (r.status !== 0) throw new Error(`ffmpeg ${argv.join(' ')}\n${r.stderr}`);
}

/** Formats a generated file with the repo's own Prettier config, so the format gate accepts it. */
async function prettier(text, path) {
  const filepath = at(path);
  return format(text, { ...(await resolveConfig(filepath)), filepath });
}

const svgPng = (svg, w, h = w) =>
  sharp(Buffer.from(svg), { density: 288 }).resize(w, h).png({ compressionLevel: 9 }).toBuffer();

let browser;
const getBrowser = async () => (browser ??= await launch());

// ---------- 1. motion ----------
const VIDEO_PX = 720;
const GIF_PX = 360;
const BG = { dark: colour.night.hex, light: colour.paper.hex };

/** Renders one motion's frames for a background ('transparent' keeps alpha). */
async function frames(id, variant, fps) {
  const mo = MOTIONS[id];
  const dir = join(tmp, `${id}-${variant}-${fps}`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const b = await getBrowser();
  const ctx = await b.newContext({
    viewport: { width: VIDEO_PX, height: VIDEO_PX },
    colorScheme: variant === 'light' ? 'light' : 'dark',
    reducedMotion: 'no-preference',
  });
  const page = await ctx.newPage();
  const bg = variant === 'transparent' ? 'transparent' : BG[variant];
  const svg = animatedSvg(id, { px: Math.round(VIDEO_PX * 0.5) });
  await page.setContent(
    `<!doctype html><style>html,body{margin:0;width:${VIDEO_PX}px;height:${VIDEO_PX}px;background:${bg};display:grid;place-items:center;overflow:hidden}</style>${svg}`,
  );
  const n = Math.round((mo.renderMs / 1000) * fps);
  for (let f = 0; f < n; f++) {
    const t = (f * 1000) / fps;
    await page.evaluate((ms) => {
      for (const a of document.getAnimations()) {
        a.pause();
        a.currentTime = ms;
      }
    }, t);
    await page.screenshot({
      path: join(dir, `f${String(f).padStart(4, '0')}.png`),
      omitBackground: variant === 'transparent',
    });
  }
  await ctx.close();
  return { dir, n };
}

async function buildMotion(out) {
  for (const [id, mo] of Object.entries(MOTIONS)) {
    const svgPath = `brand/motion/vitals-${id}.svg`;
    write(at(svgPath), animatedSvg(id));
    out.push([svgPath, `${mo.title}, animated SVG (reduced-motion aware)`]);
    const doc = lottieFor(id);
    const json = JSON.stringify(doc);
    validateLottie(JSON.parse(json), id);
    // Formatted so the repo's Prettier gate accepts the committed file; parse-checked again after.
    const pretty = await prettier(json, `brand/motion/vitals-${id}.json`);
    validateLottie(JSON.parse(pretty), id);
    write(at(`brand/motion/vitals-${id}.json`), pretty);
    out.push([
      `brand/motion/vitals-${id}.json`,
      `${mo.title}, Lottie ${doc.fr} fps ${doc.w}×${doc.h}`,
    ]);
  }
  log('motion: svg + lottie written and validated');
  for (const id of Object.keys(MOTIONS)) {
    for (const variant of ['dark', 'light']) {
      const { dir, n } = await frames(id, variant, FPS);
      log(`frames ${id}/${variant}: ${n}`);
      if (!ffmpeg) continue;
      const input = ['-framerate', String(FPS), '-i', join(dir, 'f%04d.png')];
      const mp4 = `brand/motion/vitals-${id}-${variant}.mp4`;
      ff([
        ...input,
        '-c:v',
        'libx264',
        '-preset',
        'slow',
        '-crf',
        '22',
        '-pix_fmt',
        'yuv420p',
        '-movflags',
        '+faststart',
        at(mp4),
      ]);
      out.push([mp4, `${MOTIONS[id].title}, ${variant} background, ${VIDEO_PX}² H.264`]);
      const gif = `brand/motion/vitals-${id}-${variant}.gif`;
      ff([
        ...input,
        '-vf',
        `fps=30,scale=${GIF_PX}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=full[p];[b][p]paletteuse=dither=bayer:bayer_scale=4`,
        '-loop',
        '0',
        at(gif),
      ]);
      out.push([gif, `${MOTIONS[id].title}, ${variant} background, ${GIF_PX}² GIF`]);
    }
    const { dir } = await frames(id, 'transparent', 30);
    if (ffmpeg) {
      const webm = `brand/motion/vitals-${id}.webm`;
      ff([
        '-framerate',
        '30',
        '-i',
        join(dir, 'f%04d.png'),
        '-c:v',
        'libvpx-vp9',
        '-pix_fmt',
        'yuva420p',
        '-b:v',
        '0',
        '-crf',
        '34',
        '-auto-alt-ref',
        '0',
        '-row-mt',
        '1',
        at(webm),
      ]);
      out.push([webm, `${MOTIONS[id].title}, transparent VP9 WebM (alpha)`]);
    }
    log(`motion ${id}: encoded`);
  }
  if (!ffmpeg) log('ffmpeg not found: frames kept in .copilot-tmp/kit-frames, no MP4/GIF/WebM');
}

// ---------- 2. splash ----------
async function buildSplash(out) {
  const items = [
    [
      'brand/splash/desktop-splash-1280x720.png',
      fieldSvg(1280, 720, { markFrac: 0.2 }),
      'Desktop splash / loading screen 16:9',
    ],
    [
      'brand/splash/desktop-splash-620x300.png',
      fieldSvg(620, 300, { markFrac: 0.28 }),
      'Installer / sidebar-wide splash',
    ],
    [
      'brand/splash/mobile-splash-1080x1920.png',
      fieldSvg(1080, 1920, { markFrac: 0.3 }),
      'Portrait splash (PWA / store)',
    ],
  ];
  for (const [p, svg, d] of items) {
    const m = /(\d+)x(\d+)/.exec(p);
    write(at(p), await svgPng(svg, Number(m[1]), Number(m[2])));
    out.push([p, d]);
  }
  // NSIS (MUI2) header 150×57 and welcome/finish sidebar 164×314, BMP as NSIS requires.
  const nsis = [
    [
      'nsis-header-150x57',
      fieldSvg(150, 57, { markFrac: 0.62 }),
      150,
      57,
      'NSIS header image (installer pages)',
    ],
    [
      'nsis-sidebar-164x314',
      fieldSvg(164, 314, { markFrac: 0.55 }),
      164,
      314,
      'NSIS welcome/finish sidebar',
    ],
  ];
  for (const [name, svg, w, h, d] of nsis) {
    const { data, info } = await sharp(Buffer.from(svg), { density: 288 })
      .resize(w, h)
      .removeAlpha()
      .raw()
      .toBuffer({ resolveWithObject: true });
    write(at(`brand/splash/${name}.bmp`), bmp(data, w, h, info.channels));
    write(at(`brand/splash/${name}.png`), await svgPng(svg, w, h));
    out.push([`brand/splash/${name}.bmp`, `${d} (24-bit BMP)`]);
  }
  // Android 12+: 288 dp canvas, 192 dp circle; xxxhdpi = 4× → 1152 px.
  write(
    at('brand/splash/android/splash-icon-xxxhdpi.png'),
    await svgPng(androidSplashSvg(1152), 1152),
  );
  write(
    at('brand/splash/android/splash-icon-line-xxxhdpi.png'),
    await svgPng(androidSplashSvg(1152, { plate: false }), 1152),
  );
  write(
    at('brand/splash/android/splash-icon-guide.png'),
    await svgPng(androidSplashSvg(576, { guide: true }), 576),
  );
  write(at('brand/splash/android/splash-icon.svg'), androidSplashSvg(288));
  write(at('brand/splash/android/avd_vitals_splash.xml'), androidAvd());
  out.push(
    [
      'brand/splash/android/splash-icon-xxxhdpi.png',
      'Android 12+ splash icon, 1152 px (288 dp @ 4×), tile inside the 192 dp circle',
    ],
    [
      'brand/splash/android/splash-icon-line-xxxhdpi.png',
      'Android 12+ splash icon, line only (on a fern or night background)',
    ],
    [
      'brand/splash/android/splash-icon-guide.png',
      'Splash icon with the 192 dp mask drawn, for review',
    ],
    [
      'brand/splash/android/avd_vitals_splash.xml',
      'AnimatedVectorDrawable suggestion (intro, 900 ms) — not wired into the app',
    ],
  );
  log('splash: done');
}

// ---------- 3. animated icons ----------
function buildIcons(out) {
  const map = { idle: 'pulse', alert: 'alert', thinking: 'thinking' };
  for (const [state, id] of Object.entries(map)) {
    for (const px of [16, 24, 32]) {
      const p = `brand/icons/animated/vitals-${state}-${px}.svg`;
      write(at(p), animatedSvg(id, { px, title: `Vitals — ${state}` }));
      out.push([p, `${state}, ${px} px (optical variant ${px <= 20 ? 'micro' : 'small'})`]);
    }
  }
  log('icons: done');
}

// ---------- 4. logos (PNG for press) ----------
async function buildLogos(out) {
  for (const px of [512, 1024, 2048]) {
    const p = `brand/logo/png/mark-${px}.png`;
    write(at(p), await svgPng(markSvg(px), px));
    out.push([p, `Mark, ${px} px, transparent`]);
    for (const [name, ink] of [
      ['light', colour.ink.hex],
      ['dark', colour.paper.hex],
    ]) {
      const svg = lockupSvg(ink, 48);
      const w = Number(/width="([\d.]+)"/.exec(svg)[1]);
      const h = Math.round((px * 48) / w);
      const q = `brand/logo/png/lockup-${name}-${px}.png`;
      write(at(q), await svgPng(svg, px, h));
      out.push([q, `Lockup for ${name} backgrounds, ${px} px wide`]);
    }
  }
  log('logos: done');
}

// ---------- 5. social ----------
async function buildSocial(out) {
  const b = await getBrowser();
  for (const s of SOCIAL) {
    const ctx = await b.newContext({
      viewport: { width: s.w, height: s.h },
      deviceScaleFactor: s.scale ?? 1,
    });
    const page = await ctx.newPage();
    await page.setContent(socialHtml(s), { waitUntil: 'load' });
    await page.evaluate(() => document.fonts.ready);
    // Fonts load lazily, so the text-less hero never loads InterKit; nothing to check there.
    if (s.layout !== 'hero') {
      const ok = await page.evaluate(
        ([text]) => document.fonts.check('40px InterKit', text),
        [COPY[s.lang].tagline + COPY[s.lang].sub],
      );
      if (!ok) throw new Error(`${s.name}: InterKit does not cover the copy`);
    }
    const shot = await page.screenshot({ type: 'png' });
    const p = `brand/social/${s.name}.png`;
    write(
      at(p),
      await sharp(shot)
        .png({ compressionLevel: 9, palette: s.layout === 'text' || s.layout === 'banner' })
        .toBuffer(),
    );
    out.push([p, s.use]);
    await ctx.close();
  }
  log('social: done');
}

// ---------- 6. brand book ----------
const GROUPS = [
  ['Logo (SVG + PNG)', ['logo']],
  ['Motion — animated SVG, Lottie, MP4, GIF, WebM', ['motion']],
  ['Animated UI icons', ['icons/animated']],
  ['Splash, installer and Android splash', ['splash']],
  ['Social and store images', ['social']],
];
function downloadGroups() {
  const rel = (f) => relative(root, f).replace(/\\/g, '/');
  return [
    ...GROUPS.map(([title, dirs]) => [
      title,
      dirs.flatMap((d) => walk(at('brand', d)).map(rel)).sort(),
    ]),
    [
      'Brand book and press kit',
      [
        'brand/book/vitals-brand-book.pdf',
        'brand/press/fact-sheet.md',
        `brand/dist/vitals-press-kit-${VERSION}.zip`,
      ],
    ],
  ];
}

async function buildBook(out) {
  // Formatted before the PDF is printed, so the PDF is rendered from exactly the committed bytes.
  const html = await prettier(bookHtml({ downloads: downloadGroups() }), 'brand/book/index.html');
  write(at('brand/book/index.html'), html);
  const b = await getBrowser();
  const ctx = await b.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(`file:///${at('brand/book/index.html').replace(/\\/g, '/')}`);
  await page.evaluate(() => document.fonts.ready);
  await page.pdf({
    path: at('brand/book/vitals-brand-book.pdf'),
    format: 'A4',
    printBackground: true,
    margin: { top: '12mm', bottom: '12mm', left: '0', right: '0' },
  });
  // Review copy for a human/agent to look at (not shipped).
  await page.waitForTimeout(2500);
  await page.screenshot({ path: join(tmp, '..', 'kit-book-full.png'), fullPage: true });
  await ctx.close();
  out.push(
    ['brand/book/index.html', 'Brand book, self-contained HTML'],
    ['brand/book/vitals-brand-book.pdf', 'Brand book, PDF'],
  );
  log('book: done');
}

// ---------- 7. press kit ----------
function walk(dir) {
  return existsSync(dir)
    ? readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
        e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
      )
    : [];
}
function buildPress(out) {
  const files = [];
  const add = (abs, name) =>
    files.push({ name: `vitals-press-kit-${VERSION}/${name}`, data: readFileSync(abs) });
  for (const f of walk(at('brand/logo')))
    add(f, `logo/${relative(at('brand/logo'), f).replace(/\\/g, '/')}`);
  for (const f of walk(at('brand/motion')))
    add(f, `motion/${relative(at('brand/motion'), f).replace(/\\/g, '/')}`);
  for (const f of walk(at('brand/icons/animated')))
    add(f, `icons/animated/${relative(at('brand/icons/animated'), f).replace(/\\/g, '/')}`);
  for (const f of walk(at('brand/social')))
    add(f, `social/${relative(at('brand/social'), f).replace(/\\/g, '/')}`);
  for (const f of walk(at('brand/splash')))
    add(f, `splash/${relative(at('brand/splash'), f).replace(/\\/g, '/')}`);
  for (const f of walk(at('packaging/msix/listing/screenshots')))
    add(
      f,
      `screenshots/${relative(at('packaging/msix/listing/screenshots'), f).replace(/\\/g, '/')}`,
    );
  add(at('brand/press/fact-sheet.md'), 'fact-sheet.md');
  add(at('brand/book/vitals-brand-book.pdf'), 'vitals-brand-book.pdf');
  add(at('brand/book/index.html'), 'brand-book.html');
  add(at('brand/OFL-Inter.txt'), 'logo/OFL-Inter.txt');
  const p = `brand/dist/vitals-press-kit-${VERSION}.zip`;
  write(at(p), zip(files, DAY));
  log(`press: ${files.length} files → ${p} (${(statSync(at(p)).size / 1048576).toFixed(2)} MB)`);
}

// ---------- run ----------
const produced = [];
try {
  if (want('motion')) await buildMotion(produced);
  if (want('splash')) await buildSplash(produced);
  if (want('icons')) buildIcons(produced);
  if (want('logos')) await buildLogos(produced);
  if (want('social')) await buildSocial(produced);
  if (want('book')) await buildBook(produced);
  if (want('press')) buildPress(produced);
} finally {
  await browser?.close();
}
log(`done: ${produced.length} assets, ffmpeg ${ffmpeg ? 'used' : 'absent'}`);
