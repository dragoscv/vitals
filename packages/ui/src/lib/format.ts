/**
 * Value formatters.
 *
 * Centralised because inconsistent number formatting is the most visible
 * quality tell in a monitoring tool: "1.5 GB" in one panel and "1536 MB" in
 * the next makes the whole product feel unfinished, and the two cannot be
 * compared at a glance.
 *
 * Every formatter is locale-aware via `Intl`, which also handles the decimal
 * separator — a Romanian user expects "1,5 GB", not "1.5 GB".
 */

const BINARY_UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'] as const;

/**
 * Formats a byte count using binary multiples (1 KB = 1024 B).
 *
 * Binary rather than decimal because that is what every OS reports and what
 * users cross-check against. Being technically correct with 1000-byte
 * kilobytes would make our numbers disagree with Explorer, and the user would
 * — reasonably — conclude that we are the ones that are wrong.
 */
export function formatBytes(bytes: number | null, locale?: string, precision?: number): string {
  if (bytes === null || !Number.isFinite(bytes)) return '—';
  if (bytes === 0) return '0 B';

  const negative = bytes < 0;
  const abs = Math.abs(bytes);

  const exponent = Math.min(Math.floor(Math.log(abs) / Math.log(1024)), BINARY_UNITS.length - 1);
  const value = abs / 1024 ** exponent;
  const unit = BINARY_UNITS[exponent] ?? 'B';

  // Fewer decimals as the number grows: "923 MB" and "1.4 GB" both read at a
  // glance, whereas "923.47 MB" is noise nobody acts on.
  const digits = precision ?? (exponent === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : 2);

  const formatted = new Intl.NumberFormat(locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);

  return `${negative ? '-' : ''}${formatted} ${unit}`;
}

/** Formats a throughput in bytes per second. */
export function formatThroughput(bytesPerSec: number | null, locale?: string): string {
  if (bytesPerSec === null || !Number.isFinite(bytesPerSec)) return '—';
  if (bytesPerSec === 0) return '0 B/s';
  return `${formatBytes(bytesPerSec, locale)}/s`;
}

/**
 * Formats a 0..100 percentage.
 *
 * Through `style: 'percent'` rather than a hand-appended "%", because the
 * sign's position and spacing are part of the locale: English writes
 * "43.2%", Romanian "43,2 %". Gluing "%" on produced "43,2%" for Romanian,
 * which is wrong there in the same way "43.2 %" would be in English.
 */
