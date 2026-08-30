/**
 * The dashboard's view of the sampler.
 *
 * Separate from `useProcessSnapshot` in the Processes feature because the two
 * want different things from the same frame: that one needs the full process
 * map and re-renders a virtualised table, this one needs the system totals
 * plus a handful of top rows. Sharing a hook would mean the dashboard
 * re-renders whenever any of ~600 process rows changes, which is every tick.
 *
 * Frames also feed {@link history} from here, and that side effect is the
 * reason collection is wired at boot rather than in this hook — see the note
 * in `history.ts` on why a chart that empties when you navigate away is worse
 * than no chart.
 */

import { useCallback, useEffect, useRef, useSyncExternalStore } from 'react';

import type { Process, SystemMetrics } from '@vitals/protocol';

import { subscribeToMetrics, type Snapshot } from '@/lib/metrics';
import { hasTauriHost } from '../../shell/host';

const NO_PROCESSES: ReadonlyMap<string, Process> = new Map();

export interface SystemSnapshot {
  readonly system: SystemMetrics | null;
  readonly processes: ReadonlyMap<string, Process>;
  readonly seq: number;
  readonly timestampMs: number;
  readonly error: string | null;
  /** True until the first frame lands, so the grid can show skeletons. */
  readonly pending: boolean;
}

export const INITIAL_SYSTEM_SNAPSHOT: SystemSnapshot = {
  system: null,
  processes: NO_PROCESSES,
  seq: 0,
  timestampMs: 0,
  error: null,
  pending: true,
};

export interface SystemSource {
  subscribe(listener: () => void): () => void;
  current(): SystemSnapshot;
}

/** Backed by the Tauri event channel. */
export function createTauriSystemSource(onFrame?: (snapshot: Snapshot) => void): SystemSource {
  let value = INITIAL_SYSTEM_SNAPSHOT;
  const listeners = new Set<() => void>();

  const publish = (next: SystemSnapshot): void => {
    value = next;
    for (const listener of listeners) listener();
  };

  let unlisten: (() => void) | null = null;
  let disposed = false;

  const start = (): void => {
    // Without a host there is no IPC to listen on, and `listen()` dereferences
    // an internals global that does not exist — throwing
    // "Cannot read properties of undefined (reading 'transformCallback')"
    // from inside a promise nobody awaits. Guarding here keeps the dashboard
    // renderable under `vite preview` and in tests, showing skeletons rather
    // than dying on an unhandled rejection.
    if (!hasTauriHost()) return;

    void subscribeToMetrics({
      onSnapshot(snapshot: Snapshot) {
        onFrame?.(snapshot);
        publish({
          system: snapshot.system,
          // A copy, because the reconciler mutates its map in place and
          // `useSyncExternalStore` compares by identity — an unchanged
          // reference means the render this frame exists to cause is skipped.
          processes: new Map(snapshot.processes),
          seq: snapshot.seq,
          timestampMs: snapshot.timestampMs,
          error: null,
          pending: false,
        });
      },
      onError(message: string) {
        // The last good readings are kept. Blanking the dashboard on a
        // transient sampler error reads as "your machine stopped", which is a
        // far more alarming statement than the error itself.
        publish({ ...value, error: message });
      },
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
  };

  return {
    subscribe(listener) {
      const first = listeners.size === 0;
      listeners.add(listener);
      if (first) start();
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) {
          disposed = true;
          unlisten?.();
          unlisten = null;
        }
      };
    },
    current: () => value,
  };
}

export function useSystemSnapshot(source: SystemSource): SystemSnapshot {
  const ref = useRef(source);
  ref.current = source;

  const subscribe = useCallback((listener: () => void) => ref.current.subscribe(listener), []);
  const getSnapshot = useCallback(() => ref.current.current(), []);

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** Driven by hand, for tests and the sampler-less preview. */
export function createManualSystemSource(
  initial: SystemSnapshot = INITIAL_SYSTEM_SNAPSHOT,
): SystemSource & { push(next: Partial<SystemSnapshot>): void } {
  let value = initial;
  const listeners = new Set<() => void>();

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    current: () => value,
    push(next) {
      value = {
        ...value,
        seq: value.seq + 1,
        timestampMs: value.timestampMs + 1000,
        pending: false,
        ...next,
      };
      for (const listener of listeners) listener();
    },
  };
}

/** Subscribes the history collector to the sampler for the window's lifetime. */
export function useHistoryFeed(source: SystemSource): void {
  useEffect(() => source.subscribe(() => undefined), [source]);
}
