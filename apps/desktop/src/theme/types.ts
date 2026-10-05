/** The independent appearance axes. See `packages/ui/src/styles/theme.css`. */

export const themeModes = ['light', 'dark', 'system'] as const;
export type ThemeMode = (typeof themeModes)[number];

export const accents = [
  'blue',
  'violet',
  'pink',
  'red',
  'orange',
  'amber',
  'green',
  'teal',
  'cyan',
  'graphite',
] as const;
export type Accent = (typeof accents)[number];

/**
 * Row height and spacing scale.
 *
 * A process table is read by scanning, and the right density depends on the
 * display: 28px rows are comfortable on a laptop and waste half an ultrawide.
 */
export const densities = ['compact', 'default', 'comfortable'] as const;
export type Density = (typeof densities)[number];

export interface ThemeSettings {
  readonly mode: ThemeMode;
  readonly accent: Accent;
  readonly density: Density;
  /**
   * Overrides the OS `prefers-reduced-motion` setting.
   *
   * `null` means follow the system, which is the default and the correct
   * behaviour. An explicit value exists because some users want motion
   * reduced in this app specifically — a dense, constantly-updating UI is
   * exactly where animation becomes fatiguing — without changing it globally.
   */
  readonly reduceMotion: boolean | null;
}

export const defaultTheme: ThemeSettings = {
  mode: 'system',
  // Fern is the brand colour (brand/mark.mjs); the other nine stay a choice.
  accent: 'green',
  density: 'default',
  reduceMotion: null,
};

export function isAccent(value: string): value is Accent {
  return (accents as readonly string[]).includes(value);
}

export function isThemeMode(value: string): value is ThemeMode {
  return (themeModes as readonly string[]).includes(value);
}

export function isDensity(value: string): value is Density {
  return (densities as readonly string[]).includes(value);
}
