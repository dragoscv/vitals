/**
 * State for the Remote access panel.
 *
 * One hook, one status object, refetched after every mutation. The server is
 * the source of truth for "is it running" and "what tokens exist" — the panel
 * never optimistically assumes a start succeeded, because on Windows the first
 * bind raises a firewall prompt and a user who declines it gets a socket that
 * is open and unreachable.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import {
  defaultLanApi,
  type LanApi,
  type LanStatus,
  type Pairing,
  type PairingCode,
  type Scope,
} from './api';

/**
 * Where the code on screen stands. `paired` means the TV used it; `ended`
 * means it stopped working without a pairing — too many wrong guesses.
 */
export type CodeOutcome = 'waiting' | 'paired' | 'expired' | 'ended';

/** How often the panel asks whether the TV has used the code. */
export const CODE_POLL_MS = 2000;

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
  /** The TV code on screen, until dismissed. Holds a live credential. */
  readonly code: PairingCode | null;
  readonly codeOutcome: CodeOutcome | null;
  showCode(scope: Scope): Promise<void>;
  cancelCode(): Promise<void>;
  dismissCode(): void;
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

export function useLan(api?: LanApi, pollMs: number = CODE_POLL_MS): LanState {
  const [resolved] = useState<LanApi>(() => api ?? defaultLanApi());
  const [status, setStatus] = useState<LanStatus | null>(null);
  const [pending, setPending] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pairing, setPairing] = useState<Pairing | null>(null);
  const [code, setCode] = useState<PairingCode | null>(null);
  const [codeOutcome, setCodeOutcome] = useState<CodeOutcome | null>(null);
  // The code a poll was started for. A poll answered after the user
  // cancelled, or after a newer code replaced it, must not announce
  // "paired" for a code that is no longer on screen.
  const shownCode = useRef<PairingCode | null>(null);
  // Paired devices when the code was shown. A code that goes inactive early
  // was either redeemed or burnt by wrong guesses; only the first adds a
  // token, and announcing "paired" for the second would be a lie. Prefixes,
  // not a count: revoking another device meanwhile would hide the new one.
  const tokensAtShow = useRef<ReadonlySet<string>>(new Set());

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
        // closed port. The backend withdraws the TV code on stop too.
        setPairing(null);
        shownCode.current = null;
        setCode(null);
        setCodeOutcome(null);
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

  const showCode = useCallback(
    (scope: Scope) =>
      run(async () => {
        const created = await resolved.createCode(scope);
        tokensAtShow.current = new Set((await resolved.status()).tokens.map((t) => t.prefix));
        shownCode.current = created;
        setCode(created);
        setCodeOutcome('waiting');
      }),
    [run, resolved],
  );

  const cancelCode = useCallback(
    () =>
      run(async () => {
        shownCode.current = null;
        setCode(null);
        setCodeOutcome(null);
        await resolved.cancelCode();
      }),
    [run, resolved],
  );

  const dismissCode = useCallback(() => {
    shownCode.current = null;
    setCode(null);
    setCodeOutcome(null);
  }, []);

  // Polling, not an event: the redemption happens on the server's thread and
  // the webview has no subscription to it. Every two seconds is quick enough
  // that the panel says "paired" about when the TV does, and costs one IPC
  // round trip only while a code is on screen.
  useEffect(() => {
    if (code === null || codeOutcome !== 'waiting') return undefined;
    const check = async () => {
      let answer;
      try {
        answer = await resolved.codeStatus();
      } catch {
        // A missed poll is retried at the next tick; saying nothing is
        // better than flashing an error over a code that still works.
        return;
      }
      if (shownCode.current !== code || answer.active) return;
      if (Date.now() >= code.expiresAtMs) {
        setCodeOutcome('expired');
        return;
      }
      // Inactive before its expiry: redeemed or burnt. The token list
      // says which, and is what the panel needs to show anyway.
      let fresh;
      try {
        fresh = await resolved.status();
      } catch {
        return;
      }
      if (shownCode.current !== code) return;
      setStatus(fresh);
      const added = fresh.tokens.some((t) => !tokensAtShow.current.has(t.prefix));
      setCodeOutcome(added ? 'paired' : 'ended');
    };
    const timer = setInterval(() => {
      void check();
    }, pollMs);
    return () => {
      clearInterval(timer);
    };
  }, [code, codeOutcome, pollMs, resolved]);

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
    code,
    codeOutcome,
    showCode,
    cancelCode,
    dismissCode,
  };
}
