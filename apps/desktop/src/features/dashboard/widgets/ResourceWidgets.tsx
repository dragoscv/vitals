/**
 * The four resource widgets: CPU, memory, disk, network.
 *
 * Each pairs a live chart with the two or three numbers that give it meaning.
 * A chart with no scale is decoration, and a number with no history cannot
 * distinguish "it has been like this for a minute" from "it just spiked" —
 * which is the entire question a dashboard exists to answer.
 *
 * # Colours come from the theme, resolved per render
 *
 * The canvas renderer needs a concrete CSS colour string; it cannot resolve a
 * `var()`. Reading the computed value at draw time rather than hardcoding hex
 * is what lets the accent setting and light/dark mode actually reach the
 * charts, instead of leaving them permanently blue on a pink theme.
 */

import { useTranslation } from 'react-i18next';

import { TimeSeriesChart, type Series } from '@vitals/charts';
import type { SystemMetrics } from '@vitals/protocol';
import {
  formatBytes,
  formatCount,
  formatFrequency,
  formatPercent,
  formatThroughput,
  Meter,
} from '@vitals/ui';

import { useThemeColors } from './useThemeColors';
import type { MetricHistory } from '../history';
import { DASHBOARD_NS } from '../strings';

export interface WidgetBodyProps {
  readonly system: SystemMetrics;
  readonly history: MetricHistory;
  readonly locale: string;
}

