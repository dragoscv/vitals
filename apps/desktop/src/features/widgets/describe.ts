/**
 * What a widget is showing, as data.
 *
 * One function per kind turns a metrics frame into a list of readings, and the
 * renderer draws whatever it is handed. That split is what lets two kinds share
 * three layouts without any of the six combinations being its own component:
 * the CPU widget and the memory widget differ only in the numbers they
 * produce, and a new layout is written once rather than once per kind.
 *
 * Every value is a formatted string, and `null` from the sampler is rendered
 * as an em dash by the formatters rather than as zero — an unmeasured reading
 * and a genuine zero are different facts, and this screen is the one most
 * likely to be glanced at without context.
 */

import {
  busiestNic,
  gpuPercent,
  headlineTemperature,
  memoryPercent,
  primaryGpu,
  type SystemMetrics,
} from '@vitals/protocol';
import {
  formatBytes,
  formatFrequency,
  formatPercent,
  formatTemperature,
  formatThroughput,
} from '@vitals/ui';

import type { WidgetKind } from './model';

/** Which accent a reading is drawn in. Mirrors the `--color-chart-*` tokens. */
export type WidgetTone = 'cpu' | 'memory' | 'gpu' | 'disk' | 'network' | 'thermal' | 'neutral';

/** One number on a widget, with everything a layout needs to draw it. */
export interface WidgetReading {
  /** Stable within a description; used as a React key and for tests. */
  readonly id: string;
  /** An i18n key, resolved by the renderer. */
  readonly labelKey: string;
  /** Already formatted, in the active locale. An em dash when unmeasured. */
  readonly value: string;
  /** 0–100 for a bar, ring or gauge. `null` when the reading has no scale. */
  readonly percent: number | null;
  /** An optional second line — a unit detail rather than another reading. */
  readonly detail: string | null;
  /** Recent history for a sparkline; empty when the reading has no trace. */
  readonly trace: readonly number[];
  readonly tone: WidgetTone;
}

/** The live data a widget renders from. */
export interface WidgetInput {
  readonly system: SystemMetrics | null;
  readonly cpuHistory: readonly number[];
  readonly memoryHistory: readonly number[];
  readonly gpuHistory: readonly number[];
  /**
   * Total received throughput, bytes per second — the series the download
   * reading traces. Not rx plus tx: a sparkline that added the upload would
   * disagree with the number printed above it.
   */
  readonly netHistory: readonly number[];
  /** Total read throughput, bytes per second. See {@link WidgetInput.netHistory}. */
  readonly diskHistory: readonly number[];
}

export const EMPTY_WIDGET_INPUT: WidgetInput = {
  system: null,
  cpuHistory: [],
  memoryHistory: [],
  gpuHistory: [],
  netHistory: [],
  diskHistory: [],
};

export interface WidgetDescription {
  /** An i18n key for the panel heading. `null` for layouts with no heading. */
  readonly titleKey: string | null;
  /** The readings, most important first. */
  readonly readings: readonly WidgetReading[];
}

function reading(partial: WidgetReading): WidgetReading {
  return partial;
}

function cpuDescription(input: WidgetInput, locale: string): WidgetDescription {
  const cpu = input.system?.cpu ?? null;
  const total = cpu?.total ?? null;
  return {
    titleKey: 'metric.cpu',
    readings: [
      reading({
        id: 'cpu',
        labelKey: 'metric.cpu',
        value: formatPercent(total, locale, 0),
        percent: total,
        detail: cpu?.effectiveClock == null ? null : formatFrequency(cpu.effectiveClock, locale),
        trace: input.cpuHistory,
        tone: 'cpu',
      }),
      reading({
        id: 'cpu-temp',
        labelKey: 'widgets.reading.temperature',
        value: formatTemperature(cpu?.temperature ?? null, locale),
        percent: null,
        detail: null,
        trace: [],
        tone: 'thermal',
      }),
    ],
  };
}

