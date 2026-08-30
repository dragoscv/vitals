/**
 * Guards the initialisation order that shipped a broken app.
 *
 * `registerShellStrings()` was called at module scope in `App.tsx`. i18next
 * does not define `addResourceBundle` until `init()` has run, so the call
 * threw `i18n.addResourceBundle is not a function` while the module graph was
 * still evaluating — before React mounted, before any error boundary existed.
 * The window opened, the inline splash pulsed, and nothing ever replaced it.
 *
 * Every existing test passed because each one calls `await initI18n()` first
 * and then `registerShellStrings()`. Only the real application had the order
 * backwards, and nothing exercised the real application's boot path.
 */

import { describe, expect, it, vi } from 'vitest';

describe('shell string registration order', () => {
  it('fails loudly when called before i18n is initialised', async () => {
    // A fresh module registry, so i18next is genuinely uninitialised rather
    // than left over from another test file.
    vi.resetModules();

    const { registerShellStrings } = await import('./strings');

    expect(() => {
      registerShellStrings();
    }).toThrow(/before initI18n/i);
  });

  it('succeeds once i18n is initialised', async () => {
    vi.resetModules();

    const { initI18n, i18n } = await import('@vitals/i18n');
    const { registerShellStrings, SHELL_NS } = await import('./strings');

    await initI18n('en');
    registerShellStrings();

    expect(i18n.exists('window.close', { ns: SHELL_NS })).toBe(true);
  });

  it('registers both locales, so a Romanian UI is not half English', async () => {
    vi.resetModules();

    const { initI18n, i18n } = await import('@vitals/i18n');
    const { registerShellStrings, SHELL_NS } = await import('./strings');

    await initI18n('en');
    registerShellStrings();

    for (const locale of ['en', 'ro']) {
      expect(i18n.hasResourceBundle(locale, SHELL_NS), `${locale} has no ${SHELL_NS} bundle`).toBe(
        true,
      );
    }
  });

  it('defers to a translation that already exists', async () => {
    // These strings are a placeholder until they move into `@vitals/i18n`, and
    // the whole point of `overwrite: false` is that this module goes inert at
    // that moment rather than shadowing the real translations.
    //
    // It did not work. `deep: false` makes i18next replace the bundle
    // wholesale, so `overwrite: false` protected nothing — the placeholder
    // would have won. Verified by reverting to `false, false`: this fails.
    vi.resetModules();

    const { initI18n, i18n } = await import('@vitals/i18n');
    const { registerShellStrings, SHELL_NS } = await import('./strings');

    await initI18n('en');
    i18n.addResourceBundle('en', SHELL_NS, { window: { close: 'Real translation' } }, true, true);
    registerShellStrings();

    expect(i18n.t('window.close', { ns: SHELL_NS })).toBe('Real translation');
    // The keys it does not collide with are still registered.
    expect(i18n.exists('window.minimise', { ns: SHELL_NS })).toBe(true);
  });
});
