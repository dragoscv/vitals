import { describe, expect, it } from 'vitest';

import type { Frame, Process } from '@vitals/protocol';

import { fold } from './frames';

function proc(pid: number, startTime: number): Process {
  return { key: { pid, startTime }, name: `p${pid}`, cpu: 0 } as unknown as Process;
}

function frame(seq: number, payload: Frame['payload']): Frame {
  return { seq, timestampMs: seq * 1000, elapsedMs: 1000, payload };
}

const system = {} as Frame['payload']['system'];

describe('fold', () => {
  it('applies exits before changes so a PID recycled within one tick keeps the new process', () => {
    const key = fold(null, frame(1, { kind: 'keyframe', system, processes: [proc(100, 1)] }));
    const next = fold(
      key,
      frame(2, { kind: 'delta', system, exited: [100], changed: [proc(100, 2)] }),
    );
    expect(Array.from(next.processes.values()).map((p) => p.key.startTime)).toEqual([2]);
  });

  it('replaces everything on a keyframe so a process missing from it is gone', () => {
    const a = fold(
      null,
      frame(1, { kind: 'keyframe', system, processes: [proc(1, 1), proc(2, 1)] }),
    );
    const b = fold(a, frame(2, { kind: 'keyframe', system, processes: [proc(2, 1)] }));
    expect(b.processes.size).toBe(1);
  });
});
