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
 * # Some devices are hidden by default, and the user can hide the rest
 *
 * A developer machine reports a dozen adapters — Hyper-V switches, WSL, npcap,
 * loopback, WAN miniports — and none of them answer "why is my internet slow".
 * Measured on a Hyper-V host: 34 interfaces, two of them network cards. The
 * same goes for a GPU with no counters at all (a virtual display, the Basic
 * Render Driver): a row that can never show a number.
 *
 * Those start hidden, anything else can be hidden from its context menu, and
 * a hidden default can be shown again the same way. Every entry is still
 * built, flagged `hidden`, rather than dropped: the rail needs the count for
 * its "show hidden" switch, and when the question IS about a VPN or a
 * container bridge, the hidden rows are the only interesting ones.
 */

import type { GpuMetrics, NetworkMetrics, SystemMetrics } from '@vitals/protocol';

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
  /**
   * Identity that survives a reboot, for remembering a hide.
   *
   * Not `id`: a GPU id is the low half of its LUID, which Windows reassigns
   * at boot, and an interface index is renumbered when adapters come and
   * go. A name or a drive letter is what the user recognises and what stays
   * put. `null` for singletons, which cannot be hidden.
   */
  readonly key: string | null;
  readonly kind: ResourceKind;
  /** Device name, or empty for singletons whose label comes from i18n. */
  readonly name: string;
  /** Secondary line in the rail — the mount point, link speed, vendor. */
  readonly detail: string | null;
  /**
   * Headline reading, 0-100, for the rail's inline meter.
   *
   * `null` when there is no honest percentage: a network adapter (link speed
   * is a nominal ceiling), or a GPU with no engine counters. The rail omits
   * the meter for these rather than drawing an empty one.
   */
  readonly utilization: number | null;
  /**
   * The device's own temperature, shown small beside the detail line so
   * a hot drive or GPU is visible without opening it. `null` when the
   * device does not report one; the thermals entry has its own headline.
   */
  readonly temperature: number | null;
  /** The device's own id within its kind, for looking the device back up. */
  readonly deviceId: number | null;
  /** Whether this device starts hidden, before the user says otherwise. */
  readonly hiddenByDefault: boolean;
  /** Whether it is hidden now, after the user's choice is applied. */
  readonly hidden: boolean;
}

/**
 * The user's choices, keyed by `ResourceEntry.key`.
 *
 * Both directions are stored, because a default can be overridden either way:
 * `hidden` for a device they do not care about, `shown` for one the defaults
 * got wrong. A key absent from the map follows the default.
 */
export type ResourceVisibility = Readonly<Record<string, 'hidden' | 'shown'>>;

export interface ResourceListOptions {
  readonly visibility?: ResourceVisibility;
}

/**
 * Adapters that are never the answer.
 *
 * Matched on the protocol's own `kind` rather than on the adapter name: names
 * are localised and vendor-specific, and a substring match on "Virtual" would
 * hide a physical adapter from a manufacturer that used the word.
 */
const BACKGROUND_ADAPTERS: ReadonlySet<string> = new Set(['loopback', 'virtual']);

/** Kinds that are always a physical radio or port the user can point at. */
const PHYSICAL_ADAPTERS: ReadonlySet<string> = new Set(['ethernet', 'wiFi']);

/**
 * Whether an adapter starts hidden.
 *
 * A VPN or tunnel is software, but it is frequently the actual cause of slow
 * traffic, so it is not hidden for what it is — only for being unused: down,
 * or never having carried a byte. That is what separates Tailscale from the
 * six WAN miniports and the Teredo pseudo-interface Windows installs on every
 * machine. A physical adapter is never hidden by default, even idle: an
 * unused Wi-Fi card is still a card the user owns.
 */
export function isNetworkHiddenByDefault(nic: NetworkMetrics): boolean {
  if (BACKGROUND_ADAPTERS.has(nic.kind)) return true;
  if (PHYSICAL_ADAPTERS.has(nic.kind)) return false;
  return !nic.connected || nic.rxTotal + nic.txTotal === 0;
}

/**
 * Whether a GPU starts hidden: it exposes no counters of any kind.
 *
 * An indirect display (Parsec, Miracast) and the Basic Render Driver are real
 * adapters with nothing to report, so their panel is a page of em dashes.
 * Decided by readings rather than by name, for the same reason as adapters.
 */
