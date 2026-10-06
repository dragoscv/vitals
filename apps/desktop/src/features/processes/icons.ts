/**
 * Program icons for the process rows.
 *
 * The sampler does not carry icons (an extraction is a file open and a GDI
 * round trip), so rows ask for them as they scroll into view. Requests made
 * in the same frame are sent as one batch — the table mounts forty rows at
 * once, and forty IPC calls per scroll is the cost being avoided — and every
 * answer, including "none", is kept for the life of the process.
 */

import { invoke } from '@tauri-apps/api/core';
import { useEffect, useSyncExternalStore } from 'react';

import type { Process, ProcessKey } from '@vitals/protocol';

import { hasTauriHost } from '../../shell/host';

/** Fetches icons for several processes, in order; `null` where there is none. */
export type IconFetcher = (keys: readonly ProcessKey[]) => Promise<readonly (string | null)[]>;

export interface IconStore {
  /** The icon URL, `null` for none, `undefined` while unknown. */
  get(id: string): string | null | undefined;
  /** Asks for an icon if it has not been asked for already. */
  request(id: string, key: ProcessKey): void;
  /**
   * Forgets processes that have exited. Each URL is a few kilobytes, and a
   * week-long session sees tens of thousands of short-lived processes.
   */
  retain(live: ReadonlySet<string> | ReadonlyMap<string, unknown>): void;
  subscribe: (listener: () => void) => () => void;
}

export function createIconStore(fetch: IconFetcher): IconStore {
  const known = new Map<string, string | null>();
  const inFlight = new Set<string>();
  let queue: { id: string; key: ProcessKey }[] = [];
  let scheduled = false;
  const listeners = new Set<() => void>();

  const flush = (): void => {
    scheduled = false;
    const batch = queue;
    queue = [];
    if (batch.length === 0) return;
    void fetch(batch.map((item) => item.key))
      .catch(() => batch.map(() => null))
      .then((urls) => {
        batch.forEach((item, i) => {
          inFlight.delete(item.id);
          known.set(item.id, urls[i] ?? null);
        });
        for (const listener of listeners) listener();
      });
  };

  return {
    get: (id) => known.get(id),
    request(id, key) {
      if (known.has(id) || inFlight.has(id)) return;
      inFlight.add(id);
      queue.push({ id, key });
      if (!scheduled) {
        scheduled = true;
        queueMicrotask(flush);
      }
    },
    retain(live) {
      for (const id of known.keys()) {
        if (!live.has(id)) known.delete(id);
      }
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

/** The live store, backed by the Tauri host; answers `null` without one. */
export const tauriIconStore: IconStore = createIconStore((keys) =>
  hasTauriHost()
    ? invoke<(string | null)[]>('get_process_icons', {
        keys: keys.map((k) => ({ pid: k.pid, startTime: k.startTime })),
      })
    : Promise.resolve(keys.map(() => null)),
);

/** One row's icon, requested on mount. */
export function useProcessIcon(
  store: IconStore,
  id: string,
  process: Process,
): string | null | undefined {
  const { key } = process;
  useEffect(() => {
    store.request(id, key);
  }, [store, id, key]);
  return useSyncExternalStore(
    store.subscribe,
    () => store.get(id),
    () => store.get(id),
  );
}
