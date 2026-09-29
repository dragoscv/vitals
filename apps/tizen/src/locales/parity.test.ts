/**
 * `scripts/check-drift.ps1` checks locale parity for `packages/i18n` and the
 * desktop, not for this app, so the same rule is enforced here.
 */

import { describe, expect, it } from 'vitest';

import en from './en.json';
import ro from './ro.json';

function keys(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keys(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

/** CLDR plural forms that one language needs and the other does not. */
const PLURAL_ONLY = /_(zero|two|few|many)$/;

function leaves(value: unknown): string[] {
  if (typeof value === 'string') return [value];
  if (typeof value !== 'object' || value === null) return [];
  return Object.values(value as Record<string, unknown>).flatMap(leaves);
}

describe('locales', () => {
  it('have the same keys in English and Romanian, apart from plural forms only one language uses', () => {
    const a = new Set(keys(en).filter((k) => !PLURAL_ONLY.test(k)));
    const b = new Set(keys(ro).filter((k) => !PLURAL_ONLY.test(k)));
    expect([...a].filter((k) => !b.has(k))).toEqual([]);
    expect([...b].filter((k) => !a.has(k))).toEqual([]);
  });

  it('use the same interpolation variables in both languages for every key', () => {
    const vars = (s: string) => [...s.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();
    const flat = (o: unknown, p = ''): [string, string][] =>
      typeof o === 'string'
        ? [[p, o]]
        : Object.entries(o as Record<string, unknown>).flatMap(([k, v]) =>
            flat(v, p === '' ? k : `${p}.${k}`),
          );
    const roMap = new Map(flat(ro));
    for (const [key, text] of flat(en)) {
      const other = roMap.get(key);
      if (other === undefined) continue;
      expect(vars(other), key).toEqual(vars(text));
    }
  });

  it('have no empty strings, so a missing translation cannot render as a blank', () => {
    expect([...leaves(en), ...leaves(ro)].filter((s) => s.trim() === '')).toEqual([]);
  });
});
