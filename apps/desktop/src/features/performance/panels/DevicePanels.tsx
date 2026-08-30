/**
 * GPU, disk and network detail.
 *
 * All three share a shape: a headline chart, then the readings the hardware
 * is willing to give. What differs is which reading is the *honest* headline,
 * and each choice here is deliberate:
 *
 * - **GPU** — per-engine, not a single percentage. A machine can sit at 5%
 *   overall while the video decode engine is pinned, and the summary number
 *   hides exactly the case someone is investigating.
 * - **Disk** — active time and response, not throughput. A drive saturated by
 *   small random reads moves almost no bytes while being unusable.
 * - **Network** — bytes per second, with no percentage at all. Link speed is a
 *   nominal ceiling Wi-Fi never reaches and that many adapters report as null,
 *   so a utilisation percentage would be invented rather than measured.
 */

import { useTranslation } from 'react-i18next';

import { TimeSeriesChart } from '@vitals/charts';
import type { DiskMetrics, GpuMetrics, NetworkMetrics } from '@vitals/protocol';
import {
  Badge,
  Meter,
  formatBytes,
  formatCount,
  formatFrequency,
  formatLatency,
  formatPercent,
  formatThroughput,
  formatUptime,
  formatWatts,
} from '@vitals/ui';

import type { MetricHistory } from '../../dashboard/history';
import { useThemeColors } from '../../dashboard/widgets/useThemeColors';
import { StatList } from '../StatList';
import { PERFORMANCE_NS } from '../strings';

export function GpuPanel({
  gpu,
  history,
  locale,
}: {
  readonly gpu: GpuMetrics;
  readonly history: MetricHistory;
  readonly locale: string;
}): React.JSX.Element {
  const { t } = useTranslation(PERFORMANCE_NS);
  const colors = useThemeColors();
  const buffer = history.gpu.get(gpu.id);

  return (
    <div className="flex flex-col gap-4">
      <div>
        <div className="mb-2 flex items-baseline justify-between gap-3">
          <span className="truncate text-sm font-medium">{gpu.name}</span>
          <span className="tnum font-mono text-2xl">{formatPercent(gpu.utilization, locale)}</span>
        </div>
        {buffer !== undefined && (
          <TimeSeriesChart
            series={[{ buffer, color: colors.accent, fillOpacity: 0.18 }]}
            revision={history.revision}
            scale={{ min: 0, max: 100 }}
            grid={{ horizontalLines: 4 }}
            className="h-40 w-full"
            ariaLabel={gpu.name}
          />
        )}
      </div>

      {gpu.throttled !== null && <Badge tone="warn">{t(`throttle.${gpu.throttled}`)}</Badge>}

      {gpu.engines.length > 0 && (
        <div>
          <p className="text-2xs mb-0.5 text-[var(--color-fg-muted)]">{t('gpu.engines')}</p>
          <p className="text-2xs mb-2 text-[var(--color-fg-subtle)]">{t('gpu.enginesHint')}</p>
          <div className="flex flex-col gap-1.5">
            {gpu.engines.map((engine) => (
              <Meter
                key={engine.name}
                label={engine.name}
                accessibleLabel={`${gpu.name} ${engine.name}`}
                value={engine.utilization}
                valueText={formatPercent(engine.utilization, locale)}
              />
            ))}
          </div>
        </div>
      )}

      <StatList
        columns={3}
        stats={[
          {
            key: 'dedicated',
            label: t('gpu.dedicatedMemory'),
            value:
              gpu.memoryUsed !== null && gpu.memoryTotal !== null
                ? `${formatBytes(gpu.memoryUsed, locale)} / ${formatBytes(gpu.memoryTotal, locale)}`
                : t('unavailable'),
          },
          {
            key: 'shared',
            label: t('gpu.sharedMemory'),
            value: gpu.sharedMemoryUsed !== null ? formatBytes(gpu.sharedMemoryUsed, locale) : null,
          },
          {
            key: 'coreClock',
            label: t('gpu.coreClock'),
            value: gpu.coreClock !== null ? formatFrequency(gpu.coreClock, locale) : null,
          },
          {
            key: 'memClock',
            label: t('gpu.memoryClock'),
            value: gpu.memoryClock !== null ? formatFrequency(gpu.memoryClock, locale) : null,
          },
          {
            key: 'power',
            label: t('gpu.power'),
            value:
              gpu.power !== null
                ? gpu.powerLimit !== null
                  ? `${formatWatts(gpu.power, locale)} / ${formatWatts(gpu.powerLimit, locale)}`
                  : formatWatts(gpu.power, locale)
                : null,
          },
          {
            key: 'fan',
            label: t('gpu.fan'),
            value:
              gpu.fanRpm !== null
                ? `${formatCount(gpu.fanRpm, locale)} RPM`
                : gpu.fanPercent !== null
                  ? formatPercent(gpu.fanPercent, locale, 0)
                  : null,
          },
          { key: 'vendor', label: t('gpu.vendor'), value: gpu.vendor },
          { key: 'driver', label: t('gpu.driver'), value: gpu.driverVersion },
        ]}
      />
    </div>
  );
}

