/**
 * The devcloud live status, read from agentq's own read-only status document
 * (ADR 0050).
 *
 * # The webview does not spawn anything
 *
 * `agentq-cloud.ps1 -Status -Json` makes a handful of `gcloud` calls — several
 * seconds each — so it runs in Rust behind `get_devcloud_status`, through the
 * same profile-and-ownership guards the agentq actions use, and the webview
 * only ever receives the parsed document.
 *
 * # A read is a cost, so there is no timer
 *
 * Unlike the signed-in Cloud roster, which polls on the page's 30 s tick, a
 * devcloud read spawns several processes. So this reads once when the panel
 * appears, again only when asked, and once more on a return to the tab if the
 * last reading is older than [`DEVCLOUD_STALE_MS`]. It never polls.
 *
 * # Failure is a state, not an empty document
 *
 * No agentq on the machine, an elevated Vitals, a non-zero exit or unreadable
 * output all arrive as `available: false` with a `failure`, and the panel says
 * which one. A figure that could not be measured is `null` — never `0`.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { hasTauriHost } from '../../shell/host';

/** One devcloud workspace's live reading. Every unmeasured figure is `null`. */
export interface DevcloudWorkspace {
  readonly ws: string;
  readonly vm: string | null;
  readonly project: string | null;
  readonly zone: string | null;
  /** `idle` | `busy` | `suspended` | `stopped` | `suspending` | `unknown`. */
  readonly state: string;
  readonly heartbeatAgeSec: number | null;
  readonly idleSec: number | null;
  readonly queueDepth: number | null;
  /** Estimated cost in thousandths of a euro; the document carries none yet. */
  readonly costMilliEur: number | null;
}

/** A workspace agentq's own document flagged as impossible to measure. */
export interface DevcloudUnreadable {
  readonly ws: string;
  readonly why: string;
}

/** Why the document could not be read. The panel maps this to a sentence. */
export type DevcloudFailureKind = 'noAgentq' | 'blocked' | 'failed' | 'unparsable';

export interface DevcloudFailure {
  readonly kind: DevcloudFailureKind;
  readonly detail: string;
}

/** The whole answer from `get_devcloud_status`. */
export interface DevcloudStatus {
  readonly available: boolean;
  readonly failure: DevcloudFailure | null;
  readonly generatedAt: string | null;
  readonly workspaces: readonly DevcloudWorkspace[];
  readonly unreadable: readonly DevcloudUnreadable[];
}

export type CloudStatusErrorKind = 'no-host' | 'unreachable';

/**
 * A discriminated union, as `useProjects` uses: `loading` before the first
 * answer, `ok` with a document, and `error` which keeps the last good document
 * across a failed refresh so a blip shows an "out of date" reading rather than
 * blanking it.
 */
export type CloudStatusState =
  | { readonly status: 'loading'; readonly data: null; readonly error: null }
  | { readonly status: 'ok'; readonly data: DevcloudStatus; readonly error: null }
  | {
      readonly status: 'error';
      readonly data: DevcloudStatus | null;
      readonly error: CloudStatusErrorKind;
    };

/** The seam the panel depends on, swappable in tests like `ProjectsSource`. */
export interface CloudStatusSource {
  read(): Promise<DevcloudStatus>;
}

/** The Tauri-backed source. */
export const tauriCloudStatusSource: CloudStatusSource = {
  async read(): Promise<DevcloudStatus> {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DevcloudStatus>('get_devcloud_status');
  },
};

export interface CloudStatusHook {
  readonly state: CloudStatusState;
  /**
   * Read again now — the panel's Refresh button. A property holding a function,
   * not a method, because the panel hands it to `onClick` with no receiver
   * (`@typescript-eslint/unbound-method`).
   */
  readonly refresh: () => void;
}

/**
 * How long a reading stays fresh before a return to the tab reads again.
 * A read spawns several `gcloud` processes, so this is deliberately not the
 * 30 s the signed-in Cloud roster uses.
 */
export const DEVCLOUD_STALE_MS = 60_000;

export function useCloudStatus(
  source?: CloudStatusSource,
  doc: Document = document,
): CloudStatusHook {
  const injected = source !== undefined;
  const src = source ?? tauriCloudStatusSource;
  const [state, setState] = useState<CloudStatusState>({
    status: 'loading',
    data: null,
    error: null,
  });
  const srcRef = useRef(src);
  srcRef.current = src;
  const lastAt = useRef(0);
  const running = useRef(false);

  const load = useCallback(async (): Promise<void> => {
    // A read is several processes; never stack two of them.
    if (running.current) return;
    running.current = true;
    try {
      const data = await srcRef.current.read();
      lastAt.current = Date.now();
      setState({ status: 'ok', data, error: null });
    } catch {
      // Keep the last good document: a failed refresh is "out of date", not
      // "gone" (the `useHealth` rule).
      setState((prev) => ({
        status: 'error',
        data: prev.status === 'ok' ? prev.data : null,
        error: 'unreachable',
      }));
    } finally {
      running.current = false;
    }
  }, []);

  useEffect(() => {
    if (!injected && !hasTauriHost()) {
      setState({ status: 'error', data: null, error: 'no-host' });
      return undefined;
    }
    void load();
    const onVisible = (): void => {
      if (doc.visibilityState === 'visible' && Date.now() - lastAt.current >= DEVCLOUD_STALE_MS) {
        void load();
      }
    };
    doc.addEventListener('visibilitychange', onVisible);
    return () => doc.removeEventListener('visibilitychange', onVisible);
  }, [injected, doc, load]);

  return { state, refresh: () => void load() };
}
