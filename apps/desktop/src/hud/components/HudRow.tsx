import { formatPercent, formatTemperature } from '@vitals/ui';

import { Sparkline } from './Sparkline';

export interface HudRowProps {
  readonly label: string;
  /** `null` renders an em dash — unmeasured is not zero. */
  readonly percent: number | null;
  readonly history: readonly number[];
  readonly stroke: string;
  readonly locale: string;
  /** Shown inline beside the percentage when the sensor reports one. */
  readonly temperature?: number | null;
}

/** One reading: label, number, optional temperature, trace. */
export function HudRow({
  label,
  percent,
  history,
  stroke,
  locale,
  temperature = null,
}: HudRowProps) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-10 shrink-0 text-[10px] leading-none font-medium tracking-wide text-[var(--color-text-muted)] uppercase">
        {label}
      </span>
      <span
        // Tabular figures stop the number jittering sideways as digits change.
        // On a window the user is watching peripherally, that movement is the
        // single most distracting thing it can do.
        className="w-[3.25rem] shrink-0 text-right font-mono text-sm leading-none font-semibold tabular-nums"
      >
        {formatPercent(percent, locale, 0)}
      </span>
      <span className="w-9 shrink-0 text-right font-mono text-[10px] leading-none text-[var(--color-text-muted)] tabular-nums">
        {temperature === null ? '' : formatTemperature(temperature, locale)}
      </span>
      <Sparkline values={history} stroke={stroke} className="h-4 min-w-0 flex-1" />
    </div>
  );
}
