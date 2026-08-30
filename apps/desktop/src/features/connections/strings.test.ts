import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { bundles, CONNECTIONS_NS, registerConnectionStrings } from './strings';
import { connectionFilters } from './model';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('connection translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every filter', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const filter of connectionFilters) {
      expect(paths, `missing filter.${filter}`).toContain(`filter.${filter}`);
    }
  });

  it('names every TCP state the protocol can report', () => {
    // The table indexes `state.<value>` straight off the wire type, so a
    // missing entry renders as "state.finWait2" in front of the user.
    const states = [
      'established',
      'listen',
      'synSent',
      'synReceived',
      'finWait1',
      'finWait2',
      'timeWait',
      'closed',
      'closeWait',
      'lastAck',
      'closing',
      'deleteTcb',
      'stateless',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const state of states) {
      expect(paths, `missing state.${state}`).toContain(`state.${state}`);
    }
  });
});

describe('registerConnectionStrings', () => {
  beforeEach(async () => {
    await initI18n();
    i18n.removeResourceBundle('en', CONNECTIONS_NS);
    i18n.removeResourceBundle('ro', CONNECTIONS_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerConnectionStrings();
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
    i18n.addResourceBundle('en', CONNECTIONS_NS, { title: 'Real translation' }, true, true);
    registerConnectionStrings();

    expect(i18n.t(`${CONNECTIONS_NS}:title`)).toBe('Real translation');
    expect(i18n.t(`${CONNECTIONS_NS}:filter.all`)).toBe('All');
  });
});
