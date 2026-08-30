import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { APPS_NS, bundles, registerAppsStrings } from './strings';
import { appSorts } from './model';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('apps translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every sort option', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const sort of appSorts) {
      expect(paths, `missing sort.${sort}`).toContain(`sort.${sort}`);
    }
  });

  it('names every rejection reason the backend can emit', () => {
    // The summary indexes `reject.<key>` with whatever Rust sent. A missing
    // entry prints "reject.orphanPatch" into the sentence explaining what was
    // filtered out — which is precisely the sentence meant to build trust in
    // the filter.
    const reasons = [
      'noDisplayName',
      'systemComponent',
      'updateOrHotfix',
      'childOfAnotherEntry',
      'orphanPatch',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const reason of reasons) {
      expect(paths, `missing reject.${reason}`).toContain(`reject.${reason}`);
    }
  });

  it('names every registry view', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const source of ['machineNative', 'machineWow64', 'userNative', 'userWow64']) {
      expect(paths, `missing source.${source}`).toContain(`source.${source}`);
    }
  });

  it('keeps the dialog dismiss label distinct from Cancel', () => {
    // Two controls in one dialog sharing an accessible name is ambiguous for a
    // screen reader. Asserted on the strings so a future translation edit
    // cannot quietly reintroduce the collision.
    expect(bundles.en.uninstall.close).not.toBe(bundles.en.uninstall.cancel);
    expect(bundles.ro.uninstall.close).not.toBe(bundles.ro.uninstall.cancel);
  });
});

describe('registerAppsStrings', () => {
  beforeEach(async () => {
    await initI18n();
    i18n.removeResourceBundle('en', APPS_NS);
    i18n.removeResourceBundle('ro', APPS_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerAppsStrings();
      }).toThrow(/before initI18n/);
    } finally {
      if (original === undefined) {
        Object.defineProperty(i18n, 'isInitialized', { value: true, configurable: true });
      } else {
        Object.defineProperty(i18n, 'isInitialized', original);
      }
    }
  });

  it('defers to a translation that already exists', () => {
    i18n.addResourceBundle('en', APPS_NS, { refresh: 'Real translation' }, true, true);
    registerAppsStrings();

    expect(i18n.t(`${APPS_NS}:refresh`)).toBe('Real translation');
    expect(i18n.t(`${APPS_NS}:sort.name`)).toBe('Name');
  });
});
