/**
 * Power usage per process, on Task Manager's five-step scale.
 *
 * Windows does not publish per-process energy: Task Manager reads it from the
 * Energy Estimation Engine, which has no public API. What drives a process's
 * draw is the same everywhere — CPU time, GPU time, storage and radio
 * traffic — so this weights the readings the sampler already has into one
 * score and buckets it. It is an estimate, and the column's tooltip says so.
 *
 * Calibrated against Task Manager on this machine (2026-10-06): Defender at
 * 11 % CPU = Very high, the search indexer at 2 % = High, Node at 1 % CPU and
 * 20 MB/s of disk = Moderate, idle services = Very low.
 */

export const POWER_LEVELS = ['veryLow', 'low', 'moderate', 'high', 'veryHigh'] as const;
export type PowerLevel = (typeof POWER_LEVELS)[number];

/** Score boundaries between consecutive levels. */
const BOUNDS = [0.3, 1, 2, 5] as const;

export interface PowerInputs {
  /** Percent of the whole machine. */
  readonly cpu: number;
  /** Percent of the busiest engine; `null` when not measured. */
  readonly gpu: number | null;
  /** Bytes per second, read plus write. */
  readonly disk: number;
  /** Bytes per second; `null` when not measured. */
  readonly network: number | null;
}

/** A unitless score; higher draws more. */
export function powerScore(inputs: PowerInputs): number {
  const mb = 1024 * 1024;
  return (
    inputs.cpu +
    // A GPU engine at full tilt costs less per percent than the CPU package
    // on most laptops and desktops alike.
    (inputs.gpu ?? 0) * 0.6 +
    (inputs.disk / mb) * 0.02 +
    ((inputs.network ?? 0) / mb) * 0.05
  );
}

export function powerLevel(score: number): PowerLevel {
  let index = 0;
  while (index < BOUNDS.length && score >= (BOUNDS[index] ?? Number.POSITIVE_INFINITY)) index += 1;
  return POWER_LEVELS[index] ?? 'veryHigh';
}

/**
 * Smoothed power score per process — the "trend" column.
 *
 * An exponential moving average with a two-minute horizon, which is what
 * Task Manager's trend column summarises: a process that spikes for a
 * second is not a battery drain, one that idles at Moderate is.
 */
export function foldTrend(
  previous: ReadonlyMap<string, number>,
  scores: ReadonlyMap<string, number>,
  elapsedMs: number,
): ReadonlyMap<string, number> {
  const horizonMs = 120_000;
  const alpha = 1 - Math.exp(-Math.max(0, elapsedMs) / horizonMs);
  const next = new Map<string, number>();
  for (const [id, score] of scores) {
    const before = previous.get(id);
    // A new process starts at its current score rather than at zero, or
    // every launch would read "Very low" for its first two minutes.
    next.set(id, before === undefined ? score : before + alpha * (score - before));
  }
  return next;
}

/**
 * Holds the trend across frames. Folding is idempotent per frame number, so
 * calling it from a render (a memo keyed on the snapshot) is safe however
 * often React re-runs that render.
 */
export function createTrendStore(): {
  fold: (
    seq: number,
    timestampMs: number,
    scores: ReadonlyMap<string, number>,
  ) => ReadonlyMap<string, number>;
} {
  let seen = -1;
  let at = 0;
  let values: ReadonlyMap<string, number> = new Map();
  return {
    fold(seq, timestampMs, scores) {
      if (seq === seen) return values;
      values = foldTrend(values, scores, seen < 0 ? 0 : timestampMs - at);
      seen = seq;
      at = timestampMs;
      return values;
    },
  };
}
