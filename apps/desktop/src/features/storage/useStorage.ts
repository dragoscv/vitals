/**
 * Reads volumes, runs directory scans, and measures cleanup candidates.
 *
 * Nothing here is polled. Volumes change when media is attached; a scan is
 * an explicit operation the user starts, which reports its own progress as
 * events while it runs. Re-running either on a timer would burn a whole
 * disk's worth of IO to produce the same answer.
 *
 * # Every loading state must be able to end
 *
 * This project has shipped an unbounded spinner three times. So: the volume
 * read resolves immediately with `NO_HOST` when there is no Tauri host rather
 * than sitting `pending` forever; a failed scan clears `scanning` *and* keeps
 * the previous snapshot, so the user sees the last good data with an error
 * line above it instead of skeletons on top of a message; and cancelling
 * resolves through the same path as a completed scan, because the backend
 * returns a partial result rather than rejecting.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { peekPrefetched, prefetch, takePrefetched } from '../../lib/prefetch';
import { hasTauriHost } from '../../shell/host';
import type {
  BasketItem,
  CleanupCandidate,
  Holder,
  MapCell,
  MapShape,
  RecycleReport,
  ScanMode,
  ScanProgress,
  ScanSnapshot,
  StorageListing,
  Volume,
  WindowsCleanupProgress,
  WindowsCleanupReport,
} from './model';
import { addToBasket, removeFromBasket } from './model';
import { errorMessage, isCommandError } from '../../lib/commandError';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

/** How many directory rows the backend returns. A table, not a database. */
export const TOP_N = 200;

/** Emitted by `scan_storage` about ten times a second while it runs. */
export const SCAN_PROGRESS_EVENT = 'vitals://storage/scan-progress';

/** Emitted by `run_windows_cleanup` about twice a second while a tool runs. */
export const CLEANUP_PROGRESS_EVENT = 'vitals://storage/cleanup-progress';

export interface StorageSource {
  readonly volumes: () => Promise<readonly Volume[]>;
  readonly scan: (path: string, mode: ScanMode) => Promise<ScanSnapshot>;
  readonly cancelScan: () => Promise<void>;
  readonly cleanup: () => Promise<readonly CleanupCandidate[]>;
  readonly cancelCleanup: () => Promise<void>;
  /** Subscribes to scan progress; resolves to the unsubscribe function. */
  readonly onProgress: (listener: (progress: ScanProgress) => void) => Promise<() => void>;
  /** Opens one folder of the kept scan. Rejects once the scan is released. */
  readonly children: (scanId: number, node: number) => Promise<StorageListing>;
  /** Lays out the map of one folder; `aspect` is width over height. */
  readonly map: (
    scanId: number,
    node: number,
    shape: MapShape,
    aspect: number,
  ) => Promise<readonly MapCell[]>;
  /** Opens File Explorer with the item selected. */
  readonly reveal: (path: string) => Promise<void>;
  /**
   * Sends confirmed items to the Recycle Bin; one report line per item.
   * `scanId` lets the backend take them out of the kept scan.
   */
  readonly recycle: (paths: readonly string[], scanId: number | null) => Promise<RecycleReport>;
  /** Which programs have a file, or a file in a folder, open. */
  readonly holders: (path: string) => Promise<readonly Holder[]>;
  /**
   * Frees one Windows-managed location with Windows' own tool, after one
   * UAC prompt. `confirmed` is required for a tool that cannot be undone.
   */
  readonly windowsCleanup: (path: string, confirmed: boolean) => Promise<WindowsCleanupReport>;
  readonly onCleanupProgress: (
    listener: (progress: WindowsCleanupProgress) => void,
  ) => Promise<() => void>;
}

/** The real source. Dynamic imports so a browser never evaluates the IPC module. */
export const tauriSource: StorageSource = {
  volumes: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<readonly Volume[]>('get_volumes');
  },
  scan: async (path, mode) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<ScanSnapshot>('scan_storage', { path, topN: TOP_N, mode });
  },
  cancelScan: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<void>('cancel_storage_scan');
  },
  cleanup: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<readonly CleanupCandidate[]>('find_cleanup_candidates');
  },
  cancelCleanup: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<void>('cancel_cleanup_search');
  },
  onProgress: async (listener) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<ScanProgress>(SCAN_PROGRESS_EVENT, (event) => {
      listener(event.payload);
    });
  },
  children: async (scanId, node) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<StorageListing>('get_storage_children', { scanId, node });
  },
  map: async (scanId, node, shape, aspect) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<readonly MapCell[]>('get_storage_map', { scanId, node, shape, aspect });
  },
  reveal: async (path) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<void>('open_file_location', { path });
  },
  recycle: async (paths, scanId) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<RecycleReport>('recycle_storage_items', { paths, scanId });
  },
  holders: async (path) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<readonly Holder[]>('get_file_holders', { path });
  },
  windowsCleanup: async (path, confirmed) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<WindowsCleanupReport>('run_windows_cleanup', { path, confirmed });
  },
  onCleanupProgress: async (listener) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<WindowsCleanupProgress>(CLEANUP_PROGRESS_EVENT, (event) => {
      listener(event.payload);
    });
  },
};

