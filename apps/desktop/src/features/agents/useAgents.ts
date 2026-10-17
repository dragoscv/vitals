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
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import type { AgentqAction, AgentsSnapshot } from '@vitals/protocol';

import { errorMessage } from '../../lib/commandError';
import { hasTauriHost } from '../../shell/host';

export const AGENTS_EVENT = 'vitals://agents';
export const NO_HOST = 'no-host';
const RENEW_MS = 15_000;

export interface AgentsSource {
  subscribe(id: number | null): Promise<{ id: number; snapshot: AgentsSnapshot }>;
  unsubscribe(id: number): Promise<void>;
  listen(onSnapshot: (s: AgentsSnapshot) => void): Promise<() => void>;
  run(action: AgentqAction): Promise<AgentqActionResult>;
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
};

export interface AgentsState {
  readonly snapshot: AgentsSnapshot | null;
  readonly pending: boolean;
  readonly error: string | null;
  /** Wall clock, ticking once a second so durations count up between pushes. */
  readonly now: number;
  run: (action: AgentqAction) => Promise<AgentqActionResult>;
}

export function useAgents(source?: AgentsSource): AgentsState {
  const injected = source !== undefined;
  const src = source ?? tauriSource;
  const [snapshot, setSnapshot] = useState<AgentsSnapshot | null>(null);
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
  }, [injected]);

  const run = useCallback((action: AgentqAction) => srcRef.current.run(action), []);

  return { snapshot, pending, error, now, run };
}
