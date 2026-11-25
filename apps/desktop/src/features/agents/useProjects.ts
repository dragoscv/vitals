/**
 * The Projects tab's data, polled from the backend while the tab is visible.
 *
 * # The same clock as the Cloud tab
 *
 * The brief folds this read into the account's 30 s tick (docs/codwen-projects-page.md
 * §4.4) rather than adding a second subscription: one refresh interval, one
 * wall clock that ages the `lastCheckedAt` readings between reads. The panel
 * renders from one snapshot so nothing on the page disagrees with anything
 * else about when it was measured.
 *
 * # Subscribed only while visible
 *
 * The same rule as `useAgents` and `useCollab`: a launch that mounts the AI
 * page in a window hidden in the tray must not poll for data nobody can see,
 * so the interval follows `document.visibilityState`. The last snapshot stays
 * on screen for when the window comes back.
 *
 * # Signed out is a state, not an empty list
 *
 * When no Codwen account is stored the source answers `signedOut`, and the
 * panel shows the sign-in state. It is deliberately not an empty list: "no
 * projects" and "no account" are different facts, and an empty list reads as
 * the first (docs/codwen-projects-page.md rule 4).
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';

import { hasTauriHost } from '../../shell/host';
import { tauriProjectsSource, type ProjectsSource } from './projectsApi';
import type { ProjectsSnapshot } from './projectsModel';

/** Marks the no-host case, which the panel renders as its own message. */
export const NO_HOST = 'no-host';
/** How often the tab re-reads while visible — the Cloud tab's own cadence. */
export const PROJECTS_REFRESH_MS = 30_000;

export type ProjectsErrorKind = typeof NO_HOST | 'unreachable';

/**
 * A discriminated union, as `useHealth` uses: `loading` before the first
 * answer, `signedOut`/`unavailable` for the two documented absences, `ok` with
 * a body, and `error` which keeps the last good `data` across a failed refresh
 * so a single blip shows an "out of date" list rather than blanking it.
 */
export type ProjectsState =
  | { readonly status: 'loading'; readonly data: null; readonly error: null }
  | { readonly status: 'signedOut'; readonly data: null; readonly error: null }
  | { readonly status: 'unavailable'; readonly data: null; readonly error: null }
  | { readonly status: 'ok'; readonly data: ProjectsSnapshot; readonly error: null }
  | {
      readonly status: 'error';
      readonly data: ProjectsSnapshot | null;
      readonly error: ProjectsErrorKind;
    };

export interface ProjectsHook {
  readonly state: ProjectsState;
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

export function useProjects(
  source?: ProjectsSource,
  refreshMs: number = PROJECTS_REFRESH_MS,
  doc: Document = document,
): ProjectsHook {
  const injected = source !== undefined;
  const src = source ?? tauriProjectsSource;
  const visible = useVisible(doc);
  const [state, setState] = useState<ProjectsState>({
    status: 'loading',
    data: null,
    error: null,
  });
  const [now, setNow] = useState(() => Date.now());
  const srcRef = useRef(src);
  srcRef.current = src;

  useEffect(() => {
    if (!injected && !hasTauriHost()) {
      setState({ status: 'error', data: null, error: NO_HOST });
      return undefined;
    }
    // Hidden: hold no timer, so a tray launch costs nothing. The last snapshot
    // stays on screen for when the window comes back.
    if (!visible) return undefined;
    let alive = true;
    const load = async (): Promise<void> => {
      try {
        const read = await srcRef.current.read();
        if (!alive) return;
        if (read.state === 'signedOut') {
          setState({ status: 'signedOut', data: null, error: null });
        } else if (read.state === 'unavailable') {
          setState({ status: 'unavailable', data: null, error: null });
        } else {
          setState({ status: 'ok', data: read.snapshot, error: null });
        }
      } catch {
        if (!alive) return;
        // Keep the last good body: a failed refresh is "out of date", not
        // "gone" (the `useHealth` rule).
        setState((prev) => ({
          status: 'error',
          data: prev.status === 'ok' ? prev.data : null,
          error: 'unreachable',
        }));
      }
    };
    void load();
    const refresh = setInterval(() => void load(), refreshMs);
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      alive = false;
      clearInterval(refresh);
      clearInterval(tick);
    };
  }, [injected, visible, refreshMs]);

  return { state, now };
}
