import fc from 'fast-check';
import { describe, expect, it } from 'vitest';

import { processKeyId } from '@vitals/protocol';
import type { Frame, Process } from '@vitals/protocol';

import { fold, type Snapshot } from './frames';

/**
 * Properties of the phone's frame fold.
 *
 * The example tests in `frames.test.ts` pin the two orderings someone once got
 * wrong. These state what the phone's process list promises for every stream:
 * that it is exactly the machine the sampler described, however that
 * description was split into a keyframe and deltas.
 */

const RUNS = 256;

// Small ranges on purpose: PID collisions and recycling are the interesting
// cases, and they only happen often when the space is tight.
const pidArb = fc.integer({ min: 1, max: 12 });
const startArb = fc.integer({ min: 1, max: 4 });

function proc(pid: number, startTime: number, cpu = 0): Process {
  return { key: { pid, startTime }, name: `p${pid}`, cpu } as unknown as Process;
}

const processArb = fc
  .tuple(pidArb, startArb, fc.integer({ min: 0, max: 100 }))
  .map(([pid, start, cpu]) => proc(pid, start, cpu));

/** What the sampler guarantees of one tick: PIDs are unique among the living. */
const livingArb = fc.uniqueArray(processArb, {
  maxLength: 8,
  selector: (p) => p.key.pid,
});

const system = {} as Frame['payload']['system'];

function frame(seq: number, payload: Frame['payload']): Frame {
  return { seq, timestampMs: seq * 1000, elapsedMs: 1000, payload };
}

function keyframe(seq: number, processes: readonly Process[]): Frame {
  return frame(seq, { kind: 'keyframe', system, processes: [...processes] });
}

function delta(seq: number, changed: readonly Process[], exited: readonly number[]): Frame {
  return frame(seq, { kind: 'delta', system, changed: [...changed], exited: [...exited] });
}

/**
 * The delta a correct sampler would send to move from `before` to `after`:
 * every PID whose owner vanished or was replaced exits, every new or changed
 * row is listed.
 */
function diff(seq: number, before: readonly Process[], after: readonly Process[]): Frame {
  const now = new Map(after.map((p) => [p.key.pid, p] as const));
  const exited = before
    .filter((p) => now.get(p.key.pid)?.key.startTime !== p.key.startTime)
    .map((p) => p.key.pid);
  const was = new Map(before.map((p) => [processKeyId(p.key), p] as const));
  const changed = after.filter((p) => was.get(processKeyId(p.key))?.cpu !== p.cpu);
  return delta(seq, changed, exited);
}

function materialised(snapshot: Snapshot | null): Map<string, number> {
  const out = new Map<string, number>();
  for (const [id, p] of snapshot?.processes ?? []) out.set(id, p.cpu);
  return out;
}

function foldAll(frames: readonly Frame[], from: Snapshot | null = null): Snapshot | null {
  return frames.reduce<Snapshot | null>((s, f) => fold(s, f), from);
}

describe('fold, for any stream', () => {
  it('folding a keyframe then deltas gives the same process list as a keyframe of the final state', () => {
    fc.assert(
      fc.property(fc.array(livingArb, { minLength: 1, maxLength: 10 }), (states) => {
        const frames: Frame[] = [keyframe(1, states[0] ?? [])];
        for (let i = 1; i < states.length; i++) {
          frames.push(diff(i + 1, states[i - 1] ?? [], states[i] ?? []));
        }

        const final = states[states.length - 1] ?? [];
        const folded = foldAll(frames);
        const fresh = fold(null, keyframe(frames.length + 1, final));

        expect(materialised(folded)).toEqual(materialised(fresh));
      }),
      { numRuns: RUNS },
    );
  });

  it('a keyframe discards everything folded before it', () => {
    fc.assert(
      fc.property(
        livingArb,
        fc.array(fc.tuple(fc.array(processArb, { maxLength: 4 }), fc.array(pidArb)), {
          maxLength: 6,
        }),
        livingArb,
        (start, deltas, reset) => {
          const noise = [
            keyframe(1, start),
            ...deltas.map(([changed, exited], i) => delta(i + 2, changed, exited)),
          ];
          const after = fold(foldAll(noise), keyframe(noise.length + 1, reset));

          expect(materialised(after)).toEqual(materialised(fold(null, keyframe(1, reset))));
        },
      ),
      { numRuns: RUNS },
    );
  });

  it('a delta that arrives before any keyframe neither throws nor invents processes it did not list', () => {
    fc.assert(
      fc.property(fc.array(processArb, { maxLength: 6 }), fc.array(pidArb), (changed, exited) => {
        const snapshot = fold(null, delta(1, changed, exited));
        const listed = new Set(changed.map((p) => processKeyId(p.key)));

        for (const id of snapshot.processes.keys()) expect(listed.has(id)).toBe(true);
      }),
      { numRuns: RUNS },
    );
  });

  it('an exited PID never reappears unless a later frame lists it again', () => {
    fc.assert(
      fc.property(
        livingArb,
        fc.array(fc.tuple(fc.array(processArb, { maxLength: 4 }), fc.array(pidArb)), {
          minLength: 1,
          maxLength: 8,
        }),
        (start, deltas) => {
          let snapshot = fold(null, keyframe(1, start));
          /** PIDs exited and not re-added since, by the frames seen so far. */
          const gone = new Set<number>();

          deltas.forEach(([changed, exited], i) => {
            snapshot = fold(snapshot, delta(i + 2, changed, exited));
            for (const pid of exited) gone.add(pid);
            for (const p of changed) gone.delete(p.key.pid);

            for (const p of snapshot.processes.values()) {
              expect(gone.has(p.key.pid)).toBe(false);
            }
          });
        },
      ),
      { numRuns: RUNS },
    );
  });

  it('a process listed as changed is always present afterwards with the values it was listed with', () => {
    fc.assert(
      fc.property(
        livingArb,
        fc.array(processArb, { maxLength: 6 }),
        fc.array(pidArb),
        (start, changed, exited) => {
          const snapshot = fold(fold(null, keyframe(1, start)), delta(2, changed, exited));
          // The last listing of a key wins, as it would in the sampler's own map.
          const expected = new Map(changed.map((p) => [processKeyId(p.key), p.cpu] as const));

          for (const [id, cpu] of expected) {
            expect(snapshot.processes.get(id)?.cpu).toBe(cpu);
          }
        },
      ),
      { numRuns: RUNS },
    );
  });

  it('folding never mutates the snapshot it was given', () => {
    fc.assert(
      fc.property(
        livingArb,
        fc.array(processArb, { maxLength: 6 }),
        fc.array(pidArb),
        (start, changed, exited) => {
          const before = fold(null, keyframe(1, start));
          const copy = materialised(before);
          fold(before, delta(2, changed, exited));

          expect(materialised(before)).toEqual(copy);
        },
      ),
      { numRuns: RUNS },
    );
  });
});
