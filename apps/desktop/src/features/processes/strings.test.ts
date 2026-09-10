import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { PROCESSES_NS, bundles, registerProcessesStrings } from './strings';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('processes translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every affinity preset the UI can build', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const id of ['all', 'performance', 'efficiency', 'firstHalf', 'secondHalf']) {
      expect(paths, `missing detail.affinity.preset.${id}`).toContain(
        `detail.affinity.preset.${id}`,
      );
    }
  });

  it('names both disk counter sources the backend can report', () => {
    // The tooltip indexes `disk.<source>` with whatever Rust sent. A missing
    // entry prints "disk.storageStack" into the tooltip meant to explain the
    // number the user is squinting at.
    const paths = new Set(keyPaths(bundles.en));
    for (const source of ['storageStack', 'allIo', 'unknown']) {
      expect(paths, `missing disk.${source}`).toContain(`disk.${source}`);
    }
  });

  it('keeps the unavailable label distinct from Off in both languages', () => {
    // These are the two states the switch must never conflate.
    expect(bundles.en.detail.efficiency.unavailable).not.toBe(bundles.en.detail.efficiency.off);
    expect(bundles.ro.detail.efficiency.unavailable).not.toBe(bundles.ro.detail.efficiency.off);
  });
});

describe('registerProcessesStrings', () => {
  beforeEach(async () => {
    await initI18n();
    i18n.removeResourceBundle('en', PROCESSES_NS);
    i18n.removeResourceBundle('ro', PROCESSES_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerProcessesStrings();
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
    i18n.addResourceBundle(
      'en',
      PROCESSES_NS,
      { detail: { handles: { title: 'Real translation' } } },
      true,
      true,
    );
    registerProcessesStrings();

    expect(i18n.t(`${PROCESSES_NS}:detail.handles.title`)).toBe('Real translation');
    expect(i18n.t(`${PROCESSES_NS}:detail.modules.title`)).toBe('Modules');
  });
});
