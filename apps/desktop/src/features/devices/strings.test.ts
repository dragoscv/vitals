import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { bundles, DEVICES_NS, registerDevicesStrings } from './strings';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('devices translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every thermal availability the backend can emit', () => {
    // Three of these four produce an empty zone list and only one is fixed by
    // elevating. A missing key would print a raw path in the one place the
    // screen most needs to be understood.
    const paths = new Set(keyPaths(bundles.en));

    for (const key of ['available', 'accessDenied', 'noZonesPresent', 'providerMissing']) {
      expect(paths, `missing availability.${key}`).toContain(`availability.${key}`);
    }
  });

  it('names every reason a capability can be unavailable', () => {
    const paths = new Set(keyPaths(bundles.en));

    for (const key of [
      'notSupportedOnPlatform',
      'noSuchHardware',
      'needsElevation',
      'needsHelper',
      'needsPlugin',
      'disabledByUser',
    ]) {
      expect(paths, `missing reason.${key}`).toContain(`reason.${key}`);
    }
  });

  it('names every capability the gap list can carry, including the fallback', () => {
    // `Capability` is `#[non_exhaustive]`, so `capability_key` maps anything
    // new to "other". Without a translation for it a future gap would print
    // "capability.other" to the user.
    const paths = new Set(keyPaths(bundles.en));

    for (const key of ['thermals', 'powerDraw', 'fanControl', 'other']) {
      expect(paths, `missing capability.${key}`).toContain(`capability.${key}`);
    }
  });

  it('names every sensor source, unit-bearing quality and charge state', () => {
    const paths = new Set(keyPaths(bundles.en));

    for (const key of ['acpiThermalZone', 'batteryMiniport', 'systemPowerStatus', 'kernelDriver']) {
      expect(paths, `missing source.${key}`).toContain(`source.${key}`);
    }
    for (const key of ['measured', 'derived', 'nameplate']) {
      expect(paths, `missing quality.${key}`).toContain(`quality.${key}`);
      expect(paths, `missing qualityHint.${key}`).toContain(`qualityHint.${key}`);
    }
    for (const key of ['charging', 'discharging', 'idle', 'unknown']) {
      expect(paths, `missing chargeState.${key}`).toContain(`chargeState.${key}`);
    }
  });

  it('names every line status and power mode', () => {
    const paths = new Set(keyPaths(bundles.en));

    for (const key of ['ac', 'battery', 'unknown']) {
      expect(paths, `missing line.${key}`).toContain(`line.${key}`);
    }
    for (const key of ['bestPowerEfficiency', 'balanced', 'bestPerformance', 'custom']) {
      expect(paths, `missing mode.${key}`).toContain(`mode.${key}`);
    }
  });
});

describe('registerDevicesStrings', () => {
  beforeEach(async () => {
    await initI18n();
    i18n.removeResourceBundle('en', DEVICES_NS);
    i18n.removeResourceBundle('ro', DEVICES_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerDevicesStrings();
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
    i18n.addResourceBundle('en', DEVICES_NS, { refresh: 'Real translation' }, true, true);
    registerDevicesStrings();

    expect(i18n.t(`${DEVICES_NS}:refresh`)).toBe('Real translation');
    expect(i18n.t(`${DEVICES_NS}:unavailable`)).toBe('Not available');
  });
});
