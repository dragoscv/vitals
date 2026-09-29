import { describe, expect, it } from 'vitest';

import { DASH, bytes, celsius, duration, percent, rate, sensor, volts, watts } from './format';

describe('format', () => {
  it('renders an unmeasured reading as an em dash and never as zero', () => {
    for (const missing of [null, undefined, Number.NaN]) {
      expect(percent(missing)).toBe(DASH);
      expect(celsius(missing)).toBe(DASH);
      expect(bytes(missing, 'en')).toBe(DASH);
      expect(rate(missing, 'en')).toBe(DASH);
      expect(watts(missing, 'en')).toBe(DASH);
      expect(duration(missing)).toBe(DASH);
    }
    // A genuine zero is still a zero.
    expect(percent(0)).toBe('0 %');
    expect(bytes(0, 'en')).toBe('0 B');
  });

  it('uses binary units and the locale decimal separator, so Romanian reads 12,5 GB', () => {
    const b = 12.5 * 1024 ** 3;
    expect(bytes(b, 'en')).toBe('12.5 GB');
    expect(bytes(b, 'ro')).toBe('12,5 GB');
    expect(bytes(1023, 'en')).toBe('1023 B');
    expect(bytes(1024, 'en')).toBe('1.0 KB');
    expect(bytes(150 * 1024 ** 2, 'en')).toBe('150 MB');
    expect(rate(2048, 'ro')).toBe('2,0 KB/s');
  });

  it('prints percentages and temperatures with a space before the unit, as every other surface does', () => {
    expect(percent(12.4)).toBe('12 %');
    expect(celsius(45.6)).toBe('46 °C');
    expect(watts(4.25, 'ro')).toBe('4,3 W');
    expect(watts(120.4, 'en')).toBe('120 W');
    expect(volts(1.2, 'ro')).toBe('1,20 V');
  });

  it('formats a sensor by its unit key and falls back to one decimal for units it does not know', () => {
    expect(sensor('temperature', 51, 'en')).toBe('51 °C');
    expect(sensor('fanSpeed', 1200.4, 'en')).toBe('1200 RPM');
    expect(sensor('mystery', 3.14159, 'en')).toBe('3.1');
  });

  it('shortens a duration to its two largest units', () => {
    expect(duration(59)).toBe('0m');
    expect(duration(3_660)).toBe('1h 1m');
    expect(duration(90_000)).toBe('1d 1h');
  });
});
