import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile } from 'tailwindcss';
import { describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

/**
 * Compiles `styles.css` the way the Vite plugin does, with the candidate
 * classes supplied directly instead of scanned from `@source`.
 *
 * Tailwind v4 only emits a breakpoint when a utility uses it, so asserting on
 * the `@theme` block alone would pass while `3xl:` silently produced nothing.
 * Compiling with real candidates is the assertion that the variant exists.
 */
async function build(candidates: readonly string[]): Promise<string> {
  const source = await readFile(resolve(here, 'styles.css'), 'utf8');
  const compiler = await compile(source, {
    base: here,
    async loadStylesheet(id, base) {
      // `@import 'tailwindcss'` and `@import '@vitals/ui/styles.css'` both
      // resolve through the package graph, as they do under Vite.
      const path = id.startsWith('.')
        ? resolve(base, id)
        : id === 'tailwindcss'
          ? require.resolve('tailwindcss/index.css', { paths: [base] })
          : require.resolve(id, { paths: [base] });
      return { path, base: dirname(path), content: await readFile(path, 'utf8') };
    },
  });
  return compiler.build([...candidates]);
}

describe('ultrawide breakpoints', () => {
  it('emit 3xl/4xl/5xl media variants at 120/160/240rem so grids can add columns past 1536px', async () => {
    const css = await build(['3xl:grid-cols-4', '4xl:grid-cols-5', '5xl:grid-cols-6']);

    // Tailwind v4 writes range syntax; accept either spelling but require the
    // exact widths — a mistyped `--breakpoint-3xl` would still compile.
    expect(css).toMatch(/@media \(\s*(?:width\s*>=\s*|min-width:\s*)120rem\s*\)/);
    expect(css).toMatch(/@media \(\s*(?:width\s*>=\s*|min-width:\s*)160rem\s*\)/);
    expect(css).toMatch(/@media \(\s*(?:width\s*>=\s*|min-width:\s*)240rem\s*\)/);
    expect(css).toContain('grid-template-columns: repeat(4, minmax(0, 1fr))');
  });

  it('emit matching @container variants so a feature can respond to the width the main region actually gets', async () => {
    const css = await build(['@container/main', '@3xl/main:grid-cols-4']);

    expect(css).toContain('container-name: main');
    expect(css).toMatch(/@container main \(\s*(?:width\s*>=\s*|min-width:\s*)120rem\s*\)/);
  });

  it('cap reading width below the grid cap, since prose and tiles have different limits', async () => {
    const source = await readFile(resolve(here, 'styles.css'), 'utf8');
    const reading = /--reading-max:\s*([\d.]+)rem/.exec(source);
    const content = /--content-max:\s*([\d.]+)rem/.exec(source);

    expect(Number(reading?.[1])).toBeLessThanOrEqual(90);
    expect(Number(content?.[1])).toBeGreaterThan(Number(reading?.[1]));
  });
});

/**
 * How far a box-shadow paints past its box on each side, in px, ignoring
 * inset layers. A CSS blur is a Gaussian with sigma = blur / 2, so beyond
 * 0.7 × blur the tail is under ~8 % of peak; at the ≤ 0.65 alpha these tokens
 * use that is under 5 % opacity — the point where a cut edge stops showing.
 */
function reach(shadow: string): { side: number; bottom: number } {
  let side = 0;
  let bottom = 0;
  for (const layer of shadow.split(/,(?![^(]*\))/)) {
    if (/inset|var\(/.test(layer)) continue;
    const lengths = layer.replace(/oklch\([^)]*\)/g, '').match(/-?[\d.]+(?:px)?/g);
    if (lengths === null || lengths.length < 2) continue;
    const [x = 0, y = 0, blur = 0, spread = 0] = lengths.map(Number.parseFloat);
    const extent = 0.7 * blur + spread;
    side = Math.max(side, extent + Math.abs(x));
    bottom = Math.max(bottom, extent + y);
  }
  return { side, bottom };
}

/** Every value a token takes, light and dark, from the source. */
function tokenValues(css: string, name: string): string[] {
  return [...css.matchAll(new RegExp(`--${name}:([^;]+);`, 'g'))].map((m) => m[1] ?? '');
}

describe('scroll gutters', () => {
  it('are wide enough that no card shadow or hover lift is cut at a scroll region edge', async () => {
    // Found live 2026-09-28: the 8/4 px gutter sliced 21 shadows across 12
    // sections. `overflow: auto` clips at the padding box, so the gutter must
    // cover the reach of the largest shadow a scrolled item can wear.
    const app = await readFile(resolve(here, 'styles.css'), 'utf8');
    const theme = await readFile(
      resolve(here, '../../../packages/ui/src/styles/theme.css'),
      'utf8',
    );
    const rem = (name: string): number =>
      16 * Number(new RegExp(`--${name}:\\s*([\\d.]+)rem`).exec(app)?.[1] ?? Number.NaN);
    const gutterX = rem('scroll-gutter-x');
    const gutterEnd = rem('scroll-gutter-end');
    expect(gutterX).toBeGreaterThan(0);
    expect(gutterEnd).toBeGreaterThan(0);

    const shadows = [
      ...tokenValues(theme, 'shadow-card'),
      ...tokenValues(theme, 'shadow-card-hover'),
    ];
    expect(shadows.length).toBeGreaterThanOrEqual(4);
    for (const shadow of shadows) {
      const { side, bottom } = reach(shadow);
      expect(side, shadow).toBeLessThanOrEqual(gutterX);
      // The bottom of the last item sits against the scroller's padding too.
      expect(bottom, shadow).toBeLessThanOrEqual(gutterEnd);
    }
  });

  it('leave the contained accent glow inside the 8 px a nav list has', async () => {
    const theme = await readFile(
      resolve(here, '../../../packages/ui/src/styles/theme.css'),
      'utf8',
    );
    const values = tokenValues(theme, 'glow-accent-contained');
    expect(values.length).toBe(2);
    for (const value of values) expect(reach(value).side, value).toBeLessThanOrEqual(8);
  });
});

/** The class string on the first `<tag ...>` in a component's source. */
async function rootClasses(file: string, tag: string): Promise<string[]> {
  const source = await readFile(resolve(here, file), 'utf8');
  const match = new RegExp(`<${tag}\\b[^>]*?className="([^"]+)"`).exec(source);
  expect(match, `${file} <${tag} className>`).not.toBeNull();
  return (match?.[1] ?? '').split(/\s+/).filter(Boolean);
}

/** Every `selector { body }` whose body declares a background or a border. */
function paintingRules(css: string): { selector: string; body: string }[] {
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .map((m) => ({ selector: (m[1] ?? '').trim(), body: m[2] ?? '' }))
    .filter(({ body }) =>
      /(?:^|[;\s])(?:background(?:-color|-image)?|border(?:-(?:top|right|bottom|left|inline|block)[\w-]*)?(?:-width|-style)?)\s*:/.test(
        body,
      ),
    );
}

describe('one window background', () => {
  // The user asked for title bar, sidebar and content to read as one surface
  // (S12-23). They used to be three: the chrome had a tinted mica/acrylic
  // fill of `--color-bg-subtle` and a hairline border on each seam.
  const regions = [
    ['shell/TitleBar.tsx', 'header'],
    ['shell/Sidebar.tsx', 'nav'],
    ['shell/Content.tsx', 'main'],
  ] as const;

  it('is painted once, on #root, with the base token', async () => {
    const css = await build([]);
    const canvases = paintingRules(css).filter(({ body }) => body.includes('--color-bg-base'));
    const selectors = canvases.map(({ selector }) => selector);
    // `body` carries the base colour too, for the instant before #root mounts.
    expect(selectors).toContain('#root');
    for (const selector of selectors) expect(['#root', 'body']).toContain(selector);
  });

  it('leaves the title bar, sidebar and content region without a fill or a border of their own', async () => {
    for (const [file, tag] of regions) {
      const classes = await rootClasses(file, tag);
      const painted = paintingRules(await build(classes));
      // Tailwind compiles only the candidates it is given, so anything left
      // here came from one of this region's own classes.
      const own = painted.filter(({ selector }) =>
        classes.some((c) => selector.includes(c.replace(/[[\]()/:.%]/g, (ch) => `\\${ch}`))),
      );
      expect(own, `${file} <${tag}>`).toEqual([]);
    }
  });

  it('has no stylesheet rule that paints a shell region', async () => {
    const css = await build([]);
    // The subject (last compound) is what gets painted: `main h2` paints a
    // heading, `nav.shell-chrome` paints the sidebar.
    const shell = /^(?:header|nav|main)\b|#main-content|\.shell-chrome|\.surface-chrome/;
    const subject = (selector: string): string[] =>
      selector.split(',').map(
        (part) =>
          part
            .trim()
            .split(/[\s>+~]+/)
            .at(-1) ?? '',
      );
    const hits = paintingRules(css).filter(({ selector }) =>
      subject(selector).some((s) => shell.test(s)),
    );
    expect(hits).toEqual([]);
  });

  it('draws no divider inside the chrome either', async () => {
    // The sidebar footer had a border-t; a seam inside the chrome reads as a
    // panel edge just as much as one between regions.
    for (const file of ['shell/TitleBar.tsx', 'shell/Sidebar.tsx']) {
      const source = await readFile(resolve(here, file), 'utf8');
      expect(source, file).not.toMatch(/(?<![\w-])border(?:-[trblxy])?(?=[\s'"`])/);
    }
  });
});

describe('the dashboard fits the window', () => {
  // S12-25: the dashboard scrolled 438 px at 1280×800 in two columns of
  // content-height cards. It now shares the height between its rows and a
  // card that is too short scrolls inside itself.
  it('shares the height between rows instead of growing with content', async () => {
    const css = await build([]);
    const grid = /\.dashboard-grid\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';
    expect(grid).toMatch(/grid-auto-rows:\s*minmax\(0,\s*1fr\)/);
    expect(grid).toMatch(/min-height:\s*0/);
    expect(grid).not.toMatch(/overflow(-y)?:\s*(auto|scroll)/);
  });

  it('puts the grid directly in the screen, not in a scrolling wrapper', async () => {
    const source = await readFile(resolve(here, 'features/dashboard/DashboardScreen.tsx'), 'utf8');
    const grid = source.indexOf('dashboard-grid');
    expect(grid).toBeGreaterThan(0);
    // The nearest wrapper opened before the grid must not be `screen-scroll`.
    const before = source.slice(0, grid);
    const lastScroll = before.lastIndexOf('screen-scroll');
    const lastEmpty = before.lastIndexOf(': placements.length === 0');
    expect(lastScroll).toBeLessThan(lastEmpty);
  });

  it('lets a card scroll its own body when its share of the height is too small', async () => {
    const source = await readFile(resolve(here, 'features/dashboard/WidgetFrame.tsx'), 'utf8');
    expect(source).toMatch(/className="widget-body[^"]*\bmin-h-0\b[^"]*\boverflow-y-auto\b/);
    expect(source).toMatch(/'@container\/widget flex min-h-0 min-w-0 flex-col overflow-hidden'/);
  });
});

describe('screens made of panes', () => {
  // S12-26: Devices, Storage, Benchmarks and Users scrolled their whole body
  // under a fixed title, so section headers and buttons scrolled away. At a
  // normal size the body now fits and each pane scrolls itself; only a small
  // window stacks the panes and scrolls the body.
  const rule = (css: string, selector: string): string =>
    new RegExp(`${selector.replace(/[.]/g, '\\.')}\\s*\\{([^}]*)\\}`).exec(css)?.[1] ?? '';

  it('fits the window at a normal size: the body grid never scrolls', async () => {
    const css = await build([]);
    const body = rule(css, '.screen-body');
    expect(body).toMatch(/grid-auto-rows:\s*minmax\(0,\s*1fr\)/);
    expect(body).toMatch(/min-height:\s*0/);
    expect(body).not.toMatch(/overflow/);
    expect(rule(css, '.pane-scroll')).toMatch(/overflow:\s*auto/);
  });

  it('stacks the panes and scrolls the body only when the window is small', async () => {
    const source = await readFile(resolve(here, 'styles.css'), 'utf8');
    for (const query of ['@media (max-height: 640px)', '@container main (width < 40rem)']) {
      const start = source.indexOf(query);
      expect(start, query).toBeGreaterThan(0);
      const block = source.slice(start, source.indexOf('\n}\n', start));
      expect(block, query).toMatch(/\.screen-body\s*\{[^}]*overflow-y:\s*auto/);
      expect(block, query).toMatch(/grid-template-rows:\s*none/);
    }
  });

  it('leaves no screen scrolling its whole body with `screen-scroll` except the rail detail', async () => {
    const { execFileSync } = await import('node:child_process');
    const hits = execFileSync('rg', ['-l', 'className="screen-scroll', resolve(here, 'features')], {
      encoding: 'utf8',
    })
      .split(/\r?\n/)
      .filter(Boolean)
      .map((path) => path.replace(/\\/g, '/').split('/features/')[1]);
    // Performance: the detail beside the rail, a pane in all but name.
    // Dashboard: only its loading skeleton.
    expect(hits.sort()).toEqual([
      'dashboard/DashboardScreen.tsx',
      'performance/PerformanceScreen.tsx',
    ]);
  });
});
