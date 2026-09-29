import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { describe, expect, it } from 'vitest';

/**
 * The overlay is its own page. One import from the shell, a feature, settings
 * or the router pulls the entire desktop bundle — router, charts, virtualised
 * tables, zustand store — into a 220×96 window that shows three numbers, and
 * the cost lands on every launch because the HUD is created at startup when
 * the user has left it on.
 *
 * `src/mobile` is fenced off in the same way, from the other direction: two
 * secondary entries that share internals drift into one coupled bundle, and
 * the phone must never download window-management code it cannot use.
 *
 * Scanned by text rather than by bundling, because the bundler would happily
 * follow the import and never complain.
 */
const HUD_ROOT = resolve(import.meta.dirname);
const ALLOWED_PACKAGES =
  /^(@vitals\/(ui|i18n|protocol)|react|react-dom|react-i18next|lucide-react)(\/|$)/;
/** The overlay's own window controls; nothing else from Tauri is permitted. */
const ALLOWED_TAURI = /^@tauri-apps\/api\/(window|event)$/;
const IMPORT = /from\s+['"]([^'"]+)['"]/g;
/**
 * The one module outside the tree the overlay may use: the log forwarder,
 * without which the overlay's errors go nowhere. It imports only
 * `@tauri-apps/api/core`, so it carries no desktop bundle with it.
 */
const ALLOWED_OUTSIDE = new Set([resolve(HUD_ROOT, '..', 'lib', 'logToFile')]);

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    return statSync(full).isDirectory() ? walk(full) : /\.(ts|tsx)$/.test(entry) ? [full] : [];
  });
}

/** Returns a reason when `specifier` imported from `file` leaves the hud tree. */
export function violation(file: string, specifier: string): string | undefined {
  if (specifier.startsWith('.')) {
    const target = resolve(dirname(file), specifier);
    if (ALLOWED_OUTSIDE.has(target)) return undefined;
    const inside = target === HUD_ROOT || target.startsWith(HUD_ROOT + sep);
    return inside ? undefined : `relative import escapes src/hud: ${specifier}`;
  }
  if (specifier.startsWith('@/')) return `desktop alias import: ${specifier}`;
  if (specifier.startsWith('@tauri-apps/')) {
    return ALLOWED_TAURI.test(specifier) ? undefined : `tauri import: ${specifier}`;
  }
  if (specifier.endsWith('.css')) return undefined;
  return ALLOWED_PACKAGES.test(specifier)
    ? undefined
    : `package not on the allow list: ${specifier}`;
}

describe('hud import boundary', () => {
  it('never imports the shell, features, settings, routes or mobile from src/hud', () => {
    const offenders: string[] = [];
    for (const file of walk(HUD_ROOT)) {
      if (file.endsWith('.test.ts') || file.endsWith('.test.tsx')) continue;
      const source = readFileSync(file, 'utf8');
      for (const match of source.matchAll(IMPORT)) {
        const specifier = match[1];
        if (specifier === undefined) continue;
        const why = violation(file, specifier);
        if (why !== undefined) offenders.push(`${relative(HUD_ROOT, file)}: ${why}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('would refuse a shell, feature, settings, routes, mobile or alias import', () => {
    const file = join(HUD_ROOT, 'components', 'X.tsx');
    expect(violation(file, '../../shell/host')).toMatch(/escapes/);
    expect(violation(file, '../../features/dashboard/useSystemSnapshot')).toMatch(/escapes/);
    expect(violation(file, '../../settings/store')).toMatch(/escapes/);
    expect(violation(file, '../../routes')).toMatch(/escapes/);
    expect(violation(file, '../../mobile/components/Sparkline')).toMatch(/escapes/);
    expect(violation(file, '@/shell/AppShell')).toMatch(/alias/);
    expect(violation(file, '@tauri-apps/plugin-store')).toMatch(/tauri/);
    expect(violation(file, 'zustand')).toMatch(/allow list/);
    expect(violation(file, '../lib/live')).toBeUndefined();
    expect(violation(join(HUD_ROOT, 'main.tsx'), '../lib/logToFile')).toBeUndefined();
    expect(violation(join(HUD_ROOT, 'main.tsx'), '../lib/metrics')).toMatch(/escapes/);
    expect(violation(file, '@tauri-apps/api/window')).toBeUndefined();
    expect(violation(file, '@vitals/ui')).toBeUndefined();
  });
});
