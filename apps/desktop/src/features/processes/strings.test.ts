import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

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

/**
 * Every user-facing string must exist in both locales, which an English
 * default defeats silently: i18next renders the default, the screen looks
 * finished in English, and Romanian users read English. This screen shipped
 * sixty-four strings that way, through a `fallback()` helper, while every
 * parity check stayed green because the keys were in neither locale.
 *
 * Scanned by text so the guard covers the whole desktop app, not just the
 * strings someone remembered to list. `defaultValue` with a variable stays
 * legal: it shows a raw value from the backend (a GPU engine name) when no
 * translation exists, which is data, not copy.
 */
const SRC_ROOT = resolve(import.meta.dirname, '..', '..');

const DEFAULT_STRING_PATTERNS: readonly { readonly name: string; readonly pattern: RegExp }[] = [
  {
    name: "t('key', 'English default')",
    pattern: /(?<![\w.])(?:t|tp|ts|i18n\.t)\(\s*(?:'[^']*'|"[^"]*"|`[^`]*`)\s*,\s*['"`]/,
  },
  { name: "defaultValue: 'English default'", pattern: /\bdefaultValue\s*:\s*['"`]/ },
  { name: "fallback('key') helper", pattern: /(?<![\w.])fallback\(\s*['"`]/ },
];

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) return sourceFiles(full);
    return /\.tsx?$/.test(entry) && !/\.(test|spec)\.tsx?$/.test(entry) ? [full] : [];
  });
}

function defaultStringViolations(source: string): string[] {
  return DEFAULT_STRING_PATTERNS.filter(({ pattern }) => pattern.test(source)).map(
    ({ name }) => name,
  );
}

describe('no English default strings', () => {
  it('recognises every form of inline English default it is meant to forbid', () => {
    // Without this the scan below could pass by matching nothing at all.
    expect(defaultStringViolations("t('process.columns', 'Columns')")).toHaveLength(1);
    expect(defaultStringViolations('tp(\n  "a.b",\n  "Text",\n)')).toHaveLength(1);
    expect(defaultStringViolations("t('x', { defaultValue: 'Text' })")).toHaveLength(1);
    expect(defaultStringViolations("label: fallback('process.filter.all')")).toHaveLength(1);

    expect(defaultStringViolations("t('process.results', { shown, total })")).toEqual([]);
    expect(defaultStringViolations('t(`gpu.engine.${n}`, { defaultValue: n })')).toEqual([]);
    expect(defaultStringViolations('this.props.fallback(error, retry)')).toEqual([]);
    expect(defaultStringViolations("toggleColumn('a', 'b')")).toEqual([]);
  });

  it('never ships a translation with an inline English default anywhere in the desktop app', () => {
    const offenders = sourceFiles(SRC_ROOT).flatMap((file) =>
      defaultStringViolations(readFileSync(file, 'utf8')).map(
        (name) => `${relative(SRC_ROOT, file)}: ${name}`,
      ),
    );

    expect(offenders, 'add the key to both locales instead').toEqual([]);
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
