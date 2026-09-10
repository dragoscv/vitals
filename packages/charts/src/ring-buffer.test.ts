import { describe, expect, it } from 'vitest';
import { RingBuffer } from './ring-buffer';

describe('RingBuffer', () => {
  it('rejects a non-positive capacity instead of silently misbehaving', () => {
    expect(() => new RingBuffer(0)).toThrow(RangeError);
    expect(() => new RingBuffer(-5)).toThrow(RangeError);
    expect(() => new RingBuffer(1.5)).toThrow(RangeError);
  });

  it('reads back samples oldest-first', () => {
    const buf = new RingBuffer(4);
    buf.push(1);
    buf.push(2);
    buf.push(3);

    expect(buf.size).toBe(3);
    expect(buf.toArray()).toEqual([1, 2, 3]);
    expect(buf.last).toBe(3);
  });

  it('overwrites the oldest sample once full', () => {
    const buf = new RingBuffer(3);
    for (const v of [1, 2, 3, 4, 5]) buf.push(v);

    expect(buf.size).toBe(3);
    expect(buf.isFull).toBe(true);
    expect(buf.toArray()).toEqual([3, 4, 5]);
  });

  it('keeps ordering stable across many wraps', () => {
    // Guards the modular arithmetic: an off-by-one in the head calculation
    // only shows up after the buffer has wrapped more than once.
    const buf = new RingBuffer(5);
    for (let i = 0; i < 53; i += 1) buf.push(i);

    expect(buf.toArray()).toEqual([48, 49, 50, 51, 52]);
    expect(buf.last).toBe(52);
  });

  it('returns undefined outside the retained window', () => {
    const buf = new RingBuffer(4);
    buf.push(1);

    expect(buf.at(-1)).toBeUndefined();
    expect(buf.at(1)).toBeUndefined();
    expect(buf.at(0)).toBe(1);
  });

  it('reports a zero extent when empty rather than Infinity', () => {
    // Infinity would propagate into the axis scale and blank the chart.
    expect(new RingBuffer(8).extent()).toEqual({ min: 0, max: 0 });
  });

  it('computes extent over the retained window only', () => {
    const buf = new RingBuffer(3);
    for (const v of [100, 1, 2, 3]) buf.push(v);

    expect(buf.extent()).toEqual({ min: 1, max: 3 });
  });

  it('excludes gaps from extent and average', () => {
    const buf = new RingBuffer(4);
    buf.push(10);
    buf.pushGap();
    buf.push(20);

    expect(buf.extent()).toEqual({ min: 10, max: 20 });
    expect(buf.average()).toBe(15);
  });

  it('treats an all-gap buffer as having no data', () => {
    const buf = new RingBuffer(3);
    buf.pushGap();
    buf.pushGap();

    expect(buf.extent()).toEqual({ min: 0, max: 0 });
    expect(buf.average()).toBe(0);
  });

  it('distinguishes a gap from a zero reading', () => {
    // The whole point: a dropped frame must not be drawn as 0% CPU.
    const gap = new RingBuffer(2);
    gap.pushGap();
    const zero = new RingBuffer(2);
    zero.push(0);

    expect(Number.isNaN(gap.last!)).toBe(true);
    expect(zero.last).toBe(0);
  });

  it('keeps a gap in place across a wrap and still excludes it from extent', () => {
    // Fills past capacity so the gap is read through the wrapped-head branch
    // of `at()`; a wrong start offset would either lose the gap or shift it
    // onto a neighbouring real sample.
    const buf = new RingBuffer(3);
    buf.push(100);
    buf.push(5);
    buf.pushGap();
    buf.push(7);

    expect(buf.toArray().map((v) => (Number.isNaN(v) ? 'gap' : v))).toEqual([5, 'gap', 7]);
    expect(buf.extent()).toEqual({ min: 5, max: 7 });
    expect(buf.average()).toBe(6);
  });

  it('clears back to an empty state', () => {
    const buf = new RingBuffer(3);
    buf.push(1);
    buf.push(2);
    buf.clear();

    expect(buf.size).toBe(0);
    expect(buf.last).toBeUndefined();
    expect(buf.isFull).toBe(false);
  });
});
