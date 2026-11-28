/**
 * Fixtures for the remote-widget tests.
 *
 * Kept out of `remote.ts` on purpose: the module must stay free of anything a
 * test needs and production does not, and a scripted `fetch` living beside the
 * real poll loop is how a shipped file ends up carrying test branches.
 *
 * `makeSystem` is reused from the dashboard fixtures rather than rebuilt here,
 * so a change to the protocol's shape fixes one builder, not two — and so the
 * "remote traces equal local traces" test can fold the SAME reading down both
 * paths and compare like for like.
 */

import type { Summary, SystemMetrics } from '@vitals/protocol';

/** One scripted HTTP reply. */
export interface Reply {
  readonly status: number;
  readonly body?: string;
  readonly contentType?: string;
}

/** A `Summary` — the machine now, with an empty process list — for one reading. */
export function makeSummary(system: SystemMetrics, overrides: Partial<Summary> = {}): Summary {
  return {
    seq: 1,
    timestampMs: 1_700_000_000_000,
    system,
    top: [],
    processCount: null,
    ...overrides,
  };
}

/** What a scripted `fetch` recorded and what it will replay. */
export interface ScriptedFetch {
  readonly impl: typeof globalThis.fetch;
  /** Every URL asked for, in order — the evidence that a poll did or did not happen. */
  readonly calls: string[];
}

/**
 * A `fetch` that replays `replies` in order and then fails the way an
 * unreachable host does.
 *
 * The exhaustion behaviour is the point: a machine that answered once and then
 * went away is exactly what the "keeps the previous traces" test needs, and a
 * rejection is what the real client turns into a `network` `VitalsError`.
 */
export function scriptedFetch(...replies: Reply[]): ScriptedFetch {
  const calls: string[] = [];
  const impl: typeof globalThis.fetch = (input) => {
    calls.push(input instanceof Request ? input.url : input.toString());
    const next = replies.shift();
    if (next === undefined) return Promise.reject(new TypeError('fetch failed'));
    return Promise.resolve(
      new Response(next.body ?? null, {
        status: next.status,
        headers: { 'Content-Type': next.contentType ?? 'application/json' },
      }),
    );
  };
  return { impl, calls };
}
