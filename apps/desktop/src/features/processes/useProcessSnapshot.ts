/**
 * Subscription to the sampler, kept out of the render path.
 *
 * Frames arrive at 1 Hz carrying ~600 processes. Storing the whole snapshot
 * in React state and letting every consumer read it would re-render the
 * entire tree each tick; instead the snapshot lives in a ref and subscribers
 * are notified through `useSyncExternalStore`, so only the components that
 * actually read it re-render.
 */

import { useCallback, useEffect, useRef, useSyncExternalStore } from 'react';

import type { Process } from '@vitals/protocol';

import { subscribeToMetrics, type Snapshot } from '@/lib/metrics';
import { hasTauriHost } from '../../shell/host';

const EMPTY_PROCESSES: ReadonlyMap<string, Process> = new Map();

export interface ProcessSnapshot {
  readonly processes: ReadonlyMap<string, Process>;
  readonly seq: number;
  readonly timestampMs: number;
  /** Set when the sampler reported a failure; the table keeps its last rows. */
  readonly error: string | null;
  /** True until the first frame lands, so the table can show skeletons. */
  readonly pending: boolean;
}

const INITIAL: ProcessSnapshot = {
  processes: EMPTY_PROCESSES,
  seq: 0,
  timestampMs: 0,
  error: null,
  pending: true,
};

export interface SnapshotSource {
  subscribe(listener: (snapshot: ProcessSnapshot) => void): () => void;
  current(): ProcessSnapshot;
}

/** Creates a source backed by the Tauri event channel. */
export function createTauriSnapshotSource(): SnapshotSource {
  let value = INITIAL;
  const listeners = new Set<(snapshot: ProcessSnapshot) => void>();

  const publish = (next: ProcessSnapshot): void => {
    value = next;
    for (const listener of listeners) listener(next);
  };

  let unlisten: (() => void) | null = null;
  let disposed = false;

  const start = (): void => {
    // No host means no IPC: `listen()` reaches into an internals global that
    // is undefined and throws "Cannot read properties of undefined (reading
    // 'transformCallback')" inside a floating promise. The table stays on its
    // skeletons instead, which is correct under `vite preview` and in tests.
    if (!hasTauriHost()) return;

    void subscribeToMetrics({
      onSnapshot(snapshot: Snapshot) {
        publish({
          // The map is mutated in place by the reconciler, so a new object
          // identity is created here for each frame — otherwise
          // `useSyncExternalStore` sees an unchanged snapshot and skips the
          // render that the frame exists to trigger.
          processes: new Map(snapshot.processes),
          seq: snapshot.seq,
          timestampMs: snapshot.timestampMs,
          error: null,
          pending: false,
        });
      },
      onError(message: string) {
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

/** Reads the latest snapshot, re-rendering only the calling component. */
export function useProcessSnapshot(source: SnapshotSource): ProcessSnapshot {
  const ref = useRef(source);
  ref.current = source;

  const subscribe = useCallback((listener: () => void) => ref.current.subscribe(listener), []);
  const getSnapshot = useCallback(() => ref.current.current(), []);

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/**
 * A source that can be driven by hand, for tests and for the sampler-less
 * developer preview.
 */
export function createManualSnapshotSource(initial: ProcessSnapshot = INITIAL): SnapshotSource & {
  push(snapshot: Partial<ProcessSnapshot> & { processes: ReadonlyMap<string, Process> }): void;
} {
  let value = initial;
  const listeners = new Set<(snapshot: ProcessSnapshot) => void>();

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    current: () => value,
    push(next) {
      value = {
        seq: value.seq + 1,
        timestampMs: value.timestampMs + 1000,
        error: null,
        pending: false,
        ...next,
      };
      for (const listener of listeners) listener(value);
    },
  };
}

/** Kept exported so tests can assert against the documented initial state. */
export const INITIAL_SNAPSHOT = INITIAL;

/**
 * Keeps a stable reference to the previous value, for effects that must not
 * re-run when only the snapshot's identity changed.
 */
export function useLatest<T>(value: T): { readonly current: T } {
  const ref = useRef(value);
  useEffect(() => {
    ref.current = value;
  });
  return ref;
}
