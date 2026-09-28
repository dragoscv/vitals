/**
 * "Why is my PC slow?" — the pure half.
 *
 * The verdict itself is computed in Rust (`vitals_core::diagnosis`) from the
 * alert engine's active list joined against the process list, so the desktop,
 * the CLI and a future LAN route agree on who is responsible. What lives here
 * is the part that needs the user's language: turning a `Diagnosis` into a
 * headline sentence, picking which history series backs it, and flattening it
 * into the plain-text report the Copy button puts on the clipboard.
 *
 * Plain text, not JSON, because the report's destination is a chat window or
 * a support ticket read by a person. The machine-readable form is the LAN API.
 */

import type { TFunction } from 'i18next';

import type { Contributor, Diagnosis, Subsystem, SystemMetrics } from '@vitals/protocol';
import { formatBytes, formatPercent, formatThroughput } from '@vitals/ui';

import { alertCause, alertTitle } from '../alerts/alertText';
import type { MetricHistory } from '../dashboard/history';

/**
 * The series that shows the last minute of the subsystem the verdict names.
 *
 * `null` when there is no time series for it — a failing drive or a worn
 * battery is a state, not a load, and drawing an unrelated chart under the
 * headline would suggest a correlation that does not exist.
 */
export function seriesFor(
  subsystem: Subsystem,
  history: MetricHistory,
): MetricHistory['core'][keyof MetricHistory['core']] | null {
  switch (subsystem) {
    case 'cpu':
    case 'thermal':
      return history.core.cpu;
    case 'memory':
      return history.core.memoryPercent;
    case 'disk':
      return history.core.diskRead;
    case 'network':
      return history.core.netRx;
    case 'gpu':
    case 'storage':
    case 'battery':
      return null;
  }
}

/** Scale for the chart: percentages are 0-100, throughput autoscale. */
export function isPercentSubsystem(subsystem: Subsystem): boolean {
  return subsystem === 'cpu' || subsystem === 'thermal' || subsystem === 'memory';
}

/**
 * The raw reading behind a contributor's share, in the unit the subsystem
 * measures. Matches `Contributor::value` in Rust: hundredths of a percent for CPU and GPU, bytes for
 * memory, bytes per second for disk and network.
 */
export function formatContributorValue(
  subsystem: Subsystem,
  contributor: Contributor,
  locale: string,
): string {
  switch (subsystem) {
    case 'cpu':
    case 'thermal':
      // Hundredths of a percent on the wire (see `Contributor::value`).
      return formatPercent(contributor.value / 100, locale);
    case 'memory':
      return formatBytes(contributor.value, locale);
    case 'disk':
    case 'network':
      return formatThroughput(contributor.value, locale);
    case 'gpu':
      return formatPercent(contributor.value / 100, locale);
    case 'storage':
    case 'battery':
      return formatPercent(contributor.value, locale);
  }
}

export interface ReportContext {
  readonly t: TFunction;
  readonly locale: string;
  readonly system: SystemMetrics | null;
  readonly generatedAt: Date;
}

/**
 * The text behind the Copy button.
 *
 * Deterministic for a given diagnosis and clock, so the test can pin the
 * exact shape — a support engineer will paste this into a search box, and a
 * layout that drifts between versions makes those searches miss.
 */
export function formatReport(diagnosis: Diagnosis, ctx: ReportContext): string {
  const { t, locale } = ctx;
  const lines: string[] = [];

  lines.push(t('diagnosis.report.heading'));
  lines.push(
    t('diagnosis.report.generated', {
      date: ctx.generatedAt.toLocaleString(locale, { dateStyle: 'medium', timeStyle: 'short' }),
    }),
  );
  lines.push('');

  const verdict = diagnosis.verdict;
  if (verdict === null) {
    lines.push(t('diagnosis.healthy.title'));
    lines.push(t('diagnosis.healthy.body'));
  } else {
    lines.push(alertTitle(verdict.alert, t));
    lines.push(alertCause(verdict.alert, t));
    lines.push('');
    if (verdict.contributors.length > 0) {
      lines.push(t('diagnosis.report.responsible'));
      for (const c of verdict.contributors) {
        lines.push(
          `  ${c.name} (PID ${String(c.pid)}) — ${formatPercent(c.share, locale, 0)} · ${formatContributorValue(verdict.subsystem, c, locale)}`,
        );
      }
    } else if (verdict.diffuse) {
      lines.push(t('diagnosis.diffuse'));
    }
  }

  if (diagnosis.also.length > 0) {
    lines.push('');
    lines.push(t('diagnosis.report.also'));
    for (const alert of diagnosis.also) {
      lines.push(`  ${alertTitle(alert, t)} — ${alertCause(alert, t)}`);
    }
  }

  const system = ctx.system;
  if (system !== null) {
    lines.push('');
    lines.push(t('diagnosis.report.snapshot'));
    lines.push(`  CPU ${formatPercent(system.cpu.total, locale)}`);
    lines.push(
      `  ${t('diagnosis.report.memory')} ${formatBytes(system.memory.used, locale)} / ${formatBytes(system.memory.total, locale)}`,
    );
    lines.push(
      `  ${t('diagnosis.report.processes')} ${String(system.cpu.processCount)} · ${t('diagnosis.report.threads')} ${String(system.cpu.threadCount)}`,
    );
  }

  return lines.join('\n');
}
