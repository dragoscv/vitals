/**
 * Jobs & releases, the journal timeline, cloud state and insights.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { CloudOff, Cloud, Square } from 'lucide-react';
import { useDeferredValue, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useVirtualizer } from '@tanstack/react-virtual';

import type { AgentqJob, AgentsSnapshot, JournalEvent } from '@vitals/protocol';
import { Badge, Button, EmptyState, Input, SegmentedControl, cn } from '@vitals/ui';

import { eventTone, formatDuration, since } from './model';
import { HourBars, Kpi } from './parts';
import type { RequestAction } from './QueuesPanel';
import { AGENTS_NS } from './strings';

const JOB_TONE: Record<string, 'ok' | 'warn' | 'danger' | 'info' | 'neutral'> = {
  running: 'info',
  done: 'ok',
  failed: 'danger',
  killed: 'neutral',
  suspect: 'warn',
  dead: 'danger',
};

export function JobsPanel({
  snapshot,
  now,
  onAction,
}: {
  readonly snapshot: AgentsSnapshot;
  readonly now: number;
  readonly onAction: RequestAction;
}) {
  const { t } = useTranslation(AGENTS_NS);
  return (
    <div className="flex flex-col gap-4">
      <section aria-label={t('tab.jobs')} className="flex flex-col gap-2">
        {snapshot.jobs.length === 0 ? (
          <EmptyState title={t('job.empty')} />
        ) : (
          <ul className="flex flex-col gap-1.5">
            <AnimatePresence initial={false}>
              {snapshot.jobs.map((j) => (
                <JobRow key={j.id} job={j} now={now} onAction={onAction} />
              ))}
            </AnimatePresence>
          </ul>
        )}
      </section>
      <section aria-label={t('job.releases')} className="flex flex-col gap-2">
        <h3 className="text-sm font-semibold">{t('job.releases')}</h3>
        {snapshot.releases.length === 0 ? (
          <p className="text-2xs text-[var(--color-fg-subtle)]">{t('job.releasesEmpty')}</p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {snapshot.releases.map((r) => (
              <li
                key={r.runId}
                className="flex flex-wrap items-center gap-2 rounded-lg border border-[var(--color-border-subtle)] bg-[var(--surface-card)] px-3 py-2 text-2xs"
              >
                <span className="font-mono font-semibold">
                  {r.repo}:{r.target}
                </span>
                {r.phase !== null && (
                  <Badge tone="info">{t('job.phase', { phase: r.phase })}</Badge>
                )}
                {r.state !== null && (
                  <Badge
                    tone={r.state === 'done' ? 'ok' : r.state === 'failed' ? 'danger' : 'neutral'}
                  >
                    {r.state}
                  </Badge>
                )}
                <span className="font-mono text-[var(--color-fg-muted)]">{r.sha?.slice(0, 9)}</span>
                <span className="min-w-0 flex-1 truncate">{r.purpose}</span>
                <span className="text-[var(--color-fg-subtle)]">
                  {since(r.phaseAtMs ?? r.startedMs, now)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function JobRow({
  job: j,
  now,
  onAction,
}: {
  readonly job: AgentqJob;
  readonly now: number;
  readonly onAction: RequestAction;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const running = j.state === 'running' || j.state === 'suspect';
  const elapsed = j.startedMs === null ? null : (j.endedMs ?? now) - j.startedMs;
  const expectedMs = j.expectedMin === null ? null : j.expectedMin * 60_000;
  const progress =
    elapsed !== null && expectedMs !== null && expectedMs > 0
      ? Math.min(1.5, elapsed / expectedMs)
      : null;
  const stateKey = j.state in JOB_TONE ? j.state : 'unknown';
  return (
    <m.li
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0 }}
      className="flex flex-col gap-1 rounded-lg border border-[var(--color-border-subtle)] bg-[var(--surface-card)] px-3 py-2 text-2xs"
    >
      <div className="flex items-center gap-2">
        <Badge tone={JOB_TONE[j.state] ?? 'neutral'}>{t(`job.state.${stateKey}`)}</Badge>
        <span className="font-medium">{j.label ?? j.id}</span>
        {j.repo !== null && <span className="text-[var(--color-fg-muted)]">{j.repo}</span>}
        <span
          className="min-w-0 flex-1 truncate font-mono text-[var(--color-fg-subtle)]"
          title={j.command ?? undefined}
        >
          {j.command}
        </span>
        <span className="text-[var(--color-fg-muted)] tabular-nums">{formatDuration(elapsed)}</span>
        {j.exitCode !== null && <span>{t('job.exit', { code: j.exitCode })}</span>}
        {running && (
          <Button
            variant="ghost"
            size="sm"
            aria-label={`${t('action.killJob')} ${j.label ?? j.id}`}
            onClick={() =>
              onAction({ kind: 'killJob', job: j.id }, `${t('action.killJob')} ${j.label ?? j.id}`)
            }
          >
            <Square aria-hidden className="size-3.5" />
          </Button>
        )}
      </div>
      {progress !== null && running && (
        <div className="relative h-1 overflow-hidden rounded-full bg-[var(--color-bg-subtle)]">
          <m.div
            className={cn(
              'h-full rounded-full',
              progress > 1 ? 'bg-[var(--color-status-warn)]' : 'bg-[var(--color-accent)]',
            )}
            animate={{ width: `${Math.min(100, progress * 100).toFixed(1)}%` }}
            transition={{ duration: 0.8 }}
          />
        </div>
      )}
      {running && j.lastOutputMs !== null && now - j.lastOutputMs > 120_000 && (
        <span className="text-[var(--color-warn-text)]">
          {t('job.silent', { ago: since(j.lastOutputMs, now) })}
        </span>
      )}
    </m.li>
  );
}

export function JournalPanel({
  events,
  now,
}: {
  readonly events: readonly JournalEvent[];
  readonly now: number;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const [query, setQuery] = useState('');
  const [mode, setMode] = useState<'all' | 'problems'>('all');
  const deferred = useDeferredValue(query);
  const rows = useMemo(() => {
    const q = deferred.trim().toLowerCase();
    return events.filter((e) => {
      if (mode === 'problems' && eventTone(e.event) !== 'danger' && eventTone(e.event) !== 'warn')
        return false;
      if (q === '') return true;
      return [e.event, e.resource, e.repo, e.session, e.text].some(
        (v) => v?.toLowerCase().includes(q) ?? false,
      );
    });
  }, [events, deferred, mode]);
  const parent = useRef<HTMLDivElement>(null);
  // Same reasoning as ProcessTable: the compiler cannot memoise the
  // virtualiser's functions, and nothing from it crosses a memo boundary.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parent.current,
    estimateSize: () => 34,
    overscan: 10,
    initialRect: { width: 1024, height: 480 },
  });
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t('journal.filter')}
          aria-label={t('journal.filter')}
          className="max-w-xs"
        />
        <SegmentedControl<'all' | 'problems'>
          value={mode}
          onValueChange={setMode}
          options={[
            { value: 'all', label: t('journal.all') },
            { value: 'problems', label: t('journal.problems') },
          ]}
          ariaLabel={t('journal.filter')}
          size="sm"
        />
      </div>
      {rows.length === 0 ? (
        <EmptyState title={t('journal.empty')} />
      ) : (
        <div
          ref={parent}
          className="table-scroll min-h-[18rem] flex-1 overflow-auto"
          role="log"
          aria-label={t('tab.journal')}
        >
          <div style={{ height: virt.getTotalSize(), position: 'relative' }}>
            {virt.getVirtualItems().map((v) => {
              const e = rows[v.index];
              if (e === undefined) return null;
              const tone = eventTone(e.event);
              return (
                <div
                  key={v.key}
                  className="absolute inset-x-0 flex items-center gap-2 border-b border-[var(--color-border-subtle)] px-2 text-2xs"
                  style={{ height: v.size, transform: `translateY(${String(v.start)}px)` }}
                >
                  <span className="w-14 shrink-0 text-[var(--color-fg-subtle)] tabular-nums">
                    {since(e.tsMs, now)}
                  </span>
                  <Badge
                    tone={tone === 'neutral' ? 'neutral' : tone}
                    className="w-28 justify-center"
                  >
                    {e.event}
                  </Badge>
                  <span className="w-36 shrink-0 truncate font-mono text-[var(--color-fg-muted)]">
                    {e.resource}
                  </span>
                  <span className="min-w-0 flex-1 truncate">{e.text}</span>
                  {e.holdSec !== null && (
                    <span className="tabular-nums">{formatDuration(e.holdSec * 1000)}</span>
                  )}
                  {e.exit !== null && e.exit !== 0 && <Badge tone="danger">exit {e.exit}</Badge>}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

export function CloudPanel({
  snapshot,
  account,
}: {
  readonly snapshot: AgentsSnapshot;
  readonly account?: React.ReactNode;
}) {
  const { t, i18n } = useTranslation(AGENTS_NS);
  const time = (ms: number | null) =>
    ms === null ? '—' : new Intl.DateTimeFormat(i18n.language, { timeStyle: 'short' }).format(ms);
  const cloudSessions = snapshot.sessions.filter((s) => s.host !== 'local');
  return (
    <div className="flex flex-col gap-3">
      {snapshot.cloudDown.length === 0 ? (
        <p className="inline-flex items-center gap-2 text-sm text-[var(--color-ok-text)]">
          <Cloud aria-hidden className="size-4" />
          {t('cloud.healthy')}
        </p>
      ) : (
        snapshot.cloudDown.map((c) => (
          <div
            key={c.workspace}
            role="status"
            className="flex flex-col gap-1 rounded-xl border border-[var(--color-status-warn)]/60 bg-[var(--surface-card)] p-3"
          >
            <p className="inline-flex items-center gap-2 text-sm font-medium text-[var(--color-warn-text)]">
              <CloudOff aria-hidden className="size-4" />
              {t('cloud.down', { ws: c.workspace, until: time(c.untilMs) })}
            </p>
            {c.reason !== null && (
              <p className="text-2xs text-[var(--color-fg-muted)]">
                {t('cloud.downReason', { reason: c.reason })}
              </p>
            )}
          </div>
        ))
      )}
      <Kpi
        label={t('cloud.runs', { hours: snapshot.stats.windowHours })}
        value={snapshot.stats.cloudRuns}
      />
      {account}
      {cloudSessions.length > 0 && (
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {t('cloud.sessions', { count: cloudSessions.length })}
        </p>
      )}
    </div>
  );
}

export function InsightsPanel({ snapshot }: { readonly snapshot: AgentsSnapshot }) {
  const { t } = useTranslation(AGENTS_NS);
  const s = snapshot.stats;
  const sec = (v: number | null) => (v === null ? '—' : formatDuration(v * 1000));
  return (
    <div className="flex flex-col gap-4">
      <p className="text-2xs text-[var(--color-fg-muted)]">
        {t('insights.window', { hours: s.windowHours })}
      </p>
      <div className="flex flex-wrap gap-2">
        <Kpi label={t('insights.runs')} value={s.runs} />
        <Kpi
          label={t('insights.failures')}
          value={s.failures}
          tone={s.failures > 0 ? 'danger' : 'neutral'}
        />
        <Kpi label={t('insights.commits')} value={s.commits} />
        <Kpi label={t('insights.waitP50')} value={sec(s.waitP50Sec)} />
        <Kpi label={t('insights.waitP90')} value={sec(s.waitP90Sec)} tone="warn" />
        <Kpi label={t('insights.holdP50')} value={sec(s.holdP50Sec)} />
        <Kpi label={t('insights.holdP90')} value={sec(s.holdP90Sec)} />
        <Kpi
          label={t('insights.waitTotal')}
          value={formatDuration(s.waitTotalSec * 1000)}
          tone="warn"
        />
      </div>
      <section aria-label={t('insights.perHour')} className="flex flex-col gap-1">
        <h3 className="text-sm font-semibold">{t('insights.perHour')}</h3>
        <HourBars values={s.runsPerHour} label={t('insights.perHour')} />
      </section>
      <section aria-label={t('insights.byRepo')} className="flex flex-col gap-1.5">
        <h3 className="text-sm font-semibold">{t('insights.byRepo')}</h3>
        {s.byRepo.map((r) => {
          const max = Math.max(1, ...s.byRepo.map((x) => x.holdTotalSec + x.waitTotalSec));
          return (
            <div key={r.repo} className="flex flex-col gap-0.5 text-2xs">
              <div className="flex justify-between">
                <span className="font-medium">{r.repo}</span>
                <span className="text-[var(--color-fg-muted)]">
                  {t('insights.repoRow', {
                    runs: r.runs,
                    fail: r.failures,
                    hold: formatDuration(r.holdTotalSec * 1000),
                    wait: formatDuration(r.waitTotalSec * 1000),
                  })}
                </span>
              </div>
              <div className="flex h-2 overflow-hidden rounded-full bg-[var(--color-bg-subtle)]">
                <m.div
                  className="h-full bg-[var(--color-accent)]"
                  initial={{ width: 0 }}
                  animate={{ width: `${((r.holdTotalSec / max) * 100).toFixed(1)}%` }}
                />
                <m.div
                  className="h-full bg-[var(--color-status-warn)]"
                  initial={{ width: 0 }}
                  animate={{ width: `${((r.waitTotalSec / max) * 100).toFixed(1)}%` }}
                />
              </div>
            </div>
          );
        })}
      </section>
    </div>
  );
}
