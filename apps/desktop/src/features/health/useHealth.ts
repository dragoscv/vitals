/**
 * Reads the owner's health from vmui every minute while the page is visible.
 *
 * Not at all while hidden — the contract (docs/api/vmui-health.md) asks for
 * it, and a minimised window has nobody to show a number to. The screen sits
 * in an `<Activity>`, which tears effects down when another route is shown,
 * so leaving the page stops the poll as well.
 *
 * The last good body is kept, in memory only, across an "unreachable" error:
 * yesterday's step count with an "out of date" badge is more useful than an
 * empty page. A signed-out or not-owner answer drops it, because those mean
 * the data is no longer this device's to show.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import {
  defaultHealthApi,
  errorKind,
  type HealthApi,
  type HealthErrorKind,
  type MeHealth,
} from './api';

export const HEALTH_POLL_MS = 60_000;

export type HealthState =
  | { readonly status: 'loading'; readonly data: null; readonly error: null }
  | { readonly status: 'ok'; readonly data: MeHealth; readonly error: null }
  | {
      readonly status: 'error';
      /** The last good body, only after an `unreachable` error. */
      readonly data: MeHealth | null;
      readonly error: HealthErrorKind;
    };

export interface UseHealth {
  readonly state: HealthState;
  readonly refresh: () => void;
}

const LOADING: HealthState = { status: 'loading', data: null, error: null };

export function useHealth(
  api?: HealthApi,
  pollMs: number = HEALTH_POLL_MS,
  doc: Document = document,
): UseHealth {
  const [resolved] = useState<HealthApi>(() => api ?? defaultHealthApi());
  const [state, setState] = useState<HealthState>(LOADING);
  const last = useRef<MeHealth | null>(null);
  const live = useRef(true);

  const load = useCallback(async () => {
    try {
      const data = await resolved.fetch();
      if (!live.current) return;
      last.current = data;
      setState({ status: 'ok', data, error: null });
    } catch (cause) {
      if (!live.current) return;
      const kind = errorKind(cause);
      if (kind !== 'unreachable') last.current = null;
      setState({ status: 'error', data: last.current, error: kind });
    }
  }, [resolved]);

  useEffect(() => {
    live.current = true;
    let timer: ReturnType<typeof setInterval> | null = null;
    const stop = (): void => {
      if (timer !== null) clearInterval(timer);
      timer = null;
    };
    const start = (): void => {
      if (timer !== null) return;
      void load();
      timer = setInterval(() => {
        void load();
      }, pollMs);
    };
    const onVisibility = (): void => {
      if (doc.visibilityState === 'visible') start();
      else stop();
    };
    onVisibility();
    doc.addEventListener('visibilitychange', onVisibility);
    return () => {
      live.current = false;
      stop();
      doc.removeEventListener('visibilitychange', onVisibility);
    };
  }, [doc, load, pollMs]);

  const refresh = useCallback(() => {
    void load();
  }, [load]);

  return { state, refresh };
}
