/**
 * Number formatting, matching the Android surfaces
 * (`apps/android/shared-ui/.../Format.kt`) so the TV, the phone and the watch
 * print the same reading the same way.
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

export function percent(v: Num): string {
  return missing(v) ? DASH : `${Math.round(v)} %`;
}

/** Without the space, for a ring where width is scarce. */
export function percentCompact(v: Num): string {
  return missing(v) ? DASH : `${Math.round(v)}%`;
}

export function celsius(v: Num): string {
  return missing(v) ? DASH : `${Math.round(v)} °C`;
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
      return celsius(value);
    case 'power':
      return watts(value, locale);
    case 'voltage':
      return volts(value, locale);
    case 'fanSpeed':
      return rpm(value);
    case 'charge':
    case 'percent':
      return percent(value);
    default:
      return missing(value) ? DASH : one(value, locale);
  }
}
