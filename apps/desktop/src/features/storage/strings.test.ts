import { beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { STORAGE_NS, bundles, registerStorageStrings } from './strings';
import {
  cleanupStages,
  directorySorts,
  namedScanStages,
  protections,
  recycleOutcomes,
  safetyOrder,
  windowsTools,
} from './model';

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
      'componentStore',
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

  it('names every recycle outcome and every protection rule the backend can send', () => {
    // The report indexes these with whatever Rust sent; a missing key would
    // print "protection.systemFolder" where the reason belongs.
    const paths = new Set(keyPaths(bundles.en));
    for (const outcome of recycleOutcomes) {
      expect(paths, `missing outcome.${outcome}`).toContain(`outcome.${outcome}`);
    }
    for (const why of ['wouldBePermanent', 'missing', 'accessDenied', 'failed']) {
      expect(paths, `missing outcomeWhy.${why}`).toContain(`outcomeWhy.${why}`);
    }
    for (const rule of protections) {
      expect(paths, `missing protection.${rule}`).toContain(`protection.${rule}`);
    }
  });

  it('never calls a recycle a deletion in the confirmation', () => {
    // The one confirmation must say where the items go; "delete" would
    // suggest they are gone for good, which is the thing this never does.
    expect(bundles.en.basket.confirm_other).toMatch(/Recycle/);
    expect(bundles.en.basket.confirmTitle).toMatch(/Recycle Bin/);
    expect(bundles.ro.basket.confirmTitle).toMatch(/Coșul de reciclare/);
    expect(bundles.en.basket.confirmBody).not.toMatch(/\bdelete/i);
  });

  it('names every Windows tool, stage and outcome the backend can send', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const tool of windowsTools) {
      for (const key of [
        `cleanup.free.${tool}`,
        `windows.tool.${tool}`,
        `windows.confirm.${tool}`,
      ]) {
        expect(paths, `missing ${key}`).toContain(key);
      }
    }
    for (const stage of cleanupStages) {
      expect(paths, `missing windows.stage.${stage}`).toContain(`windows.stage.${stage}`);
    }
    for (const outcome of ['done', 'needsRestart', 'toolFailed']) {
      expect(paths, `missing windows.outcome.${outcome}`).toContain(`windows.outcome.${outcome}`);
    }
  });

  it('states the consequence of every irreversible Windows cleanup in both languages', () => {
    // These three cannot be undone; the dialog shows this sentence above the
    // tick box, and a missing one would ask for consent to nothing.
    for (const bundle of [bundles.en, bundles.ro]) {
      for (const kind of ['recycleBin', 'previousWindows', 'hibernation'] as const) {
        expect(bundle.consequence[kind].length, kind).toBeGreaterThan(40);
      }
    }
    expect(bundles.en.consequence.hibernation).toMatch(/fast startup/);
    expect(bundles.en.consequence.previousWindows).toMatch(/no longer be able to go back/);
    expect(bundles.en.consequence.recycleBin).toMatch(/cannot be restored/);
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

  it('names every scan stage and every informative full-walk reason', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const stage of namedScanStages) {
      expect(paths, `missing scan.stage.${stage}`).toContain(`scan.stage.${stage}`);
    }
    for (const reason of [
      'journalChanged',
      'hardLinksChanged',
      'notSupported',
      'indexUnreadable',
    ]) {
      expect(paths, `missing result.fullReason.${reason}`).toContain(`result.fullReason.${reason}`);
    }
  });

  it('gives the three scan buttons different names in both languages', () => {
    for (const bundle of [bundles.en, bundles.ro]) {
      const names = [
        bundle.scan.start,
        bundle.scan.rescan,
        bundle.scan.fullRescan,
        bundle.scan.turbo,
      ];
      expect(new Set(names).size).toBe(names.length);
    }
  });

  it('explains Turbo without file-system jargon', () => {
    // The audience is someone whose computer is slow, not a file-system
    // engineer: "MFT", "USN" and "journal" mean nothing to them.
    for (const bundle of [bundles.en, bundles.ro]) {
      // Values only: the key `journalChanged` is code, not something shown.
      const values = (value: unknown): string[] =>
        typeof value === 'string'
          ? [value]
          : Object.values(value as Record<string, unknown>).flatMap(values);
      for (const text of values([bundle.scan, bundle.result])) {
        expect(text).not.toMatch(/\bMFT\b|\bUSN\b|journal|jurnal/i);
      }
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
