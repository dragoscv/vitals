import { describe, expect, it } from 'vitest';

import type { SystemMetrics } from '@vitals/protocol';

import { makeSystem } from '../dashboard/test-fixtures';
import { columnTotals } from './totals';

function system(patch: Partial<SystemMetrics>): SystemMetrics {
  return { ...makeSystem(), ...patch };
}

describe('columnTotals', () => {
  it('reports nothing before the first frame rather than zeros', () => {
    expect(columnTotals(null)).toEqual({});
  });

  it('shows memory in use as a share of installed memory', () => {
    const base = makeSystem();
    const totals = columnTotals(
      system({ memory: { ...base.memory, total: 32 * 2 ** 30, used: 8 * 2 ** 30 } }),
    );
    expect(totals.memory).toBe(25);
  });

  it('takes the busiest disk so an idle second disk cannot hide a pegged one', () => {
    const [disk] = makeSystem().disks;
    if (disk === undefined) throw new Error('fixture has a disk');
    const totals = columnTotals(
      system({
        disks: [
          { ...disk, activeTime: 97 },
          { ...disk, activeTime: 1 },
        ],
      }),
    );
    expect(totals.disk).toBe(97);
  });

  it('treats a GPU with no utilisation counter as unknown, not idle', () => {
    const [gpu] = makeSystem().gpus;
    if (gpu === undefined) throw new Error('fixture has a GPU');
    expect(columnTotals(system({ gpus: [{ ...gpu, utilization: null }] })).gpu).toBeNull();
    expect(columnTotals(system({ gpus: [] })).gpu).toBeNull();
  });

  it('measures network against link speed and ignores links with no speed', () => {
    const [nic] = makeSystem().networks;
    if (nic === undefined) throw new Error('fixture has a NIC');
    const totals = columnTotals(
      system({
        networks: [
          // 12.5 MB/s down on a gigabit link: 100 Mbit/s of 1000 = 10 %.
          { ...nic, connected: true, rx: 12_500_000, tx: 0, linkSpeed: 1_000_000_000 },
          { ...nic, connected: true, rx: 99_000_000, tx: 0, linkSpeed: null },
        ],
      }),
    );
    expect(totals.network).toBeCloseTo(10, 5);
    expect(
      columnTotals(system({ networks: [{ ...nic, connected: true, linkSpeed: null }] })).network,
    ).toBeNull();
  });
});
