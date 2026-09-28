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
  AnimatedValue,
  formatBytes,
  formatCount,
  formatFrequency,
  formatPercent,
  formatThroughput,
} from '@vitals/ui';

import { useThemeColors } from './useThemeColors';
import type { MetricHistory } from '../history';
import { DASHBOARD_NS } from '../strings';

export interface WidgetBodyProps {
  readonly system: SystemMetrics;
  readonly history: MetricHistory;
  readonly locale: string;
}

/**
 * The chart area: takes whatever height the card has left after the stats.
 *
 * The dashboard fits the window (S12-25), so a card's height is its share
 * of the window, not its content. A fixed-height chart either left a hole
 * at 1440p or pushed the stats out of a card at 768p; flexing it means the
 * numbers always stay visible and the chart grows into the rest. The floor
 * keeps a trace legible in the smallest card.
 */
function ChartArea({ children }: { readonly children: React.ReactNode }) {
  return <div className="relative min-h-8 flex-1">{children}</div>;
}

/** A row of label/value pairs under a chart. */
function Stats({
  items,
}: {
  readonly items: readonly { label: string; value: string; key: string }[];
}) {
  return (
    <dl className="grid shrink-0 grid-cols-[repeat(auto-fill,minmax(6.5rem,1fr))] gap-x-3 gap-y-1">
      {items.map((item) => (
        <div key={item.key} className="min-w-0">
          <dt className="truncate text-2xs leading-4 text-[var(--color-fg-muted)]">{item.label}</dt>
          <dd className="tnum truncate font-mono text-xs leading-4 text-[var(--color-fg-default)]">
            <AnimatedValue value={item.value} />
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
    { buffer: history.core.cpu, color: colors.accent, fillOpacity: 0.18, headDot: true },
    // Kernel time is drawn under the total rather than beside it: the useful
    // reading is what share of a busy CPU is the OS rather than the user's
    // programs, and two independent lines make that a subtraction the reader
    // has to do in their head.
    { buffer: history.core.cpuKernel, color: colors.muted, lineWidth: 1 },
  ];

  return (
    <>
      <ChartArea>
        <TimeSeriesChart
          series={series}
          revision={history.revision}
          scale={{ min: 0, max: 100 }}
          className="absolute inset-0"
          ariaLabel={t('widget.cpu.title')}
        />
      </ChartArea>
      <PerCore values={cpu.perCore} label={t('cpu.perCore')} />
      <Stats
        items={[
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
        ]}
      />
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
    <div
      className="flex h-5 shrink-0 items-end gap-px"
      role="img"
      // One accessible summary rather than 32 announcements: a screen reader
      // reading "core 1, 4%, core 2, 7%…" every second is unusable, and the
      // per-core detail is available on the Performance page as a table.
      aria-label={`${label}: ${values.length}`}
      title={label}
    >
      {values.map((value, index) => (
        <div
          key={index}
          className="flex min-w-0 flex-1 flex-col justify-end rounded-t-[2px] bg-[var(--color-bg-inset)]"
        >
          <div
            // The fill was never at the bottom: `marginTop: auto` does
            // nothing outside a flex column, so every bar hung from the
            // top. The column above is what makes it grow upwards.
            className="w-full rounded-t-[2px] bg-gradient-to-t from-[var(--color-accent)] to-[color-mix(in_oklch,var(--color-accent)_60%,white)] transition-[height] duration-(--duration-slow) ease-(--ease-out-quart)"
            style={{
              height: `${Math.min(Math.max(value, 0), 100).toFixed(1)}%`,
            }}
          />
        </div>
      ))}
    </div>
  );
}

export function MemoryWidget({ system, history, locale }: WidgetBodyProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const colors = useThemeColors();
  const { memory } = system;

  return (
    <>
      <ChartArea>
        <TimeSeriesChart
          series={[
            {
              buffer: history.core.memoryPercent,
              color: colors.accent,
              fillOpacity: 0.18,
              headDot: true,
            },
          ]}
          revision={history.revision}
          scale={{ min: 0, max: 100 }}
          className="absolute inset-0"
          ariaLabel={t('widget.memory.title')}
        />
      </ChartArea>
      <Stats
        items={[
          { key: 'used', label: t('memory.used'), value: formatBytes(memory.used, locale) },
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
      <ChartArea>
        <TimeSeriesChart
          series={[
            {
              buffer: history.core.diskRead,
              color: colors.accent,
              fillOpacity: 0.15,
              headDot: true,
            },
            { buffer: history.core.diskWrite, color: colors.warning, fillOpacity: 0.15 },
          ]}
          revision={history.revision}
          // Autoscaled, unlike CPU: throughput has no natural ceiling, and a
          // fixed axis picked for an NVMe drive would flatten a USB stick's
          // trace to the baseline. `niceTo` keeps the axis from re-fitting on
          // every frame, which otherwise makes a steady line visibly breathe.
          scale={{ min: 0 }}
          className="absolute inset-0"
          ariaLabel={t('widget.disk.title')}
        />
      </ChartArea>
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
      <ChartArea>
        <TimeSeriesChart
          series={[
            { buffer: history.core.netRx, color: colors.accent, fillOpacity: 0.15, headDot: true },
            { buffer: history.core.netTx, color: colors.warning, fillOpacity: 0.15 },
          ]}
          revision={history.revision}
          scale={{ min: 0 }}
          className="absolute inset-0"
          ariaLabel={t('widget.network.title')}
        />
      </ChartArea>
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
