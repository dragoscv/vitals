/**
 * Recorded history for the History widget.
 *
 * Unlike every other dashboard widget, this one is not fed by the live 1 Hz
 * frame: it reads the SQLite time-series store through
 * `query_machine_history`, which returns the span already rolled up to the
 * resolution the retention policy assigns to it. The webview never
 * downsamples — a week at one-second resolution would be 600k rows crossing
 * IPC to draw a 600-pixel line.
 *
 * # Polling is tied to the mount
 *
 * The store is written once a second by the sampler, but a chart of the last
 * day does not change visibly in under a minute, so the widget re-queries on a
 * 60 s timer and only while it is rendered. A user who removes the widget from
 * their layout must not keep paying for a SQLite read they cannot see.
 */

import { useEffect, useMemo, useState } from 'react';

import { RingBuffer } from '@vitals/charts';
import type { MachineSample } from '@vitals/protocol';

import { hasTauriHost } from '../../../shell/host';
import { errorMessage } from '../../../lib/commandError';

export const historyRanges = ['h1', 'h6', 'h24', 'd7'] as const;
export type HistoryRange = (typeof historyRanges)[number];

export const rangeSeconds: Readonly<Record<HistoryRange, number>> = {
  h1: 3_600,
  h6: 6 * 3_600,
  h24: 24 * 3_600,
  d7: 7 * 24 * 3_600,
};

/** How often a mounted widget re-reads the store. */
export const HISTORY_POLL_MS = 60_000;

export interface HistorySource {
  /** Samples covering the last `seconds`, oldest first. */
  query(seconds: number): Promise<readonly MachineSample[]>;
}

/** Shown when there is no Tauri host at all (browser preview). */
export const NO_HOST = 'no-host';

/** Backed by the Tauri command; the one place its name is spelled. */
export function createTauriHistorySource(): HistorySource {
  return {
    async query(seconds) {
      if (!hasTauriHost()) throw new Error(NO_HOST);
      const { invoke } = await import('@tauri-apps/api/core');
      return invoke<MachineSample[]>('query_machine_history', { seconds });
    },
  };
}

export interface HistorySeries {
  readonly cpu: RingBuffer;
  readonly memory: RingBuffer;
  readonly gpu: RingBuffer;
  readonly count: number;
}

/**
 * Turns stored samples into chart buffers.
 *
 * A `null` GPU reading becomes a gap, never a zero: the store keeps "no GPU
 * reported anything" as `None` all the way through rollup, and drawing it as
 * 0 % would show an idle card where there was no card at all. Memory is
 * converted to a percentage here so the three lines share one 0–100 scale.
 */
export function seriesFromSamples(samples: readonly MachineSample[]): HistorySeries {
  const capacity = Math.max(1, samples.length);
  const cpu = new RingBuffer(capacity);
  const memory = new RingBuffer(capacity);
  const gpu = new RingBuffer(capacity);

  for (const sample of samples) {
    cpu.push(sample.cpuPercent);
    if (sample.memoryTotal > 0) {
      memory.push((sample.memoryUsed / sample.memoryTotal) * 100);
    } else {
      memory.pushGap();
    }
    if (sample.gpuPercent === null) {
      gpu.pushGap();
    } else {
      gpu.push(sample.gpuPercent);
    }
  }

  return { cpu, memory, gpu, count: samples.length };
}

export interface MachineHistoryState {
  readonly samples: readonly MachineSample[];
  readonly series: HistorySeries;
  /** Bumped on every successful read, for `TimeSeriesChart`'s `revision`. */
  readonly revision: number;
  /** True until the first answer for the current range, success or failure. */
  readonly pending: boolean;
  readonly error: string | null;
}

const NO_SAMPLES: readonly MachineSample[] = [];

/**
 * Reads the store for `range`, on mount and every {@link HISTORY_POLL_MS}.
 *
 * `enabled: false` skips the read entirely rather than reading an empty
 * store: when recording is off the widget shows a different message and a
 * query would only cost a SQLite open for nothing.
 */
export function useMachineHistory(
  range: HistoryRange,
  enabled: boolean,
  source?: HistorySource,
): MachineHistoryState {
  const [fallback] = useState<HistorySource | null>(() =>
    source === undefined ? createTauriHistorySource() : null,
  );
  const active = source ?? fallback;
  if (active === null) throw new Error('history widget has no source');

  const [samples, setSamples] = useState<readonly MachineSample[]>(NO_SAMPLES);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [answered, setAnswered] = useState<{ range: HistoryRange } | null>(null);

  // `active` is in the dependency list, so a caller must hold a stable
  // source (the dashboard keeps one in state) — an inlined object literal
  // would restart the timer on every render.
  useEffect(() => {
    if (!enabled) return undefined;

    let cancelled = false;
    const seconds = rangeSeconds[range];

    const read = (): void => {
      active
        .query(seconds)
        .then((next) => {
          if (cancelled) return;
          setSamples(next);
          setError(null);
          setRevision((value) => value + 1);
          setAnswered({ range });
        })
        .catch((cause: unknown) => {
          if (cancelled) return;
          setError(errorMessage(cause));
          setAnswered({ range });
        });
    };

    read();
    const timer = setInterval(read, HISTORY_POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [range, enabled, active]);

  const series = useMemo(() => seriesFromSamples(samples), [samples]);

  return {
    samples,
    series,
    revision,
    pending: enabled && answered?.range !== range,
    error,
  };
}
