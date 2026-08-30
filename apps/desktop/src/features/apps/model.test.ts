import { describe, expect, it } from 'vitest';

import { canUninstall, declaredTotal, filterApps, sortApps, type InstalledApp } from './model';

function app(overrides: Partial<InstalledApp> = {}): InstalledApp {
  return {
    keyName: '{GUID}',
    name: 'Thing',
    publisher: 'Vendor',
    version: '1.0.0',
    installDate: '2024-03-07',
    installLocation: 'C:\\Program Files\\Thing',
    estimatedSize: 1024 * 1024,
    uninstallString: '"C:\\Program Files\\Thing\\unins000.exe"',
    quietUninstallString: null,
    isMsi: false,
    perUser: false,
    source: 'machineNative',
    ...overrides,
  };
}

describe('canUninstall', () => {
  it('is false without a published command', () => {
    // Store apps, MSI child products and entries left by a failed removal all
    // land here. The button is disabled rather than hidden, so the user can
    // see the option exists and that this product does not offer it.
    expect(canUninstall(app({ uninstallString: null }))).toBe(false);
  });

  it('is false for a blank command', () => {
    // A present-but-empty registry value is common enough that treating it as
    // usable would spawn `cmd /c ` and appear to do nothing.
    expect(canUninstall(app({ uninstallString: '   ' }))).toBe(false);
  });

  it('is true for a real command', () => {
    expect(canUninstall(app())).toBe(true);
  });
});

describe('sortApps', () => {
  it('orders by name', () => {
    const sorted = sortApps([app({ name: 'Zeta' }), app({ name: 'Alpha' })], 'name', 'en');
    expect(sorted.map((a) => a.name)).toEqual(['Alpha', 'Zeta']);
  });

  it('puts the largest first, not the smallest', () => {
    // "What is taking up space" is the question, so descending is the only
    // ordering that answers it.
    const sorted = sortApps(
      [app({ name: 'Small', estimatedSize: 100 }), app({ name: 'Big', estimatedSize: 9000 })],
      'size',
      'en',
    );
    expect(sorted.map((a) => a.name)).toEqual(['Big', 'Small']);
  });

  it('sorts a missing size last rather than treating it as zero', () => {
    // The central subtlety. Zero would rank a 10 GB product that omitted the
    // value below a 2 MB utility that supplied one, inverting the exact
    // ordering the user asked for.
    const sorted = sortApps(
      [app({ name: 'Unknown', estimatedSize: null }), app({ name: 'Tiny', estimatedSize: 1 })],
      'size',
      'en',
    );
    expect(sorted.map((a) => a.name)).toEqual(['Tiny', 'Unknown']);
  });

  it('orders by date newest first', () => {
    // Recently installed is what someone looks for when something started
    // misbehaving.
    const sorted = sortApps(
      [
        app({ name: 'Old', installDate: '2020-01-01' }),
        app({ name: 'New', installDate: '2025-06-01' }),
      ],
      'date',
      'en',
    );
    expect(sorted.map((a) => a.name)).toEqual(['New', 'Old']);
  });

  it('relies on ISO dates comparing correctly as strings', () => {
    // Which is why the Rust side normalises the registry's bare `YYYYMMDD`
    // rather than passing it through: unpadded components sort wrongly.
    const sorted = sortApps(
      [
        app({ name: 'March', installDate: '2024-03-07' }),
        app({ name: 'December', installDate: '2024-12-01' }),
      ],
      'date',
      'en',
    );
    expect(sorted.map((a) => a.name)).toEqual(['December', 'March']);
  });

  it('sorts a missing date last', () => {
    const sorted = sortApps(
      [app({ name: 'Undated', installDate: null }), app({ name: 'Dated' })],
      'date',
      'en',
    );
    expect(sorted.map((a) => a.name)).toEqual(['Dated', 'Undated']);
  });

  it('sorts a missing publisher last', () => {
    const sorted = sortApps(
      [app({ name: 'Anon', publisher: null }), app({ name: 'Known', publisher: 'Acme' })],
      'publisher',
      'en',
    );
    expect(sorted.map((a) => a.name)).toEqual(['Known', 'Anon']);
  });

  it('breaks every tie on name, so the order is stable', () => {
    // Otherwise the list reflects registry enumeration order, which can differ
    // between reads and makes the table appear to shuffle on refresh.
    const equal = [
      app({ name: 'Zeta', estimatedSize: 500 }),
      app({ name: 'Alpha', estimatedSize: 500 }),
    ];
    expect(sortApps(equal, 'size', 'en').map((a) => a.name)).toEqual(['Alpha', 'Zeta']);
  });

  it('does not mutate its input', () => {
    const input = [app({ name: 'B' }), app({ name: 'A' })];
    sortApps(input, 'name', 'en');
    expect(input.map((a) => a.name)).toEqual(['B', 'A']);
  });
});

describe('filterApps', () => {
  const apps = [
    app({ name: 'Steam', publisher: 'Valve', version: '3.1' }),
    app({
      name: 'Blender',
      publisher: 'Blender Foundation',
      installLocation: 'D:\\Tools\\Blender',
    }),
  ];

  it('returns everything for an empty query', () => {
    expect(filterApps(apps, '   ')).toHaveLength(2);
  });

  it('searches name, publisher, version and location', () => {
    expect(filterApps(apps, 'valve')).toHaveLength(1);
    expect(filterApps(apps, '3.1')).toHaveLength(1);
    expect(filterApps(apps, 'd:\\tools')).toHaveLength(1);
  });

  it('is case-insensitive', () => {
    expect(filterApps(apps, 'STEAM')).toHaveLength(1);
  });
});

describe('declaredTotal', () => {
  it('sums only what was declared, and says how many were silent', () => {
    // The figure is a floor, not a total. Presenting it as "space used by
    // applications" would be wrong by however much the silent ones occupy.
    const totals = declaredTotal([
      app({ estimatedSize: 100 }),
      app({ estimatedSize: 200 }),
      app({ estimatedSize: null }),
    ]);

    expect(totals).toEqual({ bytes: 300, withSize: 2, withoutSize: 1 });
  });

  it('handles an empty list', () => {
    expect(declaredTotal([])).toEqual({ bytes: 0, withSize: 0, withoutSize: 0 });
  });
});
