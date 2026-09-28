/**
 * Polls the connections table while the screen is open.
 *
 * # Why this polls rather than receiving pushed frames
 *
 * Everything else in this app is pushed from the sampler, and for good reason
 * — request/response at 1 Hz for thousands of rows serialises on the webview's
 * main thread. Connections are the exception, on three counts:
 *
 * - They are **not** part of the sampler's 30 ms budget, and adding a socket
 *   table read to every tick would consume a large share of it for data that
 *   only one screen displays.
 * - The table is only interesting **while this screen is open**. Pushing it
 *   permanently would cost every user the work for a screen most never visit.
 * - Two seconds is fast enough. Sockets are not a 60 Hz phenomenon, and a
 *   connection list that reshuffles every second is harder to read, not more
 *   informative.
 *
 * # Overlapping polls are prevented, not queued
 *
 * If a read takes longer than the interval — plausible on a machine with
 * thousands of sockets — the next tick is skipped rather than stacking. A
 * queue here would let a slow machine accumulate pending reads until the
 * backlog is the bottleneck, which is precisely the pathology this app exists
 * to make visible.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import type { Connection } from '@vitals/protocol';

import { peekPrefetched, prefetch, takePrefetched } from '../../lib/prefetch';
import { hasTauriHost } from '../../shell/host';
import { errorMessage } from '../../lib/commandError';

/** Refresh cadence. Sockets are not a high-frequency phenomenon. */
export const POLL_INTERVAL_MS = 2000;

export interface ProcessConnectionSummary {
  readonly pid: number;
  readonly total: number;
  readonly tcp: number;
  readonly udp: number;
  readonly active: number;
  readonly listening: number;
  readonly remoteHosts: number;
  readonly public: number;
}

export interface ConnectionsSnapshot {
  readonly connections: readonly Connection[];
  readonly byProcess: readonly ProcessConnectionSummary[];
}

export interface ConnectionsState {
  readonly snapshot: ConnectionsSnapshot | null;
  /** True until the first read completes, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly error: string | null;
  /** Wall-clock time of the last successful read, for "updated N ago". */
  readonly updatedAt: number | null;
  refresh: () => void;
}

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

export type ConnectionsReader = () => Promise<ConnectionsSnapshot>;

/** The real reader. Dynamic import so a browser never evaluates the IPC module. */
export async function readConnections(): Promise<ConnectionsSnapshot> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<ConnectionsSnapshot>('get_connections');
}

const PREFETCH_KEY = 'connections';

/**
 * Background read for the first visit (lib/prefetch). Thirty seconds: the
 * table is polled every two once the screen is open, so this only has to
 * bridge the first paint, and an older table would show sockets long closed.
 */
export function prefetchConnections(): Promise<void> {
  return prefetch(PREFETCH_KEY, readConnections, 30_000);
}

export function useConnections(reader?: ConnectionsReader): ConnectionsState {
  // An injected reader replaces the host check entirely, rather than being
  // consulted after it. The earlier version defaulted the argument and then
  // short-circuited on `hasTauriHost()`, so in a test — where there is no
  // host — the injected reader was never called and the screen rendered
  // nothing at all. An injection seam that the production path can veto is
  // not a seam; it is a trap that makes every test fail for a reason
  // unrelated to what it is testing.
  const injected = reader !== undefined;

  const [seed] = useState(() =>
    injected ? undefined : peekPrefetched<ConnectionsSnapshot>(PREFETCH_KEY),
  );
  const [snapshot, setSnapshot] = useState<ConnectionsSnapshot | null>(seed?.value ?? null);
  const [pending, setPending] = useState(seed === undefined);
  const [error, setError] = useState<string | null>(null);
  // The seed's own read time, so "updated" never claims the prefetch is newer than it is.
  const [updatedAt, setUpdatedAt] = useState<number | null>(seed?.at ?? null);

  const readerRef = useRef<ConnectionsReader>(reader ?? readConnections);
  readerRef.current = reader ?? readConnections;

  const inFlight = useRef(false);
  const mounted = useRef(true);

  const load = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;

    try {
      const next = await ((injected
        ? undefined
        : takePrefetched<ConnectionsSnapshot>(PREFETCH_KEY, 3000)) ?? readerRef.current());
      if (!mounted.current) return;
      setSnapshot(next);
      setError(null);
      setUpdatedAt(Date.now());
    } catch (cause: unknown) {
      if (!mounted.current) return;
      // The previous snapshot is kept. A transient failure should not blank a
      // list the user is reading; the error line says the data is stale.
      setError(errorMessage(cause));
    } finally {
      inFlight.current = false;
      if (mounted.current) setPending(false);
    }
  }, [injected]);

  const refresh = useCallback(() => {
    void load();
  }, [load]);

  useEffect(() => {
    mounted.current = true;

    if (!injected && !hasTauriHost()) {
      // Resolved immediately rather than left pending. A loading state that
      // can never end makes a broken app look busy — the failure this project
      // has already shipped three times.
      setPending(false);
      setError(NO_HOST);
      return () => {
        mounted.current = false;
      };
    }

    void load();
    const timer = setInterval(() => {
      void load();
    }, POLL_INTERVAL_MS);

    return () => {
      mounted.current = false;
      clearInterval(timer);
    };
  }, [load, injected]);

  return { snapshot, pending, error, updatedAt, refresh };
}