export function isGpuHiddenByDefault(gpu: GpuMetrics): boolean {
  return gpu.utilization === null && gpu.engines.length === 0 && gpu.memoryUsed === null;
}

function applyVisibility(
  key: string,
  hiddenByDefault: boolean,
  visibility: ResourceVisibility,
): boolean {
  const choice = visibility[key];
  if (choice === 'hidden') return true;
  if (choice === 'shown') return false;
  return hiddenByDefault;
}

export function buildResourceList(
  system: SystemMetrics | null,
  options: ResourceListOptions = {},
): readonly ResourceEntry[] {
  if (system === null) return [];
  const visibility = options.visibility ?? {};

  const entries: ResourceEntry[] = [
    {
      id: 'cpu',
      key: null,
      kind: 'cpu',
      name: '',
      detail: null,
      utilization: system.cpu.total,
      temperature: system.cpu.temperature,
      deviceId: null,
      hiddenByDefault: false,
      hidden: false,
    },
    {
      id: 'memory',
      key: null,
      kind: 'memory',
      name: '',
      detail: null,
      utilization: system.memory.total > 0 ? (system.memory.used / system.memory.total) * 100 : 0,
      temperature: null,
      deviceId: null,
      hiddenByDefault: false,
      hidden: false,
    },
  ];

  for (const gpu of system.gpus) {
    const key = `gpu:${gpu.name}`;
    const hiddenByDefault = isGpuHiddenByDefault(gpu);
    entries.push({
      id: `gpu:${gpu.id}`,
      key,
      kind: 'gpu',
      name: gpu.name,
      detail: gpu.driverVersion,
      utilization: gpu.utilization,
      temperature: gpu.temperature,
      deviceId: gpu.id,
      hiddenByDefault,
      hidden: applyVisibility(key, hiddenByDefault, visibility),
    });
  }

  for (const disk of system.disks) {
    const key = `disk:${disk.mount ?? disk.name}`;
    entries.push({
      id: `disk:${disk.id}`,
      key,
      kind: 'disk',
      name: disk.mount ?? disk.name,
      detail: disk.model ?? disk.name,
      // Active time, not throughput: a disk saturated by tiny random reads
      // shows almost no bytes per second while being completely unusable, and
      // a rail scaled on throughput would rank it as idle.
      utilization: disk.activeTime,
      temperature: disk.temperature,
      deviceId: disk.id,
      hiddenByDefault: false,
      hidden: applyVisibility(key, false, visibility),
    });
  }

  for (const nic of system.networks) {
    const key = `network:${nic.name}`;
    const hiddenByDefault = isNetworkHiddenByDefault(nic);
    entries.push({
      id: `network:${nic.id}`,
      key,
      kind: 'network',
      name: nic.name,
      detail: nic.adapter,
      // Networks have no meaningful percentage: link speed is a nominal
      // ceiling that Wi-Fi never reaches and that is null on many adapters.
      // The rail shows throughput as text instead.
      utilization: null,
      temperature: null,
      deviceId: nic.id,
      hiddenByDefault,
      hidden: applyVisibility(key, hiddenByDefault, visibility),
    });
  }

  if (hasThermalReadings(system)) {
    entries.push({
      id: 'thermals',
      key: null,
      kind: 'thermals',
      name: '',
      detail: null,
      utilization: hottest(system),
      temperature: null,
      deviceId: null,
      hiddenByDefault: false,
      hidden: false,
    });
  }

  return entries;
}

/** The rail's contents: everything, or only what is not hidden. */
export function visibleResources(
  entries: readonly ResourceEntry[],
  showHidden: boolean,
): readonly ResourceEntry[] {
  return showHidden ? entries : entries.filter((entry) => !entry.hidden);
}

/**
 * The visibility map after the user hides or shows one device.
 *
 * A choice that matches the default is removed rather than stored, so the
 * map holds only real overrides and a later change to a default — a better
 * rule for spotting phantom adapters — still reaches devices the user never
 * touched.
 */
export function setResourceHidden(
  visibility: ResourceVisibility,
  entry: ResourceEntry,
  hidden: boolean,
): ResourceVisibility {
  if (entry.key === null) return visibility;
  const { [entry.key]: _previous, ...rest } = visibility;
  if (hidden === entry.hiddenByDefault) return rest;
  return { ...rest, [entry.key]: hidden ? 'hidden' : 'shown' };
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
