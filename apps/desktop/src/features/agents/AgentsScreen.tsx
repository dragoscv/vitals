/**
 * The AI page (ADR-0039): which AI agents are working on this machine, who
 * started them, what they hold and wait for in agentq, and what it costs.
 *
 * Data arrives as a pushed snapshot every ~2 s while the page is visible
 * (useAgents). Tabs switch with a short cross-fade; within a tab, cards enter
 * and leave with presence animations so a ticket moving from "waiting" to
 * "running" is seen to move rather than re-render.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { Bot, Eraser, MessageSquarePlus, ShieldAlert } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import type { AgentqAction } from '@vitals/protocol';
import {
  Button,
  DialogContent,
  DialogRoot,
  EmptyState,
  Input,
  Skeleton,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@vitals/ui';

import { CloudPanel, InsightsPanel, JobsPanel, JournalPanel } from './ActivityPanels';
import { CloudAccount } from './CloudAccount';
import { agentsTabs, headline, since, type AgentsTab } from './model';
import { Kpi } from './parts';
import { QueuesPanel, WorktreesPanel, type RequestAction } from './QueuesPanel';
import { SessionsPanel } from './SessionsPanel';
import { AGENTS_NS } from './strings';
import { NO_HOST, useAgents, type AgentsSource } from './useAgents';

export interface AgentsScreenProps {
  readonly source?: AgentsSource;
}

interface Pending {
  readonly title: string;
  readonly build: AgentqAction | ((reason: string) => AgentqAction);
}

export function AgentsScreen({ source }: AgentsScreenProps): React.JSX.Element {
  const { t } = useTranslation(AGENTS_NS);
  const state = useAgents(source);
  const [tab, setTab] = useState<AgentsTab>('overview');
  const [pending, setPending] = useState<Pending | null>(null);
  const [reason, setReason] = useState('');
  const [busy, setBusy] = useState(false);

  const request: RequestAction = (build, title) => {
    if (typeof build === 'function') {
      setReason('');
      setPending({ title, build });
    } else {
      void execute(build);
    }
  };

  async function execute(action: AgentqAction): Promise<boolean> {
    setBusy(true);
    try {
      const res = await state.run(action);
      if (res.ok) {
        toast.success(
          action.kind === 'reapTerminals'
            ? reapMessage(res.output, action.dryRun)
            : t('action.done'),
        );
        return true;
      }
      toast.error(t('action.failed', { output: res.output.slice(-200) }));
      return false;
    } catch (cause: unknown) {
      toast.error(t('action.failed', { output: String(cause) }));
      return false;
    } finally {
      setBusy(false);
    }
  }

  function reapMessage(output: string, dryRun: boolean): string {
    try {
      const line =
        output
          .split('\n')
          .reverse()
          .find((l) => l.trim().startsWith('{')) ?? '{}';
      const r = JSON.parse(line) as {
        reapable?: number;
        reapableMB?: number;
        killed?: number;
        freedMB?: number;
      };
      return dryRun
        ? t('action.reapPreviewResult', { count: r.reapable ?? 0, mb: r.reapableMB ?? 0 })
        : t('action.reapResult', { killed: r.killed ?? 0, mb: r.freedMB ?? 0 });
    } catch {
      return t('action.done');
    }
  }

  if (state.pending && state.snapshot === null) return <AgentsSkeleton />;
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
  const h = headline(s);
  const now = state.now;
  const machineCap =
    s.machine?.cap ?? s.resources.find((r) => r.kind === 'machine')?.capacity ?? null;

  return (
    <div className="screen">
      <header className="screen-header">
        <div className="min-w-0">
          <h2 className="flex items-center gap-2 text-lg font-semibold">
            <Bot aria-hidden className="size-5 text-[var(--color-accent)]" />
            {t('title')}
            <span className="inline-flex items-center gap-1 rounded-full bg-[var(--color-status-ok)]/12 px-2 py-0.5 text-2xs font-medium text-[var(--color-ok-text)]">
              <m.span
                aria-hidden
                className="size-1.5 rounded-full bg-[var(--color-status-ok)]"
                animate={{ opacity: [1, 0.3, 1] }}
                transition={{ duration: 1.6, repeat: Infinity }}
              />
              {t('live')}
            </span>
          </h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('subtitle')} · {t('updated', { ago: since(s.takenAtMs, now) })}
          </p>
        </div>
        <div className="flex gap-1">
          <Button
            variant="ghost"
            size="sm"
            disabled={busy || !s.agentqInstalled}
            onClick={() => void execute({ kind: 'reapTerminals', dryRun: true })}
          >
            <Eraser aria-hidden className="size-4" />
            {t('action.reapPreview')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={busy || !s.agentqInstalled}
            onClick={() => void execute({ kind: 'reapTerminals', dryRun: false })}
          >
            {t('action.reap')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={busy || !s.agentqInstalled}
            onClick={() => request((message) => ({ kind: 'note', message }), t('action.note'))}
          >
            <MessageSquarePlus aria-hidden className="size-4" />
            {t('action.note')}
          </Button>
        </div>
      </header>

      {!s.agentqInstalled && (
        <p className="text-2xs text-[var(--color-warn-text)]">{t('notInstalled')}</p>
      )}
      {state.error !== null && (
        <p role="alert" className="text-2xs text-[var(--color-danger-text)]">
          {state.error}
        </p>
      )}

      <div className="flex flex-wrap gap-2">
        <Kpi label={t('kpi.working')} value={h.working} tone="ok" />
        <Kpi label={t('kpi.waiting')} value={h.waiting} tone="warn" />
        <Kpi label={t('kpi.idle')} value={h.idle} tone="accent" />
        <Kpi
          label={t('kpi.building')}
          value={machineCap === null ? h.building : `${h.building}/${machineCap}`}
        />
        <Kpi label={t('kpi.queued')} value={h.queued} tone={h.queued > 0 ? 'warn' : 'neutral'} />
        <Kpi label={t('kpi.stale')} value={h.stale} tone={h.stale > 0 ? 'danger' : 'neutral'} />
        <Kpi label={t('kpi.jobs')} value={h.runningJobs} />
        <Kpi label={t('kpi.leases')} value={h.leases} />
      </div>

      <Tabs
        value={tab}
        onValueChange={(v) => setTab(v as AgentsTab)}
        className="flex min-h-0 flex-1 flex-col gap-3"
      >
        <TabsList aria-label={t('title')} className="overflow-x-auto">
          {agentsTabs.map((id) => (
            <TabsTrigger key={id} value={id}>
              {t(`tab.${id}`)}
              {id === 'sessions' && <Count n={s.sessions.length} />}
              {id === 'queues' && <Count n={h.queued} />}
              {id === 'jobs' && h.suspectJobs > 0 && <Count n={h.suspectJobs} warn />}
              {id === 'cloud' && s.cloudDown.length > 0 && <Count n={s.cloudDown.length} warn />}
            </TabsTrigger>
          ))}
        </TabsList>
        <div className="screen-body">
          <AnimatePresence mode="wait" initial={false}>
            <m.div
              key={tab}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.18 }}
              className="flex min-h-0 flex-col gap-3"
            >
              {/* Radix mounts only the active panel, so hidden tabs cost nothing. */}
              <TabsContent value="overview" className="flex flex-col gap-4">
                <SessionsPanel snapshot={s} now={now} limit="live" />
                <QueuesPanel snapshot={s} now={now} onAction={request} />
              </TabsContent>
              <TabsContent value="sessions">
                <SessionsPanel snapshot={s} now={now} />
              </TabsContent>
              <TabsContent value="queues">
                <QueuesPanel snapshot={s} now={now} onAction={request} />
              </TabsContent>
              <TabsContent value="worktrees">
                <WorktreesPanel leases={s.leases} now={now} onAction={request} />
              </TabsContent>
              <TabsContent value="jobs">
                <JobsPanel snapshot={s} now={now} onAction={request} />
              </TabsContent>
              <TabsContent value="journal" className="flex min-h-0 flex-1 flex-col">
                <JournalPanel events={s.journal} now={now} />
              </TabsContent>
              <TabsContent value="cloud">
                <CloudPanel snapshot={s} account={<CloudAccount />} />
              </TabsContent>
              <TabsContent value="insights">
                <InsightsPanel snapshot={s} />
              </TabsContent>
            </m.div>
          </AnimatePresence>
        </div>
      </Tabs>

      <DialogRoot open={pending !== null} onOpenChange={(open) => !open && setPending(null)}>
        {pending !== null && (
          <DialogContent
            title={pending.title}
            closeLabel={t('action.cancel')}
            footer={
              <div className="flex justify-end gap-2">
                <Button variant="ghost" onClick={() => setPending(null)}>
                  {t('action.cancel')}
                </Button>
                <Button
                  disabled={busy || reason.trim().length < 4}
                  onClick={() => {
                    const action =
                      typeof pending.build === 'function'
                        ? pending.build(reason.trim())
                        : pending.build;
                    void execute(action).then((ok) => ok && setPending(null));
                  }}
                >
                  {t('action.run')}
                </Button>
              </div>
            }
          >
            <label className="flex flex-col gap-1 text-2xs">
              {t('action.reason')}
              <Input autoFocus value={reason} onChange={(e) => setReason(e.target.value)} />
            </label>
          </DialogContent>
        )}
      </DialogRoot>
    </div>
  );
}

function Count({ n, warn = false }: { readonly n: number; readonly warn?: boolean }) {
  if (n === 0) return null;
  return (
    <m.span
      key={n}
      initial={{ scale: 0.6 }}
      animate={{ scale: 1 }}
      className={
        warn
          ? 'rounded-full bg-[var(--color-status-warn)]/20 px-1.5 text-[10px] text-[var(--color-warn-text)]'
          : 'rounded-full bg-[var(--color-bg-subtle)] px-1.5 text-[10px] text-[var(--color-fg-muted)]'
      }
    >
      {n}
    </m.span>
  );
}

function AgentsSkeleton() {
  return (
    <div className="screen" aria-busy="true">
      <Skeleton className="h-6 w-48" />
      <div className="flex gap-2">
        {Array.from({ length: 6 }, (_, i) => (
          <Skeleton key={i} className="h-16 flex-1" />
        ))}
      </div>
      <Skeleton className="h-64 w-full" />
    </div>
  );
}
