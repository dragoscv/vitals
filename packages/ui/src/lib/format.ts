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

/** Formats a 0..100 percentage. */
export function formatPercent(value: number | null, locale?: string, digits = 1): string {
  if (value === null || !Number.isFinite(value)) return '—';
  return (
    new Intl.NumberFormat(locale, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(value) + '%'
  );
}

/** Formats a frequency given in hertz, scaling to MHz or GHz. */
export function formatFrequency(hertz: number | null, locale?: string): string {
  if (hertz === null || !Number.isFinite(hertz) || hertz === 0) return '—';

  if (hertz >= 1e9) {
    return `${new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(hertz / 1e9)} GHz`;
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(hertz / 1e6)} MHz`;
}

/** Formats a temperature in Celsius. */
export function formatTemperature(celsius: number | null, locale?: string): string {
  if (celsius === null || !Number.isFinite(celsius)) return '—';
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(celsius)}°C`;
}

export function formatWatts(watts: number | null, locale?: string): string {
  if (watts === null || !Number.isFinite(watts)) return '—';
  const digits = Math.abs(watts) >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(watts)} W`;
}

/**
 * Formats a duration in seconds as an uptime string.
 *
 * Uses the largest two units that are non-zero — "3d 4h" rather than
 * "3d 4h 12m 7s". Precision below the second-largest unit is never what
 * someone reading an uptime wants.
 */
export function formatUptime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '—';

  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const secs = Math.floor(seconds % 60);

  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${secs}s`;
  return `${secs}s`;
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
