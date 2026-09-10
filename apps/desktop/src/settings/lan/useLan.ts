/**
 * State for the Remote access panel.
 *
 * One hook, one status object, refetched after every mutation. The server is
 * the source of truth for "is it running" and "what tokens exist" — the panel
 * never optimistically assumes a start succeeded, because on Windows the first
 * bind raises a firewall prompt and a user who declines it gets a socket that
 * is open and unreachable.
 */

import { useCallback, useEffect, useState } from 'react';

import { defaultLanApi, type LanApi, type LanStatus, type Pairing, type Scope } from './api';

export interface LanState {
  readonly status: LanStatus | null;
  readonly pending: boolean;
  readonly busy: boolean;
  readonly error: string | null;
  /** The most recent pairing, shown until dismissed. Holds the secret. */
  readonly pairing: Pairing | null;
  start(port?: number): Promise<void>;
  stop(): Promise<void>;
  pair(label: string, scope: Scope, address?: string): Promise<void>;
  revoke(prefix: string): Promise<void>;
  revokeAll(): Promise<void>;
  dismissPairing(): void;
}

const EMPTY: LanStatus = { running: false, port: null, interfaces: [], tokens: [] };

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'object' && error !== null && 'message' in error) {
    const { message: m } = error;
    if (typeof m === 'string') return m;
  }
  return String(error);
}

export function useLan(api?: LanApi): LanState {
  const [resolved] = useState<LanApi>(() => api ?? defaultLanApi());
  const [status, setStatus] = useState<LanStatus | null>(null);
  const [pending, setPending] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pairing, setPairing] = useState<Pairing | null>(null);

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
    async (action: () => Promise<void>) => {
      setBusy(true);
      setError(null);
      try {
        await action();
      } catch (cause) {
        setError(message(cause));
      } finally {
        await refresh();
        setBusy(false);
      }
    },
    [refresh],
  );

  const start = useCallback(
    (port?: number) =>
      run(async () => {
        await resolved.start(port);
      }),
    [run, resolved],
  );

  const stop = useCallback(
    () =>
      run(async () => {
        await resolved.stop();
        // A pairing QR for a server that is off would send the phone to a
        // closed port.
        setPairing(null);
      }),
    [run, resolved],
  );

  const pair = useCallback(
    (label: string, scope: Scope, address?: string) =>
      run(async () => {
        const created = await resolved.pair({
          label,
          scope,
          ...(address !== undefined && { address }),
        });
        setPairing(created);
      }),
    [run, resolved],
  );

  const revoke = useCallback(
    (prefix: string) =>
      run(async () => {
        await resolved.revoke(prefix);
        setPairing((current) => (current?.token.prefix === prefix ? null : current));
      }),
    [run, resolved],
  );

  const revokeAll = useCallback(
    () =>
      run(async () => {
        await resolved.revokeAll();
        setPairing(null);
      }),
    [run, resolved],
  );

  const dismissPairing = useCallback(() => {
    setPairing(null);
  }, []);

  return {
    status,
    pending,
    busy,
    error,
    pairing,
    start,
    stop,
    pair,
    revoke,
    revokeAll,
    dismissPairing,
  };
}
