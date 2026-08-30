import { beforeAll, describe, expect, it } from 'vitest';

import { defaultLocale, i18n, initI18n, isSupportedLocale, locales } from './config';
import en from './locales/en.json';
import ro from './locales/ro.json';

/** Every leaf key path in a nested resource object. */
function keyPaths(obj: unknown, prefix = ''): string[] {
  if (typeof obj !== 'object' || obj === null) return [prefix];
  return Object.entries(obj).flatMap(([k, v]) => keyPaths(v, prefix ? `${prefix}.${k}` : k));
}

/**
 * i18next plural suffixes vary by language, so a key present as `count_one` in
 * English and `count_few` in Romanian is correct, not missing. Comparing the
 * base key avoids false positives.
 */
function stripPluralSuffix(path: string): string {
  return path.replace(/_(zero|one|two|few|many|other)$/, '');
}

describe('locale resources', () => {
  it('ships every declared locale', () => {
    expect(locales).toContain(defaultLocale);
    expect(locales.length).toBeGreaterThan(1);
  });

  it('has no key present in English but missing in Romanian', () => {
    const enKeys = new Set(keyPaths(en).map(stripPluralSuffix));
    const roKeys = new Set(keyPaths(ro).map(stripPluralSuffix));

    const missing = [...enKeys].filter((k) => !roKeys.has(k));
    expect(missing, `missing Romanian translations: ${missing.join(', ')}`).toEqual([]);
  });

  it('has no orphaned Romanian key with no English counterpart', () => {
    // An orphan means a key was renamed in English and the old one left
    // behind — dead weight that looks like coverage.
    const enKeys = new Set(keyPaths(en).map(stripPluralSuffix));
    const roKeys = new Set(keyPaths(ro).map(stripPluralSuffix));

    const orphans = [...roKeys].filter((k) => !enKeys.has(k));
    expect(orphans, `orphaned Romanian keys: ${orphans.join(', ')}`).toEqual([]);
  });

  it('provides the three Romanian plural forms for counted strings', () => {
    // Romanian pluralisation is one / few / other:
    //   1 proces · 5 procese · 20 DE procese
    // Shipping only one/other would render "20 procese", which is wrong.
    const roKeys = keyPaths(ro);
    expect(roKeys).toContain('process.count_one');
    expect(roKeys).toContain('process.count_few');
    expect(roKeys).toContain('process.count_other');
  });

  it('keeps interpolation placeholders identical across locales', () => {
    // A translator dropping `{{name}}` produces a confirmation dialog that
    // does not say what it is about — dangerous on a "kill process" prompt.
    const placeholders = (value: unknown): string[] =>
      typeof value === 'string' ? [...value.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]!) : [];

    const walk = (a: unknown, b: unknown, path = ''): string[] => {
      if (typeof a === 'string') {
        const left = placeholders(a).sort();
        const right = placeholders(b).sort();
        return left.join() === right.join() ? [] : [path];
      }
      if (typeof a !== 'object' || a === null || typeof b !== 'object' || b === null) return [];
      return Object.entries(a).flatMap(([k, v]) =>
        walk(v, (b as Record<string, unknown>)[k], path ? `${path}.${k}` : k),
      );
    };

    const mismatched = walk(en, ro);
    expect(mismatched, `placeholder mismatch at: ${mismatched.join(', ')}`).toEqual([]);
  });
});

describe('isSupportedLocale', () => {
  it('accepts shipped locales and rejects others', () => {
    expect(isSupportedLocale('en')).toBe(true);
    expect(isSupportedLocale('ro')).toBe(true);
    expect(isSupportedLocale('klingon')).toBe(false);
  });
});

describe('initI18n', () => {
  beforeAll(async () => {
    await initI18n('ro');
  });

  it('initialises in the requested locale', () => {
    expect(i18n.language).toBe('ro');
  });

  it('resolves a nested key', () => {
    expect(i18n.t('nav.processes')).toBe('Procese');
  });

  it('applies Romanian plural rules', () => {
    expect(i18n.t('process.count', { count: 1 })).toBe('1 proces');
    expect(i18n.t('process.count', { count: 5 })).toBe('5 procese');
    expect(i18n.t('process.count', { count: 20 })).toBe('20 de procese');
  });

  it('is idempotent and can switch language', async () => {
    await initI18n('en');
    expect(i18n.language).toBe('en');
    expect(i18n.t('nav.processes')).toBe('Processes');
  });
});
