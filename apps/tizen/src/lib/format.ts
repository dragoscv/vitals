/**
 * Number formatting, matching the Android surfaces
 * (`apps/android/shared-ui/.../Format.kt`) so the TV, the phone and the watch
 * print the same reading the same way. Units that `Intl` knows go through it
 * (`percent`, `celsius`), so the sign's position and spacing follow the
 * locale — English `43%`, Romanian `43 %` with a no-break space — and agree
 * with the desktop.
 *
 * Every function takes a nullable and returns {@link DASH} for `null`, `undefined`
 * or `NaN`: an unmeasured reading is an em dash, never "0". Decimals use the
 * chosen locale's separator, so Romanian shows `12,5 GB`.
 */

export const DASH = '\u2014';

type Num = number | null | undefined;

function missing(v: Num): v is null | undefined {
  return v === null || v === undefined || Number.isNaN(v);
}

/** One decimal place in the locale's own notation. */
function one(v: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
    useGrouping: false,
  }).format(v);
}

/**
 * A 0–100 reading as a percentage, through `Intl`'s `percent` style rather
 * than gluing "%" on: the sign's position and spacing are the locale's, so
 * English writes `43%` and Romanian `43 %` (with a no-break space). Doing it
 * by hand produced `43 %` for English and `43%` for Romanian, each wrong in
 * the other's notation, and disagreed with the desktop, which uses Intl.
 */
export function percent(v: Num, locale: string): string {
  if (missing(v)) return DASH;
  return new Intl.NumberFormat(locale, {
    style: 'percent',
    maximumFractionDigits: 0,
  }).format(v / 100);
}

/**
 * Without the locale's spacing, for a ring where width is scarce — the
 * value sits inside a fixed circle, and Romanian's `43 %` would push it out.
 */
export function percentCompact(v: Num): string {
  return missing(v) ? DASH : `${Math.round(v)}%`;
}

/**
 * A whole-number count (processes, threads). Absent is {@link DASH}, never
 * `0`: a PC that could not count must not read as "nothing running".
 */
export function count(v: Num): string {
  return missing(v) ? DASH : `${v}`;
}

/**
 * A temperature as an `Intl` unit, so the spacing follows the locale
 * (`46°C` in English, `46 °C` in Romanian). A value that rounds to zero is
 * written as zero: "-0 °C" is a rounding artefact, not a reading.
 */
export function celsius(v: Num, locale: string): string {
  if (missing(v)) return DASH;
  const rounded = Math.round(v) === 0 ? 0 : v;
  return new Intl.NumberFormat(locale, {
    style: 'unit',
    unit: 'celsius',
    maximumFractionDigits: 0,
  }).format(rounded);
}

export function celsiusCompact(v: Num): string {
  return missing(v) ? DASH : `${Math.round(v)}°`;
}

export function watts(v: Num, locale: string): string {
  if (missing(v)) return DASH;
  return Math.abs(v) < 10 ? `${one(v, locale)} W` : `${Math.round(v)} W`;
}

export function rpm(v: Num): string {
  return missing(v) ? DASH : `${Math.round(v)} RPM`;
}

export function volts(v: Num, locale: string): string {
  if (missing(v)) return DASH;
  const text = new Intl.NumberFormat(locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    useGrouping: false,
  }).format(v);
  return `${text} V`;
}

export function ghz(hz: Num, locale: string): string {
  return missing(hz) ? DASH : `${one(hz / 1e9, locale)} GHz`;
}

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'] as const;

/** Binary units, as the desktop uses: 1 GB = 1024³ bytes. */
export function bytes(b: Num, locale: string): string {
  if (missing(b)) return DASH;
  let v = b;
  let i = 0;
  while (Math.abs(v) >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const unit = UNITS[i] ?? 'B';
  return i === 0 || Math.abs(v) >= 100 ? `${Math.round(v)} ${unit}` : `${one(v, locale)} ${unit}`;
}

export function rate(bps: Num, locale: string): string {
  return missing(bps) ? DASH : `${bytes(bps, locale)}/s`;
}

export function duration(secs: Num): string {
  if (missing(secs)) return DASH;
  const s = Math.max(0, Math.floor(secs));
  const d = Math.floor(s / 86_400);
  const h = Math.floor((s % 86_400) / 3_600);
  const m = Math.floor((s % 3_600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

/** A sensor line's value, by its unit key (`SensorLine.unit`). */
export function sensor(unit: string, value: number, locale: string): string {
  switch (unit) {
    case 'temperature':
      return celsius(value, locale);
    case 'power':
      return watts(value, locale);
    case 'voltage':
      return volts(value, locale);
    case 'fanSpeed':
      return rpm(value);
    case 'charge':
    case 'percent':
      return percent(value, locale);
    default:
      return missing(value) ? DASH : one(value, locale);
  }
}
