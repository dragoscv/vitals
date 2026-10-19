/**
 * Sessions: every AI conversation on this machine, grouped by client.
 *
 * A card per session. Selecting one expands it in place (height + opacity,
 * not a layout animation — ADR-0017 does not load `domMax`) to show what the
 * agent is doing through agentq: the queues it holds or waits on and the
 * worktree it leased, joined through agentq's caller registry.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { ChevronDown, GitBranch, Wrench } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { AgentSession, AgentsSnapshot } from '@vitals/protocol';
import { Badge, EmptyState, cn, useHoverHold, useStableOrder } from '@vitals/ui';

import {
  activeShare,
  byClient,
  formatCount,
  formatDuration,
  formatTokens,
  groupKey,
  groupRank,
  sessionKey,
  sessionRank,
  since,
  workFor,
} from './model';
import { ClientMark, ShareBar, StateChip } from './parts';
import { AGENTS_NS } from './strings';

export function SessionsPanel({
  snapshot,
  now,
  limit,
}: {
  readonly snapshot: AgentsSnapshot;
  readonly now: number;
  /** Overview shows only the live ones. */
  readonly limit?: 'live';
}) {
  const { t } = useTranslation(AGENTS_NS);
  const sessions =
    limit === 'live'
      ? snapshot.sessions.filter((s) => s.state === 'working' || s.state === 'waiting')
      : snapshot.sessions;
  // The desktop sorts by state, then by last activity, which changes every
  // second: followed literally, two busy sessions swap places on every push.
  // Shown order moves only when a state changes or every 30 s (S22-10).
  // Nothing moves at all while the pointer or focus is on the cards.
  const { held, bind } = useHoverHold();
  const ordered = useStableOrder(sessions, sessionKey, { rankOf: sessionRank, now, held });
  const groups = useStableOrder(byClient(ordered), groupKey, { rankOf: groupRank, now, held });
  if (sessions.length === 0) {
    return <EmptyState title={t('empty.title')} description={t('empty.body')} />;
  }
  return (
    <div className="flex flex-col gap-4" {...bind}>
      {groups.map(({ client, sessions: list }) => (
        <section key={client} aria-label={t(`client.${client}`)} className="flex flex-col gap-2">
          <h3 className="flex items-center gap-2 text-sm font-semibold">
            <ClientMark client={client} size="sm" />
            {t(`client.${client}`)}
            <span className="text-2xs font-normal text-[var(--color-fg-muted)]">{list.length}</span>
          </h3>
          <ul className="grid [grid-template-columns:repeat(auto-fill,minmax(22rem,1fr))] gap-2">
            <AnimatePresence initial={false}>
              {list.map((s) => (
                <SessionCard key={s.id} session={s} snapshot={snapshot} now={now} />
              ))}
            </AnimatePresence>
          </ul>
        </section>
      ))}
    </div>
  );
}

