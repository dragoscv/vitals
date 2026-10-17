import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import { render, resizeCanvas } from './renderer';
import type { GridOptions, ScaleOptions, Series } from './renderer';

/** What a screen reader is told about the primary (first) series. */
export interface ChartSummary {
  /** Mean of the retained samples, gaps ignored. */
  readonly average: number;
  /** Highest retained sample. */
  readonly peak: number;
  /** Newest sample, or `null` when it is a gap. */
  readonly latest: number | null;
  /** Samples that are real readings rather than gaps. */
  readonly samples: number;
}

/**
 * The accessible name is refreshed at most this often. A label rewritten
 * every second is noise for anyone reading it, and the canvas is never a
 * live region, so nothing is announced on change — the figure is simply
 * current enough when someone moves to it.
 */
export const SUMMARY_INTERVAL_MS = 5_000;

/** Average, peak and latest of the first series, or `null` with no readings. */
export function summariseSeries(series: readonly Series[]): ChartSummary | null {
  const buffer = series[0]?.buffer;
  if (buffer === undefined) return null;
  let sum = 0;
  let count = 0;
  let peak = Number.NEGATIVE_INFINITY;
  for (let i = 0; i < buffer.size; i += 1) {
    const v = buffer.at(i);
    if (v === undefined || Number.isNaN(v)) continue;
    sum += v;
    count += 1;
    if (v > peak) peak = v;
  }
  if (count === 0) return null;
  const last = buffer.last;
  return {
    average: sum / count,
    peak,
    latest: last === undefined || Number.isNaN(last) ? null : last,
    samples: count,
  };
}

export interface TimeSeriesChartProps {
  readonly series: readonly Series[];
  readonly scale?: ScaleOptions;
  readonly grid?: GridOptions;
  readonly background?: string;
  readonly className?: string;
  /**
   * Bumped by the caller whenever new data has been pushed into the buffers.
   *
   * The buffers are mutable and deliberately live outside React state: putting
   * 600 floats per widget through `useState` would allocate a new array on
   * every tick and re-render the whole subtree. This counter is the single
   * cheap signal that a repaint is due.
   */
  readonly revision: number;
  /** The chart's name, used alone until there is data to summarise. */
  readonly ariaLabel?: string;
  /**
   * Turns the first series' figures into the accessible name, e.g.
   * "Processor, recent history: average 12 %, peak 48 %". A title alone tells
   * a screen-reader user that a chart exists and nothing about what it
   * shows. Formatting stays with the caller, which knows the unit and the
   * locale.
   */
  readonly describe?: (summary: ChartSummary) => string;
}

/**
 * A live time-series chart.
 *
 * Draws on demand — when `revision` changes or the element resizes — rather
 * than running a `requestAnimationFrame` loop. A permanent RAF loop per widget
 * would keep the compositor awake at 60 Hz to redraw data that only changes
 * once or twice a second, which on a laptop is a measurable battery cost for
 * no visual benefit.
 */
export function TimeSeriesChart({
  series,
  scale,
  grid,
  background,
  className,
  revision,
  ariaLabel,
  describe,
}: TimeSeriesChartProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Latest render options, kept in a ref so the ResizeObserver can redraw
  // with current values without being torn down and rebuilt on every parent
  // render. Written in a layout effect rather than during render: mutating a
  // ref while rendering is unsafe under concurrent React, which may render a
  // component and then discard the result.
  const optionsRef = useRef<{
    series: readonly Series[];
    scale: ScaleOptions | undefined;
    grid: GridOptions | undefined;
    background: string | undefined;
  }>({ series, scale, grid, background });

  useLayoutEffect(() => {
    optionsRef.current = { series, scale, grid, background };
  }, [series, scale, grid, background]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const { width, height, ctx } = resizeCanvas(canvas);
    if (!ctx) return;

    const o = optionsRef.current;
    render(ctx, width, height, {
      series: o.series,
      ...(o.scale !== undefined && { scale: o.scale }),
      ...(o.grid !== undefined && { grid: o.grid }),
      ...(o.background !== undefined && { background: o.background }),
    });
  }, []);

  // Layout effect so the first paint already has content: a plain effect
  // would show an empty canvas for one frame, which reads as a flicker when
  // twenty widgets mount at once.
  useLayoutEffect(() => {
    draw();
  }, [draw, revision]);

  // Written straight to the attribute rather than through state: a state
  // update per tick would re-render every chart's parent for a string that
  // changes at most every five seconds. React never sets `aria-label` on
  // this element when `describe` is given, so it cannot overwrite it.
  const describedAt = useRef(Number.NEGATIVE_INFINITY);
  const describeRef = useRef(describe);
  useLayoutEffect(() => {
    describeRef.current = describe;
  }, [describe]);
  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    const fn = describeRef.current;
    if (!canvas || fn === undefined) return;
    const now = performance.now();
    const hasLabel = canvas.hasAttribute('aria-label');
    if (hasLabel && now - describedAt.current < SUMMARY_INTERVAL_MS) return;
    const summary = summariseSeries(optionsRef.current.series);
    if (summary === null) {
      if (ariaLabel !== undefined) canvas.setAttribute('aria-label', ariaLabel);
      return;
    }
    canvas.setAttribute('aria-label', fn(summary));
    describedAt.current = now;
  }, [revision, ariaLabel]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // ResizeObserver rather than a window resize listener: widgets are
    // resized by the dashboard grid without the window changing at all.
    const observer = new ResizeObserver(() => draw());
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [draw]);

  return (
    <canvas
      ref={canvasRef}
      className={className}
      role="img"
      {...(describe === undefined && ariaLabel !== undefined && { 'aria-label': ariaLabel })}
      // Charts are decorative to a screen reader; the accompanying numeric
      // readout carries the information, so this is labelled but not focusable.
      style={{ display: 'block', width: '100%', height: '100%' }}
    />
  );
}
