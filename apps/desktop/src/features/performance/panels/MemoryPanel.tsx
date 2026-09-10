/**
 * Memory detail.
 *
 * Built around correcting the single most common misreading in any task
 * manager: that high memory usage is a problem. On a healthy machine most of
 * RAM is file cache, which Windows gives back the instant a program asks for
 * it. The composition bar and the persistent hints exist to say that in the
 * place where the alarming number appears, rather than in documentation
 * nobody reads.
 *
 * Page faults are given equal billing with the percentage, because they are
 * the figure that actually separates a machine that is fine from one that is
 * thrashing.
 */

import { useTranslation } from 'react-i18next';

import { TimeSeriesChart } from '@vitals/charts';
import type { SystemMetrics } from '@vitals/protocol';
import { formatBytes, formatCount, formatFrequency, formatPercent } from '@vitals/ui';

import type { MetricHistory } from '../../dashboard/history';
import { useThemeColors } from '../../dashboard/widgets/useThemeColors';
import { StatList } from '../StatList';
import { PERFORMANCE_NS } from '../strings';

export function MemoryPanel({
  system,
  history,
  locale,
}: {
  readonly system: SystemMetrics;
  readonly history: MetricHistory;
  readonly locale: string;
}): React.JSX.Element {
  const { t } = useTranslation(PERFORMANCE_NS);
  const colors = useThemeColors();
  const { memory } = system;

  const usedPercent = memory.total > 0 ? (memory.used / memory.total) * 100 : 0;

  return (
    <div className="flex flex-col gap-4">
      <div>
        <div className="mb-2 flex items-baseline justify-between gap-3">
          <span className="text-2xs text-[var(--color-fg-muted)]">
            {t('memory.inUse')} — {formatBytes(memory.used, locale)} /{' '}
            {formatBytes(memory.total, locale)}
          </span>
          <span className="tnum font-mono text-2xl">{formatPercent(usedPercent, locale, 0)}</span>
        </div>
        <TimeSeriesChart
          series={[{ buffer: history.core.memoryPercent, color: colors.accent, fillOpacity: 0.18 }]}
          revision={history.revision}
          scale={{ min: 0, max: 100 }}
          grid={{ horizontalLines: 4 }}
          className="h-40 w-full"
          ariaLabel={t('memory.title')}
        />
      </div>

      <Composition system={system} locale={locale} />

      <StatList
        columns={3}
        stats={[
          {
            key: 'available',
            label: t('memory.available'),
            value: formatBytes(memory.available, locale),
          },
          {
            key: 'cached',
            label: t('memory.cached'),
            value: formatBytes(memory.cached, locale),
            hint: t('memory.cachedHint'),
          },
          {
            key: 'committed',
            label: t('memory.committed'),
            value: `${formatBytes(memory.committed, locale)} / ${formatBytes(memory.commitLimit, locale)}`,
            hint: t('memory.commitHint'),
          },
          {
            key: 'faults',
            label: t('memory.faults'),
            value:
              memory.pageFaultsPerSec !== null
                ? `${formatCount(Math.round(memory.pageFaultsPerSec), locale)}/s`
                : t('unavailable'),
            hint: t('memory.faultsHint'),
          },
          {
            key: 'paged',
            label: t('memory.pagedPool'),
            value: formatBytes(memory.pagedPool, locale),
          },
          {
            key: 'nonPaged',
            label: t('memory.nonPagedPool'),
            value: formatBytes(memory.nonPagedPool, locale),
          },
          {
            key: 'swap',
            label: t('memory.swap'),
            value:
              // Null and zero are different facts: Windows only exposes this
              // through WMI or a perf counter, both far too slow for the
              // per-tick path, so an unmeasured page file must not read as an
              // empty one.
              memory.swapUsed !== null
                ? `${formatBytes(memory.swapUsed, locale)} / ${formatBytes(memory.swapTotal, locale)}`
                : t('unavailable'),
          },
          {
            key: 'reserved',
            label: t('memory.hardwareReserved'),
            value: formatBytes(memory.hardwareReserved, locale),
          },
          {
            key: 'speed',
            label: t('memory.speed'),
            value: memory.speed !== null ? formatFrequency(memory.speed, locale) : null,
          },
          {
            key: 'slots',
            label: t('memory.slots'),
            value:
              memory.slotsUsed !== null && memory.slotsTotal !== null
                ? `${memory.slotsUsed} / ${memory.slotsTotal}`
                : null,
          },
          {
            key: 'form',
            label: t('memory.formFactor'),
            value: memory.formFactor,
          },
        ]}
      />
    </div>
  );
}

/**
 * A single stacked bar: in use, cached, free.
 *
 * The point is proportion at a glance. Three numbers in a list require the
 * reader to do arithmetic to discover that "87% used" is mostly cache; one bar
 * makes it immediate.
 */
function Composition({
  system,
  locale,
}: {
  readonly system: SystemMetrics;
  readonly locale: string;
}) {
  const { t } = useTranslation(PERFORMANCE_NS);
  const { memory } = system;
  if (memory.total <= 0) return null;

  const share = (value: number) => (value / memory.total) * 100;

  // `used` already includes cache on Windows, so subtracting keeps the
  // segments from summing past 100 and the bar from overflowing its track.
  const cached = Math.min(memory.cached, memory.used);
  const active = Math.max(memory.used - cached, 0);
  const free = Math.max(memory.total - memory.used, 0);

  const segments = [
    {
      key: 'active',
      label: t('memory.inUse'),
      value: active,
      className: 'bg-[var(--color-accent-solid)]',
    },
    {
      key: 'cached',
      label: t('memory.cached'),
      value: cached,
      className: 'bg-[var(--color-accent-solid)]/40',
    },
    {
      key: 'free',
      label: t('memory.available'),
      value: free,
      className: 'bg-[var(--color-bg-inset)]',
    },
  ];

  return (
    <div>
      <p className="mb-1.5 text-2xs text-[var(--color-fg-muted)]">{t('memory.composition')}</p>
      <div
        className="flex h-3 w-full overflow-hidden rounded-full bg-[var(--color-bg-inset)]"
        role="img"
        aria-label={segments
          .map((segment) => `${segment.label} ${formatBytes(segment.value, locale)}`)
          .join(', ')}
      >
        {segments.map((segment) => (
          <div
            key={segment.key}
            className={segment.className}
            style={{ width: `${share(segment.value).toFixed(2)}%` }}
          />
        ))}
      </div>
      <ul className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-2xs text-[var(--color-fg-muted)]">
        {segments.map((segment) => (
          <li key={segment.key} className="flex items-center gap-1.5">
            <span aria-hidden className={`size-2 rounded-full ${segment.className}`} />
            {segment.label}{' '}
            <span className="tnum font-mono">{formatBytes(segment.value, locale)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
