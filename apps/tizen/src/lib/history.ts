/**
 * Fits a `/api/v1/history` answer onto a chart a TV can draw.
 *
 * The store keeps per-second points for the last hour and rollups beyond, so
 * an hour is 3600 points — far more than 240 horizontal pixels of chart can
 * show, and too many SVG vertices for a TV's CPU to redraw comfortably. The
 * span is cut into equal buckets and each is averaged.
 *
 * A bucket with no sample is `NaN`, not zero: the PC was off or asleep, and
 * a chart must show a gap there, not a dip to 0 %. Likewise a field the PC
 * could not measure (`gpuPercent: null`) stays `NaN` through averaging — an
 * average of unknowns is still unknown.
 */

import type { MachineSample } from '@vitals/protocol';

export const MAX_POINTS = 240;

export interface HistorySeries {
  /** Bucket start times, Unix seconds. */
  readonly ts: readonly number[];
  readonly cpu: readonly number[];
  /** Memory in use, percent of total. */
  readonly memory: readonly number[];
  readonly gpu: readonly number[];
  readonly temperature: readonly number[];
}

function median(values: number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? 0;
}

/**
 * @param endTs the right edge of the chart, Unix seconds (normally now)
 * @param spanSecs how far back the chart reaches
 */
export function bucketHistory(
  samples: readonly MachineSample[],
  endTs: number,
  spanSecs: number,
  maxPoints: number = MAX_POINTS,
): HistorySeries {
  const start = endTs - spanSecs;
  const inSpan = samples.filter((s) => s.ts >= start && s.ts <= endTs).sort((a, b) => a.ts - b.ts);

  // Never narrower than the data's own spacing: seven days of hourly rollups
  // in 240 buckets would otherwise leave every other bucket empty, and the
  // chart would be a row of dots separated by false gaps.
  const steps: number[] = [];
  for (let i = 1; i < inSpan.length; i++) {
    const a = inSpan[i - 1];
    const b = inSpan[i];
    if (a !== undefined && b !== undefined) steps.push(b.ts - a.ts);
  }
  const width = Math.max(spanSecs / maxPoints, median(steps), 1);
  const count = Math.min(maxPoints, Math.max(1, Math.ceil(spanSecs / width)));

  const sums = Array.from({ length: count }, () => ({
    n: 0,
    cpu: 0,
    memory: [0, 0] as [number, number],
    gpu: [0, 0] as [number, number],
    temperature: [0, 0] as [number, number],
  }));
  for (const s of inSpan) {
    const index = Math.min(count - 1, Math.floor((s.ts - start) / width));
    const bucket = sums[index];
    if (bucket === undefined) continue;
    bucket.n++;
    bucket.cpu += s.cpuPercent;
    if (s.memoryTotal > 0) {
      bucket.memory[0] += (s.memoryUsed / s.memoryTotal) * 100;
      bucket.memory[1]++;
    }
    if (s.gpuPercent !== null) {
      bucket.gpu[0] += s.gpuPercent;
      bucket.gpu[1]++;
    }
    if (s.cpuTempC !== null) {
      bucket.temperature[0] += s.cpuTempC;
      bucket.temperature[1]++;
    }
  }
  const avg = ([sum, n]: [number, number]) => (n > 0 ? sum / n : Number.NaN);
  return {
    ts: sums.map((_, i) => start + i * width),
    cpu: sums.map((b) => (b.n > 0 ? b.cpu / b.n : Number.NaN)),
    memory: sums.map((b) => avg(b.memory)),
    gpu: sums.map((b) => avg(b.gpu)),
    temperature: sums.map((b) => avg(b.temperature)),
  };
}

/** The highest measured value, or `null` when nothing in the series was measured. */
export function peak(values: readonly number[]): number | null {
  let best: number | null = null;
  for (const v of values) {
    if (!Number.isNaN(v) && (best === null || v > best)) best = v;
  }
  return best;
}
