/**
 * Lists the available benchmarks, and runs the selected ones.
 *
 * The listing is cheap and happens on mount. The run is not: it takes tens of
 * seconds, saturates the machine, and cannot be cancelled once the backend is
 * inside a measurement loop — so it is only ever started by an explicit click.
 *
 * # Every loading state must be able to end
 *
 * This project has shipped an unbounded spinner three times, which is why the
 * rules here are stated rather than assumed. The listing resolves immediately
 * with `NO_HOST` when there is no Tauri host instead of sitting `pending`
 * forever. A failed run clears `running` on the error path as well as the
 * success path, and keeps the previous suite on screen with an error line
 * above it — blanking a result the user was reading is a second failure on
 * top of the first.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { hasTauriHost } from '../../shell/host';
import type { BenchmarkId, BenchmarkInfo, BenchmarkSuiteDto } from './model';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

export interface BenchmarksSource {
  readonly list: () => Promise<readonly BenchmarkInfo[]>;
  readonly run: (ids: readonly BenchmarkId[]) => Promise<BenchmarkSuiteDto>;
}

/** The real source. Dynamic imports so a browser never evaluates the IPC module. */
export const tauriSource: BenchmarksSource = {
  list: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<readonly BenchmarkInfo[]>('list_benchmarks');
  },
  run: async (ids) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<BenchmarkSuiteDto>('run_benchmarks', { ids: [...ids] });
  },
};

export interface BenchmarksState {
  readonly infos: readonly BenchmarkInfo[];
  /** True until the listing settles, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly listError: string | null;

  readonly suite: BenchmarkSuiteDto | null;
  readonly running: boolean;
  /** Which benchmarks the in-flight run covers, so the screen can name them. */
  readonly runningIds: readonly BenchmarkId[];
  readonly runError: string | null;

  run: (ids: readonly BenchmarkId[]) => void;
  refresh: () => void;
}

export function useBenchmarks(source?: BenchmarksSource): BenchmarksState {
  const [infos, setInfos] = useState<readonly BenchmarkInfo[]>([]);
  const [pending, setPending] = useState(true);
  const [listError, setListError] = useState<string | null>(null);

  const [suite, setSuite] = useState<BenchmarkSuiteDto | null>(null);
  const [running, setRunning] = useState(false);
  const [runningIds, setRunningIds] = useState<readonly BenchmarkId[]>([]);
  const [runError, setRunError] = useState<string | null>(null);

  // An injected source replaces the host check rather than sitting behind it.
  // A seam the production path can veto silently never runs in tests, and
  // every assertion then fails for an unrelated reason.
  const injected = source !== undefined;
  const sourceRef = useRef<BenchmarksSource>(source ?? tauriSource);
  sourceRef.current = source ?? tauriSource;

  const mounted = useRef(true);
  const runInFlight = useRef(false);

  const load = useCallback(async () => {
    try {
      const next = await sourceRef.current.list();
      if (!mounted.current) return;
      setInfos(next);
      setListError(null);
    } catch (cause: unknown) {
      if (!mounted.current) return;
      setListError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (mounted.current) setPending(false);
    }
  }, []);

  const run = useCallback((ids: readonly BenchmarkId[]) => {
    // Guarded rather than queued: two suites measuring the same machine at the
    // same time would each be measuring the other, and both results would be
    // garbage that looks fine.
    if (runInFlight.current || ids.length === 0) return;
    runInFlight.current = true;

    setRunning(true);
    setRunningIds(ids);
    setRunError(null);

    void sourceRef.current
      .run(ids)
      .then((next) => {
        if (!mounted.current) return;
        setSuite(next);
      })
      .catch((cause: unknown) => {
        if (!mounted.current) return;
        // The previous suite stays on screen. A failed re-run should not
        // discard numbers the user was in the middle of reading.
        setRunError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => {
        runInFlight.current = false;
        // Cleared on both paths. An error that sets a message without
        // clearing `running` leaves a spinner nothing can ever stop.
        if (mounted.current) {
          setRunning(false);
          setRunningIds([]);
        }
      });
  }, []);

  const refresh = useCallback(() => {
    void load();
  }, [load]);

  useEffect(() => {
    mounted.current = true;

    if (!injected && !hasTauriHost()) {
      // Resolved at once: a loading state that cannot end makes a broken app
      // look busy, which this project has shipped three times already.
      setPending(false);
      setListError(NO_HOST);
      return () => {
        mounted.current = false;
      };
    }

    void load();

    return () => {
      mounted.current = false;
    };
  }, [load, injected]);

  return { infos, pending, listError, suite, running, runningIds, runError, run, refresh };
}
