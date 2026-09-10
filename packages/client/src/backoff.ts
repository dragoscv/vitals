/**
 * Reconnection policy shared by the SSE and WebSocket transports.
 *
 * The timer and the random source are injected so a test can step through
 * a dozen reconnects in microseconds and assert the exact delays. Production
 * code passes nothing and gets `setTimeout` and `Math.random`.
 */

/** Cancels a scheduled callback. */
export type Cancel = () => void;

/** Tunables for {@link Reconnector}. Every field has a production default. */
export interface BackoffOptions {
  /** Delay before the first retry; doubles on each consecutive failure. */
  initialMs: number;
  /**
   * Ceiling on the doubled delay. Without it a phone that was locked
   * overnight would come back to a client waiting hours for its next try.
   */
  maxMs: number;
  /** Uniform `[0, 1)` source for jitter. Injected so tests are deterministic. */
  random: () => number;
  /** Runs `fn` after `ms`; returns a canceller. Injected so tests need no real clock. */
  schedule: (fn: () => void, ms: number) => Cancel;
}

/** What production gets when the caller specifies nothing. */
export const DEFAULT_BACKOFF: Readonly<BackoffOptions> = {
  initialMs: 500,
  maxMs: 30_000,
  random: Math.random,
  schedule: (fn, ms) => {
    const handle = setTimeout(fn, ms);
    return () => {
      clearTimeout(handle);
    };
  },
};

/**
 * The delay before retry number `attempt` (zero-based).
 *
 * Equal jitter — half fixed, half random — rather than full jitter: full
 * jitter can return zero, and a zero delay against a server that is
 * restarting is a tight loop. Equal jitter still spreads a fleet of clients
 * that all lost the same server, which is what jitter is for.
 */
export function backoffDelay(attempt: number, options: BackoffOptions): number {
  // Clamp the exponent so a long outage cannot overflow to Infinity before
  // `min` gets a chance to cap it.
  const exponent = Math.min(attempt, 30);
  const capped = Math.min(options.maxMs, options.initialMs * 2 ** exponent);
  const half = capped / 2;
  return Math.round(half + options.random() * half);
}

/** Signals a transport reports to its {@link Reconnector}. */
export interface Attempt {
  /** The connection is established; the next drop starts from the first delay again. */
  opened(): void;
  /** The connection is gone. Schedules the next attempt unless stopped. */
  dropped(): void;
}

/**
 * Drives `open` repeatedly with capped exponential backoff.
 *
 * Owns exactly one live attempt at a time: `dropped()` from an attempt that
 * has already been superseded is ignored, which is what stops a late `error`
 * event from an old socket from scheduling a second, overlapping reconnect.
 */
export class Reconnector {
  private attempt = 0;
  private stopped = false;
  private pending: Cancel | undefined;
  private teardown: (() => void) | undefined;
  private generation = 0;

  constructor(
    private readonly open: (signals: Attempt) => () => void,
    private readonly options: BackoffOptions,
    private readonly onRetry?: (delayMs: number, attempt: number) => void,
  ) {}

  /** Opens the first attempt immediately. */
  start(): void {
    this.connect();
  }

  /** Closes the live attempt, cancels any scheduled retry, and stays closed. */
  stop(): void {
    this.stopped = true;
    this.pending?.();
    this.pending = undefined;
    this.teardown?.();
    this.teardown = undefined;
  }

  private connect(): void {
    if (this.stopped) return;
    this.generation += 1;
    const mine = this.generation;
    const signals: Attempt = {
      opened: () => {
        if (mine === this.generation) this.attempt = 0;
      },
      dropped: () => {
        if (mine !== this.generation || this.stopped) return;
        this.teardown = undefined;
        this.retry();
      },
    };
    this.teardown = this.open(signals);
  }

  private retry(): void {
    const delay = backoffDelay(this.attempt, this.options);
    this.onRetry?.(delay, this.attempt);
    this.attempt += 1;
    // Bump the generation now, not when the timer fires, so a second
    // `dropped()` from the same dying socket cannot schedule a second timer.
    this.generation += 1;
    this.pending = this.options.schedule(() => {
      this.pending = undefined;
      this.connect();
    }, delay);
  }
}
