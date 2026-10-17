/**
 * Queues and worktrees: agentq's resources as slot strips with their
 * holders and waiters, the adaptive machine cap, and the leased slots.
 *
 * Every action opens a reason dialog first; agentq journals the reason and
 * other agents read it.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { GitBranch, Lock, Snowflake, Unlock, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type {
  AgentqAction,
  AgentqLease,
  AgentqResource,
  AgentqTicket,
  AgentsSnapshot,
} from '@vitals/protocol';
import { Badge, Button, EmptyState, cn } from '@vitals/ui';

import { resourceSlots, since } from './model';
import { LivenessDot, SlotStrip } from './parts';
import { AGENTS_NS } from './strings';

export type RequestAction = (
  action: AgentqAction | ((reason: string) => AgentqAction),
  title: string,
) => void;

export function QueuesPanel({
  snapshot,
  now,
  onAction,
}: {
  readonly snapshot: AgentsSnapshot;
  readonly now: number;
  readonly onAction: RequestAction;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const marks = new Map(snapshot.marks.map((mk) => [mk.resource, mk]));
  const busy = snapshot.resources.filter((r) => r.tickets.length > 0 || r.kind === 'machine');
  return (
    <div className="flex flex-col gap-3">
      <MachineCard snapshot={snapshot} />
      {busy.length === 0 && <EmptyState title={t('queue.empty')} />}
      <ul className="grid [grid-template-columns:repeat(auto-fill,minmax(24rem,1fr))] gap-2">
        <AnimatePresence initial={false}>
          {busy.map((r) => (
            <ResourceCard
              key={r.name}
              resource={r}
              mark={marks.get(r.name)?.state ?? null}
              now={now}
              onAction={onAction}
            />
          ))}
        </AnimatePresence>
      </ul>
    </div>
  );
}

function MachineCard({ snapshot }: { readonly snapshot: AgentsSnapshot }) {
  const { t } = useTranslation(AGENTS_NS);
  const mc = snapshot.machine;
  if (mc === null) return null;
  const fmt = (v: number | null, unit: string) => (v === null ? '—' : `${v.toFixed(0)} ${unit}`);
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--color-accent-border)] bg-[var(--color-accent-subtle)] p-3">
      <div>
        <p className="text-sm font-semibold">{t('machine.title')}</p>
        {mc.min !== null && mc.max !== null && (
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('machine.adaptive', { min: mc.min, max: mc.max })}
          </p>
        )}
        {mc.why !== null && (
          <p className="text-2xs text-[var(--color-fg-subtle)]">
            {t('machine.why', { why: mc.why })}
          </p>
        )}
      </div>
      <m.span
        key={mc.cap}
        initial={{ scale: 1.4, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        className="text-3xl font-bold text-[var(--color-accent)] tabular-nums"
      >
        {mc.cap}
      </m.span>
      <dl className="flex gap-4 text-2xs">
        <div>
          <dt className="text-[var(--color-fg-muted)]">{t('machine.cpu')}</dt>
          <dd className="font-medium">{fmt(mc.cpu, '%')}</dd>
        </div>
        <div>
          <dt className="text-[var(--color-fg-muted)]">{t('machine.ram')}</dt>
          <dd className="font-medium">{fmt(mc.freeRamGb, 'GB')}</dd>
        </div>
        <div>
          <dt className="text-[var(--color-fg-muted)]">{t('machine.commit')}</dt>
          <dd className="font-medium">{fmt(mc.freeCommitGb, 'GB')}</dd>
        </div>
      </dl>
    </div>
  );
}

function ResourceCard({
  resource: r,
  mark,
  now,
  onAction,
}: {
  readonly resource: AgentqResource;
  readonly mark: string | null;
  readonly now: number;
  readonly onAction: RequestAction;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const { held, waiting, capacity } = resourceSlots(r);
  return (
    <m.li
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.97 }}
      className={cn(
        'flex flex-col gap-2 rounded-xl border bg-[var(--surface-card)] p-3',
        mark !== null
          ? 'border-[var(--color-status-danger)]/60'
          : 'border-[var(--color-border-subtle)]',
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate font-mono text-xs font-semibold">{r.name}</p>
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('queue.capacity', { held: held.length, capacity })}
            {waiting.length > 0 && ` · ${t('queue.waiting', { count: waiting.length })}`}
          </p>
        </div>
        <div className="flex items-center gap-1">
          {mark !== null && (
            <Badge tone="danger" icon={<Snowflake />}>
              {mark === 'blocked' ? t('queue.blocked') : t('queue.frozen')}
            </Badge>
          )}
          {mark === null ? (
            <Button
              variant="ghost"
              size="sm"
              aria-label={`${t('action.freeze')} ${r.name}`}
              onClick={() =>
                onAction(
                  (reason) => ({ kind: 'freeze', resource: r.name, reason }),
                  `${t('action.freeze')} ${r.name}`,
                )
              }
            >
              <Lock aria-hidden className="size-3.5" />
            </Button>
          ) : (
            <Button
              variant="ghost"
              size="sm"
              aria-label={`${t('action.clear')} ${r.name}`}
              onClick={() =>
                onAction(
                  (reason) => ({ kind: 'clear', resource: r.name, reason }),
                  `${t('action.clear')} ${r.name}`,
                )
              }
            >
              <Unlock aria-hidden className="size-3.5" />
            </Button>
          )}
        </div>
      </div>
      <SlotStrip
        held={held.length}
        capacity={capacity}
        waiting={waiting.length}
        label={t('queue.capacity', { held: held.length, capacity })}
      />
      <ul className="flex flex-col gap-1">
        <AnimatePresence initial={false}>
          {[...held, ...waiting].map((tk) => (
            <TicketRow key={tk.id} ticket={tk} now={now} onAction={onAction} />
          ))}
        </AnimatePresence>
      </ul>
    </m.li>
  );
}

function TicketRow({
  ticket: tk,
  now,
  onAction,
}: {
  readonly ticket: AgentqTicket;
  readonly now: number;
  readonly onAction: RequestAction;
}) {
  const { t } = useTranslation(AGENTS_NS);
  const breakable = tk.liveness === 'dead' || tk.liveness === 'stale';
  return (
    <m.li
      initial={{ opacity: 0, x: 12 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: -12 }}
      transition={{ duration: 0.22 }}
      className={cn(
        'flex items-center gap-2 rounded-lg px-2 py-1 text-2xs',
        tk.held ? 'bg-[var(--color-accent-subtle)]' : 'bg-[var(--color-bg-subtle)]',
      )}
    >
      <span
        aria-hidden
        className={cn(
          'size-1.5 shrink-0 rounded-full',
          tk.held ? 'bg-[var(--color-accent)]' : 'bg-[var(--color-status-warn)]',
        )}
      />
      <span className="min-w-0 flex-1 truncate" title={tk.cmd ?? undefined}>
        {tk.purpose ?? tk.cmd ?? tk.id}
      </span>
      {tk.lane !== null && <Badge>{tk.lane}</Badge>}
      {tk.yielded && <Badge tone="info">{t('queue.yielded')}</Badge>}
      {tk.wedged && <Badge tone="danger">{t('queue.wedged')}</Badge>}
      <span className="shrink-0 text-[var(--color-fg-muted)] tabular-nums">
        {tk.held
          ? t('queue.heldFor', { ago: since(tk.acquiredMs ?? tk.createdMs, now) })
          : t('queue.waited', { ago: since(tk.createdMs, now) })}
      </span>
      <LivenessDot value={tk.liveness} />
      {breakable && (
        <Button
          variant="ghost"
          size="sm"
          aria-label={`${t('action.break')} ${tk.id}`}
          onClick={() =>
            onAction(
              (reason) => ({ kind: 'break', resource: tk.resource, ticket: tk.id, reason }),
              `${t('action.break')} ${tk.id}`,
            )
          }
        >
          <X aria-hidden className="size-3.5" />
        </Button>
      )}
    </m.li>
  );
}

export function WorktreesPanel({
  leases,
  now,
  onAction,
}: {
  readonly leases: readonly AgentqLease[];
  readonly now: number;
  readonly onAction: RequestAction;
}) {
  const { t } = useTranslation(AGENTS_NS);
  if (leases.length === 0) return <EmptyState title={t('lease.empty')} />;
  const byRepo = new Map<string, AgentqLease[]>();
  for (const l of leases) byRepo.set(l.repo, [...(byRepo.get(l.repo) ?? []), l]);
  return (
    <div className="flex flex-col gap-4">
      {[...byRepo.entries()].map(([repo, list]) => (
        <section key={repo} className="flex flex-col gap-2" aria-label={repo}>
          <h3 className="text-sm font-semibold">{repo}</h3>
          <ul className="grid [grid-template-columns:repeat(auto-fill,minmax(16rem,1fr))] gap-2">
            {list.map((l) => (
              <m.li
                key={l.leaseId}
                initial={{ opacity: 0, scale: 0.96 }}
                animate={{ opacity: 1, scale: 1 }}
                className={cn(
                  'flex flex-col gap-1.5 rounded-xl border bg-[var(--surface-card)] p-3 text-2xs',
                  l.stale
                    ? 'border-[var(--color-status-warn)]/60'
                    : 'border-[var(--color-border-subtle)]',
                )}
              >
                <div className="flex items-center justify-between">
                  <span className="font-mono text-xs font-semibold">{l.slot}</span>
                  {l.role !== null && <Badge>{l.role}</Badge>}
                </div>
                <span className="inline-flex items-center gap-1 truncate">
                  <GitBranch aria-hidden className="size-3" />
                  {l.branch ?? '—'}
                </span>
                <span className="line-clamp-2 text-[var(--color-fg-muted)]">{l.purpose}</span>
                <span className="text-[var(--color-fg-subtle)]">
                  {t('lease.idle', { ago: since(l.beatMs, now) })}
                </span>
                <span
                  className={
                    l.ownerAlive === false
                      ? 'text-[var(--color-warn-text)]'
                      : 'text-[var(--color-ok-text)]'
                  }
                >
                  {l.ownerAlive === false
                    ? t('lease.gone')
                    : l.ownerAlive === true
                      ? t('lease.alive')
                      : '—'}
                </span>
                {l.stale && <Badge tone="warn">{t('lease.stale')}</Badge>}
                {l.ownerAlive === false && l.path !== null && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() =>
                      onAction(
                        (reason) => ({
                          kind: 'unlease',
                          path: l.path ?? '',
                          leaseId: l.leaseId,
                          reason,
                        }),
                        `${t('action.unlease')} ${l.repo}/${l.slot}`,
                      )
                    }
                  >
                    {t('action.unlease')}
                  </Button>
                )}
              </m.li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
