/**
 * Fetches the startup inventory once, on demand.
 *
 * # Not polled, unlike connections
 *
 * Startup entries and service configuration change when the user installs
 * something or runs `sc config` — not on a timer. Polling would re-walk the
 * registry and re-open the SCM every few seconds to produce a byte-identical
 * result, which on ~370 services is real work for no information.
 *
 * So: one read when the screen mounts, plus a manual refresh. The refresh
 * button exists because the data CAN change while the screen is open — that
 * is the case a user hits after disabling something — and a stale list with
 * no way to update it is worse than no list.
 *
 * # `withServiceConfig` is a real cost, not a flag for tidiness
 *
 * Reading a service's start type and binary path opens the SCM once per
 * service. The Startup tab does not need either, so it asks without; the
 * Services tab does, so it asks with. Requesting it unconditionally would make
 * the cheaper screen pay for the expensive one.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { hasTauriHost } from '../../shell/host';
import type { StartupSnapshot } from './model';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

export type StartupReader = (withServiceConfig: boolean) => Promise<StartupSnapshot>;

/** The real reader. Dynamic import so a browser never evaluates the IPC module. */
export async function readStartup(withServiceConfig: boolean): Promise<StartupSnapshot> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<StartupSnapshot>('get_startup', { withServiceConfig });
}

export interface StartupState {
  readonly snapshot: StartupSnapshot | null;
  /**
   * True only while there is nothing to show yet.
   *
   * Distinct from [`refreshing`] on purpose. Screens stay mounted between
   * navigations now, so returning to one re-reads while a perfectly good
   * list is already on screen. Treating that as "pending" replaced the list
   * with a skeleton every time the user came back — the exact flicker this
   * hook is meant to avoid.
   */
  readonly pending: boolean;
  /**
   * True while a read is in flight that is *not* the first one.
   *
   * The screen shows the existing data and a quiet indicator rather than
   * tearing the list down.
   */
  readonly refreshing: boolean;
  readonly error: string | null;
  refresh: () => void;
}

export function useStartup(withServiceConfig: boolean, reader?: StartupReader): StartupState {
  const [snapshot, setSnapshot] = useState<StartupSnapshot | null>(null);
  const [pending, setPending] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // An injected reader replaces the host check rather than being consulted
  // after it. A seam the production path can veto is not a seam — it silently
  // never runs in tests, and every assertion then fails for an unrelated
  // reason. That mistake has already been made once in this codebase.
  const injected = reader !== undefined;
  const readerRef = useRef<StartupReader>(reader ?? readStartup);
  readerRef.current = reader ?? readStartup;

  const inFlight = useRef(false);
  const mounted = useRef(true);

  // Survives the effect teardown that `<Activity mode="hidden">` performs,
  // which is what lets a return visit know it already has something to show.
  const hasData = useRef(false);

  const load = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;

    // `hasData` is read from the ref rather than the state value so this
    // does not need `snapshot` in its dependency list — which would rebuild
    // `load`, retrigger the effect below, and refetch in a loop.
    if (hasData.current) setRefreshing(true);

    try {
      const next = await readerRef.current(withServiceConfig);
      if (!mounted.current) return;
      setSnapshot(next);
      hasData.current = true;
      setError(null);
    } catch (cause: unknown) {
      if (!mounted.current) return;
      // The previous snapshot survives. A failed refresh should not blank a
      // list the user is reading; the error line says the data is stale.
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      if (mounted.current) {
        setPending(false);
        setRefreshing(false);
      }
    }
  }, [withServiceConfig]);

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
  }, [load, injected]);

  // `pending` needs no guard against the refresh case: it is set to false
  // once and never back to true, so after the first load it stays false for
  // the life of the hook. The skeleton-on-every-return that motivated this
  // work came from the screen being UNMOUNTED, which reset this to its
  // `useState(true)` initial value; `<Activity>` keeps it mounted and the
  // state survives. Verified by reintroducing a gate here and watching the
  // tests pass unchanged — it was solving a problem that did not exist.
  return { snapshot, pending, refreshing, error, refresh };
}
