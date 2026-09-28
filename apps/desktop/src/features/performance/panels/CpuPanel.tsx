/**
 * CPU detail.
 *
 * The one thing this shows that Task Manager does not make obvious: **which
 * cores**. A single "CPU 12%" cannot distinguish twelve cores lightly busy
 * from one core pinned, and one pinned core is the signature of a
 * single-threaded stall — which is the most common reason a modern machine
 * feels slow while its overall utilisation looks fine.
 */

import { useTranslation } from 'react-i18next';

import { TimeSeriesChart } from '@vitals/charts';
import type { SystemMetrics } from '@vitals/protocol';
import { Badge, cn, formatCount, formatFrequency, formatPercent, formatUptime } from '@vitals/ui';

import { useThemeColors } from '../../dashboard/widgets/useThemeColors';
import type { MetricHistory } from '../../dashboard/history';
import { StatList } from '../StatList';
import { PERFORMANCE_NS } from '../strings';

export function CpuPanel({
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
  const { cpu } = system;

  return (
    <div className="perf-panel">
      <div className="perf-chart">
        <div className="mb-2 flex items-baseline justify-between gap-3">
          <span className="text-2xs text-[var(--color-fg-muted)]">{t('cpu.utilisation')}</span>
          <span className="tnum font-mono text-2xl">{formatPercent(cpu.total, locale)}</span>
        </div>
        <TimeSeriesChart
          series={[
            { buffer: history.core.cpu, color: colors.accent, fillOpacity: 0.18, headDot: true },
            // Kernel drawn under the total, not beside it: the useful reading
            // is what share of a busy CPU is the OS rather than the user's
            // programs, and two separate lines make that a subtraction the
            // reader has to perform in their head.
            { buffer: history.core.cpuKernel, color: colors.muted, lineWidth: 1 },
          ]}
          revision={history.revision}
          scale={{ min: 0, max: 100 }}
          grid={{ horizontalLines: 4 }}
          className="min-h-0"
          ariaLabel={t('cpu.title')}
        />
      </div>

      {cpu.throttled !== null && (
        <Badge tone="warn">
          {t('cpu.throttled')} — {t(`throttle.${cpu.throttled}`)}
        </Badge>
      )}

      <PerCoreGrid values={cpu.perCore} locale={locale} />

      <StatList
        columns={3}
        stats={[
          {
            key: 'speed',
            label: t('cpu.speed'),
            // Effective, not nominal: modern chips park and boost constantly,
            // and the base clock says nothing about now.
            value:
              cpu.effectiveClock !== null
                ? formatFrequency(cpu.effectiveClock, locale)
                : t('unavailable'),
          },
          {
            key: 'maxSpeed',
            label: t('cpu.maxSpeed'),
            value: cpu.maxClock !== null ? formatFrequency(cpu.maxClock, locale) : null,
          },
          { key: 'kernel', label: t('cpu.kernelTime'), value: formatPercent(cpu.kernel, locale) },
          {
            key: 'processes',
            label: t('cpu.processes'),
            value: formatCount(cpu.processCount, locale),
          },
          { key: 'threads', label: t('cpu.threads'), value: formatCount(cpu.threadCount, locale) },
          {
            key: 'handles',
            label: t('cpu.handles'),
            value: cpu.handleCount !== null ? formatCount(cpu.handleCount, locale) : null,
          },
          { key: 'uptime', label: t('cpu.uptime'), value: formatUptime(cpu.uptimeSecs) },
          {
            key: 'switches',
            label: t('cpu.contextSwitches'),
            value: cpu.contextSwitches !== null ? formatCount(cpu.contextSwitches, locale) : null,
          },
          {
            key: 'interrupts',
            label: t('cpu.interrupts'),
            value: cpu.interrupts !== null ? formatCount(cpu.interrupts, locale) : null,
          },
        ]}
      />
    </div>
  );
}

/**
 * One cell per logical processor.
 *
 * A grid of small bars rather than a chart per core: on a 32-thread machine
 * that would be 32 canvases, each with its own resize observer, redrawing
 * every second. These are plain divs whose height changes — 32 style updates
 * a second is nothing, and the visual question ("is one of them pinned?") is
 * answered better by a dense grid than by 32 separate traces anyway.
 */
function PerCoreGrid({
  values,
  locale,
}: {
  readonly values: readonly number[];
  readonly locale: string;
}) {
  const { t } = useTranslation(PERFORMANCE_NS);
  if (values.length === 0) return null;

  return (
    <div>
      <p className="mb-1.5 text-2xs text-[var(--color-fg-muted)]">
        {t('cpu.perCore')} ({values.length})
      </p>
      <div
        className="grid gap-1"
        style={{
          gridTemplateColumns: `repeat(auto-fill, minmax(${values.length > 16 ? 44 : 64}px, 1fr))`,
        }}
        role="img"
        // One summary, not 32 announcements: a screen reader reading "core 1,
        // 4%, core 2, 7%…" every second is unusable. The busiest core is the
        // fact this grid exists to convey, so that is what gets announced.
        aria-label={`${t('cpu.perCore')}: ${formatPercent(Math.max(...values), locale)}`}
      >
        {values.map((value, index) => (
          <div
            key={index}
            className="rounded-sm bg-[var(--color-bg-inset)] p-1"
            title={t('cpu.coreLabel', { index: index + 1 })}
          >
            <div className="flex h-8 items-end">
              <div
                className={cn(
                  'w-full rounded-[1px]',
                  value > 90 ? 'bg-[var(--color-status-warn)]' : 'bg-[var(--color-accent)]',
                )}
                style={{ height: `${Math.min(Math.max(value, 2), 100).toFixed(1)}%` }}
              />
            </div>
            <p className="tnum mt-0.5 text-center font-mono text-2xs text-[var(--color-fg-subtle)]">
              {Math.round(value)}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}
