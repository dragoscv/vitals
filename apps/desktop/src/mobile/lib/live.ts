/**
 * One live connection per paired PC, held outside React.
 *
 * The stream must survive tab switches: unmounting the Machines tab and
 * reconnecting every SSE stream on return would cost a reconnect per PC per
 * tap, and the first frame after a reconnect is a keyframe, so the sparklines
 * would restart from nothing each time.
 */

import { useEffect, useState, useSyncExternalStore } from 'react';

import type { ControlRequest, VitalsClient } from '@vitals/client';
import type { Alert, Frame } from '@vitals/protocol';

import { fold, type Snapshot } from './frames';
import type { Pairing } from './pairing';

/** The subset of `VitalsClient` the phone uses; a test hands in a plain object. */
export type MobileClient = Pick<VitalsClient, 'health' | 'host' | 'stream' | 'control' | 'alerts'>;

export type ConnectionState = 'connecting' | 'live' | 'reconnecting' | 'unreachable';

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
}

const INITIAL: LiveState = {
  state: 'connecting',
  snapshot: null,
  cpuHistory: [],
  memoryHistory: [],
  alerts: null,
};

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
    const memoryPercent = memory.total > 0 ? (memory.used / memory.total) * 100 : 0;
    this.set({
      ...this.current,
      state: 'live',
      snapshot,
      cpuHistory: push(this.current.cpuHistory, snapshot.system.cpu.total),
      memoryHistory: push(this.current.memoryHistory, memoryPercent),
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
      for (const machine of machines.values()) machine.close();
    },
    [machines],
  );

  return machines;
}
