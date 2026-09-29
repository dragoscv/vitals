// `vitest/config` rather than `vite` so the `test` block is typed.
import { defineConfig, type Plugin } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as {
  version: string;
};

/**
 * Turns Vite's `<script type="module" crossorigin>` into a classic deferred
 * script.
 *
 * A packaged Tizen app is loaded from `file://`, whose origin is opaque.
 * Chromium refuses module scripts and `crossorigin` resources from an opaque
 * origin, so the page would be blank on the TV while working perfectly in a
 * desktop browser against the dev server. The bundle is IIFE for the same
 * reason; together they make the output an ordinary script, which is what
 * every Tizen version runs.
 */
function classicScript(): Plugin {
  return {
    name: 'vitals-tizen-classic-script',
    apply: 'build',
    transformIndexHtml: {
      order: 'post',
      handler: (html) =>
        html
          .replace(/<script type="module" crossorigin/g, '<script defer')
          .replace(/<link rel="stylesheet" crossorigin/g, '<link rel="stylesheet"'),
    },
  };
}

export default defineConfig({
  // Relative, because the app is opened from the package's own directory,
  // not from a server root.
  base: './',
  plugins: [react(), classicScript()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  server: { port: 5283, strictPort: true },
  build: {
    // Tizen 9 ships Chromium M120; nothing older runs this package.
    target: 'chrome120',
    minify: true,
    sourcemap: false,
    // No preload polyfill: there is one script and nothing to preload.
    modulePreload: false,
    // Inline nothing into a data: URL — the icon and config.xml must stay
    // files for the Tizen packager to find them.
    assetsInlineLimit: 0,
    rolldownOptions: {
      output: {
        format: 'iife',
        // A single file. Dynamic chunks would be fetched from file:// at
        // runtime, which is the one thing this build exists to avoid.
        codeSplitting: false,
      },
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
