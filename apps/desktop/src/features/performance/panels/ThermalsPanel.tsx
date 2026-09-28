/**
 * Thermals.
 *
 * # It lists what it cannot measure, and why
 *
 * This is the unusual part. Most of a consumer machine's temperature sensors
 * are unreachable without a kernel-mode driver — per-core CPU temperature, fan
 * RPM, rail voltages. Vitals does not install one, so those readings do not
 * exist here.
 *
 * The tempting options are both wrong. Showing zero is a lie the user will
 * believe. Showing nothing at all makes the tool look broken, and sends
 * someone hunting for a hardware fault that is not there. So the gaps are
 * enumerated with the reason for each: the page states plainly that the
 * limitation is Vitals', not the machine's.
 *
 * # The scale is fixed at 100 °C
 *
 * An autoscaled temperature bar is meaningless — 60 °C rendered full-width
 * looks alarming and 60 °C rendered at a tenth looks fine, for the same
 * reading. Every consumer CPU and GPU throttles at or below 100, so a bar
 * approaching the end genuinely means approaching the limit.
 */

import { useTranslation } from 'react-i18next';

import type { SystemMetrics } from '@vitals/protocol';
import { EmptyState, Meter, formatCount, formatTemperature } from '@vitals/ui';
import { StatList } from '../StatList';

import { PERFORMANCE_NS } from '../strings';

/** Above this the hardware is at or near its own limit. */
const CRITICAL_C = 90;
/** Warm, but within design tolerance for every consumer part. */
const WARM_C = 75;

export interface Reading {
  readonly key: string;
  readonly label: string;
  readonly celsius: number;
}

export function ThermalsPanel({
  system,
  locale,
}: {
  readonly system: SystemMetrics;
  readonly locale: string;
}): React.JSX.Element {
  const { t } = useTranslation(PERFORMANCE_NS);

  const readings = collectReadings(system, (key, options) => t(key, options ?? {}));
  const fans = collectFans(system);

  return (
    <div className="flex flex-col gap-4">
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('thermals.subtitle')}</p>

      {readings.length === 0 ? (
        <EmptyState title={t('unavailable')} description={t('thermals.gapsHint')} />
      ) : (
        <div className="flex flex-col gap-2">
          {readings.map((reading) => (
            <Meter
              key={reading.key}
              label={reading.label}
              accessibleLabel={`${reading.label} ${formatTemperature(reading.celsius, locale)}`}
              value={reading.celsius}
              max={100}
              valueText={formatTemperature(reading.celsius, locale)}
              tone={
                reading.celsius > CRITICAL_C
                  ? 'danger'
                  : reading.celsius > WARM_C
                    ? 'warn'
                    : 'accent'
              }
            />
          ))}
        </div>
      )}

      {fans.length > 0 && (
        <div>
          <p className="mb-1.5 text-2xs text-[var(--color-fg-muted)]">{t('thermals.fanSpeed')}</p>
          <StatList
            columns={3}
            stats={fans.map((fan) => ({
              key: fan.key,
              label: fan.label,
              value: fan.rpm !== null ? `${formatCount(fan.rpm, locale)} RPM` : `${fan.percent}%`,
            }))}
          />
        </div>
      )}

      <Gaps
        cpuMeasured={system.cpu.temperature !== null}
        fansMeasured={(system.fans ?? []).length > 0}
      />
    </div>
  );
}

/**
 * `TranslateFn` rather than i18next's own `TFunction`.
 *
 * `TFunction` is generic over the namespace and its overloads do not reduce
 * to a plain `(key, options) => string`, so naming it here would couple this
 * helper to the namespace it is called from. A structural alias keeps the
 * function testable with a trivial stub.
 */
type TranslateFn = (key: string, options?: Record<string, unknown>) => string;

