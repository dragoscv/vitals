/**
 * Cost of one tick at realistic scale.
 *
 * The budget is deliberately generous — CI runners are shared and a tight
 * threshold produces flaky failures that get muted, which is worse than no
 * test. What this guards against is an order-of-magnitude regression: an
 * accidental O(n²) in the tree walk or the comparator, which is the failure
 * that would actually make the screen unusable and which no unit test of
 * behaviour would catch.
 *
 * Measured on the development machine (32 cores, Node 26): 0.24 ms at 200
 * processes, 0.45 ms at 600, 0.75 ms at 1200 — linear, and roughly 0.05% of
 * the 1 Hz budget.
 */

import { describe, expect, it } from 'vitest';

import { DEADBAND, buildRows, sortValue } from './model';
import { advanceSmoothing, deadbandCompare, reconcileOrder } from './ordering';
import { makePopulation } from './test-fixtures';

const COUNT = 600;
const TICKS = 20;

describe('per-tick cost at 600 processes', () => {
  it('builds and orders a tick well inside the 1 Hz budget', () => {
    const processes = makePopulation(COUNT);
    const smoothed = new Map<string, number>();
    let order: readonly string[] = [];

    // One untimed pass so the measurement is of steady state, not of the
    // first-run cost of building the parent index and the smoothing table.
    {
      const built = buildRows({
        processes,
        query: '',
        kind: 'all',
        grouped: true,
        expanded: new Set(),
      });
      const readings = new Map(built.rows.map((r) => [r.id, sortValue(r, 'cpu')] as const));
      advanceSmoothing(smoothed, readings);
      order = reconcileOrder(order, new Set(readings.keys()), () => 0);
    }

    const started = performance.now();
    for (let tick = 0; tick < TICKS; tick += 1) {
      const built = buildRows({
        processes,
        query: '',
        kind: 'all',
        grouped: true,
        expanded: new Set(),
      });
      const readings = new Map(built.rows.map((r) => [r.id, sortValue(r, 'cpu')] as const));
      advanceSmoothing(smoothed, readings);
      const band = DEADBAND.cpu;
      order = reconcileOrder(
        order,
        new Set(readings.keys()),
        (a, b) =>
          -deadbandCompare(
            smoothed.get(a) ?? Number.NaN,
            smoothed.get(b) ?? Number.NaN,
            band.absolute,
            band.relative,
          ),
      );
    }
    const perTick = (performance.now() - started) / TICKS;

    // 50 ms of a 1000 ms budget against a measured 0.45 ms. The headroom is
    // there to absorb a slow shared CI runner; a quadratic regression would
    // still blow past it long before the screen became visibly janky.
    expect(perTick).toBeLessThan(50);
  });

  it('stays near-linear once the order has settled, because the sort is insertion on sorted input', () => {
    const processes = makePopulation(COUNT);
    const rows = buildRows({
      processes,
      query: '',
      kind: 'all',
      grouped: false,
      expanded: new Set(),
    }).rows;
    const keys = new Set(rows.map((r) => r.id));
    const values = new Map(rows.map((r) => [r.id, r.rolledCpu] as const));
    const compare = (a: string, b: string): number =>
      deadbandCompare(values.get(a) ?? 0, values.get(b) ?? 0, 0.5, 0.05);

    const cold = reconcileOrder([], keys, compare);

    const started = performance.now();
    let order = cold;
    for (let i = 0; i < 20; i += 1) order = reconcileOrder(order, keys, compare);
    const warm = (performance.now() - started) / 20;

    // A resort of already-ordered input must not cost what the first sort did.
    expect(warm).toBeLessThan(20);
  });

  it('filters a full population quickly enough to run on every keystroke', () => {
    const processes = makePopulation(COUNT);
    const started = performance.now();
    for (let i = 0; i < 20; i += 1) {
      buildRows({
        processes,
        query: 'app-1',
        kind: 'all',
        grouped: true,
        expanded: new Set(),
      });
    }
    expect((performance.now() - started) / 20).toBeLessThan(50);
  });
});
