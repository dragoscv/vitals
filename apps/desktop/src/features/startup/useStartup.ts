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
  /** True until the first read settles, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly error: string | null;
  refresh: () => void;
}

export function useStartup(withServiceConfig: boolean, reader?: StartupReader): StartupState {
  const [snapshot, setSnapshot] = useState<StartupSnapshot | null>(null);
  const [pending, setPending] = useState(true);
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

  const load = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;

    try {
      const next = await readerRef.current(withServiceConfig);
      if (!mounted.current) return;
      setSnapshot(next);
      setError(null);
    } catch (cause: unknown) {
      if (!mounted.current) return;
      // The previous snapshot survives. A failed refresh should not blank a
      // list the user is reading; the error line says the data is stale.
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      if (mounted.current) setPending(false);
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

  return { snapshot, pending, error, refresh };
}