export function formatPercent(value: number | null, locale?: string, digits = 1): string {
  if (value === null || !Number.isFinite(value)) return '—';
  return new Intl.NumberFormat(locale, {
    style: 'percent',
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value / 100);
}

/** Formats a frequency given in hertz, scaling to MHz or GHz. */
export function formatFrequency(hertz: number | null, locale?: string): string {
  if (hertz === null || !Number.isFinite(hertz) || hertz === 0) return '—';

  if (hertz >= 1e9) {
    return `${new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(hertz / 1e9)} GHz`;
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(hertz / 1e6)} MHz`;
}

/**
 * Formats a temperature in Celsius.
 *
 * As an `Intl` unit so the spacing follows the locale ("73°C" in English,
 * "73 °C" in Romanian). Values that round to zero are written as zero:
 * "-0 °C" is a rounding artefact, not a reading.
 */
export function formatTemperature(celsius: number | null, locale?: string): string {
  if (celsius === null || !Number.isFinite(celsius)) return '—';
  const rounded = Math.round(celsius) === 0 ? 0 : celsius;
  return new Intl.NumberFormat(locale, {
    style: 'unit',
    unit: 'celsius',
    maximumFractionDigits: 0,
  }).format(rounded);
}

export function formatWatts(watts: number | null, locale?: string): string {
  if (watts === null || !Number.isFinite(watts)) return '—';
  const digits = Math.abs(watts) >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(watts)} W`;
}

/** The parts of a duration, as `Intl.DurationFormat` takes them. */
interface DurationParts {
  days?: number;
  hours?: number;
  minutes?: number;
  seconds?: number;
}

type DurationFormatter = { format(parts: DurationParts): string };
type DurationFormatCtor = new (
  locale?: string,
  options?: { style?: 'long' | 'short' | 'narrow'; secondsDisplay?: 'always' | 'auto' },
) => DurationFormatter;

/**
 * `Intl.DurationFormat`, where the engine has it.
 *
 * Looked up at call time and typed by hand: it is recent (WebView2 has it,
 * older engines and some test environments do not), and the TypeScript lib
 * this package targets may not declare it.
 */
function durationFormat(
  locale: string | undefined,
  alwaysSeconds: boolean,
): DurationFormatter | null {
  const ctor = (Intl as unknown as { DurationFormat?: DurationFormatCtor }).DurationFormat;
  if (ctor === undefined) return null;
  try {
    return new ctor(locale, {
      style: 'narrow',
      ...(alwaysSeconds && { secondsDisplay: 'always' as const }),
    });
  } catch {
    return null;
  }
}

/** Unit letters used only when `Intl.DurationFormat` is missing. */
const FALLBACK_UNIT: Readonly<Record<keyof DurationParts, string>> = {
  days: 'd',
  hours: 'h',
  minutes: 'm',
  seconds: 's',
};

/**
 * Formats the given parts in the locale's own duration style ("3d 4h" in
 * English, "3 z, 4 h" in Romanian), falling back to compact English letters
 * where the engine has no `Intl.DurationFormat`.
 */
export function formatDurationParts(parts: DurationParts, locale?: string): string {
  // An all-zero duration formats as "" by default (zero units are hidden);
  // asking for seconds explicitly gives "0s" / "0 s" instead.
  const allZero = Object.values(parts).every((v) => v === 0 || v === undefined);
  const formatter = durationFormat(locale, allZero);
  if (formatter !== null) {
    const text = formatter.format(allZero ? { seconds: 0 } : parts);
    if (text !== '') return text;
  }
  const entries = (Object.keys(FALLBACK_UNIT) as (keyof DurationParts)[])
    .filter((unit) => parts[unit] !== undefined)
    .map((unit) => `${parts[unit] ?? 0}${FALLBACK_UNIT[unit]}`);
  return entries.length > 0 ? entries.join(' ') : `0${FALLBACK_UNIT.seconds}`;
}

/**
 * Formats a duration in seconds as an uptime string.
 *
 * Uses the largest two units that are non-zero — "3d 4h" rather than
 * "3d 4h 12m 7s". Precision below the second-largest unit is never what
 * someone reading an uptime wants.
 *
 * Unit names come from the locale (Romanian says "z" for days, not "d"),
 * through `Intl.DurationFormat` where available.
 */
export function formatUptime(seconds: number, locale?: string): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '—';

  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const secs = Math.floor(seconds % 60);

  // A zero second unit is dropped ("3d", not "3d 0h"), except when it is
  // the only unit, so a just-started process reads "0s" rather than nothing.
  const pair = (big: DurationParts, small: DurationParts): DurationParts =>
    Object.values(small)[0] === 0 ? big : { ...big, ...small };
  if (days > 0) return formatDurationParts(pair({ days }, { hours }), locale);
  if (hours > 0) return formatDurationParts(pair({ hours }, { minutes }), locale);
  if (minutes > 0) return formatDurationParts(pair({ minutes }, { seconds: secs }), locale);
  return formatDurationParts({ seconds: secs }, locale);
}

/**
 * Formats a count with locale-aware grouping.
 *
 * Grouping matters more than it seems: "1048576" and "104857" are hard to
 * distinguish at a glance, "1,048,576" and "104,857" are not.
 */
export function formatCount(value: number, locale?: string): string {
  if (!Number.isFinite(value)) return '—';
  return new Intl.NumberFormat(locale).format(value);
}

/**
 * Formats a latency in milliseconds.
 *
 * Sub-millisecond values are shown in microseconds rather than rounded to
 * "0 ms", which would wrongly imply the operation was free.
 */
export function formatLatency(ms: number, locale?: string): string {
  if (!Number.isFinite(ms)) return '—';
  if (ms < 1) {
    return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(ms * 1000)} µs`;
  }
  const digits = ms >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(ms)} ms`;
}