export function DiskPanel({
  disk,
  history,
  locale,
}: {
  readonly disk: DiskMetrics;
  readonly history: MetricHistory;
  readonly locale: string;
}): React.JSX.Element {
  const { t } = useTranslation(PERFORMANCE_NS);
  const colors = useThemeColors();
  const buffer = history.diskActive.get(disk.id);
  const used = disk.total - disk.free;

  return (
    <div className="flex flex-col gap-4">
      <div>
        <div className="mb-2 flex items-baseline justify-between gap-3">
          <span className="truncate text-sm font-medium">{disk.mount ?? disk.name}</span>
          <span className="tnum font-mono text-2xl">{formatPercent(disk.activeTime, locale)}</span>
        </div>
        {buffer !== undefined && (
          <TimeSeriesChart
            series={[{ buffer, color: colors.accent, fillOpacity: 0.18 }]}
            revision={history.revision}
            scale={{ min: 0, max: 100 }}
            grid={{ horizontalLines: 4 }}
            className="h-40 w-full"
            ariaLabel={disk.mount ?? disk.name}
          />
        )}
      </div>

      {disk.health?.failing === true && (
        <div className="border-[var(--color-status-danger)]/40 bg-[var(--color-status-danger)]/10 rounded-md border p-2.5">
          <p className="text-sm font-medium text-[var(--color-status-danger)]">
            {t('disk.failing')}
          </p>
          <p className="text-2xs mt-0.5 text-[var(--color-fg-muted)]">{t('disk.failingHint')}</p>
        </div>
      )}

      {disk.total > 0 && (
        <Meter
          label={t('disk.used')}
          accessibleLabel={`${disk.mount ?? disk.name} ${t('disk.used')}`}
          value={used}
          max={disk.total}
          valueText={`${formatBytes(used, locale)} / ${formatBytes(disk.total, locale)}`}
        />
      )}

      <StatList
        columns={3}
        stats={[
          {
            key: 'active',
            label: t('disk.activeTime'),
            value: formatPercent(disk.activeTime, locale),
            hint: t('disk.activeTimeHint'),
          },
          {
            key: 'response',
            label: t('disk.responseTime'),
            value: disk.responseMs !== null ? formatLatency(disk.responseMs, locale) : null,
            hint: disk.responseMs !== null ? t('disk.responseHint') : undefined,
          },
          { key: 'read', label: t('disk.readSpeed'), value: formatThroughput(disk.read, locale) },
          {
            key: 'write',
            label: t('disk.writeSpeed'),
            value: formatThroughput(disk.write, locale),
          },
          {
            key: 'queue',
            label: t('disk.queueDepth'),
            value: disk.queueDepth !== null ? formatCount(disk.queueDepth, locale) : null,
          },
          { key: 'free', label: t('disk.free'), value: formatBytes(disk.free, locale) },
          { key: 'type', label: t('disk.type'), value: t(`kind.${disk.kind}`) },
          { key: 'model', label: t('disk.model'), value: disk.model },
          {
            key: 'life',
            label: t('disk.lifeRemaining'),
            value:
              disk.health?.lifeRemaining != null
                ? formatPercent(disk.health.lifeRemaining, locale, 0)
                : null,
          },
          {
            key: 'hours',
            label: t('disk.powerOnHours'),
            value:
              disk.health?.powerOnHours != null
                ? formatUptime(disk.health.powerOnHours * 3600)
                : null,
          },
          {
            key: 'written',
            label: t('disk.totalWritten'),
            value:
              disk.health?.totalWritten != null
                ? formatBytes(disk.health.totalWritten, locale)
                : null,
          },
          {
            key: 'reallocated',
            label: t('disk.reallocated'),
            value:
              disk.health?.reallocatedSectors != null
                ? formatCount(disk.health.reallocatedSectors, locale)
                : null,
          },
        ]}
      />
    </div>
  );
}

