/**
 * State for the Health (vmui) settings section.
 *
 * Mirrors `lan/useLan`: one status object from the backend, refetched after
 * every mutation, and the nav visibility store updated from that same answer
 * so the Health page appears or disappears the moment the connection does.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import {
  defaultHealthApi,
  type HealthApi,
  type HealthStatus,
  type PairStatus,
} from '../../features/health/api';
import { useNavVisibility } from '../../shell/navigation';

/** The contract's poll interval while a code waits for approval. */
export const PAIR_POLL_MS = 3000;
/** The contract's cap on one pairing attempt. */
export const PAIR_MAX_MS = 10 * 60 * 1000;

export type CodeOutcome = Exclude<PairStatus, 'approved'>;

export interface HealthConnection {
  readonly status: HealthStatus | null;
  readonly busy: boolean;
  /** A failure key under `settings.failure.*`, or a raw message. */
  readonly error: string | null;
  readonly code: string | null;
  readonly codeOutcome: CodeOutcome | null;
  pair(base: string, name: string): Promise<void>;
  cancelPair(): void;
  signIn(base: string, email: string, password: string): Promise<void>;
  disconnect(): Promise<void>;
}

const EMPTY: HealthStatus = { configured: false, base: null, tokenPrefix: null };

function message(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

export function useHealthConnection(
  api?: HealthApi,
  pollMs: number = PAIR_POLL_MS,
): HealthConnection {
  const [resolved] = useState<HealthApi>(() => api ?? defaultHealthApi());
  const [status, setStatus] = useState<HealthStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pairing, setPairing] = useState<{ id: string; code: string; since: number } | null>(null);
  const [codeOutcome, setCodeOutcome] = useState<CodeOutcome | null>(null);
  const shown = useRef<string | null>(null);

  const refresh = useCallback(async () => {
    let next = EMPTY;
    try {
      next = await resolved.status();
    } catch (cause) {
      setError(message(cause));
    }
    setStatus(next);
    useNavVisibility.getState().setHealth(next.configured);
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

  const pair = useCallback(
    (base: string, name: string) =>
      run(async () => {
        const started = await resolved.pairStart(base, name);
        shown.current = started.id;
        setPairing({ ...started, since: Date.now() });
        setCodeOutcome('pending');
      }),
    [run, resolved],
  );

  const cancelPair = useCallback(() => {
    shown.current = null;
    setPairing(null);
    setCodeOutcome(null);
  }, []);

  const signIn = useCallback(
    (base: string, email: string, password: string) =>
      run(async () => {
        await resolved.signIn(base, email, password);
      }),
    [run, resolved],
  );

  const disconnect = useCallback(
    () =>
      run(async () => {
        cancelPair();
        await resolved.disconnect();
      }),
    [run, resolved, cancelPair],
  );

  // Polling, as the contract asks: every three seconds, for ten minutes at
  // most, until vmui answers approved, rejected or expired.
  useEffect(() => {
    if (pairing === null || codeOutcome !== 'pending') return undefined;
    const check = async (): Promise<void> => {
      if (Date.now() - pairing.since > PAIR_MAX_MS) {
        setCodeOutcome('expired');
        return;
      }
      let answer: PairStatus;
      try {
        answer = await resolved.pairPoll(pairing.id);
      } catch {
        // A missed poll is retried at the next tick.
        return;
      }
      if (shown.current !== pairing.id) return;
      if (answer === 'approved') {
        shown.current = null;
        setPairing(null);
        setCodeOutcome(null);
        await refresh();
      } else {
        setCodeOutcome(answer);
      }
    };
    const timer = setInterval(() => {
      void check();
    }, pollMs);
    return () => {
      clearInterval(timer);
    };
  }, [pairing, codeOutcome, pollMs, resolved, refresh]);

  return {
    status,
    busy,
    error,
    code: pairing?.code ?? null,
    codeOutcome,
    pair,
    cancelPair,
    signIn,
    disconnect,
  };
}
