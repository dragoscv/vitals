/**
 * The detail panel for the focused row.
 *
 * Reads from the live snapshot rather than from a copy taken at selection
 * time, so the numbers here and the numbers in the table can never disagree —
 * two different readings of the same process on one screen is the kind of
 * detail that makes people stop trusting a monitor.
 */

import { useTranslation } from 'react-i18next';

import {
  Badge,
  EmptyState,
  formatBytes,
  formatCount,
  formatPercent,
  formatThroughput,
  formatUptime,
} from '@vitals/ui';

import { UNKNOWN } from './constants';
import { flagKeys, type ProcessRow } from './model';
import { fallback } from './strings';

export interface ProcessDetailsProps {
  readonly row: ProcessRow | null;
  readonly locale: string;
}

export function ProcessDetails({ row, locale }: ProcessDetailsProps): React.JSX.Element {
  const { t } = useTranslation();

  if (row === null) {
    return (
      <aside
        className="w-72 shrink-0 border-l border-[var(--color-border-subtle)] p-4"
        aria-label={t('process.detail.title', fallback('process.detail.title'))}
      >
        <EmptyState
          title={t('process.detail.title', fallback('process.detail.title'))}
          description={t('process.detail.none', fallback('process.detail.none'))}
        />
      </aside>
    );
  }

  const p = row.process;
  const flags = flagKeys(p.flags);

  return (
    <aside
      className="w-72 shrink-0 space-y-4 overflow-auto border-l border-[var(--color-border-subtle)] p-4"
      aria-label={t('process.detail.title', fallback('process.detail.title'))}
    >
      <header className="space-y-1">
        <h2 className="truncate text-sm font-semibold" title={p.name}>
          {p.name}
        </h2>
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {t('process.pid')} {p.key.pid} ·{' '}
          {t(`process.kind.${p.kind}`, fallback(`process.kind.${p.kind}` as never))}
        </p>
      </header>

      <Section title={t('process.detail.identity', fallback('process.detail.identity'))}>
        <Field label={t('process.user')} value={p.user ?? UNKNOWN} />
        <Field label={t('process.status')} value={t(`process.state.${p.state}`)} />
        <Field label={t('process.uptime')} value={formatUptime(p.uptimeSecs)} />
        <Field
          label={t('metric.unknown')}
          value={p.integrity === null ? UNKNOWN : p.integrity}
          hidden={p.integrity === null}
        />
      </Section>

      <Section title={t('process.detail.resources', fallback('process.detail.resources'))}>
        <Field
          label={t('process.column.cpu', fallback('process.column.cpu'))}
          value={formatPercent(p.cpu, locale)}
        />
        <Field
          label={t('process.column.memory', fallback('process.column.memory'))}
          value={formatBytes(p.memoryPrivate, locale)}
        />
        <Field label="Working set" value={formatBytes(p.memoryWorkingSet, locale)} />
        <Field
          label={t('process.column.disk', fallback('process.column.disk'))}
          value={formatThroughput(p.diskRead + p.diskWrite, locale)}
        />
        {/* Same as GPU below: unmeasured, not zero. Per-process network needs
            an ETW session the unelevated app cannot start. */}
        <Field
          label={t('process.column.network', fallback('process.column.network'))}
          value={
            p.netRx === null || p.netTx === null
              ? UNKNOWN
              : formatThroughput(p.netRx + p.netTx, locale)
          }
        />
        {/* Absent GPU telemetry renders as an em-dash. Showing 0% would be a
            measurement we did not take. */}
        <Field
          label={t('process.column.gpu', fallback('process.column.gpu'))}
          value={p.gpu === null ? UNKNOWN : formatPercent(p.gpu, locale)}
        />
        <Field label={t('process.threads')} value={formatCount(p.threadCount, locale)} />
        <Field
          label={t('process.handles')}
          value={p.handleCount === null ? UNKNOWN : formatCount(p.handleCount, locale)}
        />
      </Section>

      {flags.length > 0 && (
        <Section title={t('process.detail.flags', fallback('process.detail.flags'))}>
          <div className="flex flex-wrap gap-1">
            {flags.map((flag) => (
              <Badge
                key={flag}
                tone={flag === 'critical' || flag === 'signatureBroken' ? 'danger' : 'neutral'}
              >
                {t(`process.flag.${flag}`, fallback(`process.flag.${flag}` as never))}
              </Badge>
            ))}
          </div>
        </Section>
      )}
    </aside>
  );
}

function Section({
  title,
  children,
}: {
  readonly title: string;
  readonly children: React.ReactNode;
}): React.JSX.Element {
  return (
    <section className="space-y-1.5">
      <h3 className="text-2xs font-medium uppercase tracking-wide text-[var(--color-fg-subtle)]">
        {title}
      </h3>
      <dl className="space-y-1">{children}</dl>
    </section>
  );
}

function Field({
  label,
  value,
  hidden = false,
}: {
  readonly label: string;
  readonly value: string;
  readonly hidden?: boolean;
}): React.JSX.Element | null {
  if (hidden) return null;
  return (
    <div className="text-2xs flex items-baseline justify-between gap-2">
      <dt className="text-[var(--color-fg-muted)]">{label}</dt>
      <dd className="font-mono tabular-nums text-[var(--color-fg-default)]">{value}</dd>
    </div>
  );
}
