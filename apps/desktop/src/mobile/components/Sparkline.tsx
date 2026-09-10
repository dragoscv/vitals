import { HISTORY_LENGTH } from '../lib/live';

export interface SparklineProps {
  /** Percentages, oldest first. Fewer than two points draws nothing. */
  readonly values: readonly number[];
  /** A CSS colour, normally a `var(--color-chart-*)` token. */
  readonly stroke: string;
  readonly className?: string;
}

const W = 100;
const H = 28;

/**
 * A minimal inline-SVG sparkline.
 *
 * `@vitals/charts` draws on canvas with a ResizeObserver per chart, which is
 * the right tool for the desktop's large panels and the wrong one for a card
 * that shows two 100×28 traces: a vector path costs nothing to lay out and
 * scales with the card for free. Fixed to 0–100 so two cards side by side
 * are comparable at a glance — auto-scaling a sparkline makes idle noise look
 * like a storm.
 */
export function Sparkline({ values, stroke, className }: SparklineProps) {
  if (values.length < 2) {
    // A blank strip for the first second looks like a rendering bug. The
    // baseline shows where the trace will be and that it is meant to be
    // empty for now.
    return (
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className={className}
        aria-hidden="true"
        preserveAspectRatio="none"
      >
        <line
          x1="0"
          y1={H - 1}
          x2={W}
          y2={H - 1}
          stroke="var(--color-border-subtle)"
          strokeWidth="1"
          strokeDasharray="2 3"
        />
      </svg>
    );
  }
  // Anchored to the right so a fresh trace grows in from the edge rather than
  // stretching across the whole width and re-fitting on every frame.
  const step = W / (HISTORY_LENGTH - 1);
  const offset = W - (values.length - 1) * step;
  const points = values
    .map((v, i) => {
      const x = offset + i * step;
      const y = H - (Math.min(100, Math.max(0, v)) / 100) * (H - 2) - 1;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      className={className}
      aria-hidden="true"
      preserveAspectRatio="none"
    >
      <polyline
        points={points}
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
