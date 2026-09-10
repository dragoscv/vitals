import { HISTORY_LENGTH } from '../lib/live';

export interface SparklineProps {
  /** Percentages, oldest first. Fewer than two points draws the baseline only. */
  readonly values: readonly number[];
  /** A CSS colour, normally a `var(--color-chart-*)` token. */
  readonly stroke: string;
  readonly className?: string;
}

const W = 40;
const H = 16;

/**
 * A 40×16 inline-SVG trace.
 *
 * A copy of the phone's sparkline rather than an import: the overlay entry is
 * fenced off from `src/mobile` (see `boundary.test.ts`) so neither page can
 * quietly grow a dependency on the other's internals, and at forty pixels wide
 * the divergence is the point — this one has no dashes, no rounding and a
 * thinner stroke, because at this size they read as noise.
 */
export function Sparkline({ values, stroke, className }: SparklineProps) {
  // Fixed to 0–100 so the three rows are comparable at a glance. Auto-scaling
  // would make idle jitter look like a storm, which is precisely the wrong
  // signal from a window designed to be watched out of the corner of an eye.
  const step = W / (HISTORY_LENGTH - 1);
  const offset = W - Math.max(values.length - 1, 0) * step;
  const points = values
    .map((value, index) => {
      const x = offset + index * step;
      const y = H - (Math.min(100, Math.max(0, value)) / 100) * (H - 2) - 1;
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
      {values.length < 2 ? (
        // A blank strip reads as a rendering fault. The baseline says the
        // trace belongs here and is meant to be empty for now.
        <line
          x1="0"
          y1={H - 1}
          x2={W}
          y2={H - 1}
          stroke="var(--color-border-subtle)"
          strokeWidth="1"
        />
      ) : (
        <polyline
          points={points}
          fill="none"
          stroke={stroke}
          strokeWidth="1.25"
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />
      )}
    </svg>
  );
}
