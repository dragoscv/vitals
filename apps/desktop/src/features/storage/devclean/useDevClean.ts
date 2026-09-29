/**
 * Runs the developer clean-up scan and the clean-up the user confirms.
 *
 * # Every loading state must be able to end
 *
 * Same contract as `useStorage`: without a Tauri host the default roots
 * resolve at once to an empty list with `noHost` set, rather than a spinner
 * nobody can stop; a failed scan clears `scanning` and keeps the previous
 * result; Stop resolves through the same path as a finished scan, because
 * the backend returns what it found rather than rejecting; and a clean-up
 * that rejects clears `running` and says whether it was a refusal (nothing
 * changed) or a fault.
 *
 * # Only ids cross the boundary
 *
 * `run_dev_cleanup` takes the ids the scan minted and `confirmed: true`.
 * The webview never names a folder to delete.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { errorMessage, isCommandError } from '../../../lib/commandError';
import { hasTauriHost } from '../../../shell/host';
import type { DevCleanProgress, DevCleanReport, DevScan, DevScanProgress } from './model';
import { preselectedIds, withoutIds } from './model';

/** Emitted by `scan_dev_cleanup` while it runs. */
export const DEV_SCAN_PROGRESS_EVENT = 'vitals://storage/dev-scan-progress';

/** Emitted by `run_dev_cleanup` as it moves from item to item. */
export const DEV_CLEAN_PROGRESS_EVENT = 'vitals://storage/dev-clean-progress';

export interface DevCleanSource {
  /** The roots the backend scans when given `null`. */
  readonly defaultRoots: () => Promise<readonly string[]>;
  /** `null` roots: the backend's defaults. Minutes on a large drive. */
  readonly scan: (roots: readonly string[] | null) => Promise<DevScan>;
  /** The running scan resolves with what it found so far. */
  readonly cancelScan: () => Promise<void>;
  readonly run: (
    scanId: number,
    ids: readonly string[],
    confirmed: boolean,
  ) => Promise<DevCleanReport>;
  readonly onScanProgress: (listener: (progress: DevScanProgress) => void) => Promise<() => void>;
  readonly onCleanProgress: (listener: (progress: DevCleanProgress) => void) => Promise<() => void>;
  /** The folder picker; `null` when the user closed it without choosing. */
  readonly pickFolders: () => Promise<readonly string[] | null>;
}

/** The real source. Dynamic imports so a browser never evaluates the IPC module. */
export const tauriDevSource: DevCleanSource = {
  defaultRoots: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string[]>('default_dev_roots');
  },
  scan: async (roots) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DevScan>('scan_dev_cleanup', { roots });
  },
  cancelScan: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<void>('cancel_dev_cleanup_scan');
  },
  run: async (scanId, ids, confirmed) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DevCleanReport>('run_dev_cleanup', { scanId, ids, confirmed });
  },
  onScanProgress: async (listener) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<DevScanProgress>(DEV_SCAN_PROGRESS_EVENT, (event) => {
      listener(event.payload);
    });
  },
  onCleanProgress: async (listener) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<DevCleanProgress>(DEV_CLEAN_PROGRESS_EVENT, (event) => {
      listener(event.payload);
    });
  },
  pickFolders: async () => {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked: unknown = await open({ directory: true, multiple: true });
    if (picked === null || picked === undefined) return null;
    if (typeof picked === 'string') return [picked];
    return Array.isArray(picked) ? picked.filter((p): p is string => typeof p === 'string') : null;
  },
};

/** One confirmed clean-up: its progress while it runs, then its report or error. */
export interface DevRun {
  readonly ids: readonly string[];
  readonly running: boolean;
  readonly progress: DevCleanProgress | null;
  readonly report: DevCleanReport | null;
  readonly error: string | null;
  /** The backend or the UAC prompt refused: nothing was changed. */
  readonly refused: boolean;
}

export interface DevCleanState {
  /** No Tauri host: nothing can be scanned, and the card says so. */
  readonly noHost: boolean;
  readonly roots: readonly string[];
  readonly rootsPending: boolean;
  readonly rootsError: string | null;

  readonly scan: DevScan | null;
  readonly scanning: boolean;
  readonly progress: DevScanProgress | null;
  readonly scanError: string | null;

  readonly selected: ReadonlySet<string>;
  readonly run: DevRun | null;
  /** The last report's line per id, kept on the rows that stayed. */
  readonly lastReport: DevCleanReport | null;

  addFolders: () => void;
  removeRoot: (path: string) => void;
  startScan: () => void;
  cancelScan: () => void;
  toggle: (id: string, on: boolean) => void;
  setMany: (ids: readonly string[], on: boolean) => void;
  /** Runs the clean-up of the given ids; resolves when its report is in. */
  clean: (ids: readonly string[]) => Promise<void>;
  dismissRun: () => void;
}

function sameList(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((value, i) => value === b[i]);
}

