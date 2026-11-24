/**
 * Live agentq + AI session state, pushed by the desktop every ~2 s.
 *
 * # Subscribed only while visible
 *
 * The Rust reader thread runs only while some window holds a subscription
 * (ADR-0039). This hook subscribes in an effect, so `<Activity>` — which
 * stops a hidden screen's effects — unsubscribes when the user navigates
 * away, and the reader parks. A renewal every 15 s keeps the 30 s lease alive;
 * a crashed webview simply stops renewing.
 *
 * `<Activity>` is not enough on its own: with `lastRoute = agents`, an
 * autostart launch at logon mounts this screen in a window that stays hidden
 * in the tray, and the reader would poll agentq every 2 s (and Kodwen every
 * 30 s) for a page nobody can see (ENG2, S21-04). So the subscription also
 * follows `document.visibilityState`, like the route preload (`whenVisible`):
 * hidden drops it, visible takes it again.
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';

import type { AgentqAction, AgentUsageSnapshot, AgentsSnapshot, Process } from '@vitals/protocol';

import { errorMessage } from '../../lib/commandError';
import { latestMetrics, subscribeToMetrics } from '../../lib/metrics';
import { hasTauriHost } from '../../shell/host';

export const AGENTS_EVENT = 'vitals://agents';
export const NO_HOST = 'no-host';
const RENEW_MS = 15_000;

export interface AgentsSource {
  subscribe(id: number | null): Promise<{ id: number; snapshot: AgentsSnapshot }>;
  unsubscribe(id: number): Promise<void>;
  listen(onSnapshot: (s: AgentsSnapshot) => void): Promise<() => void>;
  run(action: AgentqAction): Promise<AgentqActionResult>;
  /**
   * What agentq and the AI clients are using, folded from the given process
   * table. The table is passed in because per-process cpu is a delta against
   * the previous frame: the backend cannot enumerate it a second time without
   * every figure starting at zero.
   */
  usage(processes: readonly Process[]): Promise<AgentUsageSnapshot>;
}

export interface AgentqActionResult {
  readonly ok: boolean;
  readonly code: number | null;
  readonly output: string;
}

/** The Tauri implementation. Dynamic imports keep the IPC module out of a browser. */
export const tauriSource: AgentsSource = {
  async subscribe(id) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<{ id: number; snapshot: AgentsSnapshot }>('subscribe_agents', { id });
  },
  async unsubscribe(id) {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('unsubscribe_agents', { id });
  },
  async listen(onSnapshot) {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<AgentsSnapshot>(AGENTS_EVENT, (e) => onSnapshot(e.payload));
  },
  async run(action) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<AgentqActionResult>('run_agentq_action', { action });
  },
  async usage(processes) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<AgentUsageSnapshot>('get_agent_usage', { processes });
  },
};

export interface AgentsState {
  readonly snapshot: AgentsSnapshot | null;
  /** Resource use per tool, or `null` before the first measurement. */
  readonly usage: AgentUsageSnapshot | null;
  readonly pending: boolean;
  readonly error: string | null;
  /** Wall clock, ticking once a second so durations count up between pushes. */
  readonly now: number;
  run: (action: AgentqAction) => Promise<AgentqActionResult>;
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

export function useAgents(source?: AgentsSource, doc: Document = document): AgentsState {
  const injected = source !== undefined;
  const src = source ?? tauriSource;
  const visible = useVisible(doc);
  const [snapshot, setSnapshot] = useState<AgentsSnapshot | null>(null);
  const [usage, setUsage] = useState<AgentUsageSnapshot | null>(null);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const srcRef = useRef(src);
  srcRef.current = src;

  useEffect(() => {
    if (!injected && !hasTauriHost()) {
      setPending(false);
      setError(NO_HOST);
      return undefined;
    }
    // Hidden: hold no subscription, so the reader parks within its lease.
    // The last snapshot stays on screen for when the window comes back.
    if (!visible) return undefined;
    let alive = true;
    let id: number | null = null;
    let unlisten: (() => void) | null = null;
    const s = srcRef.current;

    const subscribe = async () => {
      try {
        const res = await s.subscribe(id);
        if (!alive) return;
        id = res.id;
        setSnapshot((prev) => prev ?? res.snapshot);
        setError(null);
      } catch (cause: unknown) {
        if (alive) setError(errorMessage(cause));
      } finally {
        if (alive) setPending(false);
      }
    };

    void s
      .listen((next) => {
        if (alive) setSnapshot(next);
      })
      .then((u) => {
        if (alive) unlisten = u;
        else u();
      });
    void subscribe();
    const renew = setInterval(() => void subscribe(), RENEW_MS);
    const tick = setInterval(() => setNow(Date.now()), 1000);

    return () => {
      alive = false;
      clearInterval(renew);
      clearInterval(tick);
      unlisten?.();
      if (id !== null) void s.unsubscribe(id).catch(() => undefined);
    };
  }, [injected, visible]);

  // The process table lives in the session-wide metrics hub, which the
  // dashboard and the Processes screen already keep installed. Subscribing
  // here matters for the case where the AI page is the *first* screen of a
  // launch: without a subscriber the hub would never start folding frames and
  // there would be no table to fold resources from. `onSnapshot` does nothing
  // — the fold is driven by the agents push below, so there is one timer per
  // page, not two competing ones.
  useEffect(() => {
    // A browser build or a test has no sampler, so there is nothing to fold
    // and the panel stays empty rather than showing a fabricated zero.
    if (!hasTauriHost()) return undefined;
    let alive = true;
    let unlisten: (() => void) | null = null;
    void subscribeToMetrics({ onSnapshot: () => undefined })
      .then((u) => {
        if (alive) unlisten = u;
        else u();
      })
      .catch(() => undefined);
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  // Recomputed on the page's own cadence — every pushed agents snapshot — so
  // the numbers share one instant with the rest of the panel and there is no
  // second interval to keep in step with it.
  useEffect(() => {
    if (snapshot === null) return undefined;
    const table = latestMetrics();
    const processes = table === null ? [] : [...table.processes.values()];
    // On a real machine an empty table means the first frame has not arrived
    // yet, and folding it would flash the panel empty. With no host there is
    // no sampler at all — an injected source is the only source of truth, and
    // it is asked even for an empty table.
    if (processes.length === 0 && hasTauriHost()) return undefined;
    let alive = true;
    void srcRef.current
      .usage(processes)
      .then((next) => {
        if (alive) setUsage(next);
      })
      // Resources are an extra on a page that is useful without them: a
      // failure here must not blank the section the user came for.
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [snapshot]);

  const run = useCallback((action: AgentqAction) => srcRef.current.run(action), []);

  return { snapshot, usage, pending, error, now, run };
}
