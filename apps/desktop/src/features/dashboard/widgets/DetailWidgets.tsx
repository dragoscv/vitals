/**
 * The remaining widgets: GPU, thermals, battery, top processes, system,
 * storage.
 *
 * These share a rule the resource widgets do not need: every one of them can
 * find that the machine reports nothing. A desktop has no battery, most
 * machines report no CPU temperature without a ring-0 driver, and an integrated
 * GPU may expose no memory figures.
 *
 * In every such case the widget says so in words. It does not render `0`,
 * `—`, or an empty chart. Zero degrees and "we cannot read the sensor" are
 * different facts, and a monitoring tool that conflates them is worse than one
 * that omits the reading, because the user believes the number.
 */

import { useTranslation } from 'react-i18next';

import type { Process, SystemMetrics } from '@vitals/protocol';
import {
  Badge,
  Meter,
  ProgressBar,
  formatBytes,
  formatCount,
  formatFrequency,
  formatPercent,
  formatTemperature,
  formatUptime,
  formatWatts,
} from '@vitals/ui';

import { DASHBOARD_NS } from '../strings';
import { topByCpu, topByMemory, type TopEntry } from '../topProcesses';

export interface DetailWidgetProps {
  readonly system: SystemMetrics;
  readonly locale: string;
  /**
   * Wall-clock time of the frame these readings came from.
   *
   * Passed in rather than read from `Date.now()` during render, which is an
   * impure call the compiler rejects — and rightly: the boot time would drift
   * by a second every time the component happened to re-render, from a clock
   * unrelated to the sample. This is the time the sampler actually measured.
   */
  readonly timestampMs: number;
}

/** Shown in place of a reading the hardware does not provide. */
function Unavailable() {
  const { t } = useTranslation(DASHBOARD_NS);
  return <p className="text-2xs text-[var(--color-fg-muted)]">{t('unavailable')}</p>;
}

export function GpuWidget({ system, locale }: DetailWidgetProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);

  return (
    <div className="flex flex-col gap-3">
      {system.gpus.map((gpu) => (
        <div key={gpu.id} className="flex flex-col gap-1.5">
          <div className="flex items-baseline justify-between gap-2">
            <span className="truncate text-sm font-medium">{gpu.name}</span>
            {gpu.throttled !== null && (
              <Badge tone="warn">{t('alert.gpuThrottled.title', { gpu: gpu.name })}</Badge>
            )}
          </div>
          <Meter
            label={t('metric.gpu', { ns: 'translation' })}
            accessibleLabel={gpu.name}
            value={gpu.utilization}
            valueText={formatPercent(gpu.utilization, locale)}
          />
          {gpu.memoryUsed !== null && gpu.memoryTotal !== null ? (
            <Meter
              label={t('gpu.memory')}
              accessibleLabel={`${gpu.name} ${t('gpu.memory')}`}
              value={gpu.memoryUsed}
              max={gpu.memoryTotal}
              valueText={`${formatBytes(gpu.memoryUsed, locale)} / ${formatBytes(gpu.memoryTotal, locale)}`}
            />
          ) : (
            <Unavailable />
          )}
        </div>
      ))}
    </div>
  );
}

export function ThermalsWidget({ system, locale }: DetailWidgetProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);

  const readings: { key: string; label: string; celsius: number }[] = [];
  if (system.cpu.temperature !== null) {
    readings.push({ key: 'cpu', label: t('cpu.total'), celsius: system.cpu.temperature });
  }
  for (const gpu of system.gpus) {
    if (gpu.temperature !== null) {
      // Prefixed rather than the bare id: gpu ids are numbers and would
      // collide with a future numeric key from another source in the same list.
      readings.push({ key: `gpu:${gpu.id}`, label: gpu.name, celsius: gpu.temperature });
    }
  }

  if (readings.length === 0) return <Unavailable />;

  return (
    <div className="flex flex-col gap-2">
      {readings.map((reading) => (
        <Meter
          key={reading.key}
          label={reading.label}
          accessibleLabel={`${reading.label} ${formatTemperature(reading.celsius, locale)}`}
          value={reading.celsius}
          // 100°C rather than autoscaling: a temperature bar means nothing
          // without a reference, and every consumer CPU and GPU throttles at
          // or below this, so the bar reaching the end is meaningful.
          max={100}
          valueText={formatTemperature(reading.celsius, locale)}
          tone={reading.celsius > 90 ? 'danger' : reading.celsius > 75 ? 'warn' : 'accent'}
        />
      ))}
    </div>
  );
}