export function NetworkPanel({
  nic,
  history,
  locale,
}: {
  readonly nic: NetworkMetrics;
  readonly history: MetricHistory;
  readonly locale: string;
}): React.JSX.Element {
  const { t } = useTranslation(PERFORMANCE_NS);
  const colors = useThemeColors();

  return (
    <div className="flex flex-col gap-4">
      <div>
        <div className="mb-2 flex items-baseline justify-between gap-3">
          <span className="truncate text-sm font-medium">{nic.name}</span>
          <Badge tone={nic.connected ? 'ok' : 'neutral'}>
            {nic.connected ? t('network.connected') : t('network.disconnected')}
          </Badge>
        </div>
        {/*
         * Totals rather than this adapter's own series: per-NIC history is not
         * collected, because on a machine with a dozen virtual adapters that
         * would be a dozen ring buffers filling with zeroes. The panel is
         * explicit that the chart is machine-wide by labelling the figures
         * beneath it, which are this adapter's.
         */}
        <TimeSeriesChart
          series={[
            { buffer: history.core.netRx, color: colors.accent, fillOpacity: 0.15 },
            { buffer: history.core.netTx, color: colors.warning, fillOpacity: 0.15 },
          ]}
          revision={history.revision}
          scale={{ min: 0 }}
          grid={{ horizontalLines: 4 }}
          className="h-40 w-full"
          ariaLabel={t('network.title')}
        />
      </div>

      <StatList
        columns={3}
        stats={[
          { key: 'rx', label: t('network.receive'), value: formatThroughput(nic.rx, locale) },
          { key: 'tx', label: t('network.send'), value: formatThroughput(nic.tx, locale) },
          {
            key: 'total',
            label: t('network.sessionTotal'),
            value: `${formatBytes(nic.rxTotal, locale)} / ${formatBytes(nic.txTotal, locale)}`,
          },
          {
            key: 'errors',
            label: t('network.errors'),
            value:
              nic.errorsPerSec !== null
                ? `${formatCount(Math.round(nic.errorsPerSec), locale)}/s`
                : null,
            hint: nic.errorsPerSec !== null ? t('network.errorsHint') : undefined,
          },
          {
            key: 'link',
            label: t('network.linkSpeed'),
            value: nic.linkSpeed !== null ? formatFrequency(nic.linkSpeed, locale) : null,
          },
          { key: 'kind', label: t('network.connection'), value: t(`kind.${nic.kind}`) },
          { key: 'ssid', label: t('network.ssid'), value: nic.ssid },
          {
            key: 'signal',
            label: t('network.signal'),
            value: nic.signal !== null ? formatPercent(nic.signal, locale, 0) : null,
          },
          { key: 'ipv4', label: t('network.ipv4'), value: nic.ipv4 },
          { key: 'ipv6', label: t('network.ipv6'), value: nic.ipv6 },
          { key: 'mac', label: t('network.mac'), value: nic.mac },
          { key: 'adapter', label: t('network.adapter'), value: nic.adapter },
        ]}
      />
    </div>
  );
}
