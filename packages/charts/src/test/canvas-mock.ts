import { vi } from 'vitest';

/**
 * A recording stand-in for `CanvasRenderingContext2D`.
 *
 * happy-dom has no rasteriser, so `getContext('2d')` returns `null` and every
 * draw is silently skipped — which is exactly the failure mode these tests
 * exist to catch. Rather than assert pixels, the mock records the path calls
 * so a test can state the guarantee ("this gap is not bridged", "this redraw
 * did not happen") in terms of what the renderer asked for.
 */
export interface RecordingContext {
  readonly calls: Array<{ readonly op: string; readonly args: readonly number[] }>;
  readonly ctx: CanvasRenderingContext2D;
}

export function recordingContext(): RecordingContext {
  const calls: Array<{ op: string; args: number[] }> = [];
  const record =
    (op: string) =>
    (...args: number[]) => {
      calls.push({ op, args });
    };

  const ctx = {
    clearRect: record('clearRect'),
    fillRect: record('fillRect'),
    beginPath: record('beginPath'),
    moveTo: record('moveTo'),
    lineTo: record('lineTo'),
    stroke: record('stroke'),
    fill: record('fill'),
    closePath: record('closePath'),
    save: record('save'),
    restore: record('restore'),
    setLineDash: vi.fn(),
    setTransform: record('setTransform'),
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 1,
    lineJoin: 'miter',
    lineCap: 'butt',
    globalAlpha: 1,
  } as unknown as CanvasRenderingContext2D;

  return { calls, ctx };
}

/**
 * Installs the mock on every canvas for the duration of a test and gives the
 * canvas a real-looking size, since happy-dom reports every rect as 0×0 and
 * the renderer clamps that to a 1×1 chart in which every x collapses to 0.
 */
export function installCanvasMock(width = 300, height = 100): RecordingContext {
  const recording = recordingContext();
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(() => recording.ctx);
  vi.spyOn(HTMLCanvasElement.prototype, 'getBoundingClientRect').mockReturnValue({
    width,
    height,
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: width,
    bottom: height,
    toJSON: () => ({}),
  });
  return recording;
}
