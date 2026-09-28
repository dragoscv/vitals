/**
 * Polls the sensor layer on the cadence the backend asks for.
 *
 * # Not 1 Hz, and not the sampler's business
 *
 * A WMI round trip to `root\WMI` costs tens of milliseconds — measured here
 * at 53 ms cold and 11 ms warm — against a whole-system sample budget of
 * 30 ms. Reading sensors on the sampling tick would therefore blow the budget
 * before a single process had been enumerated, and would do it in a tool
 * whose headline number is CPU usage. That is a uniquely embarrassing way to
 * be wrong.
 *
 * Nor do sensors need that rate. A thermal mass large enough to chart does
 * not move measurably in a second and battery charge moves in whole percent
 * over minutes, so the backend publishes `sample_interval_hint()` (5 s) and
 * this hook honours it rather than choosing its own number. The first
 * response carries `cadenceMs`, and the timer re-arms to match — so changing
 * the cadence is a one-line change in Rust, not a change in two places that
 * drift apart.
 *
 * # A failed read keeps the last good data
 *
 * The previous snapshot survives an error and the screen says the data is
 * stale. Blanking a panel on a transient WMI hiccup would make a working
 * machine look like a broken one, and the user would refresh — paying the
 * cold-start cost again — to see the same thing.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { peekPrefetched, prefetch, takePrefetched } from '../../lib/prefetch';
import { hasTauriHost } from '../../shell/host';
import type { SensorsSnapshot } from './model';
import { errorMessage } from '../../lib/commandError';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

/**
 * Used until the first response says otherwise.
 *
 * Matches `SensorReader::sample_interval_hint()`. It is a fallback for the
 * window before the first read lands, not a second source of truth.
 */
export const DEFAULT_CADENCE_MS = 5000;

export type SensorsReader = () => Promise<SensorsSnapshot>;

/** The real reader. Dynamic import so a browser never evaluates the IPC module. */
export async function readSensors(): Promise<SensorsSnapshot> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SensorsSnapshot>('get_sensors');
}

const PREFETCH_KEY = 'sensors';

/**
 * Background read for the first visit (lib/prefetch). One minute: the screen
 * polls every five seconds once open, and temperatures older than a minute
 * would describe a different load.
 */
export function prefetchSensors(): Promise<void> {
  return prefetch(PREFETCH_KEY, readSensors, 60_000);
}

export interface SensorsState {
  readonly snapshot: SensorsSnapshot | null;
  /** True until the first read settles, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly error: string | null;
  /** Wall-clock time of the last successful read, for "updated at". */
  readonly updatedAt: number | null;
  refresh: () => void;
}

export function useSensors(reader?: SensorsReader): SensorsState {
  // An injected reader replaces the host check rather than being consulted
  // after it. A seam the production path can veto is not a seam — it silently
  // never runs in tests, and every assertion then fails for an unrelated
  // reason. That mistake has already been made once in this codebase.
  const injected = reader !== undefined;

  const [seed] = useState(() =>
    injected ? undefined : peekPrefetched<SensorsSnapshot>(PREFETCH_KEY),
  );
  const [snapshot, setSnapshot] = useState<SensorsSnapshot | null>(seed?.value ?? null);
  const [pending, setPending] = useState(seed === undefined);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<number | null>(seed?.at ?? null);

  const readerRef = useRef<SensorsReader>(reader ?? readSensors);
  readerRef.current = reader ?? readSensors;

  const inFlight = useRef(false);
  const mounted = useRef(true);
  // Held in a ref rather than state so that adopting the backend's cadence
  // does not re-run the effect and restart the timer on every response.
  const cadence = useRef(DEFAULT_CADENCE_MS);

  const load = useCallback(async () => {
    // Skipped rather than queued. A read slower than the interval would
    // otherwise accumulate a backlog until the backlog is the bottleneck —
    // exactly the pathology this application exists to make visible.
    if (inFlight.current) return;
    inFlight.current = true;

    try {
      const next = await ((injected
        ? undefined
        : takePrefetched<SensorsSnapshot>(PREFETCH_KEY, 5000)) ?? readerRef.current());
      if (!mounted.current) return;
      setSnapshot(next);
      setError(null);
      setUpdatedAt(Date.now());
      if (Number.isFinite(next.cadenceMs) && next.cadenceMs > 0) {
        cadence.current = next.cadenceMs;
      }
    } catch (cause: unknown) {
      if (!mounted.current) return;
      // The previous snapshot is kept; the error line says it is stale.
      setError(errorMessage(cause));
    } finally {
      inFlight.current = false;
      // Cleared on both paths. A `pending` flag that survives a failure makes
      // a broken app look merely busy, which is the failure this project has
      // already shipped three times.
      if (mounted.current) setPending(false);
    }
  }, [injected]);

  const refresh = useCallback(() => {
    void load();
  }, [load]);

  useEffect(() => {
    mounted.current = true;

    if (!injected && !hasTauriHost()) {
      setPending(false);
      setError(NO_HOST);
      return () => {
        mounted.current = false;
      };
    }

    void load();

    // Re-armed each tick rather than `setInterval`, so a cadence the backend
    // revises downward takes effect on the next cycle instead of at the next
    // remount.
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      timer = setTimeout(() => {
        void load();
        tick();
      }, cadence.current);
    };
    tick();

    return () => {
      mounted.current = false;
      clearTimeout(timer);
    };
  }, [load, injected]);

  return { snapshot, pending, error, updatedAt, refresh };
}
