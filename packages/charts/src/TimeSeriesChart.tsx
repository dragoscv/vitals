import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import { render, resizeCanvas } from './renderer';
import type { GridOptions, ScaleOptions, Series } from './renderer';

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
  readonly ariaLabel?: string;
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
      aria-label={ariaLabel}
      // Charts are decorative to a screen reader; the accompanying numeric
      // readout carries the information, so this is labelled but not focusable.
      style={{ display: 'block', width: '100%', height: '100%' }}
    />
  );
}
