import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { buildRows } from './model';
import { makeMap, makeProcess } from './test-fixtures';
import { RESORT_INTERVAL_MS, useStableOrder } from './useStableOrder';

function rows(cpu: readonly [number, number]) {
  return buildRows({
    processes: makeMap([
      makeProcess({ pid: 1, name: 'a.exe', cpu: cpu[0] }),
      makeProcess({ pid: 2, name: 'b.exe', cpu: cpu[1] }),
    ]),
    query: '',
    kind: 'all',
    grouped: false,
    expanded: new Set(),
  }).rows;
}

describe('useStableOrder (processes)', () => {
  it('re-sorts a decisive overtake only once the settle interval has passed', () => {
    let clock = 0;
    const now = () => clock;
    const { result, rerender } = renderHook(
      ({ cpu }: { cpu: readonly [number, number] }) =>
        useStableOrder({
          rows: rows(cpu),
          column: 'cpu',
          direction: 'desc',
          frozen: false,
          locale: 'en',
          now,
        }),
      { initialProps: { cpu: [50, 1] as readonly [number, number] } },
    );
    const pids = () => result.current.map((row) => row.process.key.pid);
    expect(pids()).toEqual([1, 2]);

    // b overtakes decisively, but within the interval: positions hold.
    for (let i = 1; i <= 2; i += 1) {
      clock = i * 1000;
      rerender({ cpu: [1, 90] });
    }
    expect(pids()).toEqual([1, 2]);

    clock = RESORT_INTERVAL_MS;
    rerender({ cpu: [1, 90] });
    expect(pids()).toEqual([2, 1]);
  });

  it('answers a reversed sort direction at once rather than at the next settle', () => {
    let clock = 0;
    const now = () => clock;
    const { result, rerender } = renderHook(
      ({ direction }: { direction: 'asc' | 'desc' }) =>
        useStableOrder({
          rows: rows([50, 1]),
          column: 'cpu',
          direction,
          frozen: false,
          locale: 'en',
          now,
        }),
      { initialProps: { direction: 'desc' as 'asc' | 'desc' } },
    );
    clock = 100;
    rerender({ direction: 'asc' });
    expect(result.current.map((row) => row.process.key.pid)).toEqual([2, 1]);
  });
});
