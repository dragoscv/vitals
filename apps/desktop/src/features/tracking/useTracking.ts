/**
 * The Tracking page's data: the cross-repo tracker index, read once per visit.
 *
 * # Read once, not polled
 *
 * The index only changes when someone runs `node tools/tracker-lint.mjs
 * --scan`, which is a deliberate act, not a steady stream. So unlike the AI
 * page's two-second push or the Projects tab's thirty-second poll, this reads
 * when the page becomes visible and then holds still, with a manual Reload for
 * the person who just re-scanned. A timer here would burn reads on a file that
 * cannot have moved.
 *
 * # Subscribed only while visible
 *
 * The same rule as `useAgents` and `useProjects`: a launch that mounts this
 * page in a window hidden in the tray must not read for a screen nobody can
 * see. The last index stays on screen for when the window comes back.
 *
 * # Four absences, four states
 *
 * `noHost` (a dev server with no Tauri backend), `empty` (the host answered but
 * there is no scan yet), `ready`, and `error` (the read itself failed). The
 * first two are normal states the page explains; only `error` is a fault.
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';

import { hasTauriHost } from '../../shell/host';
import { parseIndex, type TrackerIndex } from './model';

export type TrackingState =
  | { readonly status: 'loading' }
  /** A browser/dev-server context with no Tauri host to read the file. */
  | { readonly status: 'noHost' }
  /** No scan has run yet (or the index file was empty). */
  | { readonly status: 'empty' }
  | { readonly status: 'ready'; readonly index: TrackerIndex }
  | { readonly status: 'error'; readonly message: string };

/**
 * The one seam the screen depends on. Injectable so a test supplies an index
 * without a Tauri host, exactly as `ProjectsSource` and `HealthApi` are.
 */
export interface TrackingSource {
  /** The index, or `null` when no scan has run. Rejects on a real read failure. */
  readIndex(): Promise<TrackerIndex | null>;
  /** Whether a Codwen account is stored — the locality indicator. */
  signedIn(): Promise<boolean>;
}

/** The Tauri-backed source. Dynamic imports keep the IPC module out of a browser. */
export const tauriTrackingSource: TrackingSource = {
  async readIndex() {
    const { invoke } = await import('@tauri-apps/api/core');
    // The command returns the file verbatim (or `null` when absent); the shape
    // is validated here rather than trusted.
    return parseIndex(await invoke<unknown>('get_tracker_index'));
  },
  async signedIn() {
    const { invoke } = await import('@tauri-apps/api/core');
    const status = await invoke<{ signedIn: boolean }>('get_cloud_status');
    return status.signedIn;
  },
};

export interface TrackingView {
  readonly state: TrackingState;
  /**
   * Whether a Codwen account is stored. `false` before the answer arrives, so
   * the common local-only case is the one drawn first and never flashes an
   * "account connected" label it may have to take back.
   */
  readonly codwen: boolean;
  /**
   * Re-reads the index — the Reload button, after a fresh scan.
   *
   * Declared as a property rather than a method so it can be handed straight
   * to a click handler: `unbound-method` refuses a method reference taken off
   * an interface, and `useCallback` returns a plain function anyway.
   */
  readonly reload: () => void;
}

/** Whether `doc` is visible, re-rendering on `visibilitychange`. */
function useVisible(doc: Document): boolean {
  const subscribe = useCallback(
    (onChange: () => void) => {
      doc.addEventListener('visibilitychange', onChange);
      return () => doc.removeEventListener('visibilitychange', onChange);
    },
    [doc],
  );
  return useSyncExternalStore(subscribe, () => doc.visibilityState === 'visible');
}

export function useTracking(
  source: TrackingSource = tauriTrackingSource,
  doc: Document = document,
): TrackingView {
  const visible = useVisible(doc);
  const [state, setState] = useState<TrackingState>({ status: 'loading' });
  const [codwen, setCodwen] = useState(false);
  const [nonce, setNonce] = useState(0);
  const srcRef = useRef(source);
  srcRef.current = source;

  useEffect(() => {
    if (!hasTauriHost()) {
      setState({ status: 'noHost' });
      return undefined;
    }
    // Hidden: hold no read, so a tray launch costs nothing.
    if (!visible) return undefined;
    let alive = true;
    const load = async (): Promise<void> => {
      try {
        const index = await srcRef.current.readIndex();
        if (!alive) return;
        setState(
          index === null || index.repos.length === 0
            ? { status: 'empty' }
            : { status: 'ready', index },
        );
      } catch (cause) {
        if (!alive) return;
        setState({
          status: 'error',
          message: cause instanceof Error ? cause.message : String(cause),
        });
      }
    };
    void load();
    // The account fact is a separate, cheaper read; a failure is "local only".
    void srcRef.current
      .signedIn()
      .then((signedIn) => {
        if (alive) setCodwen(signedIn);
      })
      .catch(() => {
        if (alive) setCodwen(false);
      });
    return () => {
      alive = false;
    };
  }, [visible, nonce]);

  const reload = useCallback(() => {
    setState({ status: 'loading' });
    setNonce((n) => n + 1);
  }, []);

  return { state, codwen, reload };
}
