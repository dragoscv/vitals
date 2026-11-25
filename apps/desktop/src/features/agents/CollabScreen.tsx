/**
 * The collaboration view (ADR-0039): what the AI agents on this machine are
 * asking each other and sharing, read from agentq's coordination directory.
 *
 * Read-only — agentq's own CLI is the only writer, and this page never touches
 * a file. Data arrives from `get_collab` on a timer while the page is visible
 * (`useCollab`); the three sections are read from agentq's registry, its
 * message store and its job records. Items enter and leave with presence
 * animations so a thread gaining a reply, or a job finishing, is seen to move
 * rather than simply re-render.
 *
 * A figure that cannot be measured is an em dash here exactly as everywhere
 * else: an agent with no recorded repo shows `—`, never an empty string that
 * reads as a value.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { MessagesSquare, ShieldAlert, SquareTerminal, Users } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { CollabAgent, CollabJob, CollabThread } from '@vitals/protocol';
import { Badge, EmptyState, Skeleton, cn } from '@vitals/ui';

import { firstLine, newestFirst } from './collabModel';
import { since } from './model';
import { AGENTS_NS } from './strings';
import { NO_HOST, useCollab, type CollabSource } from './useCollab';

export interface CollabScreenProps {
  readonly source?: CollabSource;
}

const JOB_TONE: Record<string, 'ok' | 'warn' | 'danger' | 'info' | 'neutral'> = {
  running: 'info',
  done: 'ok',
  failed: 'danger',
  killed: 'neutral',
  unknown: 'neutral',
};

/** agentq's job states to the key that names them; anything else is unknown. */
const JOB_STATE_KEY: Record<string, string> = {
  running: 'collab.work.state.running',
  done: 'collab.work.state.done',
  failed: 'collab.work.state.failed',
  killed: 'collab.work.state.killed',
};

