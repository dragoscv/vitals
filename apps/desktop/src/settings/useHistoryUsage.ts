/**
 * How much space the recorded history actually occupies.
 *
 * Read on demand rather than polled: the Settings dialog is the only place it
 * is shown, and the number changes by kilobytes per minute. Refetched when the
 * caller bumps `revision`, which is how "Delete all history" makes the figure
 * update without a second mechanism.
 */

import { useEffect, useState } from 'react';

import { hasTauriHost } from '../shell/host';

export interface HistoryUsage {
  readonly bytes: number;
  readonly fineRows: number;
  readonly flightFrames: number;
}

export interface HistoryUsageState {
  readonly usage: HistoryUsage | null;
  readonly pending: boolean;
}

/** Injectable for tests and for the no-host preview. */
export type UsageReader = () => Promise<HistoryUsage>;

async function readUsage(): Promise<HistoryUsage> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<HistoryUsage>('get_history_usage');
}

export function useHistoryUsage(revision = 0, reader?: UsageReader): HistoryUsageState {
  const [usage, setUsage] = useState<HistoryUsage | null>(null);
  const [pending, setPending] = useState(false);

  // The injected reader must bypass the host guard, not sit behind it —
  // otherwise a test's reader is never called and the state never resolves.
  const injected = reader !== undefined;

  useEffect(() => {
    if (!injected && !hasTauriHost()) {
      setUsage(null);
      setPending(false);
      return;
    }

    let live = true;
    setPending(true);
    void (reader ?? readUsage)()
      .then((value) => {
        if (live) setUsage(value);
      })
      .catch(() => {
        // A store that cannot be read is reported as "no data", not as an
        // error: the user came here to look at a number, not to debug SQLite.
        if (live) setUsage(null);
      })
      .finally(() => {
        if (live) setPending(false);
      });

    return () => {
      live = false;
    };
  }, [revision, reader, injected]);

  return { usage, pending };
}
