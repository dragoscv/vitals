/**
 * Fetches the user sessions, on demand.
 *
 * # Not polled at 1 Hz, unlike connections
 *
 * Session state changes when a user disconnects or reconnects, not on a
 * sub-second interval. Polling at 1 Hz would re-enumerate sessions every
 * second to produce a byte-identical result most of the time — real work for
 * no information.
 *
 * So: one read when the screen mounts, plus a manual refresh. The refresh
 * button exists because the data CAN change while the screen is open — that is
 * the case a user hits after someone else logs on — and a stale list with no
 * way to update it is worse than no list.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { hasTauriHost } from '../../shell/host';
import type { UsersSnapshot } from './model';
import { errorMessage } from '../../lib/commandError';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

export type UsersReader = () => Promise<UsersSnapshot>;

/** The real reader. Dynamic import so a browser never evaluates the IPC module. */
export async function readUsers(): Promise<UsersSnapshot> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<UsersSnapshot>('get_users');
}

export interface UsersState {
  readonly snapshot: UsersSnapshot | null;
  /** True until the first read settles, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly error: string | null;
  refresh: () => void;
}

export function useUsers(reader?: UsersReader): UsersState {
  const [snapshot, setSnapshot] = useState<UsersSnapshot | null>(null);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // An injected reader replaces the host check rather than being consulted
  // after it. A seam the production path can veto is not a seam — it silently
  // never runs in tests, and every assertion then fails for an unrelated
  // reason. That mistake has already been made once in this codebase.
  const injected = reader !== undefined;
  const readerRef = useRef<UsersReader>(reader ?? readUsers);
  readerRef.current = reader ?? readUsers;

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
      // The previous snapshot survives. A failed refresh should not blank a
      // list the user is reading; the error line says the data is stale.
      setError(errorMessage(cause));
    } finally {
      inFlight.current = false;
      if (mounted.current) setPending(false);
    }
  }, []);

  const refresh = useCallback(() => {
    void load();
  }, [load]);

  useEffect(() => {
    mounted.current = true;

    if (!injected && !hasTauriHost()) {
      // Resolved at once rather than left pending: a loading state that cannot
      // end makes a broken app look busy, which this project has shipped three
      // times already.
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
  }, [injected, load]);

  return { snapshot, pending, error, refresh };
}