export function BatteryWidget({ system, locale }: DetailWidgetProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const battery = system.battery;
  if (battery === null) return <Unavailable />;

  const status = battery.charging
    ? t('battery.charging')
    : battery.timeRemainingSecs !== null
      ? t('battery.remaining', { time: formatUptime(battery.timeRemainingSecs) })
      : battery.charge >= 99
        ? t('battery.charged')
        : '';

  return (
    <div className="flex flex-col gap-2">
      <Meter
        label={status}
        accessibleLabel={t('widget.battery.title')}
        value={battery.charge}
        valueText={formatPercent(battery.charge, locale, 0)}
        tone={battery.charge < 10 && !battery.charging ? 'danger' : 'accent'}
      />
      <dl className="grid grid-cols-2 gap-x-3 gap-y-1.5">
        {battery.health !== null && (
          <div>
            <dt className="text-2xs text-[var(--color-fg-muted)]">{t('battery.health')}</dt>
            <dd className="tnum font-mono text-sm">{formatPercent(battery.health, locale, 0)}</dd>
          </div>
        )}
        {battery.cycleCount !== null && (
          <div>
            <dt className="text-2xs text-[var(--color-fg-muted)]">{t('battery.cycles')}</dt>
            <dd className="tnum font-mono text-sm">{formatCount(battery.cycleCount, locale)}</dd>
          </div>
        )}
        {battery.power !== null && (
          <div>
            <dt className="text-2xs text-[var(--color-fg-muted)]">
              {t('metric.power', { ns: 'translation' })}
            </dt>
            {/* Absolute value: the sign encodes direction, which the status
                line already says in words, and "-14 W" reads as an error. */}
            <dd className="tnum font-mono text-sm">
              {formatWatts(Math.abs(battery.power), locale)}
            </dd>
          </div>
        )}
      </dl>
    </div>
  );
}

export interface TopWidgetProps {
  readonly processes: ReadonlyMap<string, Process>;
  readonly locale: string;
  readonly onSelect: () => void;
}

function TopList({
  entries,
  format,
  max,
  onSelect,
}: {
  readonly entries: readonly TopEntry[];
  readonly format: (value: number) => string;
  readonly max: number;
  readonly onSelect: () => void;
}) {
  const { t } = useTranslation(DASHBOARD_NS);
  if (entries.length === 0) {
    return <p className="text-2xs text-[var(--color-fg-muted)]">{t('top.idle')}</p>;
  }

  return (
    <ol className="flex flex-col gap-1.5">
      {entries.map((entry) => (
        <li key={entry.name} className="flex flex-col gap-0.5">
          <div className="flex items-baseline justify-between gap-2">
            <span className="truncate text-sm">
              {entry.name}
              {entry.count > 1 && (
                <span className="ml-1.5 text-2xs text-[var(--color-fg-muted)]">
                  {t('top.processCount', { count: entry.count })}
                </span>
              )}
            </span>
            <span className="tnum shrink-0 font-mono text-sm">{format(entry.value)}</span>
          </div>
          <ProgressBar
            // Scaled against the largest entry rather than against 100% or
            // total RAM: the point of the bar is to compare these five with
            // each other, and against an absolute scale five processes using
            // 2% each are five indistinguishable slivers.
            value={max > 0 ? (entry.value / max) * 100 : 0}
            label={entry.name}
            valueText={format(entry.value)}
            size="sm"
          />
        </li>
      ))}
      <li>
        <button
          type="button"
          onClick={onSelect}
          className="text-2xs text-[var(--color-accent-fg)] underline-offset-2 hover:underline"
        >
          {t('top.viewAll')}
        </button>
      </li>
    </ol>
  );
}

