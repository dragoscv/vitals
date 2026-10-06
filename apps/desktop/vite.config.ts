// `vitest/config` rather than `vite` so the `test` block is typed. Vite's own
// `defineConfig` has no knowledge of it and rejects the property.
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

// Tauri drives the dev server, so the port must be fixed and failures must be
// loud — silently falling back to another port leaves the window pointing at
// nothing, which looks like a hung app.
const DEV_PORT = 5273;

export default defineConfig(({ command }) => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, 'src'),
    },
  },

  clearScreen: false,

  server: {
    port: DEV_PORT,
    strictPort: true,
    host: false,
    watch: {
      // Rust rebuilds are driven by cargo; watching target/ would trigger a
      // reload storm on every incremental compile.
      ignored: ['**/src-tauri/**'],
    },
  },

  envPrefix: ['VITE_', 'TAURI_'],

  build: {
    // Tauri ships its own WebView2 baseline; targeting anything older costs
    // bundle size for browsers that will never run this.
    target: 'chrome120',
    // Vite 8 is built on Rolldown, whose native Oxc minifier is the default.
    // Asking for 'esbuild' here pulls in a now-optional dependency and is
    // slower for no benefit.
    minify: true,
    // No maps in a packaged build. `frontendDist` is the whole `dist`
    // directory, so anything written there ends up inside the installer and
    // inside every differential update — and the maps were 3 MB of a 3.8 MB
    // dist, four times the size of the app they describe.
    //
    // `hidden` is not enough: it only removes the `sourceMappingURL` comment
    // and still writes the files, which Tauri would still package. Set
    // VITALS_SOURCEMAPS=1 to get them back for a debugging build.
    sourcemap: command !== 'build' || process.env.VITALS_SOURCEMAPS === '1',
    rolldownOptions: {
      // Three pages, one build: `index.html` is the desktop webview,
      // `mobile.html` is what a phone loads from the LAN server, and
      // `hud.html` is the always-on-top overlay. All land in `dist`, which
      // is Tauri's `frontendDist`, so the phone is served the exact build
      // the desktop is running and the overlay is loaded from the same
      // bundle rather than a second copy of React.
      input: {
        index: path.resolve(import.meta.dirname, 'index.html'),
        mobile: path.resolve(import.meta.dirname, 'mobile.html'),
        hud: path.resolve(import.meta.dirname, 'hud.html'),
      },
      output: {
        // Split React into its own chunk so a UI-only change does not
        // invalidate the whole bundle for the updater's differential
        // download.
        //
        // `output.manualChunks` is deprecated in Rolldown (the object form is
        // already removed); `codeSplitting.groups` is the supported successor.
        // The `[\\/]` alternation is required because module ids on Windows
        // contain backslashes and `node_modules/react` would never match.
        advancedChunks: {
          groups: [
            {
              name: 'react',
              test: /[\\/]node_modules[\\/](react|react-dom|scheduler)[\\/]/,
            },
          ],
        },
      },
    },
  },

  test: {
    environment: 'happy-dom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    // 15 s, not vitest's 5 s. The shell tests render AppShell and resolve
    // real lazy chunks; on this machine, shared with other agents' builds at
    // 90 % CPU, a DIFFERENT one of them crossed 5 s on each full run
    // (2026-09-27: "opens settings", then "names the window" and main.test
    // "reports a failure") while every one passes alone in under a second.
    // A test that truly hangs still fails; one that is slow under load no
    // longer does.
    testTimeout: 15_000,
    // Hooks get the same margin: `AppShell.test.tsx` warms the lazy
    // SettingsDialog in `beforeAll` precisely so the test itself measures
    // focus rather than a cold transform — which moved the cold transform
    // into the hook, where vitest's 10 s default then failed the whole file
    // at 100 % CPU (2026-09-29) while it passes alone in 8-12 s.
    hookTimeout: 15_000,
    // A quarter of the cores, not vitest's cores-minus-one. Each fork boots
    // its own happy-dom, and on this 32-thread machine 31 of them spent half
    // the run in environment setup fighting each other: measured back to back
    // at the same load (2026-09-28), 31 forks took 122.8 s with two timeouts,
    // 8 forks took 46.3 s with all 985 passing.
    maxWorkers: '25%',
    coverage: {
      provider: 'v8',
      reporter: ['text-summary', 'lcov'],
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/**/*.{test,spec}.{ts,tsx}', 'src/test/**', 'src/**/strings.ts'],
      // A floor, not a target. Raise it when it is comfortably exceeded;
      // never lower it to make a run pass.
      thresholds: { statements: 70, branches: 70, functions: 70, lines: 70 },
    },
  },
}));
