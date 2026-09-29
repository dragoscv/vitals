import type { MachineSample } from '@vitals/protocol';
import { describe, expect, it } from 'vitest';

import { MAX_POINTS, bucketHistory, peak } from './history';

function sample(ts: number, over: Partial<MachineSample> = {}): MachineSample {
  return {
    ts,
    cpuPercent: 50,
    cpuKernelPercent: 5,
    memoryUsed: 4,
    memoryTotal: 16,
    diskReadBps: 0,
    diskWriteBps: 0,
    netRxBps: 0,
    netTxBps: 0,
    gpuPercent: null,
    cpuTempC: null,
    powerDrawW: null,
    ...over,
  };
}

const END = 1_000_000;

describe('bucketHistory', () => {
  it('fits an hour of per-second points into at most 240 averaged buckets', () => {
    // 40, 50, 60 repeating: every 15-second bucket averages to exactly 50.
    const samples = Array.from({ length: 3600 }, (_, i) =>
      sample(END - 3600 + i, { cpuPercent: 40 + (i % 3) * 10 }),
    );
    const series = bucketHistory(samples, END, 3600);
    expect(series.cpu.length).toBeLessThanOrEqual(MAX_POINTS);
    expect(series.cpu.length).toBe(240);
    for (const v of series.cpu) expect(v).toBeCloseTo(50, 5);
    expect(series.memory[0]).toBeCloseTo(25, 5);
  });

  it('leaves a gap where the PC was off instead of drawing a dip to zero', () => {
    // Ten minutes missing in the middle of the hour.
    const samples = Array.from({ length: 3600 }, (_, i) => sample(END - 3600 + i)).filter(
      (s) => s.ts < END - 2000 || s.ts >= END - 1400,
    );
    const series = bucketHistory(samples, END, 3600);
    const gaps = series.cpu.filter((v) => Number.isNaN(v)).length;
    expect(gaps).toBeGreaterThanOrEqual(39);
    expect(series.cpu).not.toContain(0);
  });

  it('keeps an unmeasured reading unknown through averaging, never zero', () => {
    const samples = Array.from({ length: 600 }, (_, i) => sample(END - 600 + i));
    const series = bucketHistory(samples, END, 3600);
    expect(series.gpu.every((v) => Number.isNaN(v))).toBe(true);
    expect(peak(series.gpu)).toBeNull();
  });

  it('does not split hourly rollups into false gaps when the span is longer than the buckets allow', () => {
    const week = 7 * 86_400;
    const samples = Array.from({ length: 168 }, (_, i) => sample(END - week + i * 3600 + 1800));
    const series = bucketHistory(samples, END, week);
    expect(series.cpu.length).toBe(168);
    expect(series.cpu.some((v) => Number.isNaN(v))).toBe(false);
  });

  it('ignores points outside the span the chart covers', () => {
    const series = bucketHistory(
      [sample(END - 7200, { cpuPercent: 99 }), sample(END - 10)],
      END,
      3600,
    );
    expect(peak(series.cpu)).toBe(50);
  });
});
