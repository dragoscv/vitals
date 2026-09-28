/**
 * The one number a widget leads with, shown large in its header.
 *
 * A dashboard is read at a glance: the reader wants "CPU 12 %" before they
 * want the chart that explains it. Putting that number in the header, rather
 * than as the first of six equal-weight stats, is what lets the card shrink
 * to a quarter of the window and still answer the question.
 *
 * Only widgets with a single honest headline have one. A process list or a
 * set of drives has no one number that summarises it, and inventing one
 * (an average temperature, a total "storage %") would be a reading nobody
 * measured.
 */

import type { TFunction } from 'i18next';

import type { SystemMetrics } from '@vitals/protocol';
import { formatBytes, formatPercent, formatThroughput } from '@vitals/ui';

import type { WidgetId } from '../widgets';

export type HeadlineTone = 'default' | 'warn' | 'danger';

export interface Headline {
  readonly value: string;
  /** A short qualifier under the value — "4.1 GB of 16 GB". */
  readonly caption?: string;
  readonly tone: HeadlineTone;
}

/** Load-style readings turn amber at 75 % and red at 90 %, as the meters do. */
function loadTone(percent: number): HeadlineTone {
  if (percent >= 90) return 'danger';
  if (percent >= 75) return 'warn';
  return 'default';
}

export function headlineFor(
  id: WidgetId,
  system: SystemMetrics | null,
  locale: string,
  t: TFunction,
): Headline | null {
  if (system === null) return null;

  switch (id) {
    case 'cpu':
      return { value: formatPercent(system.cpu.total, locale), tone: loadTone(system.cpu.total) };
    case 'memory': {
      const { used, total } = system.memory;
      if (total <= 0) return null;
      const percent = (used / total) * 100;
      return {
        value: formatPercent(percent, locale, 0),
        caption: t('memory.ofTotal', {
          used: formatBytes(used, locale),
          total: formatBytes(total, locale),
        }),
        tone: loadTone(percent),
      };
    }
    case 'disk': {
      let sum = 0;
      for (const disk of system.disks) sum += disk.read + disk.write;
      return { value: formatThroughput(sum, locale), tone: 'default' };
    }
    case 'network': {
      let sum = 0;
      for (const nic of system.networks) sum += nic.rx + nic.tx;
      return { value: formatThroughput(sum, locale), tone: 'default' };
    }
    case 'gpu': {
      // The busiest adapter: on a laptop with two GPUs the idle integrated
      // one would otherwise report 0 % while the discrete one runs a game.
      let busiest: number | null = null;
      for (const gpu of system.gpus) {
        if (gpu.utilization !== null && (busiest === null || gpu.utilization > busiest)) {
          busiest = gpu.utilization;
        }
      }
      // Unmeasured is not zero: no headline rather than "0 %".
      return busiest === null
        ? null
        : { value: formatPercent(busiest, locale), tone: loadTone(busiest) };
    }
    case 'battery': {
      const battery = system.battery;
      if (battery === null) return null;
      return {
        value: formatPercent(battery.charge, locale, 0),
        tone: battery.charge < 10 && !battery.charging ? 'danger' : 'default',
      };
    }
    default:
      return null;
  }
}
