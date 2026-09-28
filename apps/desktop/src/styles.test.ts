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
