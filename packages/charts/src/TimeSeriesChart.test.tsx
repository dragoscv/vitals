import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RingBuffer } from './ring-buffer';
import { TimeSeriesChart } from './TimeSeriesChart';
import { installCanvasMock, type RecordingContext } from './test/canvas-mock';

let recording: RecordingContext;

beforeEach(() => {
  if (!('ResizeObserver' in globalThis)) {
    globalThis.ResizeObserver = class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    };
  }
  recording = installCanvasMock(300, 100);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function filled(values: readonly number[], capacity = values.length): RingBuffer {
  const buffer = new RingBuffer(capacity);
  for (const v of values) buffer.push(v);
  return buffer;
}

const drawCount = () => recording.calls.filter((c) => c.op === 'clearRect').length;

describe('TimeSeriesChart', () => {
  it('is an image with the caller-supplied name, so the readout beside it is not the only label', () => {
    render(
      <TimeSeriesChart
        series={[{ buffer: filled([1, 2]), color: 'red' }]}
        revision={0}
        ariaLabel="CPU history"
      />,
    );
    expect(screen.getByRole('img', { name: 'CPU history' })).toBeTruthy();
  });

  it('redraws when revision changes and NOT when the parent re-renders with the same revision', () => {
    const series = [{ buffer: filled([1, 2, 3]), color: 'red' }];
    const { rerender } = render(<TimeSeriesChart series={series} revision={1} />);
    expect(drawCount(), 'first paint').toBe(1);

    // A parent re-render with fresh object identities but the same revision:
    // the buffers are mutable and live outside React, so identity means
    // nothing here, and repainting on it would defeat the purpose of the prop.
    rerender(<TimeSeriesChart series={[...series]} revision={1} grid={{ horizontalLines: 4 }} />);
    expect(drawCount(), 'same revision must not repaint').toBe(1);

    rerender(<TimeSeriesChart series={series} revision={2} />);
    expect(drawCount(), 'new revision must repaint').toBe(2);
  });

  it('projects with the fixed scale it is given rather than autoscaling to the data', () => {
    // Values 0..50 with max pinned at 100: the top sample must sit mid-height,
    // not at the top edge where autoscale would put it.
    const series = [{ buffer: filled([0, 50]), color: 'red' }];
    render(<TimeSeriesChart series={series} revision={0} scale={{ min: 0, max: 100 }} />);

    const lineTos = recording.calls.filter((c) => c.op === 'lineTo');
    const top = lineTos[0];
    expect(top).toBeDefined();
    expect(top?.args[1]).toBeCloseTo(50, 5);
  });

  it('breaks the line at a gap instead of drawing a cliff down to zero', () => {
    // Capacity matches the sample count so x positions are deterministic.
    const buffer = new RingBuffer(5);
    buffer.push(10);
    buffer.push(10);
    buffer.pushGap();
    buffer.push(10);
    buffer.push(10);

    render(
      <TimeSeriesChart
        series={[{ buffer, color: 'red' }]}
        revision={0}
        scale={{ min: 0, max: 20 }}
      />,
    );

    const path = recording.calls.filter((c) => c.op === 'moveTo' || c.op === 'lineTo');
    // Two segments → two moveTo starts; a bridged gap would be a single path.
    expect(path.filter((c) => c.op === 'moveTo')).toHaveLength(2);
    // Every plotted y is the value's projection (mid-height); nothing reaches
    // the axis at y=100, which is where a NaN-as-zero would land.
    for (const c of path) {
      expect(c.args[1]).toBeCloseTo(50, 5);
    }
    // And no point is placed at the gap's x position (index 2 of 5 → 150px).
    expect(path.some((c) => Math.abs((c.args[0] ?? 0) - 150) < 1e-6)).toBe(false);
  });

  it('draws nothing but the clear when the context is unavailable, rather than throwing', () => {
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
    expect(() =>
      render(<TimeSeriesChart series={[{ buffer: filled([1, 2]), color: 'red' }]} revision={0} />),
    ).not.toThrow();
  });
});