function memoryDescription(input: WidgetInput, locale: string): WidgetDescription {
  const memory = input.system?.memory ?? null;
  const percent = input.system === null ? null : memoryPercent(input.system);
  return {
    titleKey: 'metric.memory',
    readings: [
      reading({
        id: 'memory',
        labelKey: 'metric.memory',
        value: formatPercent(percent, locale, 0),
        percent,
        detail:
          memory === null
            ? null
            : `${formatBytes(memory.used, locale)} / ${formatBytes(memory.total, locale)}`,
        trace: input.memoryHistory,
        tone: 'memory',
      }),
    ],
  };
}

/**
 * Bytes per second read, across every disk.
 *
 * Exported because the overlay keeps its own history (it is a separate webview
 * and cannot see the main window's collector), and a trace built from a
 * different rule than the number printed above it would draw a line that never
 * matches its own label the first time a machine had two disks in use.
 *
 * Read, not read plus written: the trace sits under the read reading, and a
 * series that silently added the writes would peak at a different moment.
 */
export function totalDiskRead(system: SystemMetrics | null): number {
  return (system?.disks ?? []).reduce((sum, disk) => sum + disk.read, 0);
}

/** Bytes per second received, across every adapter. See {@link totalDiskRead}. */
export function totalNetworkRx(system: SystemMetrics | null): number {
  return (system?.networks ?? []).reduce((sum, entry) => sum + entry.rx, 0);
}

function diskDescription(input: WidgetInput, locale: string): WidgetDescription {
  const disks = input.system?.disks ?? [];
  const busiest = busiestDisk(disks);
  const read = totalDiskRead(input.system);
  const write = disks.reduce((sum, disk) => sum + disk.write, 0);
  return {
    titleKey: 'metric.disk',
    readings: [
      reading({
        id: 'disk-activity',
        labelKey: 'widgets.reading.activity',
        value: formatPercent(busiest?.activeTime ?? null, locale, 0),
        percent: busiest?.activeTime ?? null,
        detail: busiest?.responseMs == null ? null : `${busiest.responseMs.toFixed(1)} ms`,
        trace: [],
        tone: 'disk',
      }),
      reading({
        id: 'disk-read',
        labelKey: 'widgets.reading.read',
        value: formatThroughput(read, locale),
        percent: null,
        detail: null,
        trace: input.diskHistory,
        tone: 'disk',
      }),
      reading({
        id: 'disk-write',
        labelKey: 'widgets.reading.write',
        value: formatThroughput(write, locale),
        percent: null,
        detail: null,
        trace: [],
        tone: 'disk',
      }),
    ],
  };
}

function networkDescription(input: WidgetInput, locale: string): WidgetDescription {
  const system = input.system;
  const nic = system === null ? undefined : busiestNic(system);
  const networks = system?.networks ?? [];
  const rx = totalNetworkRx(system);
  const tx = networks.reduce((sum, entry) => sum + entry.tx, 0);
  return {
    titleKey: 'metric.network',
    readings: [
      reading({
        id: 'net-down',
        labelKey: 'widgets.reading.download',
        value: formatThroughput(rx, locale),
        percent: null,
        detail: nic?.ssid ?? nic?.name ?? null,
        trace: input.netHistory,
        tone: 'network',
      }),
      reading({
        id: 'net-up',
        labelKey: 'widgets.reading.upload',
        value: formatThroughput(tx, locale),
        percent: null,
        detail: nic?.linkSpeed == null ? null : `${nic.linkSpeed} Mbps`,
        trace: [],
        tone: 'network',
      }),
    ],
  };
}

