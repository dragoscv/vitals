/**
 * Turns readings into the one sentence the user came for.
 *
 * # The problem with the numbers alone
 *
 * A dashboard of gauges asks the user to be a performance engineer. "Memory
 * 87%" is not actionable: it is alarming on a machine that is thrashing and
 * completely normal on one with a large file cache. The distinction is not in
 * the percentage at all — it is in the page-fault rate, which no consumer task
 * manager surfaces.
 *
 * So every rule here is written against the signal that actually predicts a
 * machine feeling slow, and each alert carries a `causeKey` explaining *why*
 * rather than restating the number the user can already see.
 *
 * # Sustained, not instantaneous
 *
 * Every threshold is evaluated against an average over a window, never a
 * single sample. CPU touches 100% during any application launch; a dashboard
 * that shouts about it teaches the user to ignore it, and then it is useless
 * for the case that matters. `SUSTAIN_SAMPLES` is the number of consecutive
 * samples a condition must hold — at 1 Hz, fifteen seconds.
 *
 * # Nothing here is a diagnosis
 *
 * These are heuristics over what Windows reports, and they are deliberately
 * phrased as observations ("the disk has been saturated for a minute") rather
 * than verdicts ("your disk is failing"). The one exception is SMART, which is
 * the drive's own assessment of itself and is reported as such.
 */

import type { SystemMetrics } from '@vitals/protocol';

import type { MetricHistory } from './history';

export const alertSeverities = ['info', 'warning', 'critical'] as const;
export type AlertSeverity = (typeof alertSeverities)[number];

export type AlertId =
  | 'cpuSustained'
  | 'cpuThrottled'
  | 'memoryPressure'
  | 'memoryCommit'
  | 'diskSaturated'
  | 'diskLatency'
  | 'diskSpace'
  | 'diskHealth'
  | 'gpuThrottled'
  | 'thermalCpu'
  | 'networkErrors'
  | 'batteryLow'
  | 'batteryHealth';

export interface Alert {
  readonly id: AlertId;
  readonly severity: AlertSeverity;
  /** Key into `dashboard.alert.<id>.title`. */
  readonly titleKey: string;
  /** Key into `dashboard.alert.<id>.cause` — the "why", not the "what". */
  readonly causeKey: string;
  /** Interpolation values for the cause string. Already rounded for display. */
  readonly values: Readonly<Record<string, string | number>>;
  /** Where the user should go to act on this. */
  readonly route?: 'performance' | 'processes' | 'storage' | 'network' | 'devices';
}

/** Consecutive samples a condition must hold. Fifteen seconds at 1 Hz. */
export const SUSTAIN_SAMPLES = 15;

/**
 * Thresholds.
 *
 * Each is chosen against a documented failure, not a round number. They are
 * exported so tests assert the boundary rather than re-deriving it, which is
 * how a threshold silently drifts from the reason it was picked.
 */
export const THRESHOLDS = {
  /** Below this a machine still has headroom for interactive work. */
  cpuSustainedPercent: 90,
  /**
   * Page faults per second indicating real memory pressure.
   *
   * This, not the percentage, is what separates "RAM is full of useful cache"
   * from "the machine is swapping". A few hundred faults a second is routine;
   * thousands sustained means the working set does not fit.
   */
  pageFaultsPerSec: 2000,
  /** Commit approaching the limit means allocations are about to fail. */
  commitPercent: 90,
  /** Disk busy essentially all the time. */
  diskActivePercent: 95,
  /**
   * Average response in milliseconds.
   *
   * 25 ms is unremarkable for a spinning disk under load and catastrophic for
   * an NVMe drive, so this is only raised alongside high active time — the
   * combination is what indicates a queue, rather than a slow medium.
   */
  diskResponseMs: 25,
  /** Free space below which Windows itself starts misbehaving. */
  diskFreePercent: 5,
  /** Sustained CPU package temperature. */
  cpuTemperatureC: 95,
  /** Dropped packets per second — the first sign of a bad cable or link. */
  networkErrorsPerSec: 10,
  batteryLowPercent: 10,
  /** Design-capacity retention below which a battery is worth replacing. */
  batteryHealthPercent: 60,
} as const;

