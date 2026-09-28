import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { STORAGE_NS, bundles, registerStorageStrings } from './strings';
import { directorySorts, safetyOrder } from './model';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('storage translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every sort option', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const sort of directorySorts) {
      expect(paths, `missing sort.${sort}`).toContain(`sort.${sort}`);
    }
  });

  it('names every safety tier, with its explanation', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const safety of safetyOrder) {
      expect(paths, `missing safety.${safety}`).toContain(`safety.${safety}`);
      expect(paths, `missing safety.${safety}Body`).toContain(`safety.${safety}Body`);
    }
  });

  it('names every cleanup kind the backend can emit, with a reason', () => {
    // The rows index `kindLabel.<key>` and `kindReason.<key>` with whatever
    // Rust sent. A missing entry prints "kindReason.hibernation" into the
    // sentence explaining why a 34 GB file cannot simply be deleted.
    const kinds = [
      'userTemp',
      'systemTemp',
      'browserCache',
      'windowsUpdateCache',
      'recycleBin',
      'crashDump',
      'previousWindows',
      'hibernation',
      'packageManagerCache',
      'thumbnailCache',
      'deliveryOptimisation',
    ];
    const paths = new Set(keyPaths(bundles.en));

    for (const kind of kinds) {
      expect(paths, `missing kindLabel.${kind}`).toContain(`kindLabel.${kind}`);
      expect(paths, `missing kindReason.${kind}`).toContain(`kindReason.${kind}`);
    }
  });

  it('names every skip reason the scanner can emit', () => {
    const reasons = ['accessDenied', 'reparsePoint', 'cancelled', 'vanished', 'osError'];
    const paths = new Set(keyPaths(bundles.en));

    for (const reason of reasons) {
      expect(paths, `missing skip.${reason}`).toContain(`skip.${reason}`);
    }
  });

  it('names every disk kind', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const kind of ['hdd', 'ssd', 'nvme', 'removable', 'network', 'optical', 'unknown']) {
      expect(paths, `missing kind.${kind}`).toContain(`kind.${kind}`);
    }
  });

  it('distinguishes an exact total from a floor', () => {
    // The two are shown in different circumstances and must not read the
    // same, or an unmeasured-and-therefore-understated figure looks complete.
    expect(bundles.en.cleanup.total).not.toBe(bundles.en.cleanup.totalFloor);
    expect(bundles.ro.cleanup.total).not.toBe(bundles.ro.cleanup.totalFloor);
  });

  it('keeps the two refresh buttons distinctly named', () => {
    // One re-reads the drive list, the other re-walks the tree. Sharing a
    // name makes a screen reader announce "Scan again, button. Scan again,
    // button." with no way to tell which does what. Caught by getByRole
    // refusing the ambiguous match, and asserted here so a future
    // translation edit cannot quietly reintroduce it.
    for (const bundle of [bundles.en, bundles.ro]) {
      expect(bundle.volumes.refresh).not.toBe(bundle.scan.rescan);
      expect(bundle.volumes.refresh).not.toBe(bundle.scan.start);
    }
  });

  it('explains that deletion is unavailable rather than implying it works', () => {
    for (const bundle of [bundles.en, bundles.ro]) {
      expect(bundle.cleanup.deleteUnavailable.length).toBeGreaterThan(20);
      expect(bundle.cleanup.deleteUnavailable).not.toBe(bundle.cleanup.delete);
    }
  });
});

describe('registerStorageStrings', () => {
  beforeEach(async () => {
    await initI18n();
    i18n.removeResourceBundle('en', STORAGE_NS);
    i18n.removeResourceBundle('ro', STORAGE_NS);
  });

  it('throws a descriptive error if called before init', () => {
    const original = Object.getOwnPropertyDescriptor(i18n, 'isInitialized');

    try {
      Object.defineProperty(i18n, 'isInitialized', { value: false, configurable: true });
      expect(() => {
        registerStorageStrings();
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
    i18n.addResourceBundle('en', STORAGE_NS, { title: 'Real translation' }, true, true);
    registerStorageStrings();

    expect(i18n.t(`${STORAGE_NS}:title`)).toBe('Real translation');
    expect(i18n.t(`${STORAGE_NS}:sort.allocated`)).toBe('On disk');
  });
});
