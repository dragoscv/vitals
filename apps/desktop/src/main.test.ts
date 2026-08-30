/**
 * Exercises the real boot path.
 *
 * This file exists because a startup bug shipped that every other test
 * missed: `main.tsx` and `App.tsx` were the only modules nothing imported, so
 * the one place where initialisation order actually mattered was the one
 * place never executed by the suite.
 *
 * The assertions are deliberately coarse — this is not testing what the app
 * renders, it is testing that the app *starts*.
 */

import type * as I18nModule from '@vitals/i18n';

import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
  // Reported false so the app takes its browser path: no Tauri host means no
  // store plugin and no window commands, which is exactly the surface this
  // test can exercise without a real webview.
  isTauri: vi.fn(() => false),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
    }),
  ),
}));

describe('application bootstrap', () => {
  beforeEach(() => {
    vi.resetModules();
    document.body.innerHTML =
      '<div id="splash"><div class="pulse"></div></div><div id="root"></div>';
  });

  it('mounts without throwing and dismisses the splash', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    await import('./main');

    // `bootstrap` is async and the splash is hidden inside a rAF, so give
    // both a chance to settle before asserting.
    await vi.waitFor(
      () => {
        expect(document.getElementById('root')?.childElementCount ?? 0).toBeGreaterThan(0);
      },
      { timeout: 3000 },
    );

    const splash = document.getElementById('splash');

    // THE regression. When registration ran too early the throw happened
    // during module evaluation: React never mounted and this attribute was
    // never set, leaving a pulsing splash over an empty page forever.
    await vi.waitFor(
      () => {
        expect(splash?.hasAttribute('hidden'), 'the splash was never dismissed').toBe(true);
      },
      { timeout: 3000 },
    );

    // A startup failure is reported by replacing the splash contents.
    expect(splash?.textContent).not.toMatch(/could not start/i);

    consoleError.mockRestore();
  });

  it('reports a failure visibly rather than leaving the splash up', async () => {
    // If i18n cannot initialise, the user must see why. An eternal splash
    // with the reason only in a console they cannot open is what made the
    // original bug so expensive to diagnose.
    vi.doMock('@vitals/i18n', async (importOriginal) => {
      const actual = await importOriginal<typeof I18nModule>();
      return {
        ...actual,
        initI18n: () => Promise.reject(new Error('locale files are corrupt')),
      };
    });

    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    await import('./main');

    await vi.waitFor(
      () => {
        expect(document.getElementById('splash')?.textContent).toMatch(/could not start/i);
      },
      { timeout: 3000 },
    );

    expect(document.getElementById('splash')?.textContent).toContain('locale files are corrupt');
    expect(consoleError).toHaveBeenCalled();

    consoleError.mockRestore();
    vi.doUnmock('@vitals/i18n');
  });
});
