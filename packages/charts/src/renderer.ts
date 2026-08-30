/**
 * The canvas time-series renderer.
 *
 * ## Why canvas rather than SVG
 *
 * A dashboard can hold twenty widgets, each with a 600-point series, updating
 * twice a second. In SVG that is 12,000 DOM nodes being reconciled by React
 * and re-laid-out by the browser on every tick. In canvas it is a few hundred
 * `lineTo` calls against a GPU-composited bitmap, with no DOM involvement at
 * all. This is the single decision that determines whether the app feels
 * native or feels like a web page.
 *
 * ## Why not a charting library
 *
 * Every general-purpose library re-parses configuration, recomputes scales and
 * often rebuilds internal structures on each data change. Our case is narrow —
 * one X axis that is always time, uniformly spaced samples, no zooming during
 * live playback — so the general machinery is pure overhead.
 */

import type { RingBuffer } from './ring-buffer';

export interface Series {
  readonly buffer: RingBuffer;
  /** Any CSS colour; resolved once per draw, not per point. */
  readonly color: string;
  /** Fill under the line at this opacity, 0 to disable. */
  readonly fillOpacity?: number;
  readonly lineWidth?: number;
  /** Render as a dashed line — used for limits and thresholds. */
  readonly dashed?: boolean;
}

export interface ScaleOptions {
  /** Fixed lower bound. Omit to autoscale. */
  readonly min?: number;
  /** Fixed upper bound. Omit to autoscale. */
  readonly max?: number;
  /**
   * Round the autoscaled maximum up to a multiple of this.
   *
   * Without it, an autoscaled axis re-fits on every frame and the line
   * visibly breathes even when the underlying values are steady — which reads
   * as instability that is not there.
   */
  readonly snapTo?: number;
  /** Headroom above the data, as a fraction of range. */
  readonly padding?: number;
}

export interface GridOptions {
  readonly horizontalLines?: number;
  readonly verticalLines?: number;
  readonly color?: string;
}

export interface RenderOptions {
  readonly series: readonly Series[];
  readonly scale?: ScaleOptions;
  readonly grid?: GridOptions;
  readonly background?: string;
  /** Device pixel ratio; defaults to the window's. */
  readonly dpr?: number;
}

/** Resolved vertical bounds for a draw. */
export interface Bounds {
  readonly min: number;
  readonly max: number;
}

/**
 * Computes the vertical bounds for a set of series.
 *
 * Exported and pure so the snapping behaviour — the thing that stops the axis
 * jittering — is directly testable without a canvas.
 */
export function computeBounds(series: readonly Series[], scale: ScaleOptions = {}): Bounds {
  if (scale.min !== undefined && scale.max !== undefined) {
    return { min: scale.min, max: scale.max };
  }

  let dataMin = Infinity;
  let dataMax = -Infinity;

  for (const s of series) {
    const { min, max } = s.buffer.extent();
    if (s.buffer.size === 0) continue;
    if (min < dataMin) dataMin = min;
    if (max > dataMax) dataMax = max;
  }

  if (!Number.isFinite(dataMin) || !Number.isFinite(dataMax)) {
    return { min: scale.min ?? 0, max: scale.max ?? 1 };
  }

  const min = scale.min ?? Math.min(dataMin, 0);
  let max = scale.max ?? dataMax;

  const padding = scale.padding ?? 0.1;
  if (scale.max === undefined) {
    max += (max - min) * padding;
  }

  if (scale.snapTo !== undefined && scale.snapTo > 0 && scale.max === undefined) {
    max = Math.ceil(max / scale.snapTo) * scale.snapTo;
  }

  // A zero-height range would divide by zero when projecting to pixels.
  if (max <= min) max = min + (scale.snapTo ?? 1);

  return { min, max };
}

/**
 * Draws one frame.
 *
 * Takes a context rather than a canvas so the caller controls sizing and DPR
 * scaling once, instead of this doing it on every frame.
 */
