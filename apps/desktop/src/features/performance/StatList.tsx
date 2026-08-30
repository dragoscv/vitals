/**
 * The label/value grid every detail panel is built from.
 *
 * # Absent readings are omitted, not rendered blank
 *
 * A row whose value is `null` is dropped entirely rather than shown with a
 * dash. On this page most panels have several optional fields — SMART data,
 * Wi-Fi signal, GPU power draw — and a machine reporting none of them would
 * otherwise show a column of empty rows that looks like the panel failed to
 * load. Callers that genuinely want to say "we cannot read this" pass the
 * translated `unavailable` string explicitly, which is a statement rather
 * than an absence.
 *
 * # Hints, not tooltips
 *
 * The explanatory text sits under the value permanently. A tooltip requires
 * knowing there is something to hover, which means the explanation only
 * reaches people who already suspected the number was subtle — the opposite
 * of who needs it. "Cached" and "Committed" are the whole reason: both are
 * routinely misread as problems.
 */

import { type ReactNode } from 'react';

export interface Stat {
  readonly key: string;
  readonly label: string;
  /** `null` drops the row. Pass a translated string to say "unavailable". */
  readonly value: string | null;
  /**
   * Persistent explanation, for figures that are commonly misread.
   *
   * `| undefined` explicitly: `exactOptionalPropertyTypes` is on, and callers
   * legitimately compute this as a conditional whose false branch is
   * `undefined` — a hint only makes sense when the value it explains exists.
   */
  readonly hint?: string | undefined;
  /** Rendered after the value — a badge, a warning. */
  readonly extra?: ReactNode | undefined;
}

export function StatList({
  stats,
  columns = 2,
}: {
  readonly stats: readonly Stat[];
  readonly columns?: 1 | 2 | 3;
}): React.JSX.Element | null {
  const visible = stats.filter((stat) => stat.value !== null);
  if (visible.length === 0) return null;

  const columnClass =
    columns === 1 ? '' : columns === 2 ? 'sm:grid-cols-2' : 'sm:grid-cols-2 xl:grid-cols-3';

  return (
    <dl className={`grid grid-cols-1 gap-x-4 gap-y-3 ${columnClass}`}>
      {visible.map((stat) => (
        <div key={stat.key} className="min-w-0">
          <dt className="text-2xs text-[var(--color-fg-muted)]">{stat.label}</dt>
          <dd className="flex items-baseline gap-1.5">
            <span className="tnum truncate font-mono text-sm text-[var(--color-fg-default)]">
              {stat.value}
            </span>
            {stat.extra}
          </dd>
          {stat.hint !== undefined && (
            <dd className="text-2xs mt-0.5 text-[var(--color-fg-subtle)]">{stat.hint}</dd>
          )}
        </div>
      ))}
    </dl>
  );
}
