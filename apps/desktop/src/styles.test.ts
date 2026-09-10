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