const PREFETCH_KEY = 'storage:volumes';

/**
 * Background read of the drive list for the first visit (lib/prefetch).
 * Only the volumes: a scan is minutes of disk walking the user starts.
 */
export function prefetchStorage(): Promise<void> {
  return prefetch(PREFETCH_KEY, () => tauriSource.volumes(), 60_000);
}

export interface StorageState {
  readonly volumes: readonly Volume[];
  /** True until the volume read settles, whether it succeeds or fails. */
  readonly pending: boolean;
  readonly volumeError: string | null;

  readonly snapshot: ScanSnapshot | null;
  readonly scanning: boolean;
  readonly scanRoot: string | null;
  readonly scanError: string | null;
  /**
   * The UAC prompt for a Turbo scan was dismissed: a decision, not a fault,
   * so the screen says so without the error colour.
   */
  readonly scanDeclined: boolean;
  /** Latest progress of the running scan; `null` before the first report. */
  readonly progress: ScanProgress | null;

  readonly candidates: readonly CleanupCandidate[] | null;
  readonly cleanupRunning: boolean;
  readonly cleanupError: string | null;

  /** Items the user is reviewing for the Recycle Bin. */
  readonly basket: readonly BasketItem[];
  readonly recycling: boolean;
  /** The last confirmed basket's per-item result. */
  readonly report: RecycleReport | null;
  readonly recycleError: string | null;
  /**
   * Bumped when the kept scan changed under the explorer (a recycle took
   * items out of it), so it reloads the folder in view without a rescan.
   */
  readonly revision: number;

  /** The Windows cleanup in flight or last finished; `null` before any. */
  readonly windowsRun: WindowsRun | null;

  scan: (path: string, mode?: ScanMode) => void;
  cancelScan: () => void;
  findCleanup: () => void;
  cancelCleanup: () => void;
  refreshVolumes: () => void;
  addToBasket: (item: BasketItem) => void;
  removeFromBasket: (path: string) => void;
  clearBasket: () => void;
  /** Sends the whole basket; resolves when the report is in. */
  recycleBasket: () => Promise<void>;
  dismissReport: () => void;
  /** Runs the candidate's Windows tool; resolves when its report is in. */
  runWindowsCleanup: (path: string, confirmed: boolean) => Promise<void>;
  dismissWindowsRun: () => void;
}

/** One Windows cleanup: its progress while it runs, then its report or error. */
export interface WindowsRun {
  readonly path: string;
  readonly running: boolean;
  readonly progress: WindowsCleanupProgress | null;
  readonly report: WindowsCleanupReport | null;
  readonly error: string | null;
  /** The UAC prompt was dismissed or the action refused: a decision, not a fault. */
  readonly declined: boolean;
}

