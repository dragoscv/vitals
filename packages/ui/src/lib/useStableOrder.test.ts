import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  stableOrderStep,
  stickyRowsStep,
  useStableOrder,
  type StableOrderState,
} from './useStableOrder';

const SETTLE = 30_000;

/** Replays ticks through the pure step and returns each shown order. */
function replay(
  ticks: readonly {
    keys: string[];
    ranks?: number[];
    at: number;
    reset?: unknown;
    held?: boolean;
  }[],
): string[][] {
  let state: StableOrderState | null = null;
  return ticks.map((tick) => {
    state = stableOrderStep(state, {
      keys: tick.keys,
      ranks: tick.ranks,
      now: tick.at,
      settleMs: SETTLE,
      resetKey: tick.reset,
      held: tick.held,
    });
    return [...state.order];
  });
}

describe('stableOrderStep', () => {
  it('keeps the shown order when only the values behind the source order change', () => {
    // The source swaps a and b every tick (last-activity churn); same ranks.
    const shown = replay([
      { keys: ['a', 'b', 'c'], ranks: [0, 0, 0], at: 0 },
      { keys: ['b', 'a', 'c'], ranks: [0, 0, 0], at: 1000 },
      { keys: ['c', 'a', 'b'], ranks: [0, 0, 0], at: 2000 },
    ]);
    expect(shown).toEqual([
      ['a', 'b', 'c'],
      ['a', 'b', 'c'],
      ['a', 'b', 'c'],
    ]);
  });

  it('inserts a new item where the source puts it rather than at the end', () => {
    const shown = replay([
      { keys: ['a', 'b', 'c'], at: 0 },
      { keys: ['a', 'new', 'b', 'c'], at: 1000 },
    ]);
    expect(shown[1]).toEqual(['a', 'new', 'b', 'c']);
  });

  it('drops an item that has gone without moving the others', () => {
    const shown = replay([
      { keys: ['a', 'b', 'c'], at: 0 },
      { keys: ['c', 'a'], at: 1000 },
    ]);
    expect(shown[1]).toEqual(['a', 'c']);
  });

  it('moves an item at once when its coarse rank changes, into its new group', () => {
    // b goes idle (rank 2): the source moves it after c, and so must the screen.
    const shown = replay([
      { keys: ['a', 'b', 'c'], ranks: [0, 0, 2], at: 0 },
      { keys: ['a', 'c', 'b'], ranks: [0, 2, 2], at: 1000 },
    ]);
    expect(shown[1]).toEqual(['a', 'c', 'b']);
  });

  it('adopts the source order whole once the settle interval has passed', () => {
    const shown = replay([
      { keys: ['a', 'b', 'c'], at: 0 },
      { keys: ['c', 'b', 'a'], at: SETTLE - 1 },
      { keys: ['c', 'b', 'a'], at: SETTLE },
    ]);
    expect(shown[1]).toEqual(['a', 'b', 'c']);
    expect(shown[2]).toEqual(['c', 'b', 'a']);
  });

  it('re-sorts immediately when the reset key changes, as for a new sort column', () => {
    const shown = replay([
      { keys: ['a', 'b'], at: 0, reset: 'cpu' },
      { keys: ['b', 'a'], at: 1000, reset: 'memory' },
    ]);
    expect(shown[1]).toEqual(['b', 'a']);
  });

  it('moves nothing while the pointer is over the list, then catches up once it leaves', () => {
    // b goes idle and a settle falls due, both while the user is pointing.
    const shown = replay([
      { keys: ['a', 'b', 'c'], ranks: [0, 0, 0], at: 0 },
      { keys: ['a', 'c', 'b'], ranks: [0, 0, 2], at: 1000, held: true },
      { keys: ['c', 'a', 'b'], ranks: [0, 0, 2], at: SETTLE + 1, held: true },
      { keys: ['c', 'a', 'b'], ranks: [0, 0, 2], at: SETTLE + 2 },
    ]);
    expect(shown[1]).toEqual(['a', 'b', 'c']);
    expect(shown[2]).toEqual(['a', 'b', 'c']);
    expect(shown[3]).toEqual(['c', 'a', 'b']);
  });

  it('still answers a new sort while held, because that change is the user clicking', () => {
    const shown = replay([
      { keys: ['a', 'b'], at: 0, reset: 'cpu' },
      { keys: ['b', 'a'], at: 1000, reset: 'memory', held: true },
    ]);
    expect(shown[1]).toEqual(['b', 'a']);
  });
});

describe('useStableOrder', () => {
  it('returns the held order across renders whose items arrive re-sorted', () => {
    const key = (s: { id: string }) => s.id;
    const rank = () => 0;
    const { result, rerender } = renderHook(
      ({ items, now }: { items: { id: string }[]; now: number }) =>
        useStableOrder(items, key, { rankOf: rank, now }),
      { initialProps: { items: [{ id: 'a' }, { id: 'b' }], now: 0 } },
    );
    rerender({ items: [{ id: 'b' }, { id: 'a' }], now: 1000 });
    expect(result.current.map(key)).toEqual(['a', 'b']);
    rerender({ items: [{ id: 'b' }, { id: 'a' }], now: SETTLE });
    expect(result.current.map(key)).toEqual(['b', 'a']);
  });
});

describe('stickyRowsStep', () => {
  const key = (r: { k: string }) => r.k;

  it('keeps the slot of a row that drops out and marks it absent', () => {
    const first = stickyRowsStep(new Map(), [], [{ k: 'cpu' }, { k: 'gpu' }], key);
    const second = stickyRowsStep(first.known, first.order, [{ k: 'gpu' }], key);
    expect(second.out.map((r) => [r.key, r.present])).toEqual([
      ['cpu', false],
      ['gpu', true],
    ]);
  });

  it('places a late sensor next to its neighbours, not at the end', () => {
    const first = stickyRowsStep(new Map(), [], [{ k: 'cpu' }, { k: 'disk' }], key);
    const second = stickyRowsStep(
      first.known,
      first.order,
      [{ k: 'cpu' }, { k: 'gpu' }, { k: 'disk' }],
      key,
    );
    expect(second.order).toEqual(['cpu', 'gpu', 'disk']);
  });
});
