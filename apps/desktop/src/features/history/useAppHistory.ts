/**
 * Fetches app history on demand.
 *
 * History changes slowly — only when applications start or exit — so polling
 * continuously would produce the same result at real cost. One read on mount,
 * plus a manual refresh, is the right cadence.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { hasTauriHost } from '../../shell/host';
import type { AppHistorySnapshot } from './model';
import { errorMessage } from '../../lib/commandError';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

export type HistoryReader = () => Promise<AppHistorySnapshot>;
export type HistoryClearer = () => Promise<void>;

/** The real reader. Dynamic import so a browser never evaluates the IPC module. */
export async function readHistory(): Promise<AppHistorySnapshot> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AppHistorySnapshot>('get_app_history');
}

export async function clearHistory(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<void>('clear_app_history');
}

export interface HistoryState {
  readonly snapshot: AppHistorySnapshot | null;
  /** True until the first read settles, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly error: string | null;
  refresh: () => void;
  clear: () => Promise<void>;
}

export function useAppHistory(reader?: HistoryReader, clearer?: HistoryClearer): HistoryState {
  const [snapshot, setSnapshot] = useState<AppHistorySnapshot | null>(null);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Injected functions replace the host check rather than sitting behind it.
  const injected = reader !== undefined || clearer !== undefined;
  const readerRef = useRef<HistoryReader>(reader ?? readHistory);
  const clearerRef = useRef<HistoryClearer>(clearer ?? clearHistory);
  readerRef.current = reader ?? readHistory;
  clearerRef.current = clearer ?? clearHistory;

  const inFlight = useRef(false);
  const mounted = useRef(true);

  const load = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;

    try {
      const next = await readerRef.current();
      if (!mounted.current) return;
      setSnapshot(next);
      setError(null);
    } catch (cause: unknown) {
      if (!mounted.current) return;
      // The previous snapshot survives a failed refresh; the error line says so.
      setError(errorMessage(cause));
    } finally {
      inFlight.current = false;
      if (mounted.current) setPending(false);
    }
  }, []);

  const refresh = useCallback(() => {
    void load();
  }, [load]);

  const clear = useCallback(async () => {
    try {
      await clearerRef.current();
      // Reload immediately after clearing so the UI reflects the empty state.
      void load();
    } catch (cause: unknown) {
      setError(errorMessage(cause));
    }
  }, [load]);

  useEffect(() => {
    mounted.current = true;

    if (!injected && !hasTauriHost()) {
      // Resolved at once: a loading state that cannot end makes a broken app
      // look busy.
      setPending(false);
      setError(NO_HOST);
      return () => {
        mounted.current = false;
      };
    }

    void load();

    return () => {
      mounted.current = false;
    };
  }, [load, injected]);

  return { snapshot, pending, error, refresh, clear };
}