export function TopCpuWidget({ processes, locale, onSelect }: TopWidgetProps): React.JSX.Element {
  const entries = topByCpu(processes);
  return (
    <TopList
      entries={entries}
      max={entries[0]?.value ?? 0}
      format={(value) => formatPercent(value, locale)}
      onSelect={onSelect}
    />
  );
}

export function TopMemoryWidget({
  processes,
  locale,
  onSelect,
}: TopWidgetProps): React.JSX.Element {
  const entries = topByMemory(processes);
  return (
    <TopList
      entries={entries}
      max={entries[0]?.value ?? 0}
      format={(value) => formatBytes(value, locale)}
      onSelect={onSelect}
    />
  );
}

export function SystemWidget({
  system,
  locale,
  timestampMs,
}: DetailWidgetProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const { cpu, memory } = system;

  const bootTime = new Date(timestampMs - cpu.uptimeSecs * 1000);

  return (
    <dl className="grid grid-cols-2 gap-x-3 gap-y-2">
      <div className="col-span-2">
        <dt className="text-2xs text-[var(--color-fg-muted)]">{t('system.uptime')}</dt>
        <dd className="tnum font-mono text-sm">{formatUptime(cpu.uptimeSecs)}</dd>
        <dd className="text-2xs text-[var(--color-fg-muted)]">
          {t('system.since', {
            date: new Intl.DateTimeFormat(locale, {
              dateStyle: 'medium',
              timeStyle: 'short',
            }).format(bootTime),
          })}
        </dd>
      </div>
      <div>
        <dt className="text-2xs text-[var(--color-fg-muted)]">{t('cpu.processes')}</dt>
        <dd className="tnum font-mono text-sm">{formatCount(cpu.processCount, locale)}</dd>
      </div>
      <div>
        <dt className="text-2xs text-[var(--color-fg-muted)]">{t('cpu.threads')}</dt>
        <dd className="tnum font-mono text-sm">{formatCount(cpu.threadCount, locale)}</dd>
      </div>
      {memory.speed !== null && (
        <div>
          <dt className="text-2xs text-[var(--color-fg-muted)]">{t('memory.speed')}</dt>
          <dd className="tnum font-mono text-sm">{formatFrequency(memory.speed, locale)}</dd>
        </div>
      )}
      {memory.slotsUsed !== null && memory.slotsTotal !== null && (
        <div>
          <dt className="text-2xs text-[var(--color-fg-muted)]">
            {t('metric.memory', { ns: 'translation' })}
          </dt>
          <dd className="tnum font-mono text-sm">
            {t('memory.slots', { used: memory.slotsUsed, total: memory.slotsTotal })}
          </dd>
        </div>
      )}
    </dl>
  );
}

export function StorageWidget({ system, locale }: DetailWidgetProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);

  // Drives reporting no capacity are filtered out rather than shown at 0%:
  // an empty card reader is a device, not a full disk, and a red bar for one
  // is a false alarm on a machine that is completely healthy.
  const drives = system.disks.filter((disk) => disk.total > 0);
  if (drives.length === 0) return <Unavailable />;

  return (
    <div className="flex flex-col gap-2">
      {drives.map((disk) => {
        const usedPercent = ((disk.total - disk.free) / disk.total) * 100;
        const label = disk.mount ?? disk.name;
        return (
          <Meter
            key={disk.id}
            label={label}
            accessibleLabel={`${label} ${t('storage.used', { percent: Math.round(usedPercent) })}`}
            value={usedPercent}
            valueText={t('storage.free', {
              free: formatBytes(disk.free, locale),
              total: formatBytes(disk.total, locale),
            })}
            tone={usedPercent > 95 ? 'danger' : usedPercent > 85 ? 'warn' : 'accent'}
          />
        );
      })}
    </div>
  );
}
