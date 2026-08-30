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
    rollupOptions: {
      output: {
        // Split React into its own chunk so a UI-only change does not
        // invalidate the whole bundle for the updater's differential
        // download. Expressed as a function because Vite 8 types
        // `manualChunks` as a function when `output` is a single object.
        manualChunks(id: string) {
          if (id.includes('node_modules/react') || id.includes('node_modules/scheduler')) {
            return 'react';
          }
          return undefined;
        },
      },
    },
  },

  test: {
    environment: 'happy-dom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
  },
}));
