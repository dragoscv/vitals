/**
 * The "Replace Task Manager" switch.
 *
 * This is not an `AppSettings` field on purpose. The truth lives in the
 * registry (`HKLM\...\Image File Execution Options\taskmgr.exe`), is
 * machine-wide, and can be changed by another tool, another user or an
 * uninstall while Vitals is closed. A cached boolean in settings.json would
 * confidently show the wrong state, so the switch reads the registry on
 * open and after every change, and never stores anything itself.
 */

import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useState } from 'react';

import type { TaskManagerReplacement } from '@vitals/protocol';

import { hasTauriHost } from '../shell/host';

export interface TaskManagerReplacementState {
  /** `null` until the first read has answered. */
  readonly status: TaskManagerReplacement | null;
  readonly busy: boolean;
  /** The last failure's message, cleared by the next successful call. */
  readonly error: string | null;
  set(enabled: boolean): void;
}

export interface TaskManagerReplacementApi {
  read(): Promise<TaskManagerReplacement>;
  write(enabled: boolean): Promise<TaskManagerReplacement>;
}

export const tauriApi: TaskManagerReplacementApi = {
  read: () => invoke<TaskManagerReplacement>('get_taskmgr_replacement'),
  write: (enabled) => invoke<TaskManagerReplacement>('set_taskmgr_replacement', { enabled }),
};

// In a browser there is no registry to ask; the switch renders off and
// inert rather than spinning forever on a read that will never answer.
const NOT_A_HOST: TaskManagerReplacement = {
  enabled: false,
  replacedBy: null,
  path: null,
};

export function useTaskManagerReplacement(
  api: TaskManagerReplacementApi = tauriApi,
): TaskManagerReplacementState {
  const [status, setStatus] = useState<TaskManagerReplacement | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!hasTauriHost()) {
      setStatus(NOT_A_HOST);
      return;
    }
    let cancelled = false;
    api
      .read()
      .then((value) => {
        if (!cancelled) setStatus(value);
      })
      .catch((failure: unknown) => {
        if (!cancelled) setError(describe(failure));
      });
    return () => {
      cancelled = true;
    };
  }, [api]);

  const set = useCallback(
    (enabled: boolean) => {
      setBusy(true);
      setError(null);
      api
        .write(enabled)
        .then((value) => setStatus(value))
        // A refused UAC prompt lands here. The switch stays where the
        // registry says it is, and the reason is shown beside it.
        .catch((failure: unknown) => setError(describe(failure)))
        .finally(() => setBusy(false));
    },
    [api],
  );

  return { status, busy, error, set };
}

function describe(failure: unknown): string {
  if (failure instanceof Error) return failure.message;
  if (typeof failure === 'object' && failure !== null && 'message' in failure) {
    const { message } = failure;
    if (typeof message === 'string') return message;
  }
  return String(failure);
}
