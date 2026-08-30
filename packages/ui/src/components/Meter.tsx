import { forwardRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { useReducedMotion } from '../lib/useReducedMotion';
import type { ProgressTone } from './ProgressBar';

const FILL_TONE: Record<ProgressTone, string> = {
  accent: 'bg-[var(--color-accent)]',
  ok: 'bg-[var(--color-status-ok)]',
  warn: 'bg-[var(--color-status-warn)]',
  danger: 'bg-[var(--color-status-danger)]',
  neutral: 'bg-[var(--color-fg-subtle)]',
};

export interface MeterProps {
  readonly className?: string;
  /** Visible label, e.g. "CPU". Provided by the caller, never invented here. */
  readonly label: ReactNode;
  /** Accessible name. Needed separately because `label` may be a node. */
  readonly accessibleLabel: string;
  readonly value: number;
  readonly max?: number;
  /**
   * The formatted reading shown at the trailing edge, e.g. "43%" or "6.2 GB".
   *
   * A string rather than a number plus a format option: formatting is locale
   * dependent and already solved by `lib/format`, and re-implementing it per
   * component is how "1.5 GB" and "1536 MB" end up on the same screen.
   */
  readonly valueText: string;
  readonly tone?: ProgressTone;
  /**
   * Announce updates to assistive technology.
   *
   * Off by default and it should stay off for anything sampled continuously —
   * a live region on a 1 Hz CPU meter reads the number aloud once a second
   * forever, which makes the app unusable with a screen reader. Enable it only
   * for a value the user has just acted on.
   */
  readonly live?: boolean;
}

/**
 * A labelled horizontal gauge for a bounded reading — CPU, RAM, disk usage.
 *
 * Uses `role="meter"`, not `progressbar`: a progress bar describes a task
 * advancing towards completion, whereas these values move in both directions
 * and have no end state. Screen readers announce the two differently, and
 * "89% complete" is the wrong sentence for memory pressure.
 */
export const Meter = forwardRef<HTMLDivElement, MeterProps>(function Meter(
  {
    className,
    label,
    accessibleLabel,
    value,
    max = 100,
    valueText,
    tone = 'accent',
    live = false,
    ...rest
  },
  ref,
) {
  const reduced = useReducedMotion();
  const ratio = Math.min(Math.max(value / (max || 1), 0), 1);

  return (
    <div ref={ref} className={cn('flex flex-col gap-1', className)} {...rest}>
      <div className="text-2xs flex items-baseline justify-between gap-2">
        <span className="truncate text-[var(--color-fg-muted)]">{label}</span>
        <span
          className="tnum shrink-0 font-mono text-[var(--color-fg-default)]"
          // Polite, never assertive: an assertive region interrupts whatever
          // the user is currently reading, and no metric is worth that.
          aria-live={live ? 'polite' : undefined}
        >
          {valueText}
        </span>
      </div>
      <div
        role="meter"
        aria-label={accessibleLabel}
        aria-valuemin={0}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={valueText}
        className="h-1.5 w-full overflow-hidden rounded-full bg-[var(--color-bg-inset)]"
      >
        <div
          data-testid="meter-fill"
          className={cn(
            'h-full rounded-full',
            FILL_TONE[tone],
            reduced
              ? ''
              : 'duration-(--duration-normal) ease-(--ease-out-quart) transition-[width]',
          )}
          style={{ width: `${(ratio * 100).toFixed(2)}%` }}
        />
      </div>
    </div>
  );
});
