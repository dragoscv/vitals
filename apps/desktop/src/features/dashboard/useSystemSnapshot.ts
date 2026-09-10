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

/**
 * How long to wait for the first frame before saying something is wrong.
 *
 * `pending` is the dashboard's skeleton state, and a skeleton that can never
 * resolve is the same failure as the splash screen that pulsed forever: the
 * app looks busy rather than broken, so nobody reports it and there is
 * nothing on screen to diagnose from. Observed live in the browser, where
 * there is no Tauri host and therefore no frame will EVER arrive — the grid
 * sat on skeletons indefinitely.
 *
 * Five seconds is well past the measured first-frame time (the sampler ticks
 * at 1 Hz and the first sample lands within ~1 s of window creation), so this
 * cannot fire on a merely slow start.
 */
export const FIRST_FRAME_TIMEOUT_MS = 5000;

/** Reported when no frame ever arrives; matched by the screen to explain why. */
export const NO_SAMPLER = 'no-sampler';

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
  let firstFrameTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * Leaves `pending` behind for good, with an error instead of data.
   *
   * Clearing `pending` matters as much as setting `error`: the screen keys its
   * skeletons off `pending`, so an error alone would leave them on screen
   * underneath the explanation.
   */
  const giveUp = (reason: string): void => {
    if (!value.pending) return;
    publish({ ...value, pending: false, error: reason });
  };

  const start = (): void => {
    // A restart, not a first start. `<Activity>` hides a route by unmounting
    // its effects and remounting them when it becomes visible again, and
    // StrictMode does the same on purpose in development. Leaving `disposed`
    // set from the previous stop made the *new* subscription throw itself
    // away the moment it resolved — frames kept arriving over IPC and the
    // dashboard sat on "No readings are arriving" for good.
    disposed = false;
    // Without a host there is no IPC to listen on, and `listen()` dereferences
    // an internals global that does not exist — throwing
    // "Cannot read properties of undefined (reading 'transformCallback')"
    // from inside a promise nobody awaits.
    //
    // Resolved immediately rather than left pending: in a browser the answer
    // is already known, and making the user wait five seconds to be told
    // something we could say at once is just a slower way to be unhelpful.
    if (!hasTauriHost()) {
      giveUp(NO_SAMPLER);
      return;
    }

    // The host exists but may still never deliver — a sampler thread that
    // panicked on startup produces exactly this, and it is invisible from here.
    firstFrameTimer = setTimeout(() => {
      giveUp(NO_SAMPLER);
    }, FIRST_FRAME_TIMEOUT_MS);

    void subscribeToMetrics({
      onSnapshot(snapshot: Snapshot) {
        if (firstFrameTimer !== null) {
          clearTimeout(firstFrameTimer);
          firstFrameTimer = null;
        }
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
        //
        // `pending` is cleared too: an error before the first frame means the
        // skeletons will never resolve on their own.
        publish({ ...value, pending: false, error: message });
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
          if (firstFrameTimer !== null) {
            clearTimeout(firstFrameTimer);
            firstFrameTimer = null;
          }
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
