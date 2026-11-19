/**
 * State for the MQTT section.
 *
 * One status object, refetched after every mutation. The backend is the
 * source of truth for "is it publishing" — the panel never optimistically
 * assumes a start succeeded, because an address that parses can still name a
 * broker that refuses the connection, and a switch that stays on while
 * nothing is published is the failure this hook exists to prevent.
 */

import { useCallback, useEffect, useState } from 'react';

import { defaultMqttApi, type MqttApi, type MqttStatus } from './api';

export interface MqttState {
  readonly status: MqttStatus | null;
  /** True until the backend has answered for the first time. */
  readonly pending: boolean;
  readonly busy: boolean;
  /** The failure from the last action, if any. */
  readonly error: string | null;
  start(url: string, username: string): Promise<void>;
  stop(): Promise<void>;
  /** Stores a password, or removes it with `null`. Resolves to success. */
  setPassword(password: string | null): Promise<boolean>;
}

const EMPTY: MqttStatus = { running: false, error: null, passwordSet: false };

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'object' && error !== null && 'message' in error) {
    const { message: m } = error;
    if (typeof m === 'string') return m;
  }
  return String(error);
}

export function useMqtt(api?: MqttApi): MqttState {
  const [resolved] = useState<MqttApi>(() => api ?? defaultMqttApi());
  const [status, setStatus] = useState<MqttStatus | null>(null);
  const [pending, setPending] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await resolved.status());
    } catch (cause) {
      setStatus(EMPTY);
      setError(message(cause));
    } finally {
      setPending(false);
    }
  }, [resolved]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const run = useCallback(
    async (action: () => Promise<MqttStatus>) => {
      setBusy(true);
      setError(null);
      try {
        // The command already returns the new status, so a second round trip
        // is not needed — but a failure still has to leave the panel showing
        // the truth rather than the last thing it hoped.
        setStatus(await action());
      } catch (cause) {
        setError(message(cause));
        await refresh();
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const start = useCallback(
    (url: string, username: string) =>
      run(async () => resolved.start({ url, ...(username !== '' && { username }) })),
    [run, resolved],
  );

  const stop = useCallback(() => run(async () => resolved.stop()), [run, resolved]);

  const setPassword = useCallback(
    async (password: string | null): Promise<boolean> => {
      setBusy(true);
      setError(null);
      try {
        const passwordSet = await resolved.setPassword(password);
        setStatus((current) => ({ ...(current ?? EMPTY), passwordSet }));
        return true;
      } catch (cause) {
        setError(message(cause));
        return false;
      } finally {
        setBusy(false);
      }
    },
    [resolved],
  );

  return { status, pending, busy, error, start, stop, setPassword };
}
