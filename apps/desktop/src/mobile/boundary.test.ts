import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { describe, expect, it } from 'vitest';

/**
 * The phone downloads `src/mobile/**` and nothing else. One import from the
 * shell or a feature pulls the whole desktop bundle onto a LAN page, and
 * `@tauri-apps/*` throws at runtime without a host. Scanned by text because
 * the bundler would happily follow the import and never complain.
 */
const MOBILE_ROOT = resolve(import.meta.dirname);
const ALLOWED_PACKAGES =
  /^(@vitals\/(ui|charts|i18n|protocol|client)|react|react-dom|react-i18next|lucide-react|@tanstack\/react-virtual)(\/|$)/;
const IMPORT = /from\s+['"]([^'"]+)['"]/g;

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    return statSync(full).isDirectory() ? walk(full) : /\.(ts|tsx)$/.test(entry) ? [full] : [];
  });
}

/** Returns a reason when `specifier` imported from `file` leaves the mobile tree. */
export function violation(file: string, specifier: string): string | undefined {
  if (specifier.startsWith('.')) {
    const target = resolve(dirname(file), specifier);
    const inside = target === MOBILE_ROOT || target.startsWith(MOBILE_ROOT + sep);
    return inside ? undefined : `relative import escapes src/mobile: ${specifier}`;
  }
  if (specifier.startsWith('@/')) return `desktop alias import: ${specifier}`;
  if (specifier.startsWith('@tauri-apps/')) return `tauri import: ${specifier}`;
  if (specifier.endsWith('.css')) return undefined;
  return ALLOWED_PACKAGES.test(specifier)
    ? undefined
    : `package not on the allow list: ${specifier}`;
}

describe('mobile import boundary', () => {
  it('never imports the desktop shell, features, settings, lib or tauri from src/mobile', () => {
    const offenders: string[] = [];
    for (const file of walk(MOBILE_ROOT)) {
      if (file.endsWith('.test.ts') || file.endsWith('.test.tsx')) continue;
      const source = readFileSync(file, 'utf8');
      for (const match of source.matchAll(IMPORT)) {
        const specifier = match[1];
        if (specifier === undefined) continue;
        const why = violation(file, specifier);
        if (why !== undefined) offenders.push(`${relative(MOBILE_ROOT, file)}: ${why}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('would refuse a shell, alias or tauri import if one were added', () => {
    const file = join(MOBILE_ROOT, 'components', 'X.tsx');
    expect(violation(file, '../../shell/host')).toMatch(/escapes/);
    expect(violation(file, '../../lib/metrics')).toMatch(/escapes/);
    expect(violation(file, '@/features/processes')).toMatch(/alias/);
    expect(violation(file, '@tauri-apps/api/core')).toMatch(/tauri/);
    expect(violation(file, 'zustand')).toMatch(/allow list/);
    expect(violation(file, '../lib/live')).toBeUndefined();
    expect(violation(file, '@vitals/client')).toBeUndefined();
  });
});
