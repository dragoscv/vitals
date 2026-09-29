import type { Frame, Process, SystemMetrics } from '@vitals/protocol';
import { describe, expect, it } from 'vitest';

import { INITIAL, SPARK_POINTS, foldFrame } from './live';

function system(cpu: number): SystemMetrics {
  return {
    cpu: { total: cpu, perCore: [cpu], kernel: 0 } as SystemMetrics['cpu'],
    memory: { total: 16, used: 4 } as SystemMetrics['memory'],
    disks: [],
    networks: [],
    gpus: [],
    powerDraw: null,
    battery: null,
    fans: [],
  };
}

function proc(pid: number, startTime: number, name: string): Process {
  return { key: { pid, startTime }, name } as Process;
}

function frame(seq: number, payload: Frame['payload']): Frame {
  return { seq, timestampMs: seq * 1000, elapsedMs: 1000, payload };
}

describe('foldFrame', () => {
  it('applies a delta on top of the keyframe, keeping a process whose PID was recycled within the tick', () => {
    const key = foldFrame(
      INITIAL,
      frame(1, {
        kind: 'keyframe',
        system: system(10),
        processes: [proc(4, 1, 'old.exe'), proc(8, 1, 'a.exe')],
      }),
      1,
    );
    const next = foldFrame(
      key,
      frame(2, {
        kind: 'delta',
        system: system(20),
        changed: [proc(4, 2, 'new.exe')],
        exited: [4],
      }),
      2,
    );
    expect([...(next.processes?.values() ?? [])].map((p) => p.name).sort()).toEqual([
      'a.exe',
      'new.exe',
    ]);
    expect(next.status).toBe('live');
    expect(next.cpu).toEqual([10, 20]);
    expect(next.memory).toEqual([25, 25]);
  });

  it('keeps the sparkline to the last minute so a TV left on all day does not grow without bound', () => {
    let state = INITIAL;
    for (let i = 0; i < SPARK_POINTS + 25; i++) {
      state = foldFrame(state, frame(i, { kind: 'keyframe', system: system(i), processes: [] }), i);
    }
    expect(state.cpu).toHaveLength(SPARK_POINTS);
    expect(state.cpu.at(-1)).toBe(SPARK_POINTS + 24);
  });
});
