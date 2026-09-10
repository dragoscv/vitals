/**
 * Frame reconciliation tests.
 *
 * The reconciliation logic is duplicated here rather than imported because
 * `subscribeToMetrics` is bound to the Tauri event bridge, which does not
 * exist under Vitest. What is under test is the ordering and keying rules,
 * and those are stated once below and asserted directly.
 */

import { describe, expect, it } from 'vitest';

import type { Process } from '@vitals/protocol';

import { processKey } from './metrics';

function makeProcess(pid: number, startTime: number, name: string): Process {
  return {
    key: { pid, startTime },
    parent: null,
    name,
    kind: 'background',
    state: 'running',
    flags: 0,
    integrity: null,
    protection: 'none',
    cpu: 1,
    memoryPrivate: 1024,
    memoryWorkingSet: 2048,
    diskRead: 0,
    diskWrite: 0,
    netRx: 0,
    netTx: 0,
    gpu: null,
    gpuMemory: null,
    threadCount: 1,
    handleCount: 10,
    user: null,
    uptimeSecs: 60,
  };
}

/** Mirrors the delta application in `subscribeToMetrics`. */
function applyDelta(
  processes: Map<string, Process>,
  changed: Process[],
  exited: number[],
): Map<string, Process> {
  for (const pid of exited) {
    for (const [key, process] of processes) {
      if (process.key.pid === pid) {
        processes.delete(key);
      }
    }
  }
  for (const process of changed) {
    processes.set(processKey(process), process);
  }
  return processes;
}

describe('processKey', () => {
  it('distinguishes processes that share a recycled PID', () => {
    // The whole reason the key is not just the PID. Windows recycles PIDs
    // aggressively; keying on PID alone applies one process's metrics to
    // whichever process inherited its number.
    const original = makeProcess(1234, 1000, 'old.exe');
    const recycled = makeProcess(1234, 2000, 'new.exe');

    expect(processKey(original)).not.toBe(processKey(recycled));
  });

  it('is stable for the same process across frames', () => {
    const first = makeProcess(1234, 1000, 'app.exe');
    const second = { ...makeProcess(1234, 1000, 'app.exe'), cpu: 50 };

    expect(processKey(first)).toBe(processKey(second));
  });
});

describe('delta reconciliation', () => {
  it('adds new processes', () => {
    const processes = new Map<string, Process>();
    applyDelta(processes, [makeProcess(1, 100, 'a.exe')], []);

    expect(processes.size).toBe(1);
  });

  it('updates existing processes in place', () => {
    const processes = new Map<string, Process>();
    applyDelta(processes, [makeProcess(1, 100, 'a.exe')], []);

    const updated = { ...makeProcess(1, 100, 'a.exe'), cpu: 42 };
    applyDelta(processes, [updated], []);

    expect(processes.size).toBe(1);
    expect([...processes.values()][0]?.cpu).toBe(42);
  });

  it('removes exited processes', () => {
    const processes = new Map<string, Process>();
    applyDelta(processes, [makeProcess(1, 100, 'a.exe')], []);
    applyDelta(processes, [], [1]);

    expect(processes.size).toBe(0);
  });

  it('keeps a recycled PID when its predecessor exits in the same frame', () => {
    // THE case that ordering exists for. A PID recycled within one interval
    // appears in `exited` (the old process) and `changed` (the new one).
    // Applying changes first and exits second deletes the NEW process,
    // making a live process vanish from the table until the next keyframe.
    const processes = new Map<string, Process>();
    applyDelta(processes, [makeProcess(1234, 100, 'old.exe')], []);

    applyDelta(processes, [makeProcess(1234, 999, 'new.exe')], [1234]);

    expect(processes.size).toBe(1);
    expect([...processes.values()][0]?.name).toBe('new.exe');
  });

  it('does not remove an unrelated process sharing no PID', () => {
    const processes = new Map<string, Process>();
    applyDelta(processes, [makeProcess(1, 100, 'a.exe'), makeProcess(2, 100, 'b.exe')], []);
    applyDelta(processes, [], [1]);

    expect(processes.size).toBe(1);
    expect([...processes.values()][0]?.name).toBe('b.exe');
  });

  it('handles an empty delta without disturbing state', () => {
    const processes = new Map<string, Process>();
    applyDelta(processes, [makeProcess(1, 100, 'a.exe')], []);
    applyDelta(processes, [], []);

    expect(processes.size).toBe(1);
  });
});