function SessionCard({
  session: s,
  snapshot,
  now,
}: {
  readonly session: AgentSession;
  readonly snapshot: AgentsSnapshot;
  readonly now: number;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const [open, setOpen] = useState(false);
  const share = activeShare(s, now);
  const work = open ? workFor(s, snapshot) : null;
  const panelId = `session-${s.id}`;
  return (
    <m.li
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: s.state === 'ended' ? 0.6 : 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={{ duration: 0.25 }}
      className={cn(
        'rounded-xl border bg-[var(--surface-card)] transition-colors',
        s.state === 'working'
          ? 'border-[var(--color-status-ok)]/50 shadow-[0_0_0_1px_var(--color-status-ok)/10]'
          : 'border-[var(--color-border-subtle)]',
      )}
    >
      <button
        type="button"
        className="flex w-full flex-col gap-2 rounded-xl p-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-accent)]"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((v) => !v)}
      >
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <p className="truncate text-sm font-medium">{s.title ?? t('session.untitled')}</p>
            <p className="truncate text-2xs text-[var(--color-fg-muted)]">
              {s.repo ?? s.workspace ?? '—'}
              {s.branch !== null && (
                <span className="ml-1.5 inline-flex items-center gap-0.5">
                  <GitBranch aria-hidden className="size-3" />
                  {s.branch}
                </span>
              )}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <StateChip state={s.state} />
            <ChevronDown
              aria-hidden
              className={cn(
                'size-4 text-[var(--color-fg-muted)] transition-transform',
                open && 'rotate-180',
              )}
            />
          </div>
        </div>

        <AnimatePresence initial={false}>
          {s.currentTool !== null && (
            <m.p
              key={s.currentTool}
              initial={{ opacity: 0, x: -6 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0 }}
              className="inline-flex items-center gap-1 text-2xs text-[var(--color-ok-text)]"
            >
              <Wrench aria-hidden className="size-3" />
              {t('session.nowRunning', { tool: s.currentTool })}
            </m.p>
          )}
        </AnimatePresence>

        <dl className="grid grid-cols-4 gap-2 text-2xs">
          <Stat label={t('session.active')} value={formatDuration(s.activeMs)} />
          <Stat label={t('session.started')} value={since(s.startedMs, now)} />
          <Stat label={t('session.turns')} value={formatCount(s.turns)} />
          <Stat label={t('session.tools')} value={formatCount(s.toolCalls)} />
        </dl>
        {share !== null && (
          <ShareBar
            value={share}
            label={t('session.activeShare', { pct: `${Math.round(share * 100)}%` })}
          />
        )}
      </button>

      <AnimatePresence initial={false}>
        {open && work !== null && (
          <m.div
            id={panelId}
            key="detail"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.28, ease: 'easeOut' }}
            className="overflow-hidden"
          >
            <div className="flex flex-col gap-3 border-t border-[var(--color-border-subtle)] p-3 text-2xs">
              <dl className="grid grid-cols-2 gap-2">
                <Stat label={t('session.model')} value={s.model ?? '—'} />
                <Stat label={t('session.lastActivity')} value={since(s.lastActivityMs, now)} />
                <Stat label={t('session.prompts')} value={formatCount(s.userMessages)} />
                <Stat label={t('session.failures')} value={formatCount(s.toolFailures)} />
                <Stat
                  label={t('session.tokens')}
                  value={`${formatTokens(s.tokensIn)} / ${formatTokens(s.tokensOut)}`}
                />
                <Stat label="PID" value={s.pid === null ? '—' : String(s.pid)} />
              </dl>
              {s.topTools.length > 0 && (
                <div>
                  <p className="mb-1 text-[var(--color-fg-muted)]">{t('session.topTools')}</p>
                  <div className="flex flex-wrap gap-1">
                    {s.topTools.map((tool) => (
                      <Badge key={tool.name}>
                        {tool.name} · {tool.count}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}
              <div>
                <p className="mb-1 text-[var(--color-fg-muted)]">{t('session.agentqWork')}</p>
                {work.tickets.length === 0 && work.leases.length === 0 ? (
                  <p className="text-[var(--color-fg-subtle)]">{t('session.noWork')}</p>
                ) : (
                  <ul className="flex flex-col gap-1">
                    {work.tickets.map((tk) => (
                      <li key={tk.id}>
                        <Badge tone={tk.held ? 'ok' : 'warn'}>
                          {tk.held
                            ? t('session.holds', { resource: tk.resource })
                            : t('session.waitsFor', { resource: tk.resource })}
                        </Badge>{' '}
                        <span className="text-[var(--color-fg-muted)]">{tk.purpose}</span>
                      </li>
                    ))}
                    {work.leases.map((l) => (
                      <li key={l.leaseId}>
                        <Badge tone="accent">
                          {t('session.lease', {
                            slot: `${l.repo}/${l.slot}`,
                            branch: l.branch ?? '—',
                          })}
                        </Badge>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          </m.div>
        )}
      </AnimatePresence>
    </m.li>
  );
}

function Stat({ label, value }: { readonly label: string; readonly value: string }) {
  return (
    <div className="min-w-0">
      <dt className="truncate text-[var(--color-fg-muted)]">{label}</dt>
      <dd className="truncate font-medium tabular-nums">{value}</dd>
    </div>
  );
}