export function useStorage(source?: StorageSource): StorageState {
  // An injected source replaces the host check rather than sitting behind it.
  // A seam the production path can veto silently never runs in tests, and
  // every assertion then fails for an unrelated reason.
  const injected = source !== undefined;

  const [seed] = useState(() =>
    injected ? undefined : peekPrefetched<readonly Volume[]>(PREFETCH_KEY),
  );
  const [volumes, setVolumes] = useState<readonly Volume[]>(seed?.value ?? []);
  const [pending, setPending] = useState(seed === undefined);
  const [volumeError, setVolumeError] = useState<string | null>(null);

  const [snapshot, setSnapshot] = useState<ScanSnapshot | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanRoot, setScanRoot] = useState<string | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanDeclined, setScanDeclined] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);

  const [candidates, setCandidates] = useState<readonly CleanupCandidate[] | null>(null);
  const [cleanupRunning, setCleanupRunning] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);

  const [basket, setBasket] = useState<readonly BasketItem[]>([]);
  const [recycling, setRecycling] = useState(false);
  const [report, setReport] = useState<RecycleReport | null>(null);
  const [recycleError, setRecycleError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [windowsRun, setWindowsRun] = useState<WindowsRun | null>(null);
  const windowsInFlight = useRef(false);
  const recycleInFlight = useRef(false);
  const snapshotRef = useRef<ScanSnapshot | null>(null);
  snapshotRef.current = snapshot;
  const basketRef = useRef(basket);
  basketRef.current = basket;

  const sourceRef = useRef<StorageSource>(source ?? tauriSource);
  sourceRef.current = source ?? tauriSource;

  const mounted = useRef(true);
  const scanInFlight = useRef(false);
  const cleanupInFlight = useRef(false);

  const loadVolumes = useCallback(async () => {
    try {
      const next = await ((injected
        ? undefined
        : takePrefetched<readonly Volume[]>(PREFETCH_KEY)) ?? sourceRef.current.volumes());
      if (!mounted.current) return;
      setVolumes(next);
      setVolumeError(null);
    } catch (cause: unknown) {
      if (!mounted.current) return;
      // The previous list survives a failed refresh; the error line says so.
      setVolumeError(errorMessage(cause));
    } finally {
      if (mounted.current) setPending(false);
    }
  }, [injected]);

  const scan = useCallback(
    (path: string, mode: ScanMode = 'auto') => {
      // Guarded rather than queued: the backend refuses a second concurrent
      // scan, and the screen could only show one of them anyway.
      if (scanInFlight.current) return;
      scanInFlight.current = true;

      setScanning(true);
      setScanRoot(path);
      setScanError(null);
      setScanDeclined(false);
      setProgress(null);

      // Subscribed before the scan starts, so the first report is not missed.
      // Reports for another root (a scan started from a second window) are
      // not this one's progress.
      const unsubscribe = sourceRef.current
        .onProgress((next) => {
          if (mounted.current && next.root === path) setProgress(next);
        })
        .catch(() => () => undefined);

      void sourceRef.current
        .scan(path, mode)
        .then((next) => {
          if (!mounted.current) return;
          setSnapshot(next);
          // A scan may have saved an index, which changes the drive card's
          // `indexed` flag and the buttons offered for the next scan.
          void loadVolumes();
        })
        .catch((cause: unknown) => {
          if (!mounted.current) return;
          if (isCommandError(cause) && cause.kind === 'refused') {
            // The user dismissed the UAC prompt. Nothing was scanned, and that
            // is their choice rather than a failure to report in red.
            setScanDeclined(true);
            return;
          }
          // The previous snapshot is deliberately left in place. A failed
          // rescan should not blank a result the user was reading.
          setScanError(errorMessage(cause));
        })
        .finally(() => {
          scanInFlight.current = false;
          void unsubscribe.then((stop) => {
            stop();
          });
          // Cleared on both paths. An error that sets a message without
          // clearing `scanning` renders skeletons on top of the message.
          if (mounted.current) {
            setScanning(false);
            setProgress(null);
          }
        });
    },
    [loadVolumes],
  );

  const cancelScan = useCallback(() => {
    // Deliberately does not clear `scanning`: the scan is still running and
    // will resolve with a partial, honestly-flagged result. Flipping the flag
    // here would show a complete-looking screen while the walk continues.
    void sourceRef.current.cancelScan().catch((cause: unknown) => {
      if (!mounted.current) return;
      setScanError(errorMessage(cause));
    });
  }, []);

  const findCleanup = useCallback(() => {
    if (cleanupInFlight.current) return;
    cleanupInFlight.current = true;

    setCleanupRunning(true);
    setCleanupError(null);

    void sourceRef.current
      .cleanup()
      .then((next) => {
        if (!mounted.current) return;
        setCandidates(next);
      })
      .catch((cause: unknown) => {
        if (!mounted.current) return;
        setCleanupError(errorMessage(cause));
      })
      .finally(() => {
        cleanupInFlight.current = false;
        if (mounted.current) setCleanupRunning(false);
      });
  }, []);

  const cancelCleanup = useCallback(() => {
    // Same contract as `cancelScan`: the search resolves with what it has.
    void sourceRef.current.cancelCleanup().catch((cause: unknown) => {
      if (!mounted.current) return;
      setCleanupError(errorMessage(cause));
    });
  }, []);

  const refreshVolumes = useCallback(() => {
    void loadVolumes();
  }, [loadVolumes]);

  const add = useCallback((item: BasketItem) => {
    setBasket((current) => addToBasket(current, item));
  }, []);
  const remove = useCallback((path: string) => {
    setBasket((current) => removeFromBasket(current, path));
  }, []);
  const clearBasket = useCallback(() => {
    setBasket([]);
  }, []);
  const dismissReport = useCallback(() => {
    setReport(null);
    setRecycleError(null);
  }, []);

  const recycleBasket = useCallback(async () => {
    const items = basketRef.current;
    if (recycleInFlight.current || items.length === 0) return;
    recycleInFlight.current = true;
    setRecycling(true);
    setRecycleError(null);
    setReport(null);
    try {
      const scanId = snapshotRef.current?.scanId ?? null;
      const next = await sourceRef.current.recycle(
        items.map((item) => item.path),
        scanId,
      );
      if (!mounted.current) return;
      setReport(next);
      // What went (or was already gone) leaves the basket; what could not be
      // moved stays, so the user can read why and try again.
      const settled = new Set(
        next.items
          .filter((item) => item.outcome === 'recycled' || item.outcome === 'missing')
          .map((item) => item.path.toLowerCase()),
      );
      setBasket((current) => current.filter((item) => !settled.has(item.path.toLowerCase())));
      const totals = next.scan;
      if (totals !== null) {
        setSnapshot((current) =>
          current === null || current.scanId !== scanId ? current : { ...current, ...totals },
        );
        setRevision((r) => r + 1);
      }
    } catch (cause: unknown) {
      if (mounted.current) setRecycleError(errorMessage(cause));
    } finally {
      recycleInFlight.current = false;
      if (mounted.current) setRecycling(false);
    }
  }, []);

  const runWindowsCleanup = useCallback(async (path: string, confirmed: boolean) => {
    // One at a time: the backend refuses a second, and each is its own UAC
    // prompt the user should see answered before the next.
    if (windowsInFlight.current) return;
    windowsInFlight.current = true;
    setWindowsRun({
      path,
      running: true,
      progress: null,
      report: null,
      error: null,
      declined: false,
    });

    const unsubscribe = sourceRef.current
      .onCleanupProgress((next) => {
        if (!mounted.current || next.path !== path) return;
        setWindowsRun((run) =>
          run?.path === path && run.running ? { ...run, progress: next } : run,
        );
      })
      .catch(() => () => undefined);

    try {
      const report = await sourceRef.current.windowsCleanup(path, confirmed);
      if (!mounted.current) return;
      setWindowsRun({ path, running: false, progress: null, report, error: null, declined: false });
      // The row shows what is there now, as measured after the tool exited.
      if (report.locationAfter !== null) {
        const after = report.locationAfter;
        setCandidates((current) =>
          current === null
            ? current
            : current.map((c) => (c.path === path ? { ...c, size: after } : c)),
        );
      }
    } catch (cause: unknown) {
      if (!mounted.current) return;
      setWindowsRun({
        path,
        running: false,
        progress: null,
        report: null,
        error: errorMessage(cause),
        declined: isCommandError(cause) && cause.kind === 'refused',
      });
    } finally {
      windowsInFlight.current = false;
      void unsubscribe.then((stop) => {
        stop();
      });
    }
  }, []);

  const dismissWindowsRun = useCallback(() => {
    if (!windowsInFlight.current) setWindowsRun(null);
  }, []);

  useEffect(() => {
    mounted.current = true;

    if (!injected && !hasTauriHost()) {
      // Resolved at once: a loading state that cannot end makes a broken app
      // look busy, which this project has shipped three times already.
      setPending(false);
      setVolumeError(NO_HOST);
      return () => {
        mounted.current = false;
      };
    }

    void loadVolumes();

    return () => {
      mounted.current = false;
    };
  }, [loadVolumes, injected]);

  return {
    volumes,
    pending,
    volumeError,
    snapshot,
    scanning,
    scanRoot,
    scanError,
    scanDeclined,
    progress,
    candidates,
    cleanupRunning,
    cleanupError,
    basket,
    recycling,
    report,
    recycleError,
    revision,
    windowsRun,
    scan,
    cancelScan,
    findCleanup,
    cancelCleanup,
    refreshVolumes,
    addToBasket: add,
    removeFromBasket: remove,
    clearBasket,
    recycleBasket,
    dismissReport,
    runWindowsCleanup,
    dismissWindowsRun,
  };
}
