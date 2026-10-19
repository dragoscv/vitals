import { describe, expect, it } from 'vitest';

import { sparkPath, topChanges } from './history';
import {
  decimalSeparator,
  minuteText,
  parseSchedules,
  scheduledKinds,
  scheduleToast,
  schedulableKinds,
  toWire,
} from './schedule';

describe('weekly clean-up schedules', () => {
  it('runs only schedulable kinds out of a profile, whatever else it holds', () => {
    expect(
      scheduledKinds(['compactVhd', 'tempOld', 'dockerNamedVolume', 'sparseVhd', 'npmCache']),
    ).toEqual(['tempOld', 'npmCache']);
  });

  it('never lists a kind that compacts, elevates, stops WSL or deletes a volume', () => {
    for (const kind of [
      'compactVhd',
      'sparseVhd',
      'componentStore',
      'hibernateOff',
      'dockerNamedVolume',
      'dockerAnonVolumes',
      'rustTarget',
      'recycleBin',
    ]) {
      expect(schedulableKinds as readonly string[]).not.toContain(kind);
    }
  });

  it('sends the drive profile as the kinds, or every schedulable kind without one', () => {
    const wire = toWire(
      {
        'vol-c': { mount: 'C:', weekday: 6, minute: 180, enabledAt: 9 },
        'vol-e': { mount: 'E:', weekday: 0, minute: 30, enabledAt: 9 },
      },
      { 'C:': ['tempOld'] },
    );
    expect(wire).toEqual([
      { volume: 'vol-c', mount: 'C:', weekday: 6, minute: 180, kinds: ['tempOld'], enabledAt: 9 },
      {
        volume: 'vol-e',
        mount: 'E:',
        weekday: 0,
        minute: 30,
        kinds: [...schedulableKinds],
        enabledAt: 9,
      },
    ]);
  });

  it('drops a malformed schedule alone', () => {
    expect(
      parseSchedules({
        a: { mount: 'C:', weekday: 0, minute: 0, enabledAt: 0 },
        b: { mount: 'C:', weekday: 0.5, minute: 0, enabledAt: 0 },
        '': { mount: 'C:', weekday: 0, minute: 0, enabledAt: 0 },
      }),
    ).toEqual({ a: { mount: 'C:', weekday: 0, minute: 0, enabledAt: 0 } });
    expect(parseSchedules([])).toEqual({});
  });

  it('writes times as HH:MM and the decimal separator of the language', () => {
    expect(minuteText(0)).toBe('00:00');
    expect(minuteText(185)).toBe('03:05');
    expect(decimalSeparator('ro')).toBe(',');
    expect(decimalSeparator('en')).toBe('.');
  });

  it('gives the notification both placeholders in both languages', () => {
    for (const locale of ['en', 'ro'] as const) {
      expect(scheduleToast[locale].freed).toContain('{mount}');
      expect(scheduleToast[locale].freed).toContain('{size}');
      expect(scheduleToast[locale].freedUnknown).toContain('{mount}');
      expect(scheduleToast[locale].freedUnknown).not.toContain('{size}');
    }
    expect(JSON.stringify(scheduleToast.ro)).not.toMatch(/[şţŞŢ]/);
  });
});

describe('drive history helpers', () => {
  it('draws a sparkline only from two readings, scaled to the series own range', () => {
    expect(sparkPath([{ ts: 1, used: 5, free: 0 }], 100, 10)).toBeNull();
    const path = sparkPath(
      [
        { ts: 0, used: 10, free: 0 },
        { ts: 50, used: 20, free: 0 },
        { ts: 100, used: 15, free: 0 },
      ],
      100,
      10,
    );
    expect(path).toBe('0.0,10.0 50.0,0.0 100.0,5.0');
    expect(
      sparkPath(
        [
          { ts: 0, used: 7, free: 0 },
          { ts: 1, used: 7, free: 0 },
        ],
        10,
        10,
      ),
    ).toBe('0.0,5.0 10.0,5.0');
  });

  it('orders changes by growth and keeps the top few', () => {
    const changes = [
      { kind: 'a', label: null, then: 10, now: 5 },
      { kind: 'b', label: null, then: 0, now: 50 },
      { kind: 'c', label: null, then: 1, now: 2 },
    ];
    expect(topChanges(changes, 2).map((c) => c.kind)).toEqual(['b', 'c']);
  });
});
