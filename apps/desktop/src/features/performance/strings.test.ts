import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { bundles, PERFORMANCE_NS, registerPerformanceStrings } from './strings';
import { resourceKinds } from './resources';

/** Every leaf key path in an object, dot-joined. */
function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('performance translations', () => {
  it('defines the same keys in English and Romanian', () => {
    // Drift is silent and lands in the locale nobody on the team reads: the
    // raw key path renders in place of the string.
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('has a title for every resource kind', () => {
    // The rail resolves `${kind}.title` for the singleton entries, so a kind
    // added without strings would render its own key path as a button label.
    const paths = new Set(keyPaths(bundles.en));
    for (const kind of resourceKinds) {
      expect(paths, `missing ${kind}.title`).toContain(`${kind}.title`);
    }
  });

  it('names every throttle reason the protocol can report', () => {
    // Panels index `throttle.<reason>` straight off the protocol value, so a
    // missing one surfaces as "throttle.voltageDrop" in the UI.
    const reasons = [
      'thermal',
      'powerLimit',
      'currentLimit',
      'voltageDrop',
      'powerPolicy',
      'unknown',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const reason of reasons) {
      expect(paths, `missing throttle.${reason}`).toContain(`throttle.${reason}`);
    }
  });

  it('names every GPU engine slug the backend emits', () => {
    // These come from `EngineKind::slug()` in vitals-win. The panel does
    // `t(`gpu.engine.${engine.name}`)`, so a missing one renders the raw
    // slug — "videoencode" instead of "Video encode", and untranslated in
    // Romanian. The panel falls back deliberately rather than showing
    // nothing, which makes the gap easy to miss without this test.
    const slugs = [
      '3d',
      'decode',
      'encode',
      'copy',
      'compute',
      'video-processing',
      'display',
      'other',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const slug of slugs) {
      expect(paths, `missing gpu.engine.${slug}`).toContain(`gpu.engine.${slug}`);
    }
  });

  it('names every disk and network kind', () => {
    // Same indexing hazard: `kind.<value>` comes straight from the protocol.
    const kinds = [
      'hdd',
      'ssd',
      'nvme',
      'removable',
      'network',
      'optical',
      'ethernet',
      'wiFi',
      'cellular',
      'bluetooth',
      'loopback',
      'virtual',
      'vpn',
      'unknown',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const kind of kinds) {
      expect(paths, `missing kind.${kind}`).toContain(`kind.${kind}`);
    }
  });
});

describe('registerPerformanceStrings', () => {
  beforeEach(async () => {
    // Before ANY bundle call: `removeResourceBundle` is as absent as
    // `addResourceBundle` on an uninitialised instance.
    await initI18n();
    i18n.removeResourceBundle('en', PERFORMANCE_NS);
    i18n.removeResourceBundle('ro', PERFORMANCE_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerPerformanceStrings();
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
    // `deep: true, overwrite: false`. With `deep: false` the merge is shallow
    // and the incoming bundle wins, which would silently shadow the real
    // translations when these keys move into @vitals/i18n.
    i18n.addResourceBundle('en', PERFORMANCE_NS, { title: 'Real translation' }, true, true);
    registerPerformanceStrings();

    expect(i18n.t(`${PERFORMANCE_NS}:title`)).toBe('Real translation');
    expect(i18n.t(`${PERFORMANCE_NS}:cpu.title`)).toBe('CPU');
  });
});
