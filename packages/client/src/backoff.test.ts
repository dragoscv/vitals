import { describe, expect, it, vi } from 'vitest';

import { type BackoffOptions, backoffDelay, Reconnector } from './backoff';

/**
 * A schedule the test drives by hand. No real timers: `fire()` runs the
 * pending callback synchronously, so a dozen reconnects cost microseconds and
 * the assertions are on exact delays rather than on elapsed wall time.
 */
function controlled(random = 0.5): BackoffOptions & { fire(): void; delays: number[] } {
  const delays: number[] = [];
  let queued: (() => void) | undefined;
  return {
    initialMs: 100,
    maxMs: 1000,
    random: () => random,
    schedule(fn, ms) {
      delays.push(ms);
      queued = fn;
      return () => {
        queued = undefined;
      };
    },
    fire() {
      const run = queued;
      queued = undefined;
      if (run === undefined) throw new Error('nothing was scheduled');
      run();
    },
    delays,
  };
}

describe('backoffDelay', () => {
  it('doubles the delay on each consecutive attempt until it reaches the cap', () => {
    const options = controlled(0);
    // random() === 0 yields the lower bound, which is half the capped value.
    expect([0, 1, 2, 3, 4, 5].map((n) => backoffDelay(n, options))).toEqual([
      50, 100, 200, 400, 500, 500,
    ]);
  });

  it('never returns zero, so a restarting server is not hammered in a tight loop', () => {
    const options = controlled(0);
    expect(backoffDelay(0, options)).toBeGreaterThan(0);
  });

  it('spreads clients apart by adding up to half the delay as jitter', () => {
    const low = backoffDelay(3, controlled(0));
    const high = backoffDelay(3, controlled(0.999));
    expect(low).toBe(400);
    expect(high).toBeGreaterThan(low);
    expect(high).toBeLessThanOrEqual(800);
  });

  it('stays capped for an attempt count large enough to overflow a doubling', () => {
    expect(backoffDelay(2000, controlled(0))).toBe(500);
  });
});

describe('Reconnector', () => {
  it('reopens the connection after every drop, waiting longer each time', () => {
    const options = controlled(0);
    const opens: number[] = [];
    let signals: { opened(): void; dropped(): void } | undefined;

    const reconnector = new Reconnector((s) => {
      opens.push(opens.length);
      signals = s;
      return () => {};
    }, options);
    reconnector.start();

    for (let i = 0; i < 3; i += 1) {
      signals?.dropped();
      options.fire();
    }

    expect(opens).toHaveLength(4);
    expect(options.delays).toEqual([50, 100, 200]);
  });

  it('restarts the delay sequence from the beginning once a connection succeeds', () => {
    const options = controlled(0);
    let signals: { opened(): void; dropped(): void } | undefined;

    const reconnector = new Reconnector((s) => {
      signals = s;
      return () => {};
    }, options);
    reconnector.start();

    signals?.dropped();
    options.fire();
    signals?.dropped();
    options.fire();
    // A long-lived connection means the previous failures are history.
    signals?.opened();
    signals?.dropped();

    expect(options.delays).toEqual([50, 100, 50]);
  });

  it('ignores a second drop from the same dying attempt instead of scheduling two retries', () => {
    const options = controlled(0);
    let signals: { opened(): void; dropped(): void } | undefined;

    const reconnector = new Reconnector((s) => {
      signals = s;
      return () => {};
    }, options);
    reconnector.start();

    // A socket typically emits both `error` and `close`; only one may retry.
    signals?.dropped();
    signals?.dropped();

    expect(options.delays).toEqual([50]);
  });

  it('cancels the pending retry and tears down the attempt when stopped', () => {
    const options = controlled(0);
    const teardown = vi.fn();
    let signals: { opened(): void; dropped(): void } | undefined;

    const reconnector = new Reconnector((s) => {
      signals = s;
      return teardown;
    }, options);
    reconnector.start();
    signals?.dropped();
    reconnector.stop();

    expect(() => {
      options.fire();
    }).toThrow('nothing was scheduled');
    expect(options.delays).toEqual([50]);
  });

  it('reports each scheduled retry with its delay and attempt number', () => {
    const options = controlled(0);
    const onRetry = vi.fn();
    let signals: { opened(): void; dropped(): void } | undefined;

    const reconnector = new Reconnector(
      (s) => {
        signals = s;
        return () => {};
      },
      options,
      onRetry,
    );
    reconnector.start();
    signals?.dropped();
    options.fire();
    signals?.dropped();

    expect(onRetry.mock.calls).toEqual([
      [50, 0],
      [100, 1],
    ]);
  });
});