export function useDevClean(source?: DevCleanSource): DevCleanState {
  // An injected source replaces the host check rather than sitting behind it,
  // for the reason given in `useStorage`.
  const injected = source !== undefined;
  const sourceRef = useRef<DevCleanSource>(source ?? tauriDevSource);
  sourceRef.current = source ?? tauriDevSource;

  const [noHost, setNoHost] = useState(false);
  const [roots, setRoots] = useState<readonly string[]>([]);
  const [defaults, setDefaults] = useState<readonly string[]>([]);
  const [rootsPending, setRootsPending] = useState(true);
  const [rootsError, setRootsError] = useState<string | null>(null);

  const [scan, setScan] = useState<DevScan | null>(null);
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<DevScanProgress | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);

  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
  const [run, setRun] = useState<DevRun | null>(null);
  const [lastReport, setLastReport] = useState<DevCleanReport | null>(null);

  const mounted = useRef(true);
  const scanInFlight = useRef(false);
  const runInFlight = useRef(false);
  const scanRef = useRef<DevScan | null>(null);
  scanRef.current = scan;
  const rootsRef = useRef({ roots, defaults });
  rootsRef.current = { roots, defaults };

  useEffect(() => {
    mounted.current = true;
    if (!injected && !hasTauriHost()) {
      // Resolved at once: a loading state that cannot end makes a broken app
      // look busy.
      setNoHost(true);
      setRootsPending(false);
      return () => {
        mounted.current = false;
      };
    }
    sourceRef.current
      .defaultRoots()
      .then((next) => {
        if (!mounted.current) return;
        setRoots(next);
        setDefaults(next);
      })
      .catch((cause: unknown) => {
        if (mounted.current) setRootsError(errorMessage(cause));
      })
      .finally(() => {
        if (mounted.current) setRootsPending(false);
      });
    return () => {
      mounted.current = false;
    };
  }, [injected]);

  const addFolders = useCallback(() => {
    void sourceRef.current
      .pickFolders()
      .then((picked) => {
        if (!mounted.current || picked === null) return;
        setRoots((current) => {
          const seen = new Set(current.map((p) => p.toLowerCase()));
          const added = picked.filter((p) => !seen.has(p.toLowerCase()));
          return added.length === 0 ? current : [...current, ...added];
        });
      })
      .catch((cause: unknown) => {
        if (mounted.current) setRootsError(errorMessage(cause));
      });
  }, []);

  const removeRoot = useCallback((path: string) => {
    setRoots((current) => current.filter((p) => p !== path));
  }, []);

  const startScan = useCallback(() => {
    if (scanInFlight.current) return;
    const { roots: chosen, defaults: fallback } = rootsRef.current;
    if (chosen.length === 0) return;
    scanInFlight.current = true;
    setScanning(true);
    setScanError(null);
    setProgress(null);

    // Subscribed before the scan starts, so the first report is not missed.
    const unsubscribe = sourceRef.current
      .onScanProgress((next) => {
        if (mounted.current) setProgress(next);
      })
      .catch(() => () => undefined);

    // `null` asks for the backend's defaults, so an unchanged list stays
    // whatever the backend decides the defaults are, not a snapshot of them.
    const argument = sameList(chosen, fallback) ? null : chosen;
    void sourceRef.current
      .scan(argument)
      .then((next) => {
        if (!mounted.current) return;
        setScan(next);
        setSelected(preselectedIds(next));
        setLastReport(null);
      })
      .catch((cause: unknown) => {
        // The previous result stays: a failed rescan must not blank it.
        if (mounted.current) setScanError(errorMessage(cause));
      })
      .finally(() => {
        scanInFlight.current = false;
        void unsubscribe.then((stop) => {
          stop();
        });
        if (mounted.current) {
          setScanning(false);
          setProgress(null);
        }
      });
  }, []);

  const cancelScan = useCallback(() => {
    // `scanning` stays true: the scan is still running and resolves with a
    // partial result through the normal path.
    void sourceRef.current.cancelScan().catch((cause: unknown) => {
      if (mounted.current) setScanError(errorMessage(cause));
    });
  }, []);

  const toggle = useCallback((id: string, on: boolean) => {
    setSelected((current) => {
      if (current.has(id) === on) return current;
      const next = new Set(current);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  }, []);

  const setMany = useCallback((ids: readonly string[], on: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  }, []);

  const clean = useCallback(async (ids: readonly string[]) => {
    const current = scanRef.current;
    if (runInFlight.current || current === null || ids.length === 0) return;
    runInFlight.current = true;
    setRun({ ids, running: true, progress: null, report: null, error: null, refused: false });

    const unsubscribe = sourceRef.current
      .onCleanProgress((next) => {
        if (!mounted.current) return;
        setRun((r) => (r?.running === true ? { ...r, progress: next } : r));
      })
      .catch(() => () => undefined);

    try {
      const report = await sourceRef.current.run(current.scanId, ids, true);
      if (!mounted.current) return;
      setRun({ ids, running: false, progress: null, report, error: null, refused: false });
      setLastReport(report);
      // What was removed leaves the list; everything else stays, with its
      // report line, so the user can read why and try again.
      const gone = new Set(report.items.filter((i) => i.outcome === 'done').map((i) => i.id));
      setScan((s) => (s === null || s.scanId !== current.scanId ? s : withoutIds(s, gone)));
      setSelected((s) => new Set([...s].filter((id) => !gone.has(id))));
    } catch (cause: unknown) {
      if (!mounted.current) return;
      setRun({
        ids,
        running: false,
        progress: null,
        report: null,
        error: errorMessage(cause),
        refused: isCommandError(cause) && cause.kind === 'refused',
      });
    } finally {
      runInFlight.current = false;
      void unsubscribe.then((stop) => {
        stop();
      });
    }
  }, []);

  const dismissRun = useCallback(() => {
    if (!runInFlight.current) setRun(null);
  }, []);

  return {
    noHost,
    roots,
    rootsPending,
    rootsError,
    scan,
    scanning,
    progress,
    scanError,
    selected,
    run,
    lastReport,
    addFolders,
    removeRoot,
    startScan,
    cancelScan,
    toggle,
    setMany,
    clean,
    dismissRun,
  };
}
