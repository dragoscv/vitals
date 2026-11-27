/**
 * One live connection per paired PC, held outside React.
 *
 * The stream must survive tab switches: unmounting the Machines tab and
 * reconnecting every SSE stream on return would cost a reconnect per PC per
 * tap, and the first frame after a reconnect is a keyframe, so the sparklines
 * would restart from nothing each time.
 */

import { useEffect, useRef, useState, useSyncExternalStore } from 'react';

import { isVitalsError, type ControlRequest, type VitalsClient } from '@vitals/client';
import type { Alert, Frame } from '@vitals/protocol';

import { fold, type Snapshot } from './frames';
import type { Pairing } from './pairing';

/** The subset of `VitalsClient` the phone uses; a test hands in a plain object. */
export type MobileClient = Pick<VitalsClient, 'health' | 'host' | 'stream' | 'control' | 'alerts'>;

export type ConnectionState =
  | 'connecting'
  | 'live'
  | 'reconnecting'
  | 'unreachable'
  /**
   * The PC answered 401: this phone's key is no longer accepted, so the
   * pairing was revoked or replaced on the PC. Distinct from `unreachable`
   * (the PC never answered) because the two ask the user for different
   * things — a scan of a fresh code, not patience — and because retrying a
   * dead key is pointless.
   */
  | 'unauthorised';

/** How many readings a card's sparkline remembers — about a minute at 1 Hz. */
export const HISTORY_LENGTH = 60;

/**
 * How often the alert list is re-read while a PC is connected.
 *
 * Alerts are raised on sustained conditions (a minute of busy disk, tens of
 * seconds of CPU), so a 15 s poll sees every one of them within its own
 * lifetime and costs one small request rather than a byte per frame.
 */
export const ALERTS_POLL_MS = 15_000;

/**
 * The PC's nominal sampling tick, in milliseconds.
 *
 * The phone has no settings of its own and cannot read the PC's, so it
 * assumes the default 1 Hz the sampler runs at — the same "1000 ms nominal
 * tick" the dashboard's gap detector is written against. Used only to decide
 * when a reading is old, never to pace anything.
 */
export const SAMPLE_INTERVAL_MS = 1_000;

/** How old the newest reading must be before a card calls it stale: 2× the tick. */
export const STALE_AFTER_MS = SAMPLE_INTERVAL_MS * 2;

/** Everything a screen needs about one PC. Immutable; a new object per frame. */
export interface LiveState {
  readonly state: ConnectionState;
  readonly snapshot: Snapshot | null;
  readonly cpuHistory: readonly number[];
  readonly memoryHistory: readonly number[];
  /**
   * `null` until the first successful read: "not fetched yet" and "nothing
   * is wrong" are different facts, and the card must not say "all clear"
   * about a PC it has not asked.
   */
  readonly alerts: readonly Alert[] | null;
  /**
   * The phone's clock when the last frame arrived, or `null` before the
   * first. The frame carries the PC's wall clock, which the phone cannot
   * compare against its own without assuming the two machines agree on the
   * time; recording receipt measures age without clock skew.
   */
  readonly lastFrameAtMs: number | null;
}

const INITIAL: LiveState = {
  state: 'connecting',
  snapshot: null,
  cpuHistory: [],
  memoryHistory: [],
  alerts: null,
  lastFrameAtMs: null,
};

/** How long the newest reading has been silent, or `null` before the first one. */
export function readingAgeMs(live: LiveState, now: number): number | null {
  return live.lastFrameAtMs === null ? null : Math.max(0, now - live.lastFrameAtMs);
}

/**
 * A compact age, matching the desktop's `since()`: `5s`, `1m 05s`, `2h 03m`.
 *
 * Duplicated rather than imported because the phone must not pull the
 * desktop bundle (see `boundary.test.ts`), and it is eight lines.
 */
