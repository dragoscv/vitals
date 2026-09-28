/**
 * The webview's view of the Rust alert engine.
 *
 * The rules moved out of the webview because a notification must fire once
 * per episode rather than sixty times a minute, and the tray needs the list
 * while the window is hidden. What is left here is a subscription: an initial
 * `get_alerts` for the list that already exists — a window opened mid-episode
 * must not wait for the next transition to learn about it — and then
 * `vitals://alerts`, which carries the whole active list each time it changes.
 *
 * # One store, not one subscription per component
 *
 * Modelled on `createTauriSystemSource`: the store is a module singleton so
 * the widget, a future tray badge and anything else read the same list from
 * one IPC listener. A hook that subscribed per component would open a channel
 * per mount and hold as many copies of the list.
 */

import { useCallback, useRef, useSyncExternalStore } from 'react';

import type { Alert } from '@vitals/protocol';

import { hasTauriHost } from '../../shell/host';

/** Matches `ALERTS_EVENT` in `apps/desktop/src-tauri/src/alerts.rs`. */
export const ALERTS_EVENT = 'vitals://alerts';

/**
 * Shared so every reader gets the same array identity while nothing has
 * changed. `useSyncExternalStore` compares by identity, and a fresh `[]` per
 * call is an infinite render loop rather than a wasted allocation.
 */
const NO_ALERTS: readonly Alert[] = [];

export interface AlertSource {
  subscribe(listener: () => void): () => void;
  current(): readonly Alert[];
}

/** Backed by the Tauri command and event channel. */
export function createTauriAlertSource(): AlertSource {
  let value: readonly Alert[] = NO_ALERTS;
  const listeners = new Set<() => void>();

  const publish = (next: readonly Alert[]): void => {
    value = next;
    for (const listener of listeners) listener();
  };

  let unlisten: (() => void) | null = null;
  // A generation, not a boolean — see `useProcessSnapshot` for the leak a
  // boolean allowed on a quick stop→start with a subscription in flight.
  let generation = 0;
  /**
   * Whether the channel has already delivered a list. The initial read and
   * the first event race, and the event is the newer of the two.
   */
  let seenEvent = false;

  const start = (): void => {
    // A restart after the last subscriber left (route hidden by `<Activity>`,
    // StrictMode's deliberate double-mount) owns a fresh generation.
    const mine = ++generation;
    seenEvent = false;
    // Without a host there is no IPC: `listen()` dereferences an internals
    // global that does not exist and throws from inside a promise nobody
    // awaits. In a browser the honest answer is "nothing is wrong", which is
    // also the empty list.
    if (!hasTauriHost()) return;

    void (async () => {
      const [{ invoke }, { listen }] = await Promise.all([
        import('@tauri-apps/api/core'),
        import('@tauri-apps/api/event'),
      ]);

      // Listening before the initial read, so an alert raised between the two
      // is not lost. The event carries the full list, so a late-arriving
      // initial read cannot resurrect a stale one either — see below.
      const stop = await listen<Alert[]>(ALERTS_EVENT, (event) => {
        seenEvent = true;
        publish(event.payload);
      });
      if (mine !== generation) {
        stop();
        return;
      }
      unlisten = stop;

      const initial = await invoke<Alert[]>('get_alerts');
      // An event that arrived while the invoke was in flight is newer than
      // its answer. Overwriting it would put the list back a tick.
      if (mine === generation && !seenEvent) publish(initial);
    })().catch(() => {
      // A failed read leaves the empty list, which is what the widget already
      // shows. Blanking or throwing over a missing alert list would take down
      // the dashboard to report that nothing needs attention.
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
          generation += 1;
          unlisten?.();
          unlisten = null;
        }
      };
    },
    current: () => value,
  };
}

/** The process-wide source. Created lazily so importing this module is inert. */
let shared: AlertSource | null = null;

function sharedSource(): AlertSource {
  shared ??= createTauriAlertSource();
  return shared;
}

/** Driven by hand, for tests and the sampler-less preview. */
export function createManualAlertSource(
  initial: readonly Alert[] = NO_ALERTS,
): AlertSource & { push(next: readonly Alert[]): void } {
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
      value = next;
      for (const listener of listeners) listener();
    },
  };
}

/** The alerts currently raised, most serious first. */
export function useAlerts(source?: AlertSource): readonly Alert[] {
  // Held in a ref, and the two callbacks are stable, because
  // `useSyncExternalStore` resubscribes whenever `subscribe` changes identity
  // — a fresh closure per render would tear the listener down and rebuild it
  // on every render, which on a 1 Hz feed is a channel churned every second.
  const ref = useRef<AlertSource>(source ?? sharedSource());
  ref.current = source ?? ref.current;

  const subscribe = useCallback((listener: () => void) => ref.current.subscribe(listener), []);
  const getSnapshot = useCallback(() => ref.current.current(), []);

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