function thermalDescription(input: WidgetInput, locale: string): WidgetDescription {
  const system = input.system;
  const cpuTemp = system?.cpu.temperature ?? null;
  const gpuTemp = system === null ? null : (primaryGpu(system)?.temperature ?? null);
  const headline = system === null ? null : headlineTemperature(system);
  const hottestDisk = (system?.disks ?? []).reduce<number | null>(
    (best, disk) =>
      disk.temperature === null
        ? best
        : best === null
          ? disk.temperature
          : Math.max(best, disk.temperature),
    null,
  );
  return {
    titleKey: 'metric.thermals',
    readings: [
      reading({
        id: 'thermal-headline',
        labelKey: 'widgets.reading.hottest',
        value: formatTemperature(headline, locale),
        percent: null,
        detail: null,
        trace: [],
        tone: 'thermal',
      }),
      reading({
        id: 'thermal-cpu',
        labelKey: 'widgets.reading.cpuTemperature',
        value: formatTemperature(cpuTemp, locale),
        percent: null,
        detail: null,
        trace: [],
        tone: 'cpu',
      }),
      reading({
        id: 'thermal-gpu',
        labelKey: 'widgets.reading.gpuTemperature',
        value: formatTemperature(gpuTemp, locale),
        percent: null,
        detail: null,
        trace: [],
        tone: 'gpu',
      }),
      reading({
        id: 'thermal-disk',
        labelKey: 'widgets.reading.diskTemperature',
        value: formatTemperature(hottestDisk, locale),
        percent: null,
        detail: null,
        trace: [],
        tone: 'disk',
      }),
    ],
  };
}

function overviewDescription(input: WidgetInput, locale: string): WidgetDescription {
  const system = input.system;
  const cpu = system?.cpu.total ?? null;
  const memory = system === null ? null : memoryPercent(system);
  const gpu = system === null ? null : gpuPercent(system);
  const networks = system?.networks ?? [];
  const net = networks.reduce((sum, entry) => sum + entry.rx + entry.tx, 0);
  return {
    titleKey: 'widgets.kind.overview',
    readings: [
      reading({
        id: 'overview-cpu',
        labelKey: 'metric.cpu',
        value: formatPercent(cpu, locale, 0),
        percent: cpu,
        detail: null,
        trace: input.cpuHistory,
        tone: 'cpu',
      }),
      reading({
        id: 'overview-memory',
        labelKey: 'metric.memory',
        value: formatPercent(memory, locale, 0),
        percent: memory,
        detail: null,
        trace: input.memoryHistory,
        tone: 'memory',
      }),
      reading({
        id: 'overview-gpu',
        labelKey: 'metric.gpu',
        value: formatPercent(gpu, locale, 0),
        percent: gpu,
        detail: null,
        trace: input.gpuHistory,
        tone: 'gpu',
      }),
      reading({
        id: 'overview-network',
        labelKey: 'metric.network',
        value: formatThroughput(net, locale),
        percent: null,
        detail: null,
        trace: input.netHistory,
        tone: 'network',
      }),
    ],
  };
}

/** The busiest disk by combined throughput, or `undefined` when there are none. */
function busiestDisk(disks: SystemMetrics['disks']): SystemMetrics['disks'][number] | undefined {
  let best: SystemMetrics['disks'][number] | undefined;
  for (const disk of disks) {
    if (best === undefined || disk.read + disk.write > best.read + best.write) best = disk;
  }
  return best;
}

/**
 * Turns a frame into the readings for one kind.
 *
 * Total by construction: every kind has a description, so a kind added to
 * `widgetKinds` without a case here is a compile error at the `never`, not a
 * blank panel someone discovers in the wild.
 */
export function describe(kind: WidgetKind, input: WidgetInput, locale: string): WidgetDescription {
  switch (kind) {
    case 'cpu':
      return cpuDescription(input, locale);
    case 'memory':
      return memoryDescription(input, locale);
    case 'disk':
      return diskDescription(input, locale);
    case 'network':
      return networkDescription(input, locale);
    case 'thermal':
      return thermalDescription(input, locale);
    case 'overview':
      return overviewDescription(input, locale);
    default: {
      const exhaustive: never = kind;
      return exhaustive;
    }
  }
}

/** The reading a ring or gauge layout draws: the first one, or nothing. */
export function leadReading(description: WidgetDescription): WidgetReading | null {
  return description.readings[0] ?? null;
}