export function CollabScreen({ source }: CollabScreenProps): React.JSX.Element {
  const { t } = useTranslation(AGENTS_NS);
  const state = useCollab(source);

  if (state.pending && state.snapshot === null) return <CollabSkeleton />;
  if (state.error === NO_HOST) {
    return (
      <EmptyState icon={<ShieldAlert />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }
  if (state.snapshot === null) {
    return (
      <EmptyState icon={<ShieldAlert />} title={t('error.title')} description={state.error ?? ''} />
    );
  }

  const s = state.snapshot;
  const now = state.now;
  const threads = newestFirst(s.threads);

  return (
    <div className="screen">
      <header className="screen-header">
        <div className="min-w-0">
          <h2 className="flex items-center gap-2 text-lg font-semibold">
            <MessagesSquare aria-hidden className="size-5 text-[var(--color-accent)]" />
            {t('collab.title')}
            <span className="inline-flex items-center gap-1 rounded-full bg-[var(--color-status-ok)]/12 px-2 py-0.5 text-2xs font-medium text-[var(--color-ok-text)]">
              <m.span
                aria-hidden
                className="size-1.5 rounded-full bg-[var(--color-status-ok)]"
                animate={{ opacity: [1, 0.3, 1] }}
                transition={{ duration: 1.6, repeat: Infinity }}
              />
              {t('collab.live')}
            </span>
          </h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('collab.subtitle')} · {t('collab.updated', { ago: since(s.takenAtMs, now) })}
          </p>
        </div>
      </header>

      {!s.installed && (
        <p className="text-2xs text-[var(--color-warn-text)]">{t('collab.notInstalled')}</p>
      )}

      <div className="screen-body">
        <AgentsSection agents={s.agents} now={now} liveMin={s.livenessMin} />
        <WorkSection jobs={s.jobs} now={now} />
        <ThreadsSection threads={threads} now={now} />
      </div>
    </div>
  );
}

function AgentsSection({
  agents,
  now,
  liveMin,
}: {
  readonly agents: readonly CollabAgent[];
  readonly now: number;
  readonly liveMin: number;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const live = agents.filter((a) => a.live).length;
  return (
    <section
      aria-label={t('collab.agents.title')}
      className="pane flex flex-col gap-2 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--surface-card)] p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="flex items-center gap-2 text-sm font-semibold">
          <Users aria-hidden className="size-4 text-[var(--color-fg-muted)]" />
          {t('collab.agents.title')}
          <Badge tone={live > 0 ? 'ok' : 'neutral'}>
            {t('collab.agents.count', { count: agents.length })}
          </Badge>
        </h3>
        <span className="text-2xs text-[var(--color-fg-muted)]">
          {t('collab.agents.live', { present: live, min: liveMin })}
        </span>
      </div>
      {agents.length === 0 ? (
        <EmptyState title={t('collab.agents.empty', { min: liveMin })} />
      ) : (
        <ul className="flex flex-col gap-1.5">
          <AnimatePresence initial={false}>
            {agents.map((a) => (
              <AgentRow key={a.session} agent={a} now={now} />
            ))}
          </AnimatePresence>
        </ul>
      )}
    </section>
  );
}

function AgentRow({ agent: a, now }: { readonly agent: CollabAgent; readonly now: number }) {
  const { t } = useTranslation(AGENTS_NS);
  const doing = a.lastPurpose ?? a.lastVerb;
  return (
    <m.li
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: a.live ? 1 : 0.6, y: 0 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={{ duration: 0.22 }}
      className={cn(
        'flex flex-col gap-1 rounded-lg border px-3 py-2 text-2xs',
        a.live
          ? 'border-[var(--color-status-ok)]/40 bg-[var(--color-status-ok)]/5'
          : 'border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)]',
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          <span
            aria-hidden
            className="size-1.5 shrink-0 rounded-full"
            style={{
              background: a.live ? 'var(--color-status-ok)' : 'var(--color-fg-subtle)',
            }}
          />
          <span className="truncate font-mono font-medium">{a.session}</span>
        </span>
        <span className="shrink-0 text-[var(--color-fg-subtle)]">
          {t('collab.agents.seen', { ago: since(a.lastSeenMs, now) })}
        </span>
      </div>
      <div className="flex items-center gap-1.5 text-[var(--color-fg-muted)]">
        <span>{a.client ?? '—'}</span>
        {a.model !== null && <span className="truncate">· {a.model}</span>}
        <span>· {a.repo ?? '—'}</span>
        <span className="shrink-0">· {t('collab.agents.calls', { count: a.calls ?? 0 })}</span>
      </div>
      {doing !== null && (
        <p className="truncate text-[var(--color-fg-subtle)]">
          {t('collab.agents.doing')}: {firstLine(doing, 100)}
        </p>
      )}
    </m.li>
  );
}

function ThreadsSection({
  threads,
  now,
}: {
  readonly threads: readonly CollabThread[];
  readonly now: number;
}) {
  const { t } = useTranslation(AGENTS_NS);
  return (
    <section
      aria-label={t('collab.threads.title')}
      className="pane flex flex-col gap-2 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--surface-card)] p-3"
    >
      <h3 className="flex items-center gap-2 text-sm font-semibold">
        <MessagesSquare aria-hidden className="size-4 text-[var(--color-fg-muted)]" />
        {t('collab.threads.title')}
        <Badge tone="neutral">{t('collab.threads.count', { count: threads.length })}</Badge>
      </h3>
      {threads.length === 0 ? (
        <EmptyState title={t('collab.threads.empty')} />
      ) : (
        <ul className="flex flex-col gap-1.5">
          <AnimatePresence initial={false}>
            {threads.map((thread) => (
              <ThreadRow key={thread.id} thread={thread} now={now} />
            ))}
          </AnimatePresence>
        </ul>
      )}
    </section>
  );
}

function ThreadRow({ thread, now }: { readonly thread: CollabThread; readonly now: number }) {
  const { t } = useTranslation(AGENTS_NS);
  const from = thread.from.session ?? t('collab.unknown');
  const answered = thread.answers.length > 0;
  const anchor = answered ? thread.lastMs : thread.askedAtMs;
  return (
    <m.li
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={{ duration: 0.22 }}
      className="flex flex-col gap-1.5 rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] px-3 py-2 text-2xs"
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge tone={answered ? 'ok' : 'warn'}>
          {answered ? t('collab.threads.answered') : t('collab.threads.open')}
        </Badge>
        <span className="font-mono text-[var(--color-fg-subtle)]">{from}</span>
        <span aria-hidden className="text-[var(--color-fg-subtle)]">
          →
        </span>
        <span className="font-mono">{thread.to}</span>
        {thread.resource !== null && (
          <span className="text-[var(--color-fg-muted)]">
            {t('collab.threads.about', { resource: thread.resource })}
          </span>
        )}
        <span className="ml-auto shrink-0 text-[var(--color-fg-subtle)]">
          {t(answered ? 'collab.threads.replied' : 'collab.threads.asked', {
            ago: since(anchor, now),
          })}
        </span>
      </div>
      <p className="break-words whitespace-pre-wrap text-[var(--color-fg)]">
        {firstLine(thread.message, 400)}
      </p>
      {thread.answers.length > 0 && (
        <ul className="flex flex-col gap-1 border-l-2 border-[var(--color-border-default)] pl-2">
          {thread.answers.map((answer, index) => (
            <li key={`${thread.id}-${index}`} className="flex flex-col">
              <span className="text-[var(--color-fg-muted)]">
                {answer.session ?? t('collab.unknown')}
                {answer.repo !== null && ` · ${answer.repo}`}
                {' · '}
                {since(answer.atMs, now)}
              </span>
              <span className="break-words whitespace-pre-wrap text-[var(--color-fg-subtle)]">
                {firstLine(answer.message, 400)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </m.li>
  );
}

function WorkSection({ jobs, now }: { readonly jobs: readonly CollabJob[]; readonly now: number }) {
  const { t } = useTranslation(AGENTS_NS);
  const running = jobs.filter((j) => j.state === 'running').length;
  return (
    <section
      aria-label={t('collab.work.title')}
      className="pane flex flex-col gap-2 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--surface-card)] p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="flex items-center gap-2 text-sm font-semibold">
          <SquareTerminal aria-hidden className="size-4 text-[var(--color-fg-muted)]" />
          {t('collab.work.title')}
          <Badge tone={running > 0 ? 'info' : 'neutral'}>
            {t('collab.work.count', { count: jobs.length })}
          </Badge>
        </h3>
        <span className="text-2xs text-[var(--color-fg-muted)]">
          {t('collab.work.running', { running })}
        </span>
      </div>
      {jobs.length === 0 ? (
        <EmptyState title={t('collab.work.empty')} />
      ) : (
        <ul className="flex flex-col gap-1.5">
          <AnimatePresence initial={false}>
            {jobs.map((job) => (
              <WorkRow key={job.id} job={job} now={now} />
            ))}
          </AnimatePresence>
        </ul>
      )}
    </section>
  );
}

function WorkRow({ job, now }: { readonly job: CollabJob; readonly now: number }) {
  const { t } = useTranslation(AGENTS_NS);
  return (
    <m.li
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={{ duration: 0.22 }}
      className="flex flex-col gap-0.5 rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] px-3 py-2 text-2xs"
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge tone={JOB_TONE[job.state] ?? 'neutral'}>
          {t(JOB_STATE_KEY[job.state] ?? 'collab.work.state.unknown')}
        </Badge>
        <span className="truncate font-medium">{job.label ?? job.id}</span>
        <span className="text-[var(--color-fg-muted)]">· {job.repo ?? '—'}</span>
        {job.exitCode !== null && (
          <span className="text-[var(--color-fg-muted)]">
            {t('collab.work.exit', { code: job.exitCode })}
          </span>
        )}
        <span className="ml-auto shrink-0 text-[var(--color-fg-subtle)]">
          {t(job.endedMs === null ? 'collab.work.started' : 'collab.work.ended', {
            ago: since(job.endedMs ?? job.startedMs, now),
          })}
        </span>
      </div>
      {job.command !== null && (
        <p className="truncate font-mono text-[var(--color-fg-subtle)]">
          {firstLine(job.command, 120)}
        </p>
      )}
    </m.li>
  );
}

function CollabSkeleton(): React.JSX.Element {
  return (
    <div className="screen">
      <Skeleton className="h-6 w-56" />
      <div className="grid gap-3 lg:grid-cols-2">
        <Skeleton className="h-40" />
        <Skeleton className="h-40" />
        <Skeleton className="h-32 lg:col-span-2" />
      </div>
    </div>
  );
}
