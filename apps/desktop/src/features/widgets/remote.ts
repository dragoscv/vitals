/**
 * Pointing a widget at another machine.
 *
 * The local path folds `SystemMetrics` out of the sampler's frames. A remote
 * machine has no sampler here, so this module polls its LAN API instead — and
 * folds the reading through the SAME {@link rollUp} the local path uses. That
 * is the entire point: a remote widget and a local one must not be able to
 * draw two different sparklines for the same reading, and the only way to
 * guarantee that is to share the fold rather than re-implement it beside it.
 *
 * # Where a machine comes from
 *
 * Nothing here invents machine discovery. `useRemoteWidgetInput` takes an id
 * and looks it up in a registry the app populates with
 * {@link setRemoteConnections} — the LAN address and the bearer token are the
 * app's business, decided when the user pairs a machine, not something this
 * data layer may guess. (The `features/connections` screen is the *socket*
 * table, not a list of machines, so it holds no address or token to reuse.)
 * The registry is module state rather than a hook argument because the pinned
 * call site passes only the id, and it is subscribable so a machine registered
 * after the widget mounted still starts its poll.
 *
 * # The two rules this module owes the LAN
 *
 * One request at a time, and never faster than the local sampler. A remote
 * machine is someone else's computer across a network the user shares with
 * everything else in their home; a widget that queued polls or ran faster than
 * 1 Hz would make a monitoring overlay a measurable load on the machine it is
 * monitoring.
 */

import { useEffect, useState, useSyncExternalStore } from 'react';

import { isVitalsError, VitalsClient } from '@vitals/client';

import { EMPTY_WIDGET_INPUT, type WidgetInput } from './describe';
import { rollUp } from './live';

/**
 * The remote poll cadence: the local sampler's tick, in milliseconds.
 *
 * Exported because a test drives the interval by name, and because a future
 * settings screen may want to show or slow it. It must never be lowered: the
 * `rollUp` traces assume one point per second, and a faster poll would both
 * shorten the window the sparkline covers and load the far machine more.
 */
export const REMOTE_POLL_INTERVAL_MS = 1000;

/** How a remote widget is doing. */
export type RemoteStatus =
  /** No machine is selected — the caller is showing this machine. */
  | 'idle'
  /** A machine is selected and no reading has arrived yet. */
  | 'loading'
  /** The last poll produced a reading. */
  | 'live'
  /** The machine could not be reached, or answered with something unusable. */
  | 'unreachable'
  /** The machine answered 401 or 403: this widget's key is not accepted. */
  | 'unauthorised';

/** A remote reading, plus how it is going. */
export interface RemoteWidgetInput {
  /**
   * The same shape the local path produces, so every layout renders it
   * identically. Kept whole on a failed poll — see {@link useRemoteWidgetInput}.
   */
  readonly input: WidgetInput;
  readonly status: RemoteStatus;
}

/** One machine a widget may be pointed at. */
export interface RemoteConnection {
  /** Stable identity; this is what the caller passes as `connectionId`. */
  readonly id: string;
  /** The LAN address the machine's server listens on, e.g. `http://192.168.1.20:7331`. */
  readonly baseUrl: string;
  /** The bearer token minted when the user paired the machine. */
  readonly token: string;
}

/**
 * The registry, kept as module state behind a subscription.
 *
 * Frozen on assignment so a subscriber cannot mutate it in place and leave the
 * stored identity disagreeing with its contents — `useSyncExternalStore`
 * compares the snapshot by reference, and an in-place edit would be invisible.
 */
let connections: readonly RemoteConnection[] = Object.freeze([]);
const listeners = new Set<() => void>();

/**
 * Replaces the machines a widget may be pointed at.
 *
 * Replaces rather than merges: the caller's machine list is the single source
 * of truth, and a merge would let a machine the user has forgotten keep being
 * polled by a widget that can no longer show it.
 *
 * Registered addresses are read on every effect run, so passing fresh objects
 * for unchanged machines restarts their polls — pass stable objects when the
 * list has not really changed.
 */
export function setRemoteConnections(next: readonly RemoteConnection[]): void {
  connections = Object.freeze([...next]);
  for (const listener of listeners) listener();
}

