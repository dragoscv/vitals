import type { Process } from '@vitals/protocol';
import { describe, expect, it } from 'vitest';

import { FLAG_CRITICAL, isSafelyTerminable, sortProcesses } from './processes';

function p(pid: number, over: Partial<Process> = {}): Process {
  return {
    key: { pid, startTime: 1 },
    name: `p${pid}.exe`,
    cpu: 0,
    memoryPrivate: 0,
    flags: 0,
    protection: 'none',
    kind: 'app',
    ...over,
  } as Process;
}

describe('sortProcesses', () => {
  it('keeps idle programs in a stable order so focus does not jump every second', () => {
    const a = sortProcesses([p(30), p(10), p(20)], 'cpu').map((x) => x.key.pid);
    const b = sortProcesses([p(20), p(30), p(10)], 'cpu').map((x) => x.key.pid);
    expect(a).toEqual([10, 20, 30]);
    expect(b).toEqual(a);
  });

  it('puts the busiest first and sorts names without regard to case', () => {
    expect(sortProcesses([p(1, { cpu: 2 }), p(2, { cpu: 40 })], 'cpu')[0]?.key.pid).toBe(2);
    const names = sortProcesses(
      [p(1, { name: 'zed' }), p(2, { name: 'Alpha' }), p(3, { name: 'beta' })],
      'name',
    );
    expect(names.map((x) => x.name)).toEqual(['Alpha', 'beta', 'zed']);
  });
});

describe('isSafelyTerminable', () => {
  it('never offers to end a critical, protected or system process', () => {
    expect(isSafelyTerminable(p(1))).toBe(true);
    expect(isSafelyTerminable(p(1, { flags: FLAG_CRITICAL }))).toBe(false);
    expect(isSafelyTerminable(p(1, { protection: 'light' }))).toBe(false);
    expect(isSafelyTerminable(p(1, { kind: 'system' }))).toBe(false);
  });
});
