import { defineConfig, devices } from '@playwright/test';

// Not 5273, the port Tauri's dev server uses. The first runs reused a dev
// server already listening there — started from ANOTHER checkout — and so
// tested that checkout's code: two rounds of failures that no edit here
// could change (2026-09-29). Its own port, and never reuse, so the suite
// always drives the code in this directory. VITALS_E2E_PORT overrides.
const PORT = Number(process.env['VITALS_E2E_PORT'] ?? '5373');
const BASE_URL = `http://localhost:${PORT}`;
const CI = process.env['CI'] !== undefined && process.env['CI'] !== '';

/**
 * End-to-end tests for the desktop webview served by plain Vite, with no
 * Tauri host behind it.
 *
 * That is the "no host" fallback (src/shell/host.ts): every backend read
 * answers at once with a documented absence, so these tests exercise the
 * shell, routing, layout and accessibility of every screen without a sampler.
 * What they cannot see is real data — that is what the `prove_*` examples and
 * `serve_dev` are for.
 */
export default defineConfig({
  testDir: 'e2e',
  // 60 s per test and 15 s per assertion rather than 30/5: a cold Vite
  // transform of a lazy screen on a machine at 85-100 % CPU (other builds)
  // took over 5 s, and a timeout there says nothing about the screen.
  timeout: 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: true,
  forbidOnly: CI,
  // None locally: a retry that passes hides the flake from the person who
  // could fix it. One in CI, where a shared runner's hiccup is not a
  // regression, and the trace from that retry is what explains it.
  retries: CI ? 1 : 0,
  reporter: [['list'], ['html', { outputFolder: 'e2e-report', open: 'never' }]],
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
    // Navigation runs inside a View Transition, and the route fade uses the
    // Web Animations API. Under reduced motion the shell skips both
    // (AppShell.tsx, routes.tsx), so a measurement or an axe scan never lands
    // on a half-faded frame whose contrast and geometry are mid-animation.
    contextOptions: { reducedMotion: 'reduce' },
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    // `--strictPort`: if the port is taken, fail loudly rather than drift to
    // another and leave `url` pointing at whatever holds this one.
    command: `pnpm exec vite --port ${String(PORT)} --strictPort`,
    url: BASE_URL,
    reuseExistingServer: false,
    // A cold Vite with Tailwind on a loaded machine has taken over 30 s.
    timeout: 120_000,
  },
});
