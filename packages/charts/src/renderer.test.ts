import { describe, expect, it } from 'vitest';
import { computeBounds, type Series } from './renderer';
import { RingBuffer } from './ring-buffer';

function seriesOf(values: number[]): Series {
  const buffer = new RingBuffer(Math.max(values.length, 1));
  for (const v of values) buffer.push(v);
  return { buffer, color: '#000' };
}

describe('computeBounds', () => {
  it('honours a fully fixed scale without inspecting the data', () => {
    const bounds = computeBounds([seriesOf([1, 2, 3])], { min: 0, max: 100 });
    expect(bounds).toEqual({ min: 0, max: 100 });
  });

  it('produces a usable range for an empty series', () => {
    // A degenerate range would divide by zero when projecting to pixels.
    const bounds = computeBounds([seriesOf([])]);
    expect(bounds.max).toBeGreaterThan(bounds.min);
  });

  it('autoscales across every series, not just the first', () => {
    const bounds = computeBounds([seriesOf([1, 2]), seriesOf([50, 60])], { padding: 0 });
    expect(bounds.min).toBe(0);
    expect(bounds.max).toBe(60);
  });

  it('adds headroom so the peak is not flush against the top edge', () => {
    const bounds = computeBounds([seriesOf([0, 100])], { padding: 0.2 });
    expect(bounds.max).toBeCloseTo(120);
  });

  it('snaps the maximum so the axis stops jittering frame to frame', () => {
    // The point of snapping: 61 and 67 must land on the same axis, otherwise
    // a steady signal looks like it is breathing.
    const a = computeBounds([seriesOf([0, 61])], { snapTo: 25, padding: 0 });
    const b = computeBounds([seriesOf([0, 67])], { snapTo: 25, padding: 0 });
    expect(a.max).toBe(75);
    expect(b.max).toBe(75);
  });

  it('does not snap when the maximum is pinned', () => {
    const bounds = computeBounds([seriesOf([0, 61])], { max: 100, snapTo: 25 });
    expect(bounds.max).toBe(100);
  });

  it('guarantees a non-zero range for a flat signal', () => {
    const bounds = computeBounds([seriesOf([5, 5, 5])], { min: 5, padding: 0 });
    expect(bounds.max).toBeGreaterThan(bounds.min);
  });

  it('ignores gaps when autoscaling', () => {
    const buffer = new RingBuffer(4);
    buffer.push(10);
    buffer.pushGap();
    buffer.push(20);

    const bounds = computeBounds([{ buffer, color: '#000' }], { padding: 0 });
    expect(bounds.max).toBe(20);
  });

  it('includes zero in the baseline so small values are not exaggerated', () => {
    // Without a zero baseline, a CPU hovering between 3% and 4% would render
    // as a dramatic mountain range. That is a lie about the machine's state.
    const bounds = computeBounds([seriesOf([3, 4])], { padding: 0 });
    expect(bounds.min).toBe(0);
  });
});
