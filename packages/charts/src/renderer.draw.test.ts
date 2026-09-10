import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, resizeCanvas, type Series } from './renderer';
import { RingBuffer } from './ring-buffer';
import { installCanvasMock, recordingContext } from './test/canvas-mock';

afterEach(() => {
  vi.restoreAllMocks();
});

function seriesOf(values: readonly number[], capacity = values.length): Series {
  const buffer = new RingBuffer(Math.max(capacity, 1));
  for (const v of values) buffer.push(v);
  return { buffer, color: '#000' };
}

describe('render', () => {
  it('right-aligns a partially filled buffer so the newest sample is always at "now"', () => {
    const { calls, ctx } = recordingContext();
    // Capacity 5, two samples: the last point must land on the right edge.
    render(ctx, 400, 100, { series: [seriesOf([1, 2], 5)], scale: { min: 0, max: 10 } });

    const last = calls.filter((c) => c.op === 'lineTo').at(-1);
    expect(last?.args[0]).toBeCloseTo(400, 5);
  });

  it('does not stroke a series with a single sample, since one point is not a line', () => {
    const { calls, ctx } = recordingContext();
    render(ctx, 100, 100, { series: [seriesOf([5], 10)] });
    expect(calls.some((c) => c.op === 'stroke')).toBe(false);
  });

  it('closes the fill down to the axis only when a fill opacity is requested', () => {
    const { calls, ctx } = recordingContext();
    render(ctx, 100, 100, { series: [seriesOf([1, 2])] });
    expect(calls.some((c) => c.op === 'fill')).toBe(false);

    const filled = recordingContext();
    render(filled.ctx, 100, 100, { series: [{ ...seriesOf([1, 2]), fillOpacity: 0.3 }] });
    expect(filled.calls.some((c) => c.op === 'fill')).toBe(true);
    expect(filled.calls.some((c) => c.op === 'closePath')).toBe(true);
  });

  it('lands grid lines on pixel centres so a 1px rule is not blurred across two', () => {
    const { calls, ctx } = recordingContext();
    render(ctx, 200, 100, { series: [], grid: { horizontalLines: 4 } });

    const ys = calls.filter((c) => c.op === 'moveTo').map((c) => c.args[1] ?? Number.NaN);
    expect(ys).toHaveLength(3);
    for (const y of ys) expect(y % 1).toBeCloseTo(0.5, 10);
  });

  it('paints the background before any series so the data is not hidden behind it', () => {
    const { calls, ctx } = recordingContext();
    render(ctx, 100, 100, { series: [seriesOf([1, 2])], background: '#fff' });

    const ops = calls.map((c) => c.op);
    expect(ops.indexOf('fillRect')).toBeGreaterThan(ops.indexOf('clearRect'));
    expect(ops.indexOf('fillRect')).toBeLessThan(ops.indexOf('stroke'));
  });
});

describe('resizeCanvas', () => {
  it('sizes the bitmap by device pixel ratio and scales the context back to CSS pixels', () => {
    const { calls } = installCanvasMock(300, 150);
    const canvas = document.createElement('canvas');

    const result = resizeCanvas(canvas, 2);

    expect(result).toMatchObject({ width: 300, height: 150 });
    expect([canvas.width, canvas.height]).toEqual([600, 300]);
    const transform = calls.find((c) => c.op === 'setTransform');
    expect(transform?.args).toEqual([2, 0, 0, 2, 0, 0]);
  });

  it('never produces a zero-sized canvas from an unlaid-out element', () => {
    installCanvasMock(0, 0);
    const canvas = document.createElement('canvas');
    const { width, height } = resizeCanvas(canvas, 1);
    expect(width).toBeGreaterThan(0);
    expect(height).toBeGreaterThan(0);
  });

  it('leaves the bitmap alone when the size is unchanged, because assigning width clears the drawing', () => {
    installCanvasMock(100, 50);
    const canvas = document.createElement('canvas');
    resizeCanvas(canvas, 1);

    const widthSetter = vi.fn();
    Object.defineProperty(canvas, 'width', { get: () => 100, set: widthSetter });
    Object.defineProperty(canvas, 'height', { get: () => 50, set: vi.fn() });

    resizeCanvas(canvas, 1);
    expect(widthSetter).not.toHaveBeenCalled();
  });
});
