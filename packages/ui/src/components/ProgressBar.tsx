import { forwardRef } from 'react';
import { cn } from '../lib/cn';
import { useReducedMotion } from '../lib/useReducedMotion';

/**
 * The four tones a bar can take.
 *
 * `auto` is not offered: a bar cannot know whether 90% is alarming (disk full)
 * or excellent (cache hit rate), so the caller decides.
 */
export type ProgressTone = 'accent' | 'ok' | 'warn' | 'danger' | 'neutral';

const TRACK_TONE: Record<ProgressTone, string> = {
  accent: 'bg-[var(--color-accent)]',
  ok: 'bg-[var(--color-status-ok)]',
  warn: 'bg-[var(--color-status-warn)]',
  danger: 'bg-[var(--color-status-danger)]',
  neutral: 'bg-[var(--color-fg-subtle)]',
};

const SIZES = { sm: 'h-1', md: 'h-1.5', lg: 'h-2.5' } as const;

interface ProgressBase {
  readonly className?: string;
  readonly tone?: ProgressTone;
  readonly size?: keyof typeof SIZES;
  /** Accessible name. Supplied by the caller; this package holds no English. */
  readonly label: string;
  /**
   * Spoken value, e.g. "43 percent" or "2.1 GB of 16 GB".
   *
   * Without it a screen reader reads the raw ratio, which for a byte count is
   * meaningless. Only used in the determinate case.
   */
  readonly valueText?: string;
}

/**
 * A discriminated union rather than an optional `value`.
 *
 * With `value?: number`, "indeterminate" and "0%" are the same call, and the
 * ARIA contract differs between them — a determinate bar must expose
 * `aria-valuenow` and an indeterminate one must omit it. Encoding the
 * distinction in the type makes the wrong combination unrepresentable.
 */
export type ProgressBarProps =
  | (ProgressBase & { readonly indeterminate: true })
  | (ProgressBase & {
      readonly indeterminate?: false;
      readonly value: number;
      /** Upper bound. Defaults to 100 so percentages need no ceremony. */
      readonly max?: number;
    });

export const ProgressBar = forwardRef<HTMLDivElement, ProgressBarProps>(
  function ProgressBar(props, ref) {
    const { className, tone = 'accent', size = 'md', label, valueText } = props;
    const reduced = useReducedMotion();

    const indeterminate = props.indeterminate === true;
    const max = indeterminate ? 100 : (props.max ?? 100);
    // Clamped rather than trusted: sampled counters legitimately overshoot for
    // one tick (a rate computed over a slightly short interval), and a bar
    // rendered at 104% width visibly escapes its track.
    const ratio = indeterminate ? 0 : Math.min(Math.max(props.value / (max || 1), 0), 1);

    return (
      <div
        ref={ref}
        role="progressbar"
        aria-label={label}
        aria-valuemin={indeterminate ? undefined : 0}
        aria-valuemax={indeterminate ? undefined : max}
        aria-valuenow={indeterminate ? undefined : props.value}
        aria-valuetext={indeterminate ? undefined : valueText}
        className={cn(
          'w-full overflow-hidden rounded-full bg-[var(--color-bg-inset)]',
          SIZES[size],
          className,
        )}
      >
        <div
          data-testid="progress-indicator"
          data-indeterminate={indeterminate || undefined}
          className={cn(
            'h-full rounded-full',
            TRACK_TONE[tone],
            // No transition when reduced motion is requested: at a 1 Hz sample
            // rate an eased width change is a continuously creeping edge, which
            // is exactly the kind of persistent movement the preference is for.
            reduced
              ? ''
              : 'duration-(--duration-normal) ease-(--ease-out-quart) transition-[width]',
            indeterminate && !reduced && 'w-1/3 animate-pulse',
            indeterminate && reduced && 'w-1/3 opacity-60',
          )}
          style={indeterminate ? undefined : { width: `${(ratio * 100).toFixed(2)}%` }}
        />
      </div>
    );
  },
);
