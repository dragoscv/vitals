/**
 * Pure logic for the AI page: grouping, joins and formatting decisions that
 * do not need React. Kept here so they are unit-tested without rendering.
 */

import type {
  AgentClient,
  AgentSession,
  AgentqCaller,
  AgentqLease,
  AgentqResource,
  AgentqTicket,
  AgentsSnapshot,
  SessionState,
} from '@vitals/protocol';

export type AgentsTab =
  'overview' | 'sessions' | 'queues' | 'worktrees' | 'jobs' | 'journal' | 'cloud' | 'insights';

export const agentsTabs: readonly AgentsTab[] = [
  'overview',
  'sessions',
  'queues',
  'worktrees',
  'jobs',
  'journal',
  'cloud',
  'insights',
];

/** Headline numbers for the overview strip. */
export interface Headline {
  readonly working: number;
  readonly waiting: number;
  readonly idle: number;
  readonly building: number;
  readonly queued: number;
  readonly stale: number;
  readonly frozen: number;
  readonly runningJobs: number;
  readonly suspectJobs: number;
  readonly leases: number;
  readonly staleLeases: number;
}

export function headline(s: AgentsSnapshot): Headline {
  const tickets = s.resources.flatMap((r) => r.tickets);
  const count = (st: SessionState) => s.sessions.filter((x) => x.state === st).length;
  return {
    working: count('working'),
    waiting: count('waiting'),
    idle: count('idle'),
    building: tickets.filter((t) => t.held && t.resource.startsWith('build:')).length,
    queued: tickets.filter((t) => !t.held).length,
    stale: tickets.filter((t) => t.liveness === 'stale' || t.liveness === 'dead').length,
    frozen: s.marks.length,
    runningJobs: s.jobs.filter((j) => j.state === 'running').length,
    suspectJobs: s.jobs.filter((j) => j.state === 'suspect' || j.state === 'dead').length,
    leases: s.leases.length,
    staleLeases: s.leases.filter((l) => l.stale).length,
  };
}

/** Sessions grouped by client, busiest client first. */
export function byClient(
  sessions: readonly AgentSession[],
): readonly { client: AgentClient; sessions: readonly AgentSession[] }[] {
  const m = new Map<AgentClient, AgentSession[]>();
  for (const s of sessions) {
    const list = m.get(s.client) ?? [];
    list.push(s);
    m.set(s.client, list);
  }
  return [...m.entries()]
    .map(([client, list]) => ({ client, sessions: list }))
    .sort((a, b) => live(b.sessions) - live(a.sessions) || b.sessions.length - a.sessions.length);
}

function live(list: readonly AgentSession[]): number {
  return list.filter((s) => s.state === 'working' || s.state === 'waiting').length;
}

/**
 * What agentq is doing for a session: its tickets and leases, joined through
 * the registry (session id → shell/app) and the repo the session works in.
 */
export interface SessionWork {
  readonly tickets: readonly AgentqTicket[];
  readonly leases: readonly AgentqLease[];
  readonly caller: AgentqCaller | undefined;
}

export function workFor(session: AgentSession, s: AgentsSnapshot): SessionWork {
  const repo = repoName(session.repo ?? session.workspace);
  const callers = s.registry.filter(
    (c) =>
      (session.pid !== null && (c.appPid === session.pid || c.shellPid === session.pid)) ||
      (repo !== null && repoName(c.repo ?? c.cwd) === repo),
  );
  const sessionIds = new Set(callers.map((c) => c.session));
  const tickets = s.resources
    .flatMap((r) => r.tickets)
    .filter((t) => t.session !== null && sessionIds.has(t.session));
  const leases = s.leases.filter((l) => l.session !== null && sessionIds.has(l.session));
  return { tickets, leases, caller: callers[0] };
}

/** `dragoscv/vitals`, `E:\gh\vitals`, `vitals` → `vitals`. */
export function repoName(value: string | null | undefined): string | null {
  if (value === null || value === undefined || value === '') return null;
  const parts = value.split(/[\\/]/).filter((p) => p !== '');
  // a worktree slot E:\gh\.wt\codai\task-3 belongs to codai
  const wt = parts.indexOf('.wt');
  if (wt >= 0 && parts[wt + 1] !== undefined) return parts[wt + 1]!.toLowerCase();
  return parts.at(-1)?.toLowerCase() ?? null;
}

/** Share of the session's life spent working, 0..1, or null when unknown. */
export function activeShare(s: AgentSession, now: number): number | null {
  if (s.startedMs === null) return null;
  const end = s.state === 'ended' ? (s.lastActivityMs ?? now) : now;
  const life = end - s.startedMs;
  if (life <= 0) return null;
  return Math.min(1, s.activeMs / life);
}

/** Holders then waiters, with the slot count the UI draws. */
export function resourceSlots(r: AgentqResource): {
  readonly held: readonly AgentqTicket[];
  readonly waiting: readonly AgentqTicket[];
  readonly capacity: number;
} {
  const held = r.tickets.filter((t) => t.held);
  const waiting = r.tickets.filter((t) => !t.held);
  return { held, waiting, capacity: Math.max(r.capacity ?? 1, held.length) };
}

/** `1h 04m`, `12m 30s`, `45s`. */
export function formatDuration(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms) || ms < 0) return '—';
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`;
  if (m > 0) return `${m}m ${String(sec).padStart(2, '0')}s`;
  return `${sec}s`;
}

/** `ago` from a timestamp; `—` for unknown. */
export function since(ms: number | null, now: number): string {
  return ms === null ? '—' : formatDuration(Math.max(0, now - ms));
}

/** Compact token count: 1.2k, 3.4M. */
export function formatTokens(n: number | null): string {
  if (n === null) return '—';
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  return String(n);
}

/** Journal event → visual family, for colour and icon. */
export type EventTone = 'ok' | 'warn' | 'danger' | 'info' | 'neutral';

export function eventTone(event: string): EventTone {
  if (/fail|dead|break|timeout|giveup|expire|cancel/.test(event)) return 'danger';
  if (/mark|yield|wedged|burst|bump|preempt|superseded|fallback|cap/.test(event)) return 'warn';
  if (/done|commit|push$|land|release-end|acquired|lease$|cloud-done/.test(event)) return 'ok';
  if (/start|queued|placement|cloud-queued|release-begin|release-phase/.test(event)) return 'info';
  return 'neutral';
}
