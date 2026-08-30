import { describe, expect, it } from 'vitest';

import { advanceSmoothing, deadbandCompare, reconcileOrder, smooth } from './ordering';

describe('smoothing', () => {
  it('adopts the first reading outright rather than easing up from zero', () => {
    // A process that appears at 30% CPU must be shown at 30%, otherwise the
    // number in the row and the position of the row disagree for seconds.
    expect(smooth(undefined, 30)).toBe(30);
  });

  it('damps a single-tick spike', () => {
    const settled = smooth(2, 40, 0.4);
    expect(settled).toBeLessThan(20);
    expect(settled).toBeGreaterThan(2);
  });

  it('converges on a sustained change', () => {
    let value = smooth(undefined, 0);
    for (let i = 0; i < 12; i += 1) value = smooth(value, 50, 0.4);
    expect(value).toBeGreaterThan(49);
  });

  it('drops keys that are no longer present so the table cannot grow forever', () => {
    const table = new Map<string, number>([['gone', 5]]);
    advanceSmoothing(table, new Map([['here', 1]]));
    expect(table.has('gone')).toBe(false);
    expect(table.has('here')).toBe(true);
  });
});

describe('deadbandCompare', () => {
  it('treats a difference smaller than the absolute band as equal', () => {
    expect(deadbandCompare(0.3, 0.4, 0.5, 0)).toBe(0);
  });

  it('still orders a difference that exceeds the band', () => {
    expect(deadbandCompare(0.3, 4, 0.5, 0)).toBe(-1);
  });

  it('scales the band with magnitude so large values do not shuffle', () => {
    const gigabyte = 1024 ** 3;
    expect(deadbandCompare(gigabyte, gigabyte + 20 * 1024 * 1024, 4 * 1024 * 1024, 0.05)).toBe(0);
  });

  it('sorts an unmeasurable value last in either direction', () => {
    expect(deadbandCompare(1, Number.NaN, 0, 0)).toBe(-1);
    expect(deadbandCompare(Number.NaN, 1, 0, 0)).toBe(1);
  });
});

describe('reconcileOrder', () => {
  const values = new Map<string, number>();
  const compare = (a: string, b: string): number =>
    deadbandCompare(values.get(b) ?? 0, values.get(a) ?? 0, 0.5, 0.05);

  it('keeps the order of rows whose values differ by less than the deadband', () => {
    values.clear();
    values.set('a', 5);
    values.set('b', 4.9);
    const first = reconcileOrder([], new Set(['a', 'b']), compare);
    expect(first).toEqual(['a', 'b']);

    // 'b' now reads marginally higher. Without hysteresis this swaps and the
    // user's click target moves; with it, nothing happens.
    values.set('a', 4.9);
    values.set('b', 5);
    expect(reconcileOrder(first, new Set(['a', 'b']), compare)).toEqual(['a', 'b']);
  });

  it('does reorder once a row genuinely overtakes another', () => {
    values.clear();
    values.set('a', 5);
    values.set('b', 1);
    const first = reconcileOrder([], new Set(['a', 'b']), compare);
    values.set('b', 40);
    expect(reconcileOrder(first, new Set(['a', 'b']), compare)).toEqual(['b', 'a']);
  });

  it('is stable across many noisy ticks', () => {
    values.clear();
    const keys = Array.from({ length: 30 }, (_, i) => `k${i}`);
    for (const key of keys) values.set(key, 10);

    let order = reconcileOrder([], new Set(keys), compare);
    const baseline = [...order];
    let moves = 0;
    for (let tick = 0; tick < 40; tick += 1) {
      // Peak-to-peak 0.4, inside the 0.52 band this comparator computes at a
      // value of 10. This is the sampling jitter that makes a real
      // CPU-sorted list unusable: every pair is "different" to a naive
      // comparator and none of the differences mean anything.
      for (const key of keys) values.set(key, 10 + Math.sin(tick * key.length) * 0.2);
      const next = reconcileOrder(order, new Set(keys), compare);
      next.forEach((id, index) => {
        if (baseline[index] !== id) moves += 1;
      });
      order = next;
    }
    expect(moves).toBe(0);
  });

  it('removes exited rows even while frozen, because a dead row is killable', () => {
    values.clear();
    values.set('a', 5);
    values.set('b', 4);
    const order = reconcileOrder([], new Set(['a', 'b']), compare);
    expect(reconcileOrder(order, new Set(['b']), compare, { frozen: true })).toEqual(['b']);
  });

  it('appends new rows while frozen so nothing already on screen shifts', () => {
    values.clear();
    values.set('a', 1);
    values.set('b', 2);
    values.set('new', 99);
    const order = reconcileOrder(['a', 'b'], new Set(['a', 'b']), compare);
    const frozen = reconcileOrder(order, new Set(['a', 'b', 'new']), compare, { frozen: true });
    // The busiest process by far, yet it lands at the bottom: inserting it at
    // the top would push every visible row down by one under the cursor.
    expect(frozen[frozen.length - 1]).toBe('new');
    expect(frozen.slice(0, 2)).toEqual(order);
  });

  it('does not duplicate a key present twice in the previous order', () => {
    values.clear();
    values.set('a', 1);
    expect(reconcileOrder(['a', 'a'], new Set(['a']), compare)).toEqual(['a']);
  });
});
