/**
 * Resources: what agentq and each AI client are using on this machine — cpu,
 * memory, disk, network and gpu — folded from the live process table.
 *
 * # Why the nulls are shown as an em dash
 *
 * A measurement that does not exist is not zero. Windows reports no
 * per-process network counters without an ETW kernel session, so on this
 * machine every network figure is absent, and so is the gpu figure on a
 * machine whose driver reports nothing. `formatThroughput(null)` renders an em
 * dash; printing `0 B/s` instead would state, in the same typeface as the real
 * numbers, that the agent moved no bytes — a different and false claim. The
 * footnote under the list says which figures are unavailable and why.
 *
 * One row per tool, and a tool that owns no process gets no row: "not running"
 * and "running, using nothing" are different facts.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { useTranslation } from 'react-i18next';

import type { AgentqWorkKind, AgentqWorkUsage, AgentUsageSnapshot } from '@vitals/protocol';
import { EmptyState, cn, formatBytes, formatPercent, formatThroughput } from '@vitals/ui';

import { repoName, since, usageRows, usageWork, workDetail, type UsageRow } from './model';
import { UsageMark } from './parts';
import { AGENTS_NS } from './strings';

export function ResourcesPanel({
  usage,
  now,
}: {
  readonly usage: AgentUsageSnapshot | null;
  readonly now: number;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const rows = usageRows(usage);
  const work = usageWork(usage);
  if (rows.length === 0) {
    return <EmptyState title={t('usage.empty.title')} description={t('usage.empty.body')} />;
  }
  return (
    <section aria-labelledby="resources-heading" className="flex flex-col gap-2">
      <h3 id="resources-heading" className="flex items-baseline gap-2 text-sm font-semibold">
        {t('usage.title')}
        {usage !== null && (
          <span className="text-2xs font-normal text-[var(--color-fg-muted)]">
            {t('usage.taken', { ago: since(usage.takenAtMs, now) })}
          </span>
        )}
      </h3>
      <ul className="grid [grid-template-columns:repeat(auto-fill,minmax(21rem,1fr))] gap-2">
        <AnimatePresence initial={false}>
          {rows.map((row) => (
            <UsageCard key={row.key} row={row} />
          ))}
        </AnimatePresence>
      </ul>
      {work.length > 0 && (
        <div className="mt-1 flex flex-col gap-1.5">
          <h4 className="text-xs font-semibold">{t('usage.work.title')}</h4>
          <ul className="flex flex-col gap-1.5">
            <AnimatePresence initial={false}>
              {work.map((item) => (
                <WorkRow key={item.pid} work={item} now={now} />
              ))}
            </AnimatePresence>
          </ul>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('usage.work.footnote')}</p>
        </div>
      )}
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('usage.footnote')}</p>
    </section>
  );
}

/**
 * One thing agentq is running, and what it costs.
 *
 * The row is keyed by the pid, not the label: two slots can hold the same
 * resource name, and a label that repeats is exactly what React needs to be
 * told apart from its sibling.
 */
function WorkRow({ work, now }: { readonly work: AgentqWorkUsage; readonly now: number }) {
  const { t } = useTranslation(AGENTS_NS);
  const u = work.usage;
  const detail = workDetail(work);
  const repo = repoName(work.repo);
  return (
    <m.li
      layout
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.98 }}
      transition={{ duration: 0.2 }}
      className="rounded-lg border border-[var(--color-border-subtle)] bg-[var(--surface-card)] px-3 py-2"
    >
      <div className="flex items-center gap-2">
        <span
          className={cn(
            'inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-2xs font-medium',
            'bg-[var(--color-bg-subtle)] text-[var(--color-fg-muted)]',
          )}
        >
          <span aria-hidden className={cn('size-1.5 rounded-full', workKindTone(work.kind))} />
          {t(`usage.workKind.${work.kind}`)}
        </span>
        <span className="truncate text-xs font-medium">{work.label}</span>
        <span className="ml-auto shrink-0 text-2xs text-[var(--color-fg-muted)]">
          {t('usage.processes', { count: u.processes })}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-0.5 text-2xs text-[var(--color-fg-muted)]">
        {work.startedMs !== null && (
          <span>{t('usage.work.running', { ago: since(work.startedMs, now) })}</span>
        )}
        <span className="tabular-nums">{formatPercent(u.cpu)}</span>
        <span className="tabular-nums">{formatBytes(u.memoryWorkingSet)}</span>
        {repo !== null && <span className="truncate">{repo}</span>}
        {work.session !== null && <span className="truncate">{work.session}</span>}
      </div>
      {detail !== null && (
        <p className="truncate text-2xs text-[var(--color-fg-subtle)]">{detail}</p>
      )}
    </m.li>
  );
}

