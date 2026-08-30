import type { ThemeSettings } from './types';

/**
 * Resolves `mode: 'system'` against the OS preference.
 *
 * Separated from {@link applyTheme} so it can be tested without a DOM and so
 * the media-query listener and the initial apply share one code path.
 */
export function resolveMode(
  mode: ThemeSettings['mode'],
  systemPrefersDark: boolean,
): 'light' | 'dark' {
  if (mode === 'system') return systemPrefersDark ? 'dark' : 'light';
  return mode;
}

/** Row height in pixels for a density. */
export function rowHeight(density: ThemeSettings['density']): number {
  switch (density) {
    case 'compact':
      return 24;
    case 'comfortable':
      return 36;
    default:
      return 28;
  }
}

/**
 * Writes the theme onto the document root.
 *
 * Applied as attributes and classes rather than inline styles so the CSS in
 * `theme.css` remains the single source of every colour. Setting colours from
 * JavaScript would split the palette across two languages and guarantee they
 * drift.
 */
export function applyTheme(
  settings: ThemeSettings,
  root: HTMLElement,
  systemPrefersDark: boolean,
): void {
  const mode = resolveMode(settings.mode, systemPrefersDark);

  root.classList.toggle('dark', mode === 'dark');
  root.dataset['accent'] = settings.accent;
  root.dataset['surface'] = settings.surface;
  root.dataset['density'] = settings.density;
  root.style.setProperty('--row-height', `${rowHeight(settings.density)}px`);

  // Only set when the user has overridden; absent means "follow the OS", which
  // the `prefers-reduced-motion` media query in theme.css already handles.
  if (settings.reduceMotion === null) {
    delete root.dataset['reduceMotion'];
  } else {
    root.dataset['reduceMotion'] = String(settings.reduceMotion);
  }
}
