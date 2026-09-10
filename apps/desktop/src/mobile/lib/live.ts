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
import type { Frame } from '@vitals/protocol';

import { fold, type Snapshot } from './frames';
import type { Pairing } from './pairing';

/** The subset of `VitalsClient` the phone uses; a test hands in a plain object. */
export type MobileClient = Pick<VitalsClient, 'health' | 'host' | 'stream' | 'control'>;

export type ConnectionState = 'connecting' | 'live' | 'reconnecting' | 'unreachable';

/** How many readings a card's sparkline remembers — about a minute at 1 Hz. */
export const HISTORY_LENGTH = 60;

/** Everything a screen needs about one PC. Immutable; a new object per frame. */
export interface LiveState {
  readonly state: ConnectionState;
  readonly snapshot: Snapshot | null;
  readonly cpuHistory: readonly number[];
  readonly memoryHistory: readonly number[];
}

const INITIAL: LiveState = {
  state: 'connecting',
  snapshot: null,
  cpuHistory: [],
  memoryHistory: [],
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

  constructor(private readonly client: MobileClient) {}

  /** Opens the stream. Idempotent. */
  start(): void {
    if (this.stop !== undefined) return;
    this.stop = this.client.stream({
      onFrame: (frame) => this.apply(frame),
      onOpen: () => this.set({ ...this.current, state: 'live' }),
      onDrop: () => {
        if (this.current.state === 'live') this.set({ ...this.current, state: 'reconnecting' });
      },
      onRetry: (_delay, attempt) => {
        if (attempt + 1 >= UNREACHABLE_AFTER) this.set({ ...this.current, state: 'unreachable' });
      },
    });
  }

  /** Closes the stream and cancels reconnects. */
  close(): void {
    this.stop?.();
    this.stop = undefined;
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
