/**
 * Machine-wide figures for the column headers, as Task Manager shows them.
 *
 * Each is a share of the whole machine, so a glance at the header says which
 * resource is under pressure before any row is read — "Disk 100 %" is the
 * answer to "why is it slow" more often than any single process is.
 *
 * Every figure is `null` when the machine cannot report it, never 0: a GPU
 * with no counter is unknown, not idle (the one principle in AGENTS.md).
 */

import type { SystemMetrics } from '@vitals/protocol';

import type { ColumnId } from './columns';

export type ColumnTotals = Readonly<Partial<Record<ColumnId, number | null>>>;

export function columnTotals(system: SystemMetrics | null): ColumnTotals {
  if (system === null) return {};
  return {
    cpu: finite(system.cpu.total),
    memory: system.memory.total > 0 ? (system.memory.used / system.memory.total) * 100 : null,
    disk: highest(system.disks.map((d) => d.activeTime)),
    network: highest(
      system.networks
        .filter((n) => n.connected && n.linkSpeed !== null && n.linkSpeed > 0)
        // Bytes per second against a link speed in bits per second.
        .map((n) => (((n.rx + n.tx) * 8) / (n.linkSpeed ?? 1)) * 100),
    ),
    gpu: highest(system.gpus.map((g) => g.utilization)),
  };
}

/**
 * The busiest device, not the average.
 *
 * Task Manager's Disk and GPU headers do the same: an idle second disk must
 * not halve the figure for the one that is pegged, which is the disk making
 * the machine slow.
 */
function highest(values: readonly (number | null)[]): number | null {
  let max: number | null = null;
  for (const value of values) {
    const v = finite(value);
    if (v !== null && (max === null || v > max)) max = v;
  }
  return max === null ? null : Math.min(100, max);
}

function finite(value: number | null): number | null {
  return value !== null && Number.isFinite(value) ? Math.max(0, value) : null;
}