export function formatAgo(ms: number): string {
  const s = Math.floor(Math.max(0, ms) / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`;
  if (m > 0) return `${m}m ${String(sec).padStart(2, '0')}s`;
  return `${s}s`;
}

/**
 * A clock that ticks, so a card re-renders and shows a reading ageing even
 * while nothing else changes. Without it a stale card — no frames arriving —
 * would never re-render and the "last reading" chip would never appear.
 */
export function useNow(intervalMs: number = SAMPLE_INTERVAL_MS): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(timer);
  }, [intervalMs]);
  return now;
}

/**
 * Consecutive failed attempts before a PC is called unreachable rather than
 * reconnecting. Two is a Wi-Fi hiccup; three in a row with backoff is a PC
 * that has gone to sleep or left the network.
 */
const UNREACHABLE_AFTER = 3;

/** A subscribable connection to one PC. */
export class LiveMachine {
  private current: LiveState = INITIAL;
  private readonly listeners = new Set<() => void>();
  private stop: (() => void) | undefined;
  private alertsTimer: ReturnType<typeof setTimeout> | undefined;
  /** Bumped on every drop and close so a poll that was in flight discards its reply. */
  private alertsEpoch = 0;

  constructor(private readonly client: MobileClient) {}

  /** Opens the stream. Idempotent. */
  start(): void {
    if (this.stop !== undefined) return;
    this.stop = this.client.stream({
      onFrame: (frame) => this.apply(frame),
      onOpen: () => {
        this.set({ ...this.current, state: 'live' });
        // Alerts ride on the connection, not on the frames: they start when
        // the stream opens (and again after every reconnect) and stop when
        // it drops, so a PC that is unreachable is not also polled forever.
        this.startAlerts();
      },
      onDrop: () => {
        this.stopAlerts();
        if (this.current.state === 'live') this.set({ ...this.current, state: 'reconnecting' });
      },
      onRetry: (_delay, attempt) => {
        if (attempt + 1 >= UNREACHABLE_AFTER) this.set({ ...this.current, state: 'unreachable' });
      },
    });
    // In parallel, never before: the stream is what shows a live PC fastest,
    // and the probe only ever downgrades a key the PC has already refused.
    void this.verifyToken();
  }

  /**
   * One authenticated probe, because the SSE transport cannot see a status
   * code.
   *
   * A revoked token answers 401 to every authenticated request, but a browser
   * reports an `EventSource` failure only as "error", so the stream would
   * reconnect behind "Reconnecting…" forever with no way to tell a network
   * problem from a dead key. `/host` is the cheapest authenticated read; a
   * 403 is a legitimate refusal (a read-only token, or Windows saying no) and
   * is deliberately not this state.
   */
  private async verifyToken(): Promise<void> {
    try {
      await this.client.host();
    } catch (error) {
      // Closed while this was in flight (unpaired, re-paired): a late reply
      // must not resurrect a machine nobody is watching.
      if (this.stop === undefined) return;
      if (isVitalsError(error) && error.kind === 'unauthorised') {
        this.set({ ...this.current, state: 'unauthorised' });
        // Stop the stream: a dead key will never open, and retrying it
        // forever is the loop this state exists to end.
        this.close();
      }
      // Any other failure is the network's business; the stream's own retry
      // covers it and `/host` is not worth surfacing on its own.
    }
  }

  /** Closes the stream and cancels reconnects. */
  close(): void {
    this.stopAlerts();
    this.stop?.();
    this.stop = undefined;
  }

  private startAlerts(): void {
    this.stopAlerts();
    const epoch = ++this.alertsEpoch;
    const tick = (): void => {
      this.client.alerts().then(
        (alerts) => {
          if (epoch !== this.alertsEpoch) return;
          this.set({ ...this.current, alerts });
          this.alertsTimer = setTimeout(tick, ALERTS_POLL_MS);
        },
        () => {
          // A failed read keeps the last known list rather than clearing it:
          // a transient 5xx must not flash "all clear" over a real warning.
          // The stream's own drop handling decides whether the PC is gone.
          if (epoch !== this.alertsEpoch) return;
          this.alertsTimer = setTimeout(tick, ALERTS_POLL_MS);
        },
      );
    };
    tick();
  }

  private stopAlerts(): void {
    this.alertsEpoch += 1;
    if (this.alertsTimer !== undefined) clearTimeout(this.alertsTimer);
    this.alertsTimer = undefined;
  }

  /** Sends a control request over HTTP. Errors are the client's `VitalsError`s. */
  control(request: ControlRequest): Promise<void> {
    return this.client.control(request);
  }

  get(): LiveState {
    return this.current;
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private apply(frame: Frame): void {
    const snapshot = fold(this.current.snapshot, frame);
    const memory = snapshot.system.memory;
    // A total of zero is "not reported", never "no memory". Dividing would
    // print 0 %, which the sparkline draws as the floor and a reader takes
    // for an idle machine; NaN breaks the line instead (see Sparkline).
    const memoryPercent = memory.total > 0 ? (memory.used / memory.total) * 100 : Number.NaN;
    this.set({
      ...this.current,
      state: 'live',
      snapshot,
      cpuHistory: push(this.current.cpuHistory, snapshot.system.cpu.total),
      memoryHistory: push(this.current.memoryHistory, memoryPercent),
      lastFrameAtMs: Date.now(),
    });
  }

  private set(next: LiveState): void {
    this.current = next;
    for (const listener of this.listeners) listener();
  }
}

function push(history: readonly number[], value: number): readonly number[] {
  const next = history.length >= HISTORY_LENGTH ? history.slice(1) : history.slice();
  next.push(value);
  return next;
}

/** Subscribes a component to one machine's state. */
export function useLive(machine: LiveMachine | undefined): LiveState {
  return useSyncExternalStore(
    (cb) => machine?.subscribe(cb) ?? (() => {}),
    () => machine?.get() ?? INITIAL,
    () => INITIAL,
  );
}

/**
 * Keeps one running `LiveMachine` per pairing.
 *
 * Machines are keyed by pairing id and by token: re-pairing a PC issues a new
 * token, and the old stream would keep failing forever with a 401 the SSE
 * transport cannot even see.
 */
export function useLiveMachines(
  pairings: readonly Pairing[],
  makeClient: (pairing: Pairing) => MobileClient,
): ReadonlyMap<string, LiveMachine> {
  const [machines, setMachines] = useState<ReadonlyMap<string, LiveMachine>>(() => new Map());
  const [tokens, setTokens] = useState<ReadonlyMap<string, string>>(() => new Map());
  // The unmount cleanup reads this rather than closing over `machines`: an
  // effect with `[machines]` as its dependency runs its cleanup on every
  // reconcile, so pairing a second PC closed the first PC's stream and left
  // its dead `LiveMachine` in the map — "reconnecting" forever.
  const latest = useRef(machines);
  latest.current = machines;

  useEffect(() => {
    const wanted = new Map(pairings.map((p) => [p.id, p] as const));
    let changed = false;
    const next = new Map(machines);
    const nextTokens = new Map(tokens);

    for (const [id, machine] of machines) {
      const pairing = wanted.get(id);
      if (pairing === undefined || tokens.get(id) !== pairing.token) {
        machine.close();
        next.delete(id);
        nextTokens.delete(id);
        changed = true;
      }
    }
    for (const pairing of pairings) {
      if (!next.has(pairing.id)) {
        const machine = new LiveMachine(makeClient(pairing));
        machine.start();
        next.set(pairing.id, machine);
        nextTokens.set(pairing.id, pairing.token);
        changed = true;
      }
    }
    if (changed) {
      setMachines(next);
      setTokens(nextTokens);
    }
  }, [pairings, machines, tokens, makeClient]);

  // Close everything on unmount; the effect above only reconciles.
  useEffect(
    () => () => {
      for (const machine of latest.current.values()) machine.close();
    },
    [],
  );

  return machines;
}
