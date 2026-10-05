/**
 * A label and its value, or an explicit absence — the one definition row the
 * Sensors and Hardware tabs share.
 *
 * There used to be two (`Field` on Sensors, `Row` on Hardware) that disagreed
 * on where the explanation went and on whether a long value could be read in
 * full. One component means a missing reading looks the same on every tab.
 *
 * An unmeasured value draws an em dash, as everywhere else in Vitals (see
 * AGENTS.md: unmeasured is `None`, never `0`). The dash alone says nothing to
 * a screen reader, so the words "Not available" travel with it, visually
 * hidden, and the reason is in the tooltip.
 */

import { useTranslation } from 'react-i18next';

import { DEVICES_NS } from './strings';

export function Field({
  label,
  value,
  hint,
}: {
  readonly label: string;
  /** `null` renders as an em dash with "Not available" for assistive tech. */
  readonly value: string | null;
  /** A qualifier shown after a present value; ignored when there is none. */
  readonly hint?: string;
}): React.JSX.Element {
  const { t } = useTranslation(DEVICES_NS);

  return (
    <div className="min-w-0">
      <dt className="text-2xs text-[var(--color-fg-muted)]">{label}</dt>
      {/* `title` rather than Tooltip: Tooltip throws outside the shell's
          TooltipProvider, which would crash this screen anywhere else it is
          rendered — including in tests. On a present value it shows the whole
          string, which `truncate` may have cut. */}
      <dd className="truncate text-sm" title={value ?? t('unavailableHint')}>
        {value ?? <Unavailable />}
        {hint !== undefined && value !== null && (
          <span className="ml-1 text-2xs text-[var(--color-fg-subtle)]">{hint}</span>
        )}
      </dd>
    </div>
  );
}

/** The em dash for a missing value, named for anyone not reading the screen. */
export function Unavailable(): React.JSX.Element {
  const { t } = useTranslation(DEVICES_NS);
  return (
    <span className="text-[var(--color-fg-subtle)]">
      <span aria-hidden="true">—</span>
      <span className="sr-only">{t('unavailable')}</span>
    </span>
  );
}
