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

  it('answers a reversed sort direction even while positions are frozen', () => {
    // A column header can only be clicked with the pointer over the table, so
    // `frozen` is necessarily true at that moment. If the freeze also swallows
    // the click, the header appears dead — the defect this guards.
    let clock = 0;
    const now = () => clock;
    const { result, rerender } = renderHook(
      ({ direction }: { direction: 'asc' | 'desc' }) =>
        useStableOrder({
          rows: rows([50, 1]),
          column: 'cpu',
          direction,
          frozen: true,
          locale: 'en',
          now,
        }),
      { initialProps: { direction: 'desc' as 'asc' | 'desc' } },
    );
    const pids = () => result.current.map((row) => row.process.key.pid);
    expect(pids()).toEqual([1, 2]);

    clock = 100;
    rerender({ direction: 'asc' });
    expect(pids()).toEqual([2, 1]);
  });

  it('orders a just-expanded subtree by name and pid rather than by the order the feed listed it', () => {
    // The children are all at 1% CPU, so the deadband declares every pair
    // equal and `compare` alone leaves them wherever the sampler happened to
    // put them. Expanded live, that reads as the sort having been ignored.
    const processes = makeMap([
      makeProcess({ pid: 1, name: 'parent.exe', cpu: 50 }),
      makeProcess({ pid: 12, name: 'zzz.exe', cpu: 1, parent: 1 }),
      makeProcess({ pid: 11, name: 'aaa.exe', cpu: 1, parent: 1 }),
      makeProcess({ pid: 13, name: 'aaa.exe', cpu: 1, parent: 1 }),
    ]);
    const built = (expanded: ReadonlySet<string>) =>
      buildRows({ processes, query: '', kind: 'all', grouped: true, expanded }).rows;

    let clock = 0;
    const { result, rerender } = renderHook(
      ({ expanded, frozen }: { expanded: ReadonlySet<string>; frozen: boolean }) =>
        useStableOrder({
          rows: built(expanded),
          column: 'cpu',
          direction: 'desc',
          frozen,
          locale: 'en',
          now: () => clock,
        }),
      { initialProps: { expanded: new Set<string>(), frozen: true } },
    );
    const pids = () => result.current.map((row) => row.process.key.pid);
    expect(pids()).toEqual([1]);

    const expanded = new Set(['1:1000']);
    rerender({ expanded, frozen: true });
    expect(pids()).toEqual([1, 11, 13, 12]);

    // The same rule off the freeze: expanding with the pointer elsewhere must
    // not depend on the frozen path to come out in a defined order.
    rerender({ expanded: new Set<string>(), frozen: false });
    clock = RESORT_INTERVAL_MS;
    rerender({ expanded, frozen: false });
    expect(pids()).toEqual([1, 11, 13, 12]);
  });
});
