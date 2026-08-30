import { describe, expect, it } from 'vitest';

import { defaultSettings, parseSettings, samplingIntervalMs } from './schema';

describe('parseSettings', () => {
  it('falls back to defaults for anything that is not an object', () => {
    expect(parseSettings(undefined)).toEqual(defaultSettings);
    expect(parseSettings(null)).toEqual(defaultSettings);
    expect(parseSettings('{}')).toEqual(defaultSettings);
    expect(parseSettings(42)).toEqual(defaultSettings);
  });

  it('keeps valid values and repairs invalid ones field by field', () => {
    // The file is hand-editable JSON that survives upgrades. One bad value
    // must cost that one setting, not the entire configuration.
    const parsed = parseSettings({
      theme: { mode: 'dark', accent: 'chartreuse', surface: 'acrylic' },
      locale: 'klingon',
      retentionDays: 9999,
      historyEnabled: true,
      lastRoute: 'a-route-that-was-removed',
    });

    expect(parsed.theme.mode).toBe('dark');
    expect(parsed.theme.surface).toBe('acrylic');
    expect(parsed.theme.accent).toBe(defaultSettings.theme.accent);
    expect(parsed.locale).toBe(defaultSettings.locale);
    expect(parsed.retentionDays).toBe(defaultSettings.retentionDays);
    expect(parsed.historyEnabled).toBe(true);
    expect(parsed.lastRoute).toBe(defaultSettings.lastRoute);
  });

  it('treats a non-boolean reduceMotion as "follow the system"', () => {
    expect(parseSettings({ theme: { reduceMotion: 'yes' } }).theme.reduceMotion).toBeNull();
    expect(parseSettings({ theme: { reduceMotion: true } }).theme.reduceMotion).toBe(true);
    expect(parseSettings({ theme: { reduceMotion: false } }).theme.reduceMotion).toBe(false);
  });

  it('ships every data-emitting option off by default', () => {
    // A system monitor that opts you in to telemetry is not one to trust with
    // process-level visibility. This is a product promise, so it is asserted.
    expect(defaultSettings.crashReports).toBe(false);
    expect(defaultSettings.usageData).toBe(false);
    expect(defaultSettings.reputationLookups).toBe(false);
    expect(defaultSettings.historyEnabled).toBe(false);
    expect(defaultSettings.advancedEnabled).toBe(false);
  });

  it('orders sampling intervals from fastest to slowest', () => {
    expect(samplingIntervalMs.fast).toBeLessThan(samplingIntervalMs.normal);
    expect(samplingIntervalMs.normal).toBeLessThan(samplingIntervalMs.slow);
  });
});
