import { type ReactNode } from 'react';
import { cn } from '../lib/cn';

export interface Stat {
  readonly key: string;
  readonly label: string;
  /**
   * `null` is "cannot be measured" — the app-wide rule that an unmeasured
   * reading is never a zero. It renders as an em dash unless the list is
   * given `omitNull`, in which case the row is dropped entirely.
   */
  readonly value: string | null;
  /**
   * Persistent explanation, for figures that are commonly misread ("Cached",
   * "Committed"). Under the value, not in a tooltip: a tooltip only reaches
   * people who already suspected the number was subtle.
   *
   * `| undefined` explicitly: `exactOptionalPropertyTypes` is on and callers
   * compute this as a conditional whose false branch is `undefined`.
   */
  readonly hint?: string | undefined;
  /** Rendered after the value — a badge, a warning. */
  readonly extra?: ReactNode | undefined;
}

export interface StatListProps {
  readonly stats: readonly Stat[];
  readonly columns?: 1 | 2 | 3 | 4;
  /**
   * Drop `null` rows instead of showing a dash.
   *
   * For panels with many optional fields (SMART, Wi-Fi signal, GPU power) a
   * machine reporting none of them would otherwise show a column of dashes
   * that looks like the panel failed to load. Off by default because on a
   * short list the dash is the honest answer: the field exists, the reading
   * does not.
   */
  readonly omitNull?: boolean;
  /**
   * Accessible name for an absent value. Screen readers skip a bare "—" or
   * read it as "em dash", neither of which says "not available".
   */
  readonly unavailableLabel?: string;
  readonly className?: string;
}

/*
 * `columns` is the count at the desktop breakpoint; the 3xl/4xl steps (defined
 * by the app's stylesheet) add one and two more on ultrawide windows so the
 * grid keeps filling the width instead of stretching each cell.
 */
const COLUMN_CLASS: Record<NonNullable<StatListProps['columns']>, string> = {
  1: '',
  2: 'sm:grid-cols-2',
  3: 'sm:grid-cols-2 xl:grid-cols-3 3xl:grid-cols-4 4xl:grid-cols-5',
  4: 'sm:grid-cols-2 xl:grid-cols-4 3xl:grid-cols-5 4xl:grid-cols-6',
};

/**
 * The label/value grid every detail panel is built from.
 *
 * A `<dl>` because the pairs are definitions in the HTML sense, which gives a
 * screen reader "term, description" structure for free. Values are set in the
 * mono face with tabular figures: live readings that change width every tick
 * make the whole column vibrate otherwise.
 */
export function StatList({
  stats,
  columns = 2,
  omitNull = false,
  unavailableLabel,
  className,
}: StatListProps): React.JSX.Element | null {
  const visible = omitNull ? stats.filter((stat) => stat.value !== null) : stats;
  if (visible.length === 0) return null;

  return (
    <dl className={cn('grid grid-cols-1 gap-x-4 gap-y-3', COLUMN_CLASS[columns], className)}>
      {visible.map((stat) => (
        <div key={stat.key} className="min-w-0">
          <dt className="text-2xs text-[var(--color-fg-muted)]">{stat.label}</dt>
          <dd className="flex items-baseline gap-1.5">
            {stat.value === null ? (
              <span
                className="tnum font-mono text-sm text-[var(--color-fg-subtle)]"
                {...(unavailableLabel !== undefined && { 'aria-label': unavailableLabel })}
              >
                —
              </span>
            ) : (
              <span className="tnum truncate font-mono text-sm text-[var(--color-fg-default)]">
                {stat.value}
              </span>
            )}
            {stat.extra}
          </dd>
          {stat.hint !== undefined && (
            <dd className="mt-0.5 text-2xs text-[var(--color-fg-subtle)]">{stat.hint}</dd>
          )}
        </div>
      ))}
    </dl>
  );
}
