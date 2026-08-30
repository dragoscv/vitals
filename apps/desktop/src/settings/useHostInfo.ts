/**
 * Reads the static machine facts the About panel shows.
 *
 * Fetched once when the panel mounts rather than held in a store: it is asked
 * for when someone opens Settings and never again, and caching it across the
 * session would mean owning an invalidation question for a value nobody is
 * watching.
 */

import { useEffect, useState } from 'react';

import type { HostInfo } from '@vitals/protocol';

import { hasTauriHost } from '../shell/host';

export interface HostInfoState {
  readonly info: HostInfo | null;
  readonly pending: boolean;
  /**
   * Why there is nothing to show, when there is nothing to show.
   *
   * A distinct state from `pending`: "still loading" and "this platform has
   * no backend" look identical if both render a spinner, and one of them
   * never resolves.
   */
  readonly unavailable: boolean;
}

export function useHostInfo(): HostInfoState {
  const [state, setState] = useState<HostInfoState>({
    info: null,
    pending: true,
    unavailable: false,
  });

  useEffect(() => {
    if (!hasTauriHost()) {
      setState({ info: null, pending: false, unavailable: true });
      return;
    }

    let live = true;

    void (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const info = await invoke<HostInfo>('get_host_info');
        if (live) setState({ info, pending: false, unavailable: false });
      } catch {
        // The command answers `Unsupported` on a platform with no backend,
        // which is a legitimate answer rather than a failure — so it lands in
        // the same state as having no host at all.
        if (live) setState({ info: null, pending: false, unavailable: true });
      }
    })();

    return () => {
      live = false;
    };
  }, []);

  return state;
}