/**
 * A quiet colour per kind of work, as a leading dot rather than as the text
 * colour: the chart and status fills are not proven at 11px on a subtle
 * surface, so the word carries the meaning on a text token and the dot only
 * makes a wall of rows scannable.
 */
function workKindTone(kind: AgentqWorkKind): string {
  switch (kind) {
    case 'build':
      return 'bg-[var(--color-chart-cpu)]';
    case 'test':
      return 'bg-[var(--color-chart-memory)]';
    case 'commit':
      return 'bg-[var(--color-chart-disk)]';
    case 'push':
      return 'bg-[var(--color-chart-network)]';
    case 'land':
      return 'bg-[var(--color-accent)]';
    case 'install':
      return 'bg-[var(--color-chart-power)]';
    case 'deploy':
      return 'bg-[var(--color-status-warn)]';
    case 'job':
      return 'bg-[var(--color-chart-thermal)]';
    case 'other':
      return 'bg-[var(--color-fg-muted)]';
  }
}

function UsageCard({ row }: { readonly row: UsageRow }) {
  const { t } = useTranslation(AGENTS_NS);
  const u = row.usage;
  return (
    <m.li
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={{ duration: 0.25 }}
      className={cn(
        'rounded-xl border bg-[var(--surface-card)] p-3',
        row.self ? 'border-[var(--color-accent)]/40' : 'border-[var(--color-border-subtle)]',
      )}
    >
      <div className="flex items-center gap-2">
        <UsageMark kind={row.key} size="sm" />
        <span className="truncate text-sm font-medium">{t(`usage.kind.${row.key}`)}</span>
        <span className="ml-auto shrink-0 text-2xs text-[var(--color-fg-muted)]">
          {t('usage.processes', { count: u.processes })}
        </span>
      </div>
      <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1.5 sm:grid-cols-3">
        <Metric label={t('usage.cpu')} value={formatPercent(u.cpu)} />
        <Metric
          label={t('usage.memory')}
          value={formatBytes(u.memoryWorkingSet)}
          detail={t('usage.memoryPrivate', { value: formatBytes(u.memoryPrivate) })}
        />
        <Metric label={t('usage.disk')} value={throughputPair(u.diskRead, u.diskWrite)} />
        <Metric label={t('usage.network')} value={throughputPair(u.netRx, u.netTx)} />
        <Metric
          label={t('usage.gpu')}
          value={formatPercent(u.gpu)}
          detail={
            u.gpuMemory === null ? null : t('usage.gpuMemory', { value: formatBytes(u.gpuMemory) })
          }
        />
      </dl>
    </m.li>
  );
}

/**
 * `read ↓ · write ↑`, each side on its own so one unmeasured direction does
 * not turn the other into a dash.
 */
function throughputPair(down: number | null, up: number | null): string {
  return `${formatThroughput(down)} ↓ · ${formatThroughput(up)} ↑`;
}

function Metric({
  label,
  value,
  detail = null,
}: {
  readonly label: string;
  readonly value: string;
  readonly detail?: string | null;
}) {
  return (
    <div className="min-w-0">
      <dt className="text-2xs text-[var(--color-fg-subtle)]">{label}</dt>
      <dd className="truncate text-xs tabular-nums">{value}</dd>
      {detail !== null && (
        <dd className="truncate text-2xs text-[var(--color-fg-subtle)]">{detail}</dd>
      )}
    </div>
  );
}
