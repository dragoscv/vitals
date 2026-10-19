/**
 * The drive history: wire shapes and the arithmetic the panel needs.
 *
 * `get_drive_history` answers with readings of the drive's used space (the
 * last 30 days, at most 120 points), the growth rates the backend computed,
 * what changed between the last two clean-up analyses, and the last runs.
 * A rate is `null` when the readings do not cover enough of its window: a
 * "per week" figure from two hours of data would be a guess.
 */

export interface DrivePoint {
  readonly ts: number;
  readonly used: number;
  readonly free: number;
}

export interface DriveGrowth {
  /** Bytes per day; `null` when there are not enough readings. */
  readonly day: number | null;
  readonly week: number | null;
  readonly month: number | null;
}

export interface DriveChange {
  readonly kind: string;
  readonly label: string | null;
  readonly then: number;
  readonly now: number;
}

export interface DriveRun {
  readonly ts: number;
  readonly scheduled: boolean;
  /** What the drive gained; `null` when it could not be measured. */
  readonly freed: number | null;
  readonly items: number;
  readonly failed: number;
  readonly errors: readonly string[];
}

export interface DriveHistory {
  /** The key schedules are stored under: the volume's GUID path. */
  readonly volume: string;
  readonly series: readonly DrivePoint[];
  readonly growth: DriveGrowth;
  readonly latestAnalysis: number | null;
  readonly previousAnalysis: number | null;
  readonly changes: readonly DriveChange[];
  readonly runs: readonly DriveRun[];
}

/** Kinds the history adds beside the plan's item kinds. */
export const historyOnlyKinds = ['dockerDisk', 'wslDisk'] as const;

export interface DriveHistorySource {
  readonly load: (mount: string) => Promise<DriveHistory>;
}

export const tauriHistorySource: DriveHistorySource = {
  load: async (mount) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DriveHistory>('get_drive_history', { mount });
  },
};

/** Signed change of one item. */
export function changeDelta(change: DriveChange): number {
  return change.now - change.then;
}

/** The changes worth a line: largest growth first, at most `max`. */
export function topChanges(changes: readonly DriveChange[], max = 5): readonly DriveChange[] {
  return [...changes].sort((a, b) => changeDelta(b) - changeDelta(a)).slice(0, max);
}

/**
 * An SVG polyline for the used-space series in a `width` × `height` box.
 *
 * Scaled to the series' own range, so a drive that moved by 2 GB of 2 TB
 * still shows a slope; a flat series sits in the middle. `null` for fewer
 * than two points: one reading is not a line.
 */
export function sparkPath(
  series: readonly DrivePoint[],
  width: number,
  height: number,
): string | null {
  if (series.length < 2) return null;
  const first = series[0];
  const last = series[series.length - 1];
  if (first === undefined || last === undefined) return null;
  const span = last.ts - first.ts;
  if (span <= 0) return null;
  let min = Infinity;
  let max = -Infinity;
  for (const p of series) {
    min = Math.min(min, p.used);
    max = Math.max(max, p.used);
  }
  const range = max - min;
  return series
    .map((p) => {
      const x = ((p.ts - first.ts) / span) * width;
      const y = range === 0 ? height / 2 : height - ((p.used - min) / range) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');
}
