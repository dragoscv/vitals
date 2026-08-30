/**
 * The list of things the Performance page can show, derived from a frame.
 *
 * # Why the list is computed, not fixed
 *
 * Task Manager's Performance page has a fixed left rail: CPU, Memory, then a
 * disk per drive, a NIC per adapter, a GPU per adapter. That shape is right,
 * but it cannot be hardcoded here because the set changes while the app is
 * open — plugging in a USB drive, connecting a dock, joining a VPN all add an
 * entry, and unplugging removes one.
 *
 * So the rail is rebuilt from each frame, and the selection is held by a
 * stable id rather than an index. An index-based selection would silently
 * move the user to a different device the moment one above it disappeared,
 * which on a page with per-device actions is worse than losing the selection.
 *
 * # Loopback and virtual adapters are excluded by default
 *
 * A developer machine reports a dozen adapters — Hyper-V switches, WSL, npcap,
 * loopback — and none of them answer "why is my internet slow". They are
 * available behind a toggle rather than deleted, because when the question IS
 * about a VPN or a container bridge, they are the only interesting rows.
 */

import type { SystemMetrics } from '@vitals/protocol';

export const resourceKinds = ['cpu', 'memory', 'gpu', 'disk', 'network', 'thermals'] as const;
export type ResourceKind = (typeof resourceKinds)[number];

export interface ResourceEntry {
  /**
   * Stable across frames.
   *
   * `cpu` and `memory` are singletons; devices are namespaced by kind so a
   * disk id and a NIC id of 0 cannot collide.
   */
  readonly id: string;
  readonly kind: ResourceKind;
  /** Device name, or empty for singletons whose label comes from i18n. */
  readonly name: string;
  /** Secondary line in the rail — the mount point, link speed, vendor. */
  readonly detail: string | null;
  /** Headline reading, 0-100, for the rail's inline meter. */
  readonly utilization: number;
  /** The device's own id within its kind, for looking the device back up. */
  readonly deviceId: number | null;
}

export interface ResourceListOptions {
  /** Include loopback, virtual and VPN adapters. Off by default. */
  readonly showVirtualAdapters?: boolean;
}

/**
 * Adapters that are almost never the answer.
 *
 * Matched on the protocol's own `kind` rather than on the adapter name: names
 * are localised and vendor-specific, and a substring match on "Virtual" would
 * hide a physical adapter from a manufacturer that used the word.
 */
const BACKGROUND_ADAPTERS: ReadonlySet<string> = new Set(['loopback', 'virtual']);

export function buildResourceList(
  system: SystemMetrics | null,
  options: ResourceListOptions = {},
): readonly ResourceEntry[] {
  if (system === null) return [];

  const entries: ResourceEntry[] = [
    {
      id: 'cpu',
      kind: 'cpu',
      name: '',
      detail: null,
      utilization: system.cpu.total,
      deviceId: null,
    },
    {
      id: 'memory',
      kind: 'memory',
      name: '',
      detail: null,
      utilization: system.memory.total > 0 ? (system.memory.used / system.memory.total) * 100 : 0,
      deviceId: null,
    },
  ];

  for (const gpu of system.gpus) {
    entries.push({
      id: `gpu:${gpu.id}`,
      kind: 'gpu',
      name: gpu.name,
      detail: gpu.driverVersion,
      utilization: gpu.utilization,
      deviceId: gpu.id,
    });
  }

  for (const disk of system.disks) {
    entries.push({
      id: `disk:${disk.id}`,
      kind: 'disk',
      name: disk.mount ?? disk.name,
      detail: disk.model ?? disk.name,
      // Active time, not throughput: a disk saturated by tiny random reads
      // shows almost no bytes per second while being completely unusable, and
      // a rail scaled on throughput would rank it as idle.
      utilization: disk.activeTime,
      deviceId: disk.id,
    });
  }

  for (const nic of system.networks) {
    if (!options.showVirtualAdapters && BACKGROUND_ADAPTERS.has(nic.kind)) continue;
    entries.push({
      id: `network:${nic.id}`,
      kind: 'network',
      name: nic.name,
      detail: nic.adapter,
      // Networks have no meaningful percentage: link speed is a nominal
      // ceiling that Wi-Fi never reaches and that is null on many adapters.
      // Zero here keeps the meter honest rather than inventing a scale, and
      // the rail shows throughput as text instead.
      utilization: 0,
      deviceId: nic.id,
    });
  }

  if (hasThermalReadings(system)) {
    entries.push({
      id: 'thermals',
      kind: 'thermals',
      name: '',
      detail: null,
      utilization: hottest(system) ?? 0,
      deviceId: null,
    });
  }

  return entries;
}

/**
 * Whether anything on this machine reports a temperature.
 *
 * Decided by an actual reading, not by the field existing: the Rust side
 * returns `null` for every temperature it cannot read without a ring-0
 * driver, which is most desktops. A permanently empty Thermals tab is worse
 * than no tab, because the user concludes their sensors are broken.
 */
export function hasThermalReadings(system: SystemMetrics | null): boolean {
  if (system === null) return false;
  if (system.cpu.temperature !== null) return true;
  if (system.gpus.some((gpu) => gpu.temperature !== null)) return true;
  if (system.disks.some((disk) => disk.temperature !== null)) return true;
  return system.battery?.temperature != null;
}

/** The highest temperature anything reports, for the rail's summary. */
export function hottest(system: SystemMetrics | null): number | null {
  if (system === null) return null;

  let max: number | null = null;
  const consider = (value: number | null | undefined): void => {
    if (value === null || value === undefined) return;
    if (max === null || value > max) max = value;
  };

  consider(system.cpu.temperature);
  for (const gpu of system.gpus) {
    consider(gpu.temperature);
    consider(gpu.hotspotTemperature);
  }
  for (const disk of system.disks) consider(disk.temperature);
  consider(system.battery?.temperature);

  return max;
}

/**
 * Resolves a stored selection against the current frame.
 *
 * Falls back to CPU rather than to "nothing selected": an empty detail pane
 * after unplugging a drive looks like the page broke. CPU is always present
 * and is what the page opens on anyway.
 */
export function resolveSelection(
  entries: readonly ResourceEntry[],
  wanted: string | null,
): ResourceEntry | null {
  if (entries.length === 0) return null;
  const match = entries.find((entry) => entry.id === wanted);
  return match ?? entries[0] ?? null;
}