/** The machine with `id`, or `null` when none is registered. */
export function resolveRemoteConnection(id: string): RemoteConnection | null {
  return connections.find((connection) => connection.id === id) ?? null;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function snapshotConnections(): readonly RemoteConnection[] {
  return connections;
}

/**
 * The status a failed poll leaves the widget in.
 *
 * 401 and 403 are BOTH `unauthorised`, and deliberately not `unreachable`: the
 * machine answered, so it is on the network, and the two states ask the user
 * for opposite things — re-pair, versus wait and retry. A machine that never
 * answered, or answered with a body the client could not use, is
 * `unreachable`, which keeps the last reading on screen and looks again next
 * tick. Anything that is not a `VitalsError` at all (a bug, or an abort) is
 * likewise `unreachable`: there is no reading, and there is no better word.
 */
export function remoteStatusFrom(error: unknown): RemoteStatus {
  if (isVitalsError(error)) {
    switch (error.kind) {
      case 'unauthorised':
      case 'forbidden':
        return 'unauthorised';
      case 'network':
      case 'closed':
      case 'http':
      case 'protocol':
        return 'unreachable';
      default: {
        // Exhaustiveness: a new `VitalsErrorKind` must be faced here rather
        // than silently falling through to a wrong status.
        const exhaustive: never = error.kind;
        return exhaustive;
      }
    }
  }
  return 'unreachable';
}

/**
 * Threads an abort signal through the one seam the client exposes: its
 * `fetch`.
 *
 * `VitalsClient` does not take a signal, so without this a poll in flight when
 * the widget closes — or when the user points it at a different machine —
 * would run to completion and its reply would land in a torn-down effect. The
 * `cancelled` guard in the effect already discards such a reply; aborting also
 * stops the request, which is what keeps a hidden widget from spending a
 * machine's bandwidth on a reading nobody will see.
 */
function abortableFetch(signal: AbortSignal): typeof globalThis.fetch {
  return (input, init) =>
    globalThis.fetch(input, init === undefined ? { signal } : { ...init, signal });
}

/**
 * Polls a remote machine and folds its readings into widget traces.
 *
 * `null` means "show this machine": no request is issued and the status is
 * `idle`. Any other value is looked up in the registry; an id with no machine
 * behind it is `unreachable` and also issues nothing, because there is no
 * address to call and inventing one would be the guess this module exists to
 * avoid.
 */
export function useRemoteWidgetInput(connectionId: string | null): RemoteWidgetInput {
  // Subscribed rather than read once: a widget that mounts before the machine
  // list has loaded would otherwise poll nothing forever.
  const machineList = useSyncExternalStore(subscribe, snapshotConnections, snapshotConnections);
  const connection =
    connectionId === null
      ? null
      : (machineList.find((machine) => machine.id === connectionId) ?? null);

  const [state, setState] = useState<RemoteWidgetInput>(() => ({
    input: EMPTY_WIDGET_INPUT,
    status: connectionId === null ? 'idle' : 'loading',
  }));

  // Keyed on the resolved connection, not only the id: re-pairing a machine
  // mints a new token, and the old client must be torn down rather than go on
  // presenting a key the machine no longer accepts.
  useEffect(() => {
    if (connectionId === null) {
      // Showing this machine. Drop any traces a previous remote selection left
      // behind — a line from another machine under this widget's label would be
      // a lie — and issue nothing.
      setState((previous) =>
        previous.status === 'idle' && previous.input === EMPTY_WIDGET_INPUT
          ? previous
          : { input: EMPTY_WIDGET_INPUT, status: 'idle' },
      );
      return;
    }

    if (connection === null) {
      // In the registry's own words: nothing to call. Keep whatever is on
      // screen and say it cannot be reached rather than blanking it.
      setState((previous) =>
        previous.status === 'unreachable'
          ? previous
          : { input: previous.input, status: 'unreachable' },
      );
      return;
    }

    // A resolved target starts from empty: the traces belong to the machine
    // that produced them, so switching machines must not splice a second
    // machine's line onto the first.
    setState({ input: EMPTY_WIDGET_INPUT, status: 'loading' });

    const controller = new AbortController();
    const client = new VitalsClient({
      baseUrl: connection.baseUrl,
      token: connection.token,
      fetch: abortableFetch(controller.signal),
    });

    let cancelled = false;
    let inFlight = false;

    const poll = async (): Promise<void> => {
      // One request at a time, never a queue: a slow machine must not be able
      // to make a backlog the widget's bottleneck — the pathology this app
      // exists to reveal, caused by the app itself.
      if (inFlight || cancelled) return;
      inFlight = true;
      try {
        const summary = await client.summary();
        if (cancelled) return;
        if (summary === null) {
          // The host answered but has not taken its first reading. That is
          // reachable-but-empty, not live and not a failure; inventing a zero
          // to fill the gap is the one thing AGENTS.md forbids.
          setState((previous) => ({ input: previous.input, status: 'loading' }));
          return;
        }
        setState((previous) => ({
          input: rollUp(previous.input, summary.system),
          status: 'live',
        }));
      } catch (error) {
        if (cancelled) return;
        // The traces are kept. A stale line under an honest "unreachable" is
        // better than a blank widget, which is what clearing them would give.
        setState((previous) => ({ input: previous.input, status: remoteStatusFrom(error) }));
      } finally {
        inFlight = false;
      }
    };

    void poll();
    const timer = setInterval(() => {
      void poll();
    }, REMOTE_POLL_INTERVAL_MS);

    return () => {
      cancelled = true;
      clearInterval(timer);
      controller.abort();
    };
  }, [connectionId, connection]);

  return state;
}
