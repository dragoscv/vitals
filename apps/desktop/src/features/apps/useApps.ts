/**
 * Fetches the installed-application list, and launches uninstallers.
 *
 * Not polled. Installed programs change when the user installs or removes
 * something, and re-walking four registry views on a timer would produce a
 * byte-identical result at real cost. One read on mount, plus a refresh —
 * which is genuinely needed here, because an uninstall the user just ran is
 * exactly the case where the list goes stale while they are looking at it.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { peekPrefetched, prefetch, takePrefetched } from '../../lib/prefetch';
import { hasTauriHost } from '../../shell/host';
import type { AppsSnapshot, InstalledApp } from './model';
import { errorMessage } from '../../lib/commandError';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

export type AppsReader = () => Promise<AppsSnapshot>;
/**
 * Launches an entry's own uninstaller. Takes the entry, not its command
 * line: the backend re-reads `UninstallString` from the registry, so the
 * webview cannot ask it to run anything else.
 */
export type Uninstaller = (app: Pick<InstalledApp, 'keyName' | 'source'>) => Promise<void>;

/** The real reader. Dynamic import so a browser never evaluates the IPC module. */
export async function readApps(): Promise<AppsSnapshot> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AppsSnapshot>('get_installed_apps');
}

export async function runUninstaller(app: Pick<InstalledApp, 'keyName' | 'source'>): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<void>('uninstall_app', { keyName: app.keyName, source: app.source });
}

const PREFETCH_KEY = 'apps';

/**
 * Reads the list in the background so the first visit has it (lib/prefetch).
 *
 * Ten minutes: the list changes when something is installed or removed, and
 * the screen re-reads on every visit anyway — this only decides whether the
 * first paint may show the background read while that happens.
 */
export function prefetchApps(): Promise<void> {
  return prefetch(PREFETCH_KEY, readApps, 10 * 60_000);
}

export interface AppsState {
  readonly snapshot: AppsSnapshot | null;
  /** True until the first read settles, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly error: string | null;
  refresh: () => void;
}

export function useApps(reader?: AppsReader): AppsState {
  // An injected reader replaces the host check rather than sitting behind it.
  // A seam the production path can veto silently never runs in tests, and
  // every assertion then fails for an unrelated reason.
  const injected = reader !== undefined;

  // Seeded from the background read, so a first visit renders the list
  // rather than a skeleton. Never for an injected reader: a test's data must
  // come from the test.
  const [seed] = useState(() =>
    injected ? undefined : peekPrefetched<AppsSnapshot>(PREFETCH_KEY),
  );
  const [snapshot, setSnapshot] = useState<AppsSnapshot | null>(seed?.value ?? null);
  const [pending, setPending] = useState(seed === undefined);
  const [error, setError] = useState<string | null>(null);

  const readerRef = useRef<AppsReader>(reader ?? readApps);
  readerRef.current = reader ?? readApps;

  const inFlight = useRef(false);
  const mounted = useRef(true);

  const load = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;

    try {
      // The first load reuses the background read (settled or in flight)
      // instead of repeating it; every later load is a real read.
      const next = await ((injected ? undefined : takePrefetched<AppsSnapshot>(PREFETCH_KEY)) ??
        readerRef.current());
      if (!mounted.current) return;
      setSnapshot(next);
      setError(null);
    } catch (cause: unknown) {
      if (!mounted.current) return;
      // The previous list survives a failed refresh; the error line says so.
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
      // Resolved at once: a loading state that cannot end makes a broken app
      // look busy, which this project has shipped three times already.
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
