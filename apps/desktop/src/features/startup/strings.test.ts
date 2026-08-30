import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { bundles, registerStartupStrings, STARTUP_NS } from './strings';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('startup translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every startup source the backend can emit', () => {
    // The table indexes `source.<key>` with whatever Rust sent. These keys are
    // produced by an explicit mapping in `inventory.rs` precisely so this list
    // is a fixed contract rather than Rust variant names — but the two still
    // have to agree, and this is where that is checked.
    const sources = [
      'machineRun',
      'machineRun32',
      'machineRunOnce',
      'machineRunOnce32',
      'userRun',
      'userRunOnce',
      'commonStartupFolder',
      'userStartupFolder',
      'scheduledTask',
      'service',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const source of sources) {
      expect(paths, `missing source.${source}`).toContain(`source.${source}`);
    }
  });

  it('names every service state, including the unrecognised one', () => {
    // `ServiceState::Unknown(u32)` collapses to the key "unknown" on the Rust
    // side. Without an entry here the SCM returning an undocumented value
    // would print "serviceState.unknown" to the user.
    const states = [
      'running',
      'stopped',
      'startPending',
      'stopPending',
      'continuePending',
      'pausePending',
      'paused',
      'unknown',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const state of states) {
      expect(paths, `missing serviceState.${state}`).toContain(`serviceState.${state}`);
    }
  });

  it('names every start type and startup state', () => {
    const paths = new Set(keyPaths(bundles.en));

    for (const key of ['boot', 'system', 'automatic', 'manual', 'disabled', 'unknown']) {
      expect(paths, `missing startType.${key}`).toContain(`startType.${key}`);
    }
    for (const key of ['enabled', 'disabled', 'unknown']) {
      expect(paths, `missing state.${key}`).toContain(`state.${key}`);
    }
  });
});

describe('registerStartupStrings', () => {
  beforeEach(async () => {
    await initI18n();
    i18n.removeResourceBundle('en', STARTUP_NS);
    i18n.removeResourceBundle('ro', STARTUP_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerStartupStrings();
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
    i18n.addResourceBundle('en', STARTUP_NS, { refresh: 'Real translation' }, true, true);
    registerStartupStrings();

    expect(i18n.t(`${STARTUP_NS}:refresh`)).toBe('Real translation');
    expect(i18n.t(`${STARTUP_NS}:filter.all`)).toBe('All');
  });
});
