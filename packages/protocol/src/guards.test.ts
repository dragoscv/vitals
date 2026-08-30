import { describe, expect, it } from 'vitest';

import { applyFrame, processKeyId, sameProcess } from './guards';
import type { Frame } from './generated/core/Frame';
import type { Process } from './generated/core/Process';
import type { SystemMetrics } from './generated/core/SystemMetrics';

const EMPTY_SYSTEM = {
  cpu: {
    total: 0,
    perCore: [],
    kernel: 0,
    effectiveClock: null,
    maxClock: null,
    temperature: null,
    power: null,
    throttled: null,
    processCount: 0,
    threadCount: 0,
    handleCount: null,
    uptimeSecs: 0,
    contextSwitches: null,
    interrupts: null,
  },
  memory: {
    total: 0,
    used: 0,
    available: 0,
    cached: 0,
    pagedPool: 0,
    nonPagedPool: 0,
    committed: 0,
    commitLimit: 0,
    swapTotal: 0,
    swapUsed: 0,
    hardwareReserved: 0,
    pageFaultsPerSec: null,
    speed: null,
    slotsUsed: null,
    slotsTotal: null,
    formFactor: null,
  },
  disks: [],
  networks: [],
  gpus: [],
  powerDraw: null,
  battery: null,
} satisfies SystemMetrics;

function process(pid: number, startTime: number, name = 'test.exe'): Process {
  return {
    key: { pid, startTime },
    parent: null,
    name,
    kind: 'app',
    state: 'running',
    flags: 0,
    integrity: null,
    protection: 'none',
    cpu: 0,
    memoryPrivate: 0,
    memoryWorkingSet: 0,
    diskRead: 0,
    diskWrite: 0,
    netRx: 0,
    netTx: 0,
    gpu: null,
    gpuMemory: null,
    threadCount: 1,
    handleCount: null,
    user: null,
    uptimeSecs: 0,
  };
}

function keyframe(processes: Process[]): Frame {
  return {
    seq: 1,
    timestampMs: 0,
    elapsedMs: 1000,
    payload: { kind: 'keyframe', system: EMPTY_SYSTEM, processes },
  };
}

function delta(changed: Process[], exited: number[]): Frame {
  return {
    seq: 2,
    timestampMs: 1000,
    elapsedMs: 1000,
    payload: { kind: 'delta', system: EMPTY_SYSTEM, changed, exited },
  };
}

describe('processKeyId', () => {
  it('distinguishes a recycled PID from the original process', () => {
    // The reason a bare PID is unusable as a React key: reusing it would let
    // React carry a row's DOM, selection and animation state across two
    // completely unrelated processes.
    expect(processKeyId({ pid: 42, startTime: 100 })).not.toBe(
      processKeyId({ pid: 42, startTime: 200 }),
    );
  });

  it('is stable for the same process', () => {
    expect(processKeyId({ pid: 42, startTime: 100 })).toBe(
      processKeyId({ pid: 42, startTime: 100 }),
    );
  });
});

describe('sameProcess', () => {
  it('requires both PID and start time to match', () => {
    expect(sameProcess({ pid: 1, startTime: 5 }, { pid: 1, startTime: 5 })).toBe(true);
    expect(sameProcess({ pid: 1, startTime: 5 }, { pid: 1, startTime: 6 })).toBe(false);
    expect(sameProcess({ pid: 1, startTime: 5 }, { pid: 2, startTime: 5 })).toBe(false);
  });
});

describe('applyFrame', () => {
  it('replaces the whole map on a keyframe', () => {
    const stale = new Map([[processKeyId({ pid: 9, startTime: 1 }), process(9, 1)]]);
    const next = applyFrame(stale, keyframe([process(1, 1), process(2, 1)]));

    expect(next.size).toBe(2);
    expect(next.has(processKeyId({ pid: 9, startTime: 1 }))).toBe(false);
  });

  it('merges changed processes without dropping untouched ones', () => {
    const current = applyFrame(
      new Map(),
      keyframe([process(1, 1, 'a.exe'), process(2, 1, 'b.exe')]),
    );
    const next = applyFrame(current, delta([process(1, 1, 'a.exe')], []));

    expect(next.size).toBe(2);
    expect(next.get(processKeyId({ pid: 2, startTime: 1 }))?.name).toBe('b.exe');
  });

  it('removes exited processes', () => {
    const current = applyFrame(new Map(), keyframe([process(1, 1), process(2, 1)]));
    const next = applyFrame(current, delta([], [1]));

    expect(next.size).toBe(1);
    expect(next.has(processKeyId({ pid: 1, startTime: 1 }))).toBe(false);
  });

  it('keeps a recycled PID that exits and reappears in the same frame', () => {
    // The nastiest case: PID 1 exits and the OS immediately hands the number
    // to a new process. Naive removal-after-merge would delete the newcomer
    // and the table would silently lose a live process.
    const current = applyFrame(new Map(), keyframe([process(1, 100, 'old.exe')]));
    const next = applyFrame(current, delta([process(1, 999, 'new.exe')], [1]));

    expect(next.size).toBe(1);
    // The newcomer survives; the old instance is gone.
    expect(next.get(processKeyId({ pid: 1, startTime: 999 }))?.name).toBe('new.exe');
    expect(next.has(processKeyId({ pid: 1, startTime: 100 }))).toBe(false);
  });

  it('does not mutate the map it was given', () => {
    // The store relies on reference inequality to trigger a re-render;
    // mutating in place would make the UI stop updating.
    const current = applyFrame(new Map(), keyframe([process(1, 1)]));
    const next = applyFrame(current, delta([process(2, 1)], []));

    expect(current.size).toBe(1);
    expect(next.size).toBe(2);
    expect(next).not.toBe(current);
  });

  it('handles an empty delta as a no-op', () => {
    const current = applyFrame(new Map(), keyframe([process(1, 1)]));
    const next = applyFrame(current, delta([], []));

    expect(next.size).toBe(1);
  });
});
