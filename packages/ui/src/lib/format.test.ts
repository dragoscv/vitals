import { describe, expect, it } from 'vitest';
import {
  formatBytes,
  formatCount,
  formatFrequency,
  formatLatency,
  formatPercent,
  formatTemperature,
  formatThroughput,
  formatUptime,
  formatWatts,
} from './format';

// Pinned to en-US so assertions do not depend on the CI machine's locale.
const L = 'en-US';

describe('formatBytes', () => {
  it('uses binary multiples so numbers match Explorer', () => {
    expect(formatBytes(1024, L)).toBe('1.00 KB');
    expect(formatBytes(1_048_576, L)).toBe('1.00 MB');
  });

  it('shows fewer decimals as magnitude grows', () => {
    expect(formatBytes(1536, L)).toBe('1.50 KB');
    expect(formatBytes(15_728_640, L)).toBe('15.0 MB');
    expect(formatBytes(157_286_400, L)).toBe('150 MB');
  });

  it('renders whole bytes without decimals', () => {
    expect(formatBytes(512, L)).toBe('512 B');
    expect(formatBytes(0, L)).toBe('0 B');
  });

  it('handles negative deltas', () => {
    expect(formatBytes(-2048, L)).toBe('-2.00 KB');
  });

  it('renders a non-finite value as unknown rather than NaN', () => {
    // "NaN B" in a UI is a bug report; "—" is an honest absence.
    expect(formatBytes(Number.NaN, L)).toBe('—');
    expect(formatBytes(Infinity, L)).toBe('—');
  });

  it('caps at the largest known unit instead of overflowing the table', () => {
    expect(formatBytes(1024 ** 6, L)).toContain('PB');
  });

  it('respects locale decimal separators', () => {
    expect(formatBytes(1536, 'ro-RO')).toBe('1,50 KB');
  });
});

describe('formatThroughput', () => {
  it('suffixes a rate', () => {
    expect(formatThroughput(1_048_576, L)).toBe('1.00 MB/s');
  });

  it('renders idle as an explicit zero', () => {
    expect(formatThroughput(0, L)).toBe('0 B/s');
  });
});

describe('formatPercent', () => {
  it('formats with one decimal by default', () => {
    expect(formatPercent(42.35, L)).toBe('42.4%');
  });

  it('supports whole-number display', () => {
    expect(formatPercent(42.35, L, 0)).toBe('42%');
  });
});

describe('formatFrequency', () => {
  it('scales to GHz above a gigahertz', () => {
    expect(formatFrequency(3_600_000_000, L)).toBe('3.60 GHz');
  });

  it('uses MHz below a gigahertz', () => {
    expect(formatFrequency(800_000_000, L)).toBe('800 MHz');
  });

  it('treats zero as unknown, since a parked core reports no clock', () => {
    expect(formatFrequency(0, L)).toBe('—');
  });
});

describe('formatUptime', () => {
  it('shows the two most significant units only', () => {
    expect(formatUptime(3 * 86_400 + 4 * 3_600 + 12 * 60)).toBe('3d 4h');
    expect(formatUptime(4 * 3_600 + 12 * 60)).toBe('4h 12m');
    expect(formatUptime(12 * 60 + 7)).toBe('12m 7s');
    expect(formatUptime(7)).toBe('7s');
  });

  it('rejects a negative duration', () => {
    expect(formatUptime(-1)).toBe('—');
  });
});

describe('formatLatency', () => {
  it('drops to microseconds rather than rounding to zero', () => {
    // Rounding 0.4ms to "0 ms" would imply the operation was free.
    expect(formatLatency(0.4, L)).toBe('400 µs');
  });

  it('formats milliseconds with decreasing precision', () => {
    expect(formatLatency(12.34, L)).toBe('12.3 ms');
    expect(formatLatency(123.4, L)).toBe('123 ms');
  });
});

describe('formatTemperature and formatWatts', () => {
  it('rounds temperature to whole degrees', () => {
    expect(formatTemperature(72.6, L)).toBe('73°C');
  });

  it('scales watt precision with magnitude', () => {
    expect(formatWatts(12.34, L)).toBe('12.3 W');
    expect(formatWatts(123.4, L)).toBe('123 W');
  });
});

describe('formatCount', () => {
  it('groups digits so large counts are readable', () => {
    expect(formatCount(1_048_576, L)).toBe('1,048,576');
  });
});