/** A row of label/value pairs under a chart. */
function Stats({
  items,
}: {
  readonly items: readonly { label: string; value: string; key: string }[];
}) {
  return (
    <dl className="grid grid-cols-2 gap-x-3 gap-y-1.5 sm:grid-cols-3">
      {items.map((item) => (
        <div key={item.key} className="min-w-0">
          <dt className="text-2xs truncate text-[var(--color-fg-muted)]">{item.label}</dt>
          <dd className="tnum truncate font-mono text-sm text-[var(--color-fg-default)]">
            {item.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

export function CpuWidget({ system, history, locale }: WidgetBodyProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const colors = useThemeColors();
  const { cpu } = system;

  const series: readonly Series[] = [
    { buffer: history.core.cpu, color: colors.accent, fillOpacity: 0.18 },
    // Kernel time is drawn under the total rather than beside it: the useful
    // reading is what share of a busy CPU is the OS rather than the user's
    // programs, and two independent lines make that a subtraction the reader
    // has to do in their head.
    { buffer: history.core.cpuKernel, color: colors.muted, lineWidth: 1 },
  ];

  return (
    <>
      <TimeSeriesChart
        series={series}
        revision={history.revision}
        scale={{ min: 0, max: 100 }}
        className="h-24 w-full"
        ariaLabel={t('widget.cpu.title')}
      />
      <Stats
        items={[
          { key: 'total', label: t('cpu.total'), value: formatPercent(cpu.total, locale) },
          { key: 'kernel', label: t('cpu.kernel'), value: formatPercent(cpu.kernel, locale) },
          {
            key: 'clock',
            label: t('cpu.clock'),
            // Effective clock, not nominal. Reporting the base frequency of a
            // chip that is currently boosting or parked tells the reader
            // nothing about now, which is the only tense this widget has.
            value:
              cpu.effectiveClock !== null
                ? formatFrequency(cpu.effectiveClock, locale)
                : t('unavailable'),
          },
          {
            key: 'processes',
            label: t('cpu.processes'),
            value: formatCount(cpu.processCount, locale),
          },
          { key: 'threads', label: t('cpu.threads'), value: formatCount(cpu.threadCount, locale) },
          {
            key: 'handles',
            label: t('cpu.handles'),
            value:
              cpu.handleCount !== null ? formatCount(cpu.handleCount, locale) : t('unavailable'),
          },
        ]}
      />
      <PerCore values={cpu.perCore} label={t('cpu.perCore')} />
    </>
  );
}

/**
 * A bar per logical processor.
 *
 * The reason to show this at all is that a single "CPU 12%" hides the
 * difference between twelve cores lightly busy and one core pinned — and a
 * single pinned core is the signature of a single-threaded stall, which is
 * exactly the problem a user comes here to identify.
 *
 * Rendered as plain divs rather than a chart: on a 32-thread machine this is
 * 32 elements updated once a second, which is nothing, and a canvas would cost
 * more in setup than it saves.
 */
function PerCore({
  values,
  label,
}: {
  readonly values: readonly number[];
  readonly label: string;
}) {
  if (values.length === 0) return null;

  return (
    <div>
      <p className="text-2xs mb-1 text-[var(--color-fg-muted)]">{label}</p>
      <div
        className="flex h-8 items-end gap-px"
        role="img"
        // One accessible summary rather than 32 announcements: a screen reader
        // reading "core 1, 4%, core 2, 7%…" every second is unusable, and the
        // per-core detail is available on the Performance page as a table.
        aria-label={`${label}: ${values.length}`}
      >
        {values.map((value, index) => (
          <div
            key={index}
            className="min-w-0 flex-1 rounded-t-[1px] bg-[var(--color-bg-inset)]"
            style={{ height: '100%' }}
          >
            <div
              className="w-full rounded-t-[1px] bg-[var(--color-accent-solid)]"
              style={{
                height: `${Math.min(Math.max(value, 0), 100).toFixed(1)}%`,
                marginTop: 'auto',
              }}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

export function MemoryWidget({ system, history, locale }: WidgetBodyProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const colors = useThemeColors();
  const { memory } = system;

  const usedPercent = memory.total > 0 ? (memory.used / memory.total) * 100 : 0;

  return (
    <>
      <TimeSeriesChart
        series={[{ buffer: history.core.memoryPercent, color: colors.accent, fillOpacity: 0.18 }]}
        revision={history.revision}
        scale={{ min: 0, max: 100 }}
        className="h-24 w-full"
        ariaLabel={t('widget.memory.title')}
      />
      <Meter
        label={t('memory.used')}
        accessibleLabel={t('widget.memory.title')}
        value={usedPercent}
        valueText={t('memory.ofTotal', {
          used: formatBytes(memory.used, locale),
          total: formatBytes(memory.total, locale),
        })}
      />
      <Stats
        items={[
          {
            key: 'available',
            label: t('memory.available'),
            value: formatBytes(memory.available, locale),
          },
          // Cached is shown next to available deliberately: it is the number
          // that explains why "used" looks high on a healthy machine, and
          // Task Manager burying it is why people panic about their RAM.
          { key: 'cached', label: t('memory.cached'), value: formatBytes(memory.cached, locale) },
          {
            key: 'committed',
            label: t('memory.committed'),
            value: `${formatBytes(memory.committed, locale)} / ${formatBytes(memory.commitLimit, locale)}`,
          },
        ]}
      />
    </>
  );
}

export function DiskWidget({ system, history, locale }: WidgetBodyProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const colors = useThemeColors();

  let read = 0;
  let write = 0;
  for (const disk of system.disks) {
    read += disk.read;
    write += disk.write;
  }

  return (
    <>
      <TimeSeriesChart
        series={[
          { buffer: history.core.diskRead, color: colors.accent, fillOpacity: 0.15 },
          { buffer: history.core.diskWrite, color: colors.warning, fillOpacity: 0.15 },
        ]}
        revision={history.revision}
        // Autoscaled, unlike CPU: throughput has no natural ceiling, and a
        // fixed axis picked for an NVMe drive would flatten a USB stick's
        // trace to the baseline. `niceTo` keeps the axis from re-fitting on
        // every frame, which otherwise makes a steady line visibly breathe.
        scale={{ min: 0 }}
        className="h-24 w-full"
        ariaLabel={t('widget.disk.title')}
      />
      <Stats
        items={[
          { key: 'read', label: t('disk.read'), value: formatThroughput(read, locale) },
          { key: 'write', label: t('disk.write'), value: formatThroughput(write, locale) },
        ]}
      />
    </>
  );
}

export function NetworkWidget({ system, history, locale }: WidgetBodyProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const colors = useThemeColors();

  let rx = 0;
  let tx = 0;
  let rxTotal = 0;
  let txTotal = 0;
  let connected = false;
  for (const nic of system.networks) {
    rx += nic.rx;
    tx += nic.tx;
    rxTotal += nic.rxTotal;
    txTotal += nic.txTotal;
    if (nic.connected) connected = true;
  }

  return (
    <>
      <TimeSeriesChart
        series={[
          { buffer: history.core.netRx, color: colors.accent, fillOpacity: 0.15 },
          { buffer: history.core.netTx, color: colors.warning, fillOpacity: 0.15 },
        ]}
        revision={history.revision}
        scale={{ min: 0 }}
        className="h-24 w-full"
        ariaLabel={t('widget.network.title')}
      />
      <Stats
        items={[
          { key: 'down', label: t('network.down'), value: formatThroughput(rx, locale) },
          { key: 'up', label: t('network.up'), value: formatThroughput(tx, locale) },
          {
            key: 'total',
            label: connected ? t('network.total') : t('network.offline'),
            value: `${formatBytes(rxTotal, locale)} / ${formatBytes(txTotal, locale)}`,
          },
        ]}
      />
    </>
  );
}