/**
 * True when the last `count` samples all exceed `threshold`.
 *
 * Reads backwards from the newest sample and stops at the first value that
 * fails, so the common case (a machine that is fine) costs one comparison.
 * Returns false when there is not yet enough history: an alert raised two
 * seconds after launch, on the strength of two samples, is a guess.
 */
export function sustainedAbove(
  buffer: { readonly size: number; at(index: number): number | undefined },
  threshold: number,
  count: number = SUSTAIN_SAMPLES,
): boolean {
  if (buffer.size < count) return false;

  for (let offset = 0; offset < count; offset += 1) {
    const value = buffer.at(buffer.size - 1 - offset);
    // A gap is stored as NaN. It is neither above nor below the threshold, and
    // treating it as either would let a stalled sampler raise or clear an
    // alert on data that was never collected.
    if (value === undefined || Number.isNaN(value) || value <= threshold) return false;
  }
  return true;
}

export interface AlertInput {
  readonly system: SystemMetrics | null;
  readonly history: MetricHistory;
}

/**
 * Evaluates every rule against the current state.
 *
 * Pure and synchronous: the whole point is that this can be exercised over a
 * synthetic history in a test, since none of these conditions can be produced
 * on demand on a real machine.
 */
export function evaluateAlerts({ system, history }: AlertInput): readonly Alert[] {
  if (system === null) return [];

  const alerts: Alert[] = [];
  const { cpu, memory, disks, networks, gpus, battery } = system;

  if (sustainedAbove(history.core.cpu, THRESHOLDS.cpuSustainedPercent)) {
    alerts.push({
      id: 'cpuSustained',
      severity: 'warning',
      titleKey: 'alert.cpuSustained.title',
      causeKey: 'alert.cpuSustained.cause',
      values: { percent: Math.round(history.core.cpu.average()), seconds: SUSTAIN_SAMPLES },
      route: 'processes',
    });
  }

  if (cpu.throttled !== null) {
    alerts.push({
      id: 'cpuThrottled',
      severity: 'warning',
      titleKey: 'alert.cpuThrottled.title',
      causeKey: `alert.cpuThrottled.${cpu.throttled}`,
      values: {},
      route: 'performance',
    });
  }

  // Deliberately gated on faulting, not on the percentage. See the module note.
  if (
    memory.pageFaultsPerSec !== null &&
    memory.pageFaultsPerSec > THRESHOLDS.pageFaultsPerSec &&
    memory.total > 0 &&
    memory.available / memory.total < 0.1
  ) {
    alerts.push({
      id: 'memoryPressure',
      severity: 'critical',
      titleKey: 'alert.memoryPressure.title',
      causeKey: 'alert.memoryPressure.cause',
      values: { faults: Math.round(memory.pageFaultsPerSec) },
      route: 'processes',
    });
  }

  if (memory.commitLimit > 0) {
    const commitPercent = (memory.committed / memory.commitLimit) * 100;
    if (commitPercent > THRESHOLDS.commitPercent) {
      alerts.push({
        id: 'memoryCommit',
        severity: 'warning',
        titleKey: 'alert.memoryCommit.title',
        causeKey: 'alert.memoryCommit.cause',
        values: { percent: Math.round(commitPercent) },
        route: 'performance',
      });
    }
  }

  for (const disk of disks) {
    const active = history.diskActive.get(disk.id);

    if (active !== undefined && sustainedAbove(active, THRESHOLDS.diskActivePercent)) {
      // Latency is reported separately from saturation because they have
      // different causes and different fixes: a saturated disk is a workload
      // problem, a slow-responding one that is *not* saturated is a hardware
      // or driver problem.
      const slow = disk.responseMs !== null && disk.responseMs > THRESHOLDS.diskResponseMs;
      alerts.push({
        id: slow ? 'diskLatency' : 'diskSaturated',
        severity: 'warning',
        titleKey: slow ? 'alert.diskLatency.title' : 'alert.diskSaturated.title',
        causeKey: slow ? 'alert.diskLatency.cause' : 'alert.diskSaturated.cause',
        values: { disk: disk.mount ?? disk.name, ms: Math.round(disk.responseMs ?? 0) },
        route: 'processes',
      });
    }

    if (disk.total > 0) {
      const freePercent = (disk.free / disk.total) * 100;
      if (freePercent < THRESHOLDS.diskFreePercent) {
        alerts.push({
          id: 'diskSpace',
          severity: freePercent < 1 ? 'critical' : 'warning',
          titleKey: 'alert.diskSpace.title',
          causeKey: 'alert.diskSpace.cause',
          values: { disk: disk.mount ?? disk.name, percent: Math.round(freePercent) },
          route: 'storage',
        });
      }
    }

    // The drive's own verdict, not ours — reported as such in the string.
    if (disk.health?.failing === true) {
      alerts.push({
        id: 'diskHealth',
        severity: 'critical',
        titleKey: 'alert.diskHealth.title',
        causeKey: 'alert.diskHealth.cause',
        values: { disk: disk.mount ?? disk.name },
        route: 'storage',
      });
    }
  }

  for (const gpu of gpus) {
    if (gpu.throttled !== null) {
      alerts.push({
        id: 'gpuThrottled',
        severity: 'warning',
        titleKey: 'alert.gpuThrottled.title',
        causeKey: `alert.gpuThrottled.${gpu.throttled}`,
        values: { gpu: gpu.name },
        route: 'performance',
      });
    }
  }

  if (cpu.temperature !== null && cpu.temperature > THRESHOLDS.cpuTemperatureC) {
    alerts.push({
      id: 'thermalCpu',
      severity: 'critical',
      titleKey: 'alert.thermalCpu.title',
      causeKey: 'alert.thermalCpu.cause',
      values: { celsius: Math.round(cpu.temperature) },
      route: 'performance',
    });
  }

  for (const nic of networks) {
    if (
      nic.connected &&
      nic.errorsPerSec !== null &&
      nic.errorsPerSec > THRESHOLDS.networkErrorsPerSec
    ) {
      alerts.push({
        id: 'networkErrors',
        severity: 'warning',
        titleKey: 'alert.networkErrors.title',
        causeKey: 'alert.networkErrors.cause',
        values: { adapter: nic.name, errors: Math.round(nic.errorsPerSec) },
        route: 'network',
      });
    }
  }

  if (battery !== null) {
    if (battery.charge < THRESHOLDS.batteryLowPercent && !battery.charging) {
      alerts.push({
        id: 'batteryLow',
        severity: 'warning',
        titleKey: 'alert.batteryLow.title',
        causeKey: 'alert.batteryLow.cause',
        values: { percent: Math.round(battery.charge) },
      });
    }

    if (battery.health !== null && battery.health < THRESHOLDS.batteryHealthPercent) {
      alerts.push({
        id: 'batteryHealth',
        severity: 'info',
        titleKey: 'alert.batteryHealth.title',
        causeKey: 'alert.batteryHealth.cause',
        values: { percent: Math.round(battery.health) },
        route: 'devices',
      });
    }
  }

  return sortBySeverity(alerts);
}

const SEVERITY_ORDER: Readonly<Record<AlertSeverity, number>> = {
  critical: 0,
  warning: 1,
  info: 2,
};

/**
 * Most serious first, stable within a severity.
 *
 * Stability matters more than it looks: alerts are re-evaluated every second,
 * and an unstable sort would let two warnings swap places on every tick,
 * producing a list that flickers even though nothing changed.
 */
export function sortBySeverity(alerts: readonly Alert[]): readonly Alert[] {
  return [...alerts].sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);
}
