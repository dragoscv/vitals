/**
 * Process fixtures.
 *
 * Every field is set explicitly rather than spread from a "typical" process,
 * so a test that cares about a null GPU reading says so in its own body and
 * cannot be silently changed by an edit to a shared default.
 */

import type { Process } from '@vitals/protocol';

export function makeProcess(overrides: Partial<Process> & { pid: number }): Process {
  const { pid, ...rest } = overrides;
  return {
    key: { pid, startTime: pid * 1000 },
    parent: null,
    name: `proc-${pid}.exe`,
    kind: 'background',
    state: 'running',
    flags: 0,
    integrity: 'medium',
    protection: 'none',
    cpu: 0,
    memoryPrivate: 1024 * 1024,
    memoryWorkingSet: 2 * 1024 * 1024,
    diskRead: 0,
    diskWrite: 0,
    netRx: 0,
    netTx: 0,
    gpu: null,
    gpuMemory: null,
    threadCount: 4,
    handleCount: 100,
    user: 'tester',
    description: null,
    uptimeSecs: 60,
    ...rest,
  };
}

export function makeMap(processes: readonly Process[]): Map<string, Process> {
  return new Map(processes.map((p) => [`${p.key.pid}:${p.key.startTime}`, p]));
}

/** A synthetic population large enough to exercise the virtualiser. */
export function makePopulation(count: number): Map<string, Process> {
  const list: Process[] = [];
  for (let i = 0; i < count; i += 1) {
    list.push(
      makeProcess({
        pid: 1000 + i,
        name: `app-${i % 40}.exe`,
        kind: i % 7 === 0 ? 'app' : i % 11 === 0 ? 'system' : 'background',
        cpu: (i % 17) / 4,
        memoryPrivate: (i % 300) * 1024 * 1024,
      }),
    );
  }
  return makeMap(list);
}
