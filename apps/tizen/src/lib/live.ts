/**
 * One live connection per paired PC, shared by every screen that shows it.
 *
 * The Overview card and the PC screen show the same machine; opening two
 * event streams for it would double the PC's serialisation work for no
 * benefit, so connections are reference-counted and closed when the last
 * screen lets go.
 *
 * Frames arrive over `VitalsClient.stream` (server-sent events, token in the
 * query string — the documented browser fallback, since `EventSource` cannot
 * send headers). A 401 on an event stream is invisible to a browser, so the
 * token is proven first with `/health` and `/snapshot`; only then is the
 * stream opened. Otherwise a revoked token would cost one silent retry per
 * backoff ceiling, forever, with the card saying "Connecting…".
 */

import { VitalsClient, isVitalsError, type Unsubscribe } from '@vitals/client';
import { applyFrame } from '@vitals/protocol';
import type { Alert, Frame, Process, SystemMetrics } from '@vitals/protocol';

import { MODEL_VERSION, type Pairing } from './pairing';

/** The newest point is last. Sixty points is one minute at the PC's 1 Hz. */
export const SPARK_POINTS = 60;

export type LiveStatus = 'connecting' | 'live' | 'unreachable' | 'unauthorised' | 'incompatible';

export interface LiveState {
  readonly status: LiveStatus;
  readonly system: SystemMetrics | null;
  /** Every live process, keyed by `pid:startTime`. */
  readonly processes: ReadonlyMap<string, Process> | null;
  readonly cpu: readonly number[];
  readonly memory: readonly number[];
  readonly alerts: readonly Alert[];
  readonly lastSeenMs: number | null;
}

export const INITIAL: LiveState = {
  status: 'connecting',
  system: null,
  processes: null,
  cpu: [],
  memory: [],
  alerts: [],
  lastSeenMs: null,
};

function push(series: readonly number[], value: number): number[] {
  const next = series.length >= SPARK_POINTS ? series.slice(1) : [...series];
  next.push(value);
  return next;
}

/** Applies one frame; pure so the fold is testable without a socket. */
export function foldFrame(state: LiveState, frame: Frame, now: number): LiveState {
  const system = frame.payload.system;
  const processes = applyFrame(state.processes ?? new Map(), frame);
  const memory =
    system.memory.total > 0 ? (system.memory.used / system.memory.total) * 100 : Number.NaN;
  return {
    ...state,
    status: 'live',
    system,
    processes,
    cpu: push(state.cpu, system.cpu.total),
    memory: push(state.memory, memory),
    lastSeenMs: now,
  };
}

const ALERTS_EVERY_MS = 15_000;

export class LiveConnection {
  private state: LiveState = INITIAL;
  private readonly listeners = new Set<() => void>();
  private stopStream: Unsubscribe | undefined;
  private alertsTimer: ReturnType<typeof setInterval> | undefined;
  private epoch = 0;
  readonly client: VitalsClient;

  constructor(readonly pairing: Pairing) {
    this.client = new VitalsClient({
      baseUrl: pairing.baseUrl,
      token: pairing.token,
      // Capped low: a TV on the sofa should notice a PC waking up within
      // seconds, and there is only ever a handful of PCs.
      backoff: { maxMs: 10_000 },
    });
  }

  readonly get = (): LiveState => this.state;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  start(): void {
    const epoch = ++this.epoch;
    void this.open(epoch);
  }

  stop(): void {
    this.epoch++;
    this.stopStream?.();
    this.stopStream = undefined;
    if (this.alertsTimer !== undefined) clearInterval(this.alertsTimer);
    this.alertsTimer = undefined;
  }

  /** Try again now, e.g. after the user pressed Reconnect. */
  restart(): void {
    this.stop();
    this.set({ ...this.state, status: 'connecting' });
    this.start();
  }

  private async open(epoch: number): Promise<void> {
    try {
      const health = await this.client.health();
      if (epoch !== this.epoch) return;
      if (health.modelVersion !== MODEL_VERSION) {
        this.set({ ...this.state, status: 'incompatible' });
        return;
      }
      const first = await this.client.snapshot();
      if (epoch !== this.epoch) return;
      if (first !== null) this.set(foldFrame(this.state, first, Date.now()));
    } catch (error) {
      if (epoch !== this.epoch) return;
      if (isVitalsError(error) && error.kind === 'unauthorised') {
        this.set({ ...this.state, status: 'unauthorised' });
        return;
      }
      this.set({ ...this.state, status: 'unreachable' });
      // Asleep or off: look again shortly rather than giving up.
      setTimeout(() => {
        if (epoch === this.epoch) void this.open(epoch);
      }, 10_000);
      return;
    }
    this.stopStream = this.client.stream({
      onFrame: (frame) => {
        if (epoch === this.epoch) this.set(foldFrame(this.state, frame, Date.now()));
      },
      onDrop: () => {
        if (epoch === this.epoch && this.state.status === 'live') {
          this.set({ ...this.state, status: 'unreachable' });
        }
      },
    });
    void this.refreshAlerts(epoch);
    this.alertsTimer = setInterval(() => void this.refreshAlerts(epoch), ALERTS_EVERY_MS);
  }

  private async refreshAlerts(epoch: number): Promise<void> {
    try {
      const alerts = await this.client.alerts();
      if (epoch === this.epoch) this.set({ ...this.state, alerts });
    } catch {
      // Alerts are a nicety on top of the readings; a failed poll changes nothing.
    }
  }

  private set(next: LiveState): void {
    this.state = next;
    for (const listener of this.listeners) listener();
  }
}

interface Entry {
  readonly connection: LiveConnection;
  refs: number;
  closeTimer: ReturnType<typeof setTimeout> | undefined;
  running: boolean;
}

/**
 * One connection object per PC for the life of the app, so every screen that
 * shows a PC reads the same state. Only the stream behind it is opened and
 * closed with use; the object itself is a few fields.
 */
const entries = new Map<string, Entry>();

/** Keyed by id AND token, so re-pairing a PC replaces its connection. */
function keyOf(p: Pairing): string {
  return `${p.id}|${p.baseUrl}|${p.token}`;
}

function entryFor(pairing: Pairing): Entry {
  const key = keyOf(pairing);
  let entry = entries.get(key);
  if (entry === undefined) {
    entry = {
      connection: new LiveConnection(pairing),
      refs: 0,
      closeTimer: undefined,
      running: false,
    };
    entries.set(key, entry);
  }
  return entry;
}

/** The shared connection for a PC. Getting it starts nothing; {@link acquire} does. */
export function connectionFor(pairing: Pairing): LiveConnection {
  return entryFor(pairing).connection;
}

/**
 * Borrows a live connection. Release it when done; the stream is closed a
 * few seconds after the last release, so moving from Overview into the PC
 * screen does not tear it down and rebuild it.
 */
export function acquire(connection: LiveConnection): {
  connection: LiveConnection;
  release: () => void;
} {
  const held = entryFor(connection.pairing);
  if (held.closeTimer !== undefined) clearTimeout(held.closeTimer);
  held.closeTimer = undefined;
  held.refs++;
  if (!held.running) {
    held.running = true;
    held.connection.start();
  }
  let released = false;
  return {
    connection: held.connection,
    release: () => {
      if (released) return;
      released = true;
      held.refs--;
      if (held.refs > 0) return;
      held.closeTimer = setTimeout(() => {
        if (held.refs > 0) return;
        held.running = false;
        held.connection.stop();
      }, 5_000);
    },
  };
}
