import { describe as suite, expect, it } from 'vitest';

import { makeCpu, makeDisk, makeNetwork, makeSystem } from '../dashboard/test-fixtures';
import { describe, leadReading, type WidgetInput } from './describe';
import { widgetKinds } from './model';

function input(overrides: Partial<WidgetInput> = {}): WidgetInput {
  return {
    system: makeSystem(),
    cpuHistory: [],
    memoryHistory: [],
    gpuHistory: [],
    netHistory: [],
    diskHistory: [],
    ...overrides,
  };
}

suite('describing a widget', () => {
  it('renders an unmeasured reading as an em dash, never as a zero', () => {
    const description = describe('cpu', input({ system: null }), 'en');

    expect(description.readings[0]?.value).toBe('—');
    expect(description.readings[0]?.percent).toBeNull();
  });

  it('reports no memory percentage when the total is unknown rather than assuming zero', () => {
    const description = describe(
      'memory',
      input({ system: makeSystem({ memory: { total: 0, used: 0 } }) }),
      'en',
    );

    expect(description.readings[0]?.percent).toBeNull();
    expect(description.readings[0]?.value).toBe('—');
  });

  it('shows the gpu that is actually working rather than the first adapter', () => {
    // The first enumerated adapter is routinely a virtual display with no
    // counters at all; the real card is the second one.
    const system = makeSystem({
      gpus: [{ utilization: null }, { utilization: 17 }],
    });

    const description = describe('overview', input({ system }), 'en');
    const gpu = description.readings.find((entry) => entry.id === 'overview-gpu');

    expect(gpu?.percent).toBe(17);
    expect(gpu?.value).toBe('17%');
  });

  it('falls back to the cpu temperature when no gpu reports one', () => {
    const system = makeSystem({
      cpu: { temperature: 45 },
      gpus: [{ temperature: null }],
    });

    const description = describe('thermal', input({ system }), 'en');
    const headline = description.readings.find((entry) => entry.id === 'thermal-headline');

    expect(headline?.value).toBe('45°C');
  });

  it('counts every adapter in the network total, not only the busiest one', () => {
    const system = makeSystem({
      networks: [
        { rx: 120_000, tx: 40_000 },
        { rx: 200_000, tx: 40_000 },
      ],
    });

    const description = describe('network', input({ system }), 'en');
    const down = description.readings.find((entry) => entry.id === 'net-down');

    // 120 000 + 200 000 = 320 000 B/s, which reads as 313 KB/s. Showing only
    // the busiest adapter would read 195 KB/s, so this fails if the sum is
    // replaced by a single adapter's figure.
    expect(down?.value).toBe('313 KB/s');
    expect(down?.value).not.toBe('195 KB/s');
  });

  it('reports the disk that is carrying the most traffic as the busiest one', () => {
    const system = makeSystem({
      disks: [
        { name: 'idle', read: 0, write: 0, activeTime: 1 },
        { name: 'busy', read: 5_000_000, write: 5_000_000, activeTime: 88 },
      ],
    });

    const description = describe('disk', input({ system }), 'en');
    const activity = description.readings.find((entry) => entry.id === 'disk-activity');

    expect(activity?.percent).toBe(88);
  });

  it('gives every kind at least one reading, so no widget can render a blank panel', () => {
    for (const kind of widgetKinds) {
      const description = describe(kind, input(), 'en');
      expect(description.readings.length).toBeGreaterThan(0);
      expect(description.titleKey).not.toBeUndefined();
    }
  });

  it('names the reading a ring or gauge layout should draw', () => {
    expect(leadReading(describe('cpu', input(), 'en'))?.percent).not.toBeNull();
  });

  it('carries the trace a sparkline needs', () => {
    const description = describe('cpu', input({ cpuHistory: [1, 2, 3] }), 'en');

    expect(description.readings[0]?.trace).toEqual([1, 2, 3]);
  });

  it('labels a disk reading with the unit detail rather than hiding it', () => {
    const description = describe(
      'disk',
      input({ system: makeSystem({ disks: [{ responseMs: 12.34 }] }) }),
      'en',
    );

    expect(description.readings[0]?.detail).toBe('12.3 ms');
  });

  it('keeps the cpu label distinct from its temperature so both can be shown', () => {
    const description = describe(
      'cpu',
      input({ system: makeSystem({ cpu: makeCpu({ total: 12, temperature: 61 }) }) }),
      'en',
    );

    const labels = description.readings.map((entry) => entry.labelKey);
    expect(new Set(labels).size).toBe(labels.length);
    expect(description.readings[1]?.value).toBe('61°C');
  });

  it('shows the fastest fan and the package power draw, named so it can be told apart', () => {
    // Board headers are the chip's numbering (`CPU_FAN`, `SYS_FAN2`) and mean
    // nothing to a reader, so the single number worth showing is the fastest
    // fan, with the header's name beside it.
    const system = makeSystem({
      fans: [
        { name: 'Fan 1', rpm: 900 },
        { name: 'Fan 2', rpm: 1467 },
      ],
      powerDraw: 62.5,
    });

    const description = describe('thermal', input({ system }), 'en');
    const fan = description.readings.find((entry) => entry.id === 'thermal-fan');
    const power = description.readings.find((entry) => entry.id === 'thermal-power');

    expect(fan?.value).toBe('1,467 rpm');
    expect(fan?.detail).toBe('Fan 2');
    expect(power?.value).toBe('62.5 W');
  });

  it('reports a machine with no tachometer and no power sensor as unmeasured, never as zero', () => {
    // An empty fan list means no header reported a speed — a platform that
    // cannot read one must not read as "a fan spinning at 0 rpm".
    const system = makeSystem({ fans: [], powerDraw: null });

    const description = describe('thermal', input({ system }), 'en');

    expect(description.readings.find((entry) => entry.id === 'thermal-fan')?.value).toBe('—');
    expect(description.readings.find((entry) => entry.id === 'thermal-power')?.value).toBe('—');
  });

  it('reports the hottest disk temperature rather than the first disk', () => {
    const system = makeSystem({
      disks: [{ temperature: 30 }, { temperature: 55 }],
    });

    const description = describe('thermal', input({ system }), 'en');
    const disk = description.readings.find((entry) => entry.id === 'thermal-disk');

    expect(disk?.value).toBe('55°C');
  });

  it('shows the wifi name for a wireless adapter and the link speed as detail', () => {
    const system = makeSystem({
      networks: [makeNetwork({ kind: 'wiFi', ssid: 'HomeNet', linkSpeed: 866 })],
    });

    const description = describe('network', input({ system }), 'en');

    expect(description.readings[0]?.detail).toBe('HomeNet');
    expect(description.readings[1]?.detail).toBe('866 Mbps');
  });

  it('survives a machine with no disks and no adapters at all', () => {
    const system = makeSystem({ disks: [], networks: [] });

    expect(() => describe('disk', input({ system }), 'en')).not.toThrow();
    expect(describe('disk', input({ system }), 'ro').readings[0]?.value).toBe('—');
    expect(describe('network', input({ system: makeSystem({ networks: [] }) }), 'en')).toBeTruthy();
  });

  it('formats in the requested locale rather than always English', () => {
    const system = makeSystem({ memory: { total: 32 * 1024 ** 3, used: 12 * 1024 ** 3 } });

    const english = describe('memory', input({ system }), 'en').readings[0];
    const romanian = describe('memory', input({ system }), 'ro').readings[0];

    // Romanian uses the comma as its decimal separator: "12,5 GB".
    expect(romanian?.detail).not.toBe(english?.detail);
    expect(romanian?.detail).toContain(',');
  });

  it('uses the largest of the three temperatures for the thermal headline', () => {
    const system = makeSystem({
      cpu: { temperature: 40 },
      gpus: [{ temperature: null }],
      disks: [{ temperature: 20 }],
    });

    const headline = describe('thermal', input({ system }), 'en').readings[0];

    expect(headline?.value).toBe('40°C');
  });
});

suite('the disk fixture stays plausible', () => {
  it('reports a positive active time so the activity gauge has something to draw', () => {
    expect(makeDisk().activeTime).toBeGreaterThan(0);
  });

  it('describes a connected adapter so the network widget is not empty by default', () => {
    expect(makeNetwork().connected).toBe(true);
  });
});
