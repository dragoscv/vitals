/**
 * Resolves theme colours to concrete strings for the canvas renderer.
 *
 * # Why this is needed at all
 *
 * Canvas has no cascade. `ctx.strokeStyle = 'var(--color-accent)'` is
 * silently ignored — not an error, just a stroke that never appears — so a
 * chart drawn with a CSS custom property renders in the previous colour, or
 * black. The only way to get the theme onto a canvas is to read the computed
 * value and hand over the resulting string.
 *
 * Hardcoding hex instead would mean the accent picker and light/dark mode work
 * everywhere in the app except the charts, which are the largest coloured
 * surface on the dashboard.
 *
 * # Why it re-reads rather than caching once
 *
 * The values change without any React state changing: switching to dark mode
 * flips a `data-theme` attribute on `<html>`, and `mode: 'system'` can flip it
 * from an OS setting with no user interaction at all. A value read once at
 * mount would leave every chart painted in the old palette until the component
 * happened to remount.
 *
 * The subscription is a `MutationObserver` on the root element's attributes,
 * which fires on exactly the change that matters and costs nothing in between.
 */

import { useCallback, useEffect, useState } from 'react';

export interface ThemeColors {
  readonly accent: string;
  readonly warning: string;
  readonly danger: string;
  readonly muted: string;
  readonly grid: string;
}

/**
 * Used before the document is available, and as the value in tests.
 *
 * Concrete colours rather than empty strings: a canvas handed `''` throws
 * nothing and draws nothing, so a fallback that is technically wrong but
 * visible beats one that silently produces a blank chart.
 */
const FALLBACK: ThemeColors = {
  accent: '#22864a',
  warning: '#f59e0b',
  danger: '#ef4444',
  muted: '#64748b',
  grid: '#94a3b8',
};

// The real token names. These read `--color-accent-solid` and friends, which
// no stylesheet defines, so every chart drew the fallback green whatever the
// accent was set to (found 2026-10-06 adding hardware colours).
const VARIABLES: Readonly<Record<keyof ThemeColors, string>> = {
  accent: '--color-accent',
  warning: '--color-status-warn',
  danger: '--color-status-danger',
  muted: '--color-fg-muted',
  grid: '--color-border-subtle',
};

export function readThemeColors(element: Element | null): ThemeColors {
  if (element === null || typeof globalThis.getComputedStyle !== 'function') return FALLBACK;

  const style = globalThis.getComputedStyle(element);
  const read = (key: keyof ThemeColors): string => {
    const value = style.getPropertyValue(VARIABLES[key]).trim();
    return value === '' ? FALLBACK[key] : value;
  };

  return {
    accent: read('accent'),
    warning: read('warning'),
    danger: read('danger'),
    muted: read('muted'),
    grid: read('grid'),
  };
}

/**
 * Theme colours as resolved at `scope` — an element inside a
 * `[data-category]` subtree gets that category's accent — or at the root.
 */
export function useThemeColors(scope?: React.RefObject<Element | null>): ThemeColors {
  const [colors, setColors] = useState<ThemeColors>(() =>
    readThemeColors(globalThis.document?.documentElement ?? null),
  );

  const refresh = useCallback(() => {
    const next = readThemeColors(scope?.current ?? globalThis.document?.documentElement ?? null);
    // Compared field by field before setting: the observer fires for any
    // attribute change on <html>, including ones that do not affect colour,
    // and a new object identity each time would re-render every chart on the
    // dashboard for nothing.
    setColors((current) => (sameColors(current, next) ? current : next));
  }, [scope]);

  useEffect(() => {
    const root = globalThis.document?.documentElement;
    if (root === undefined) return;

    const observer = new MutationObserver(refresh);
    observer.observe(root, { attributes: true, attributeFilter: ['class', 'style', 'data-theme'] });

    // The computed values are not final on the first paint if a stylesheet is
    // still resolving, so one read after mount catches the settled palette.
    refresh();

    return () => {
      observer.disconnect();
    };
  }, [refresh]);

  return colors;
}

function sameColors(a: ThemeColors, b: ThemeColors): boolean {
  return (
    a.accent === b.accent &&
    a.warning === b.warning &&
    a.danger === b.danger &&
    a.muted === b.muted &&
    a.grid === b.grid
  );
}
