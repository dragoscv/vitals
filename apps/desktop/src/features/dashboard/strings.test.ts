import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { bundles, DASHBOARD_NS, registerDashboardStrings } from './strings';
import { ALERT_KINDS } from '../alerts/alertText';
import { widgetCatalogue } from './widgets';

/** Every leaf key path in an object, dot-joined. */
function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('dashboard translations', () => {
  it('defines the same keys in English and Romanian', () => {
    // Locales drift silently: a key added to one and forgotten in the other
    // shows the raw key path to the user, and only in the locale nobody on the
    // team reads. i18next plural suffixes are stripped because Romanian has a
    // `_few` form English does not.
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('has a title and description for every widget in the catalogue', () => {
    // The picker renders these. A widget added without strings appears in the
    // menu as its own key path.
    const paths = new Set(keyPaths(bundles.en));

    for (const widget of widgetCatalogue) {
      expect(paths, `missing ${widget.titleKey}`).toContain(widget.titleKey);
      expect(paths, `missing ${widget.descriptionKey}`).toContain(widget.descriptionKey);
    }
  });

  it('has a title for every alert kind the Rust engine can emit', () => {
    // The engine sends `alert.<kind>.title` as a key and never as prose, so a
    // kind added in Rust without strings here reaches the user as its own key
    // path — and reaches a toast as nothing at all, because Rust skips a kind
    // it has no title for rather than falling back to English.
    const paths = new Set(keyPaths(bundles.en));

    for (const kind of ALERT_KINDS) {
      expect(paths, `missing alert.${kind}.title`).toContain(`alert.${kind}.title`);
    }
  });
});

describe('registerDashboardStrings', () => {
  beforeEach(async () => {
    // `initI18n` must run before ANY bundle call: `removeResourceBundle` is as
    // absent as `addResourceBundle` on an uninitialised instance, so a cleanup
    // hook that skips this fails with the very TypeError under test.
    await initI18n();
    i18n.removeResourceBundle('en', DASHBOARD_NS);
    i18n.removeResourceBundle('ro', DASHBOARD_NS);
  });

  it('throws a descriptive error if called before init', () => {
    // The exact failure that once left the app on its splash forever: i18next
    // has no addResourceBundle until init() has run, and the resulting
    // TypeError at module scope aborts the graph before React mounts. The
    // guard turns that into a message naming the cause.
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerDashboardStrings();
      }).toThrow(/before initI18n/);
    } finally {
      if (original === undefined) {
        Object.defineProperty(i18n, 'isInitialized', { value: true, configurable: true });
      } else {
        Object.defineProperty(i18n, 'isInitialized', original);
      }
    }
  });

  it('registers both locales once i18n is ready', () => {
    registerDashboardStrings();

    expect(i18n.getResourceBundle('en', DASHBOARD_NS)).toBeDefined();
    expect(i18n.getResourceBundle('ro', DASHBOARD_NS)).toBeDefined();
    expect(i18n.t(`${DASHBOARD_NS}:widget.cpu.title`)).toBe('CPU');
  });

  it('does not overwrite keys that already exist', () => {
    // `overwrite: false` is what makes this module inert rather than harmful
    // once these strings move into @vitals/i18n proper.
    i18n.addResourceBundle('en', DASHBOARD_NS, { title: 'Real translation' }, true, true);
    registerDashboardStrings();

    expect(i18n.t(`${DASHBOARD_NS}:title`)).toBe('Real translation');
  });
});
