import { expect, it } from 'vitest';

import { makeSystem } from '../dashboard/test-fixtures';

import { TRACE_LENGTH, rollUp, reduceWidgetInput } from './live';
import type { WidgetInput } from './describe';

/**
 * The widget tree's traces are the only thing a widget shows over time, and
 * they are folded from two different sources — the main window's snapshot and
 * a widget window's frame broadcast. These tests pin the guarantees that make
 * those two paths agree and that keep a stalled sampler from costing anything.
 */

function empty(): WidgetInput {
  return {
    system: null,
    cpuHistory: [],
    memoryHistory: [],
    gpuHistory: [],
    netHistory: [],
    diskHistory: [],
  };
}

it('leaves the previous traces untouched when there is no reading', () => {
  const before = rollUp(empty(), makeSystem());

  const after = rollUp(before, null);

  // Not a deep comparison: identity is the guarantee. React and
  // `useSyncExternalStore` both compare by reference, so a fresh object per
  // absent frame would re-render every widget on a machine whose sampler
  // stalled — the failure this assertion exists to prevent.
  expect(after).toBe(before);
});

it('appends one point per trace for a reading that carries every number', () => {
  const input = rollUp(empty(), makeSystem());

  expect(input.cpuHistory).toHaveLength(1);
  expect(input.memoryHistory).toHaveLength(1);
  expect(input.gpuHistory).toHaveLength(1);
  expect(input.netHistory).toHaveLength(1);
  expect(input.diskHistory).toHaveLength(1);
  expect(input.system).not.toBeNull();
});

it('drops the oldest point once a trace is full so a widget cannot grow forever', () => {
  let input = empty();
  for (let index = 0; index < TRACE_LENGTH + 5; index += 1) {
    input = rollUp(input, makeSystem());
  }

  expect(input.cpuHistory).toHaveLength(TRACE_LENGTH);
});

it('breaks the line rather than drawing a plunge when a reading is missing', () => {
  // A machine whose GPU does not report utilisation has no GPU reading at all
  // (`gpuPercent` returns null for exactly this reason). Folding a zero in
  // would draw a cliff to the floor that never happened; the honest render is
  // a gap, so that trace must not advance.
  const unaudited = makeSystem({ gpus: [{ utilization: null }] as never });

  const input = rollUp(empty(), unaudited);

  expect(input.gpuHistory).toHaveLength(0);
  // The traces that do have a number still advance, so one absent sensor
  // cannot freeze a whole widget.
  expect(input.cpuHistory).toHaveLength(1);
});

it('folds a frame through the same function a snapshot uses', () => {
  const system = makeSystem();
  const frame = { payload: { system } } as unknown as Parameters<typeof reduceWidgetInput>[1];

  const viaFrame = reduceWidgetInput(empty(), frame);
  const viaSnapshot = rollUp(empty(), system);

  expect(viaFrame.cpuHistory).toEqual(viaSnapshot.cpuHistory);
  expect(viaFrame.netHistory).toEqual(viaSnapshot.netHistory);
});
