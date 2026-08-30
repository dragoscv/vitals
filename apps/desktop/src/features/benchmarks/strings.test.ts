import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { BENCHMARKS_NS, bundles, registerBenchmarksStrings } from './strings';
import { benchmarkGroups, benchmarkIds } from './model';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('benchmark translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names and describes every benchmark the backend can list', () => {
    // The selection list indexes `name.<id>` with whatever Rust sent. A gap
    // prints "name.diskRandomWrite" into a checkbox label.
    const paths = new Set(keyPaths(bundles.en));
    for (const id of benchmarkIds) {
      expect(paths, `missing name.${id}`).toContain(`name.${id}`);
      expect(paths, `missing describe.${id}`).toContain(`describe.${id}`);
    }
  });

  it('names every group heading', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const group of benchmarkGroups) {
      expect(paths, `missing group.${group}`).toContain(`group.${group}`);
    }
  });

  it('explains every unavailability reason the backend sends today', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const reason of ['needsConsent', 'needsGraphicsContext', 'notImplemented', 'unknown']) {
      expect(paths, `missing unavailable.${reason}`).toContain(`unavailable.${reason}`);
    }
  });

  it('explains every distrust reason', () => {
    // A result flagged untrustworthy must say why. Falling back to a key path
    // here would defeat the entire point of the screen.
    const paths = new Set(keyPaths(bundles.en));
    for (const reason of ['throttled', 'onBattery', 'backgroundLoad', 'variability', 'tainted']) {
      expect(paths, `missing trust.reason.${reason}`).toContain(`trust.reason.${reason}`);
    }
  });

  it('carries all three Romanian plural forms on counted strings', () => {
    // Romanian pluralises at 1, 2..19 and 20+ (the last taking "de"). Two
    // forms is the standard machine-translation tell, and it reads wrong to
    // any native speaker at exactly the counts a benchmark screen produces.
    expect(bundles.ro.warning).toHaveProperty('estimate_one');
    expect(bundles.ro.warning).toHaveProperty('estimate_few');
    expect(bundles.ro.warning).toHaveProperty('estimate_other');
    expect(bundles.ro.running).toHaveProperty('remaining_few');
    expect(bundles.ro.results).toHaveProperty('headline_few');
  });

  it('states the consequence of starting a run, not just its duration', () => {
    // The machine becomes unusable. Saying only "about 40 seconds" invites
    // the user to keep working through it and then blame us for the result.
    expect(bundles.en.warning.body).toMatch(/do not use the computer/i);
    expect(bundles.ro.warning.body).toMatch(/nu îl folosi/i);
  });
});

describe('registerBenchmarksStrings', () => {
  beforeEach(async () => {
    await initI18n();
    i18n.removeResourceBundle('en', BENCHMARKS_NS);
    i18n.removeResourceBundle('ro', BENCHMARKS_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerBenchmarksStrings();
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
    i18n.addResourceBundle('en', BENCHMARKS_NS, { refresh: 'Real translation' }, true, true);
    registerBenchmarksStrings();

    expect(i18n.t(`${BENCHMARKS_NS}:refresh`)).toBe('Real translation');
    expect(i18n.t(`${BENCHMARKS_NS}:group.cpu`)).toBe('Processor');
  });
});
