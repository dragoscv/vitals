import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { describe, expect, it } from 'vitest';

/**
 * A widget window is its own page, and a second, lighter one than the main
 * window: one panel, a handful of numbers, a toolbar. One import from the
 * shell, the router or another feature drags the whole desktop bundle — router,
 * charts, virtualised tables, the zustand store — into every widget, and there
 * can be a dozen widgets, each its own webview process, all created at startup.
 *
 * # Why the feature tree is allowed, and nothing else
 *
 * A widget and the settings screen share the renderer, the description layer
 * and the configuration model, and they must: those files ARE the feature. The
 * boundary is therefore drawn around `features/widgets`, not around the widget
 * window, and it excludes every other feature. `features/dashboard` is the
 * interesting case — it is where the readings come from, and importing it here
 * would still be a mistake, because a widget window subscribes to the frame
 * broadcast itself rather than to the sampler that feature owns.
 *
 * Scanned by text rather than by bundling: the bundler follows the import
 * happily and never complains, so the cost is invisible until the app is
 * installed on someone else's machine.
 */
const WIDGET_ROOT = resolve(import.meta.dirname);
/** The one feature tree a widget window may reach into. */
const SHARED_FEATURE = resolve(WIDGET_ROOT, '..', 'features', 'widgets');
const ALLOWED_PACKAGES =
  /^(@vitals\/(ui|i18n|protocol)|react|react-dom|react-i18next|lucide-react)(\/|$)/;
/**
 * Motion is on the list because the panel morphs between its compact and
 * expanded shapes, and the morph is the feature rather than decoration. The
 * HUD never animated, which is why its fence has no such entry.
 */
const ALLOWED_MOTION = /^motion\/(react-m|react)$/;
/** The widget window's own controls; nothing else from Tauri is permitted. */
const ALLOWED_TAURI = /^@tauri-apps\/api\/(window|event)$/;
const IMPORT = /from\s+['"]([^'"]+)['"]/g;
/**
 * The one module outside both trees the window may use: the log forwarder,
 * without which a window with no devtools reports nothing at all. It imports
 * only `@tauri-apps/api/core`, so it carries no desktop bundle with it.
 */
const ALLOWED_OUTSIDE = new Set([resolve(WIDGET_ROOT, '..', 'lib', 'logToFile')]);

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    return statSync(full).isDirectory() ? walk(full) : /\.(ts|tsx)$/.test(entry) ? [full] : [];
  });
}

function inside(target: string, root: string): boolean {
  return target === root || target.startsWith(root + sep);
}

/** Returns a reason when `specifier` imported from `file` leaves the widget tree. */
export function violation(file: string, specifier: string): string | undefined {
  if (specifier.startsWith('.')) {
    const target = resolve(dirname(file), specifier);
    if (ALLOWED_OUTSIDE.has(target)) return undefined;
    if (inside(target, WIDGET_ROOT)) return undefined;
    return inside(target, SHARED_FEATURE)
      ? undefined
      : `relative import escapes src/widgets and features/widgets: ${specifier}`;
  }
  if (specifier.startsWith('@/')) return `desktop alias import: ${specifier}`;
  if (specifier.startsWith('@tauri-apps/')) {
    return ALLOWED_TAURI.test(specifier) ? undefined : `tauri import: ${specifier}`;
  }
  if (specifier.endsWith('.css')) return undefined;
  if (ALLOWED_MOTION.test(specifier)) return undefined;
  return ALLOWED_PACKAGES.test(specifier)
    ? undefined
    : `package not on the allow list: ${specifier}`;
}

describe('widget window import boundary', () => {
  it('never imports the shell, another feature, settings or routes from src/widgets', () => {
    const offenders: string[] = [];
    for (const file of walk(WIDGET_ROOT)) {
      if (file.endsWith('.test.ts') || file.endsWith('.test.tsx')) continue;
      const source = readFileSync(file, 'utf8');
      for (const match of source.matchAll(IMPORT)) {
        const specifier = match[1];
        if (specifier === undefined) continue;
        const why = violation(file, specifier);
        if (why !== undefined) offenders.push(`${relative(WIDGET_ROOT, file)}: ${why}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('would refuse a shell, another feature, settings, routes, mobile or alias import', () => {
    const file = join(WIDGET_ROOT, 'lib', 'X.tsx');
    expect(violation(file, '../../shell/host')).toMatch(/escapes/);
    expect(violation(file, '../../features/dashboard/useSystemSnapshot')).toMatch(/escapes/);
    expect(violation(file, '../../settings/store')).toMatch(/escapes/);
    expect(violation(file, '../../routes')).toMatch(/escapes/);
    expect(violation(file, '../../mobile/components/Sparkline')).toMatch(/escapes/);
    expect(violation(file, '@/shell/AppShell')).toMatch(/alias/);
    expect(violation(file, '@tauri-apps/plugin-store')).toMatch(/tauri/);
    expect(violation(file, 'zustand')).toMatch(/allow list/);
    // Allowed, each for a stated reason.
    expect(violation(file, '../../features/widgets/model')).toBeUndefined();
    expect(violation(file, '../../features/widgets/WidgetPreview')).toBeUndefined();
    expect(violation(join(WIDGET_ROOT, 'main.tsx'), '../lib/logToFile')).toBeUndefined();
    expect(violation(file, 'motion/react')).toBeUndefined();
    expect(violation(file, 'motion/react-m')).toBeUndefined();
    expect(violation(file, '@tauri-apps/api/window')).toBeUndefined();
    expect(violation(file, '@vitals/ui')).toBeUndefined();
  });
});