export function render(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  options: RenderOptions,
): void {
  const { series, grid, background } = options;

  ctx.clearRect(0, 0, width, height);

  if (background) {
    ctx.fillStyle = background;
    ctx.fillRect(0, 0, width, height);
  }

  const bounds = computeBounds(series, options.scale);

  if (grid) {
    drawGrid(ctx, width, height, grid);
  }

  for (const s of series) {
    drawSeries(ctx, width, height, s, bounds);
  }
}

function drawGrid(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  grid: GridOptions,
): void {
  const { horizontalLines = 4, verticalLines = 0, color = 'rgba(128,128,128,0.15)' } = grid;

  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineWidth = 1;
  ctx.beginPath();

  for (let i = 1; i < horizontalLines; i += 1) {
    // The 0.5 offset lands the stroke on a pixel centre. Without it a 1px
    // line straddles two device pixels and renders as a 2px blur.
    const y = Math.round((height / horizontalLines) * i) + 0.5;
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
  }

  for (let i = 1; i < verticalLines; i += 1) {
    const x = Math.round((width / verticalLines) * i) + 0.5;
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
  }

  ctx.stroke();
  ctx.restore();
}

function drawSeries(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  series: Series,
  bounds: Bounds,
): void {
  const { buffer, color, fillOpacity = 0, lineWidth = 1.5, dashed = false } = series;
  const count = buffer.size;
  if (count < 2) return;

  const range = bounds.max - bounds.min;
  const stepX = width / (buffer.capacity - 1);
  // Right-align: a partially filled buffer draws against the right edge so
  // the newest sample is always at "now", with history growing leftwards.
  const offsetX = width - (count - 1) * stepX;

  const toY = (value: number): number => height - ((value - bounds.min) / range) * height;

  ctx.save();
  ctx.lineWidth = lineWidth;
  ctx.strokeStyle = color;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  if (dashed) ctx.setLineDash([4, 4]);

  // Segments are tracked so a gap breaks the line rather than drawing a
  // straight cliff through missing data.
  const segments: Array<{ startIndex: number; endIndex: number }> = [];
  let segmentStart: number | null = null;

  for (let i = 0; i < count; i += 1) {
    const v = buffer.at(i);
    const isGap = v === undefined || Number.isNaN(v);

    if (isGap) {
      if (segmentStart !== null) {
        segments.push({ startIndex: segmentStart, endIndex: i - 1 });
        segmentStart = null;
      }
    } else if (segmentStart === null) {
      segmentStart = i;
    }
  }
  if (segmentStart !== null) {
    segments.push({ startIndex: segmentStart, endIndex: count - 1 });
  }

  for (const segment of segments) {
    if (segment.endIndex <= segment.startIndex) continue;

    ctx.beginPath();
    for (let i = segment.startIndex; i <= segment.endIndex; i += 1) {
      const v = buffer.at(i);
      if (v === undefined) continue;
      const x = offsetX + i * stepX;
      const y = toY(v);
      if (i === segment.startIndex) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    if (fillOpacity > 0) {
      const startX = offsetX + segment.startIndex * stepX;
      const endX = offsetX + segment.endIndex * stepX;
      ctx.lineTo(endX, height);
      ctx.lineTo(startX, height);
      ctx.closePath();
      ctx.globalAlpha = fillOpacity;
      ctx.fillStyle = color;
      ctx.fill();
      ctx.globalAlpha = 1;
    }
  }

  ctx.restore();
}

/**
 * Sizes a canvas for the current device pixel ratio.
 *
 * Charts render blurry on any HiDPI display without this, and every Windows
 * laptop sold in the last decade is HiDPI at some scaling factor.
 *
 * @returns the CSS-pixel dimensions to draw against.
 */
export function resizeCanvas(
  canvas: HTMLCanvasElement,
  dpr: number = globalThis.devicePixelRatio || 1,
): { width: number; height: number; ctx: CanvasRenderingContext2D | null } {
  const rect = canvas.getBoundingClientRect();
  const width = Math.max(1, Math.floor(rect.width));
  const height = Math.max(1, Math.floor(rect.height));

  const targetW = Math.floor(width * dpr);
  const targetH = Math.floor(height * dpr);

  // Assigning to width/height clears the canvas, so only do it on real change.
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  const ctx = canvas.getContext('2d');
  if (ctx) {
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  return { width, height, ctx };
}
