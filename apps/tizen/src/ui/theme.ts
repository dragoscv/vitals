/**
 * The desktop's categorical palette in sRGB, identical to
 * `apps/android/shared-ui/.../Palette.kt`: a metric has one colour on every
 * surface. These are for rings, bars and lines, where 3:1 against the
 * background is the requirement; text uses the `text*` tones in `styles.css`,
 * which clear 4.5:1.
 */

export const Palette = {
  cpu: '#0088F2',
  memory: '#00B667',
  disk: '#CB9400',
  network: '#AC67EF',
  gpu: '#F05653',
  thermal: '#F6722B',
  power: '#00C0C2',
  ok: '#1EAB53',
  warn: '#E89D00',
  danger: '#EE343B',
  accent: '#52A9FE',
  unknown: '#8A8F98',
} as const;

/** How worried to look. `unknown` is an unmeasured value, never "fine". */
export type Level = 'ok' | 'warn' | 'danger' | 'unknown';

function band(v: number | null | undefined, warn: number, danger: number): Level {
  if (v === null || v === undefined || Number.isNaN(v)) return 'unknown';
  if (v >= danger) return 'danger';
  if (v >= warn) return 'warn';
  return 'ok';
}

/** Matching the desktop's alert engine, so the TV does not turn red while the PC says all is well. */
export const Thresholds = {
  cpu: (p: number | null | undefined) => band(p, 85, 95),
  memory: (p: number | null | undefined) => band(p, 85, 95),
  cpuTemp: (c: number | null | undefined) => band(c, 85, 95),
  gpuTemp: (c: number | null | undefined) => band(c, 80, 90),
};

export function levelColour(level: Level, fallback: string): string {
  switch (level) {
    case 'warn':
      return Palette.warn;
    case 'danger':
      return Palette.danger;
    default:
      return fallback;
  }
}
