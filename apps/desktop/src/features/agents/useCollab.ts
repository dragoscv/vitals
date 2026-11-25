/**
 * The collaboration view's data, polled from the backend while the page is
 * visible.
 *
 * # A timer, not a watcher
 *
 * agentq rewrites hundreds of coordination files a second on a busy machine,
 * so a change notification per write would wake the page far more often than
 * the poll below does; a read is a handful of small JSON files. The Rust
 * reader argues the same thing (`src-tauri/src/agents.rs`), and no file
 * watcher exists anywhere in this app.
 *
 * # Subscribed only while visible
 *
 * The same rule as the AI page (`useAgents`): a logon launch that mounts this
 * screen in a window hidden in the tray must not keep asking the backend for
 * data nobody can see, so the poll follows `document.visibilityState`.
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';

import type { CollabSnapshot } from '@vitals/protocol';

import { errorMessage } from '../../lib/commandError';
import { hasTauriHost } from '../../shell/host';

/** Marks the no-host case, which the screen renders as its own message. */
export const NO_HOST = 'no-host';
/** How often the page re-reads the coordination directory while visible. */
const REFRESH_MS = 4_000;

export interface CollabSource {
  fetch(): Promise<CollabSnapshot>;
}

/** The Tauri implementation. A dynamic import keeps the IPC module out of a browser. */
export const tauriCollabSource: CollabSource = {
  async fetch() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<CollabSnapshot>('get_collab');
  },
};

export interface CollabState {
  readonly snapshot: CollabSnapshot | null;
  readonly pending: boolean;
  readonly error: string | null;
  /** Wall clock, ticking once a second so ages count up between reads. */
  readonly now: number;
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

export function useCollab(source?: CollabSource, doc: Document = document): CollabState {
  const injected = source !== undefined;
  const src = source ?? tauriCollabSource;
  const visible = useVisible(doc);
  const [snapshot, setSnapshot] = useState<CollabSnapshot | null>(null);
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
    // Hidden: hold no timer, so a tray launch costs nothing. The last snapshot
    // stays on screen for when the window comes back.
    if (!visible) return undefined;
    let alive = true;
    const load = async (): Promise<void> => {
      try {
        const next = await srcRef.current.fetch();
        if (!alive) return;
        setSnapshot(next);
        setError(null);
      } catch (cause: unknown) {
        if (alive) setError(errorMessage(cause));
      } finally {
        if (alive) setPending(false);
      }
    };
    void load();
    const refresh = setInterval(() => void load(), REFRESH_MS);
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      alive = false;
      clearInterval(refresh);
      clearInterval(tick);
    };
  }, [injected, visible]);

  return { snapshot, pending, error, now };
}
