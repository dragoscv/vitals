/**
 * Selectors over a `SystemMetrics`.
 *
 * These exist because the same question — "what is the GPU doing?" — was
 * answered three different ways in three surfaces, and two of them were
 * wrong. A machine here reports three adapters: a Parsec virtual display, a
 * render-only device, and the real GPU. Taking `gpus[0]` shows a dash while
 * the card sits at 17 %.
 *
 * Every selector returns `null` for "not measured" and never invents a zero.
 */

import type { GpuMetrics, NetworkMetrics, SystemMetrics } from './generated';

/**
 * The adapter a person would point at, or `null` when the machine has none.
 *
 * "Busiest measured" rather than "first": the first adapter the kernel
 * enumerates is routinely a virtual display. An adapter whose `utilization`
 * is `null` has no engine counters at all — unknown, not idle — so it can
 * never win, but it is still returned when it is the only thing present, so
 * a caller can name the hardware even without a reading for it.
 */
export function primaryGpu(system: SystemMetrics): GpuMetrics | null {
  let best: GpuMetrics | null = null;
  for (const gpu of system.gpus) {
    if (best === null) {
      best = gpu;
      continue;
    }
    if (gpu.utilization === null) continue;
    if (best.utilization === null || gpu.utilization > best.utilization) {
      best = gpu;
    }
  }
  return best;
}

/**
 * The busiest adapter's utilisation, or `null` when nothing reports one.
 *
 * Deliberately not `primaryGpu(...)?.utilization ?? null`: on a machine whose
 * only adapter is unmeasured, that is the same answer, but this states the
 * intent — a percentage, or nothing.
 */
export function gpuPercent(system: SystemMetrics): number | null {
  let best: number | null = null;
  for (const gpu of system.gpus) {
    if (gpu.utilization === null) continue;
    if (best === null || gpu.utilization > best) best = gpu.utilization;
  }
  return best;
}

/** Memory in use as a percentage, or `null` when the total is not known. */
export function memoryPercent(system: SystemMetrics): number | null {
  const { used, total } = system.memory;
  return total > 0 ? (used / total) * 100 : null;
}

/**
 * The adapter carrying the most traffic right now, or `undefined` when there
 * are none.
 *
 * Total throughput rather than link speed: a 10 Gb adapter doing nothing is
 * not the one the user wants to see.
 */
export function busiestNic(system: SystemMetrics): NetworkMetrics | undefined {
  let best: NetworkMetrics | undefined;
  for (const nic of system.networks) {
    if (best === undefined || nic.rx + nic.tx > best.rx + best.tx) best = nic;
  }
  return best;
}

/**
 * The temperature most worth showing: the CPU package, falling back to the
 * primary GPU. `null` when neither is readable, which is the common case on
 * a desktop without a sensor driver.
 */
export function headlineTemperature(system: SystemMetrics): number | null {
  return system.cpu.temperature ?? primaryGpu(system)?.temperature ?? null;
}