export function collectReadings(system: SystemMetrics, t: TranslateFn): readonly Reading[] {
  const readings: Reading[] = [];

  if (system.cpu.temperature !== null) {
    readings.push({
      key: 'cpu',
      label: t('thermals.cpuPackage'),
      celsius: system.cpu.temperature,
    });
  }

  for (const gpu of system.gpus) {
    if (gpu.temperature !== null) {
      readings.push({
        key: `gpu:${gpu.id}`,
        label: t('thermals.gpuCore', { name: gpu.name }),
        celsius: gpu.temperature,
      });
    }
    // Hotspot is the number that actually triggers throttling on modern cards
    // and runs 10-20 °C above the core reading, so it is shown separately
    // rather than folded into one "GPU temperature".
    if (gpu.hotspotTemperature !== null) {
      readings.push({
        key: `gpu:${gpu.id}:hotspot`,
        label: t('thermals.gpuHotspot', { name: gpu.name }),
        celsius: gpu.hotspotTemperature,
      });
    }
  }

  for (const disk of system.disks) {
    if (disk.temperature !== null) {
      readings.push({
        key: `disk:${disk.id}`,
        label: t('thermals.diskTemp', { name: disk.mount ?? disk.name }),
        celsius: disk.temperature,
      });
    }
  }

  if (system.battery?.temperature != null) {
    readings.push({
      key: 'battery',
      label: t('thermals.batteryTemp'),
      celsius: system.battery.temperature,
    });
  }

  return readings;
}

function collectFans(system: SystemMetrics) {
  // `?? []`: a phone paired with an older desktop receives frames without it.
  const board = (system.fans ?? []).map((fan, index) => ({
    key: `board:${index}`,
    label: fan.name,
    rpm: fan.rpm,
    percent: 0,
  }));
  const gpus = system.gpus
    .filter((gpu) => gpu.fanRpm !== null || gpu.fanPercent !== null)
    .map((gpu) => ({
      key: `fan:${gpu.id}`,
      label: gpu.name,
      rpm: gpu.fanRpm,
      percent: gpu.fanPercent !== null ? Math.round(gpu.fanPercent) : 0,
    }));
  return [...board, ...gpus];
}

/**
 * The honesty section.
 *
 * Mirrors `DRIVER_GAPS` in the Rust `sensors::driver` module. Kept as a static
 * list here rather than plumbed through the frame because the gaps are a
 * property of this build, not of the machine — every user has the same ones,
 * and shipping them over IPC every second to say something that never changes
 * would be silly.
 */
const CPU_GAP = 'Per-core CPU temperature';
const CPU_FAN_GAP = 'CPU fan speed';
const CASE_FAN_GAP = 'Case fan speeds';
const FAN_GAPS: readonly string[] = [CPU_FAN_GAP, CASE_FAN_GAP];

const GAPS: readonly string[] = [
  CPU_GAP,
  CPU_FAN_GAP,
  'Motherboard and VRM temperatures',
  'Rail voltages',
  CASE_FAN_GAP,
];

/**
 * The optional sensors service (ADR-0034) supplies CPU temperature and, on
 * a board with a supported Super-I/O chip, fan speeds; listing either as
 * unmeasurable beside its own reading would contradict the screen.
 */
export function gapsFor(cpuMeasured: boolean, fansMeasured = false): readonly string[] {
  return GAPS.filter(
    (gap) => !(cpuMeasured && gap === CPU_GAP) && !(fansMeasured && FAN_GAPS.includes(gap)),
  );
}

function Gaps({
  cpuMeasured,
  fansMeasured,
}: {
  readonly cpuMeasured: boolean;
  readonly fansMeasured: boolean;
}) {
  const { t } = useTranslation(PERFORMANCE_NS);

  return (
    <details className="rounded-md border border-[var(--color-border-subtle)] p-2.5">
      <summary className="cursor-pointer text-sm font-medium">{t('thermals.gaps')}</summary>
      <p className="mt-1.5 text-2xs text-[var(--color-fg-muted)]">{t('thermals.gapsHint')}</p>
      <ul className="mt-1.5 list-disc pl-4 text-2xs text-[var(--color-fg-subtle)]">
        {gapsFor(cpuMeasured, fansMeasured).map((gap) => (
          <li key={gap}>{gap}</li>
        ))}
      </ul>
    </details>
  );
}
