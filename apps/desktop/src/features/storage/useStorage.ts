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
import type { CleanupCandidate, ScanProgress, ScanSnapshot, Volume } from './model';
import { errorMessage } from '../../lib/commandError';

/** Reported when there is no Tauri host, so the screen can explain itself. */
export const NO_HOST = 'no-host';

/** How many directory rows the backend returns. A table, not a database. */
export const TOP_N = 200;

/** Emitted by `scan_storage` about ten times a second while it runs. */
export const SCAN_PROGRESS_EVENT = 'vitals://storage/scan-progress';

export interface StorageSource {
  readonly volumes: () => Promise<readonly Volume[]>;
  readonly scan: (path: string) => Promise<ScanSnapshot>;
  readonly cancelScan: () => Promise<void>;
  readonly cleanup: () => Promise<readonly CleanupCandidate[]>;
  readonly cancelCleanup: () => Promise<void>;
  /** Subscribes to scan progress; resolves to the unsubscribe function. */
  readonly onProgress: (listener: (progress: ScanProgress) => void) => Promise<() => void>;
}

/** The real source. Dynamic imports so a browser never evaluates the IPC module. */
export const tauriSource: StorageSource = {
  volumes: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<readonly Volume[]>('get_volumes');
  },
  scan: async (path) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<ScanSnapshot>('scan_storage', { path, topN: TOP_N });
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
  /** Latest progress of the running scan; `null` before the first report. */
  readonly progress: ScanProgress | null;

  readonly candidates: readonly CleanupCandidate[] | null;
  readonly cleanupRunning: boolean;
  readonly cleanupError: string | null;

  scan: (path: string) => void;
  cancelScan: () => void;
  findCleanup: () => void;
  cancelCleanup: () => void;
  refreshVolumes: () => void;
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
  const [progress, setProgress] = useState<ScanProgress | null>(null);

  const [candidates, setCandidates] = useState<readonly CleanupCandidate[] | null>(null);
  const [cleanupRunning, setCleanupRunning] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);

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

  const scan = useCallback((path: string) => {
    // Guarded rather than queued: the backend refuses a second concurrent
    // scan, and the screen could only show one of them anyway.
    if (scanInFlight.current) return;
    scanInFlight.current = true;

    setScanning(true);
    setScanRoot(path);
    setScanError(null);
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
      .scan(path)
      .then((next) => {
        if (!mounted.current) return;
        setSnapshot(next);
      })
      .catch((cause: unknown) => {
        if (!mounted.current) return;
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
  }, []);

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
    progress,
    candidates,
    cleanupRunning,
    cleanupError,
    scan,
    cancelScan,
    findCleanup,
    cancelCleanup,
    refreshVolumes,
  };
}
