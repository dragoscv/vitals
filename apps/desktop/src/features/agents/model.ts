/**
 * Pure logic for the AI page: grouping, joins and formatting decisions that
 * do not need React. Kept here so they are unit-tested without rendering.
 */

import type {
  AgentClient,
  AgentSession,
  AgentqAction,
  AgentqCaller,
  AgentqJob,
  AgentqLease,
  AgentqResource,
  AgentqTicket,
  AgentUsage,
  AgentUsageKind,
  AgentUsageSnapshot,
  AgentsSnapshot,
  RepoStat,
  SessionState,
} from '@vitals/protocol';

export type AgentsTab =
  | 'overview'
  | 'resources'
  | 'sessions'
  | 'queues'
  | 'worktrees'
  | 'actions'
  | 'jobs'
  | 'journal'
  | 'cloud'
  | 'insights';

export const agentsTabs: readonly AgentsTab[] = [
  'overview',
  // Straight after the overview: "what are the agents costing me" is the
  // question the page exists to answer, and it needs no agentq state to
  // answer it.
  'resources',
  'sessions',
  'queues',
  'worktrees',
  'actions',
  'jobs',
  'journal',
  'cloud',
  'insights',
];

/** One tool's row on the resources panel. */
export interface UsageRow {
  readonly key: AgentUsageKind;
  readonly usage: AgentUsage;
  /** agentq's own row: shown first and marked, because it is not a client. */
  readonly self: boolean;
}

/**
 * The rows to draw, agentq first and then the busiest client.
 *
 * A tool that owns no process has no row at all rather than a row of zeroes —
 * "not running" and "running, using nothing" are different facts, and a list
 * of zeroes reads as the second.
 */
export function usageRows(snapshot: AgentUsageSnapshot | null): readonly UsageRow[] {
  if (snapshot === null) return [];
  const clients: UsageRow[] = snapshot.clients.map((usage) => ({
    key: usage.kind,
    usage,
    self: false,
  }));
  const agentq = snapshot.agentq;
  return agentq === null ? clients : [{ key: agentq.kind, usage: agentq, self: true }, ...clients];
}

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

/*
 * Keys and coarse ranks for `useStableOrder`. A rank is a category whose
 * change is news and should move the item at once; it is never a live value
 * such as last activity or a ticket count, which change every push.
 */

const STATE_RANK: Record<SessionState, number> = { working: 0, waiting: 1, idle: 2, ended: 3 };

export const sessionKey = (s: AgentSession): string => s.id;
export const sessionRank = (s: AgentSession): number => STATE_RANK[s.state];

type ClientGroup = ReturnType<typeof byClient>[number];
export const groupKey = (g: ClientGroup): string => g.client;
/** Whether a client has anything live: a client waking up moves up at once. */
export const groupRank = (g: ClientGroup): number => (live(g.sessions) > 0 ? 0 : 1);

export const resourceKey = (r: AgentqResource): string => r.name;
export const resourceRank = (r: AgentqResource): number => (r.kind === 'machine' ? 0 : 1);

export const jobKey = (j: AgentqJob): string => j.id;
export const jobRank = (j: AgentqJob): number =>
  j.state === 'running' ? 0 : j.state === 'suspect' || j.state === 'dead' ? 1 : 2;

export const repoStatKey = (r: RepoStat): string => r.repo;

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
  if (s.startedMs === null || s.activeMs === null) return null;
  const end = s.state === 'ended' ? (s.lastActivityMs ?? now) : now;
  const life = end - s.startedMs;
  if (life <= 0) return null;
  return Math.min(1, s.activeMs / life);
}

/** A count the source may not report: a dash, never a made-up 0. */
export function formatCount(n: number | null): string {
  return n === null ? '—' : String(n);
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

/**
 * A money amount, or an em dash when the source prices nothing.
 *
 * A subscriber plan bills a flat fee and reports `0`, which is a real zero;
 * a source that reports no price at all gives `null`, which is not. The two
 * are never collapsed, exactly as with the token counts.
 */
export function formatCost(usd: number | null): string {
  if (usd === null || !Number.isFinite(usd)) return '—';
  if (usd === 0) return '$0';
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  if (usd < 1) return `$${usd.toFixed(3)}`;
  return `$${usd.toFixed(2)}`;
}

/** Output tokens per second, one decimal below 10 and none above. */
export function formatRate(perSec: number | null): string {
  if (perSec === null || !Number.isFinite(perSec)) return '—';
  return perSec >= 10 ? perSec.toFixed(0) : perSec.toFixed(1);
}

/** Everything a session reported about its own consumption. */
export interface UsageTotals {
  /** Sessions that reported at least one request. */
  readonly meters: number;
  readonly requests: number;
  readonly tokensIn: number;
  readonly tokensOut: number;
  readonly tokensCached: number;
  readonly tokensReasoning: number;
  readonly costUsd: number;
  readonly outputMs: number;
  /** Output tokens per second over every metered session, or null. */
  readonly tokensPerSec: number | null;
}

/**
 * Sums what the sessions reported, skipping the ones that reported nothing.
 *
 * A session with no usage is not a zero: adding it would drag a real average
 * down with a number nobody measured. The rate is recomputed from the summed
 * output and duration rather than averaging per-session rates, so one long
 * burst does not weigh the same as one short request.
 */
export function usageTotals(sessions: readonly AgentSession[]): UsageTotals {
  let meters = 0;
  let requests = 0;
  let tokensIn = 0;
  let tokensOut = 0;
  let tokensCached = 0;
  let tokensReasoning = 0;
  let costUsd = 0;
  let outputMs = 0;
  let anyCost = false;
  for (const s of sessions) {
    if (s.requests === null) continue;
    meters += 1;
    requests += s.requests;
    tokensIn += s.tokensIn ?? 0;
    tokensOut += s.tokensOut ?? 0;
    tokensCached += s.tokensCached ?? 0;
    tokensReasoning += s.tokensReasoning ?? 0;
    outputMs += s.outputMs ?? 0;
    if (s.costUsd !== null) {
      costUsd += s.costUsd;
      anyCost = true;
    }
  }
  return {
    meters,
    requests,
    tokensIn,
    tokensOut,
    tokensCached,
    tokensReasoning,
    costUsd: anyCost ? costUsd : 0,
    outputMs,
    tokensPerSec: outputMs > 0 ? (tokensOut * 1000) / outputMs : null,
  };
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

/*
 * The action builder.
 *
 * A verb plus a flat map of strings goes in, an `AgentqAction` comes out — or
 * `null` while a required field is still empty, which is what keeps the Run
 * button honest instead of letting it send something agentq will refuse. The
 * Rust planner validates again; this only spares the user a round trip.
 */

/** Every agentq verb the AI page can run. */
export type VerbId =
  | 'propose'
  | 'sweep'
  | 'reapTerminals'
  | 'freeze'
  | 'block'
  | 'clear'
  | 'break'
  | 'bump'
  | 'burst'
  | 'land'
  | 'run'
  | 'commit'
  | 'note'
  | 'unlease'
  | 'killJob';

export type FieldKind = 'text' | 'number' | 'area' | 'select' | 'toggle';

export interface FieldSpec {
  readonly key: string;
  readonly labelKey: string;
  readonly kind: FieldKind;
  readonly options?: readonly string[];
  readonly placeholder?: string;
  readonly helpKey?: string;
  /** A verb's Run button waits for these. */
  readonly required?: boolean;
  /** Rendered on its own row: a command, a path list. */
  readonly wide?: boolean;
  readonly rows?: number;
  /** Toggle fields only; the value used when the form opens. */
  readonly initial?: string;
}

export interface VerbSpec {
  readonly id: VerbId;
  readonly labelKey: string;
  readonly hintKey: string;
  /** `safe` never surprises, `careful` changes someone else's work, `danger` moves history. */
  readonly tone: 'safe' | 'careful' | 'danger';
  /** Asked before running, because the effect is not obvious from the preview. */
  readonly confirm: boolean;
  readonly fields: readonly FieldSpec[];
}

const REASON: FieldSpec = {
  key: 'reason',
  labelKey: 'field.reason',
  kind: 'text',
  placeholder: 'why',
  required: true,
  wide: true,
};

const DRY_RUN: FieldSpec = {
  key: 'dryRun',
  labelKey: 'field.dryRun',
  kind: 'toggle',
  helpKey: 'field.dryRunHelp',
  initial: 'true',
};

/**
 * The form every verb needs, in the order it is shown.
 *
 * The resource/ticket/lease/job fields are free text with a datalist of what
 * the snapshot currently holds, rather than a `<select>`: an agentq name is
 * not always something the snapshot has seen yet (a ticket queued a second
 * ago, a resource another machine holds), and refusing to name it would make
 * the page less capable than the command line it fronts.
 */
export const verbs: readonly VerbSpec[] = [
  {
    id: 'propose',
    labelKey: 'verb.propose',
    hintKey: 'verb.proposeHint',
    tone: 'safe',
    confirm: false,
    fields: [{ ...REASON, required: false }],
  },
  {
    id: 'sweep',
    labelKey: 'verb.sweep',
    hintKey: 'verb.sweepHint',
    tone: 'safe',
    confirm: true,
    fields: [DRY_RUN],
  },
  {
    id: 'reapTerminals',
    labelKey: 'verb.reapTerminals',
    hintKey: 'verb.reapTerminalsHint',
    tone: 'safe',
    confirm: true,
    fields: [DRY_RUN],
  },
  {
    id: 'freeze',
    labelKey: 'verb.freeze',
    hintKey: 'verb.freezeHint',
    tone: 'careful',
    confirm: false,
    fields: [{ key: 'resource', labelKey: 'field.resource', kind: 'text', required: true }, REASON],
  },
  {
    id: 'block',
    labelKey: 'verb.block',
    hintKey: 'verb.blockHint',
    tone: 'careful',
    confirm: false,
    fields: [{ key: 'resource', labelKey: 'field.resource', kind: 'text', required: true }, REASON],
  },
  {
    id: 'clear',
    labelKey: 'verb.clear',
    hintKey: 'verb.clearHint',
    tone: 'safe',
    confirm: false,
    fields: [{ key: 'resource', labelKey: 'field.resource', kind: 'text', required: true }, REASON],
  },
  {
    id: 'break',
    labelKey: 'verb.break',
    hintKey: 'verb.breakHint',
    tone: 'danger',
    confirm: true,
    fields: [
      { key: 'resource', labelKey: 'field.resource', kind: 'text', required: true },
      { key: 'ticket', labelKey: 'field.ticket', kind: 'text', required: true },
      REASON,
    ],
  },
  {
    id: 'bump',
    labelKey: 'verb.bump',
    hintKey: 'verb.bumpHint',
    tone: 'safe',
    confirm: false,
    fields: [
      { key: 'ticket', labelKey: 'field.ticket', kind: 'text' },
      { key: 'by', labelKey: 'field.by', kind: 'number', placeholder: '20', initial: '20' },
      REASON,
    ],
  },
  {
    id: 'burst',
    labelKey: 'verb.burst',
    hintKey: 'verb.burstHint',
    tone: 'safe',
    confirm: false,
    fields: [
      {
        key: 'resource',
        labelKey: 'field.resource',
        kind: 'text',
        placeholder: 'build:machine',
      },
      { key: 'by', labelKey: 'field.by', kind: 'number', placeholder: '1', initial: '1' },
      {
        key: 'minutes',
        labelKey: 'field.minutes',
        kind: 'number',
        placeholder: '30',
        initial: '30',
      },
      REASON,
    ],
  },
  {
    id: 'land',
    labelKey: 'verb.land',
    hintKey: 'verb.landHint',
    tone: 'danger',
    confirm: true,
    fields: [
      { key: 'repo', labelKey: 'field.repo', kind: 'text', required: true, wide: true },
      { key: 'branch', labelKey: 'field.branch', kind: 'text', required: true },
      {
        key: 'onto',
        labelKey: 'field.onto',
        kind: 'select',
        options: ['main', 'dev'],
        initial: 'main',
      },
      { key: 'purpose', labelKey: 'field.purpose', kind: 'text', required: true, wide: true },
      { key: 'gate', labelKey: 'field.gate', kind: 'text', wide: true },
    ],
  },
  {
    id: 'run',
    labelKey: 'verb.run',
    hintKey: 'verb.runHint',
    tone: 'careful',
    confirm: true,
    fields: [
      {
        key: 'resource',
        labelKey: 'field.resource',
        kind: 'text',
        required: true,
        placeholder: 'build:codai',
      },
      { key: 'purpose', labelKey: 'field.purpose', kind: 'text', required: true },
      {
        key: 'lane',
        labelKey: 'field.lane',
        kind: 'select',
        options: ['', 'quick', 'gate', 'heavy', 'batch'],
      },
      {
        key: 'place',
        labelKey: 'field.place',
        kind: 'select',
        options: ['', 'local', 'cloud', 'auto'],
      },
      { key: 'repo', labelKey: 'field.repo', kind: 'text', wide: true },
      { key: 'command', labelKey: 'field.command', kind: 'area', rows: 3, wide: true },
    ],
  },
  {
    id: 'commit',
    labelKey: 'verb.commit',
    hintKey: 'verb.commitHint',
    tone: 'danger',
    confirm: true,
    fields: [
      { key: 'repo', labelKey: 'field.repo', kind: 'text', required: true, wide: true },
      { key: 'purpose', labelKey: 'field.purpose', kind: 'text', required: true },
      { key: 'message', labelKey: 'field.message', kind: 'text', required: true, wide: true },
      {
        key: 'paths',
        labelKey: 'field.paths',
        kind: 'area',
        rows: 3,
        required: true,
        wide: true,
        helpKey: 'field.pathsHelp',
      },
    ],
  },
  {
    id: 'note',
    labelKey: 'verb.note',
    hintKey: 'verb.noteHint',
    tone: 'safe',
    confirm: false,
    fields: [
      { key: 'message', labelKey: 'field.message', kind: 'text', required: true, wide: true },
    ],
  },
  {
    id: 'unlease',
    labelKey: 'verb.unlease',
    hintKey: 'verb.unleaseHint',
    tone: 'careful',
    confirm: true,
    fields: [
      { key: 'path', labelKey: 'field.path', kind: 'text', required: true, wide: true },
      { key: 'leaseId', labelKey: 'field.leaseId', kind: 'text', required: true },
      REASON,
    ],
  },
  {
    id: 'killJob',
    labelKey: 'verb.killJob',
    hintKey: 'verb.killJobHint',
    tone: 'danger',
    confirm: true,
    fields: [{ key: 'job', labelKey: 'field.job', kind: 'text', required: true }],
  },
];

export function verbSpec(id: VerbId): VerbSpec {
  const spec = verbs.find((v) => v.id === id);
  if (!spec) throw new Error(`unknown verb ${id}`);
  return spec;
}

/** The opening value of every field of a verb. */
export function initialFields(id: VerbId): Record<string, string> {
  const out: Record<string, string> = {};
  for (const f of verbSpec(id).fields) out[f.key] = f.initial ?? '';
  return out;
}

const text = (v: string | undefined): string => (v ?? '').trim();
const optional = (v: string | undefined): string | null => (text(v) === '' ? null : text(v));
const num = (v: string | undefined, fallback: number): number => {
  const n = Number.parseInt((v ?? '').trim(), 10);
  return Number.isFinite(n) ? n : fallback;
};

/**
 * Turns the form into an action, or `null` when a required field is empty.
 *
 * Optional flags are sent as `null` rather than omitted, because the wire type
 * says they are nullable and a missing key is not the same fact to a
 * serializer that has a default.
 */
export function buildAction(id: VerbId, f: Readonly<Record<string, string>>): AgentqAction | null {
  for (const field of verbSpec(id).fields) {
    if (field.required && text(f[field.key]) === '') return null;
  }
  switch (id) {
    case 'propose':
      return { kind: 'propose', reason: optional(f.reason) };
    case 'sweep':
      return { kind: 'sweep', dryRun: f.dryRun !== 'false' };
    case 'reapTerminals':
      return { kind: 'reapTerminals', dryRun: f.dryRun !== 'false' };
    case 'freeze':
      return { kind: 'freeze', resource: text(f.resource), reason: text(f.reason) };
    case 'block':
      return { kind: 'block', resource: text(f.resource), reason: text(f.reason) };
    case 'clear':
      return { kind: 'clear', resource: text(f.resource), reason: text(f.reason) };
    case 'break':
      return {
        kind: 'break',
        resource: text(f.resource),
        ticket: text(f.ticket),
        reason: text(f.reason),
      };
    case 'bump':
      return {
        kind: 'bump',
        ticket: optional(f.ticket),
        by: num(f.by, 20),
        reason: text(f.reason),
      };
    case 'burst':
      return {
        kind: 'burst',
        resource: optional(f.resource),
        by: num(f.by, 1),
        minutes: num(f.minutes, 30),
        reason: text(f.reason),
      };
    case 'land':
      return {
        kind: 'land',
        repo: text(f.repo),
        branch: text(f.branch),
        onto: text(f.onto) === '' ? 'main' : text(f.onto),
        purpose: text(f.purpose),
        gate: optional(f.gate),
      };
    case 'run':
      return {
        kind: 'run',
        resource: text(f.resource),
        purpose: text(f.purpose),
        command: optional(f.command),
        lane: optional(f.lane),
        place: optional(f.place),
        repo: optional(f.repo),
      };
    case 'commit':
      return {
        kind: 'commit',
        repo: text(f.repo),
        purpose: text(f.purpose),
        message: text(f.message),
        paths: (f.paths ?? '')
          .split('\n')
          .map((p) => p.trim())
          .filter((p) => p !== ''),
      };
    case 'note':
      return { kind: 'note', message: text(f.message) };
    case 'unlease':
      return {
        kind: 'unlease',
        path: text(f.path),
        leaseId: text(f.leaseId),
        reason: text(f.reason),
      };
    case 'killJob':
      return { kind: 'killJob', job: text(f.job) };
  }
}

/**
 * A one-line preview of the command an action becomes.
 *
 * Descriptive, not authoritative: the Rust planner owns the real argument
 * vector, and this exists so the user can read what they are about to run.
 * Every value is one argument, so the quoting here is only for legibility —
 * nothing in this application ever joins an action into a shell string.
 */
export function actionPreview(action: AgentqAction): string {
  const q = (v: string): string => (/\s/.test(v) ? `"${v}"` : v);
  const opt = (flag: string, v: string | null): string => (v === null ? '' : ` ${flag} ${q(v)}`);
  switch (action.kind) {
    case 'freeze':
    case 'block':
    case 'clear':
      return `agentq ${action.kind} -Resource ${q(action.resource)} -Reason ${q(action.reason)}`;
    case 'break':
      return `agentq break -Resource ${q(action.resource)} -Id ${action.ticket} -Reason ${q(action.reason)}`;
    case 'note':
      return `agentq note -Message ${q(action.message)}`;
    case 'unlease':
      return `agentq unlease -Repo ${q(action.path)} -LeaseId ${action.leaseId} -Reason ${q(action.reason)}`;
    case 'killJob':
      return `jobs.ps1 kill ${action.job}`;
    case 'reapTerminals':
      return `reap-terminals.ps1 -Json${action.dryRun ? ' -WhatIf' : ''}`;
    case 'propose':
      return `agentq propose${opt('-Reason', action.reason)}`;
    case 'sweep':
      return `agentq sweep -Json${action.dryRun ? ' -WhatIf' : ''}`;
    case 'bump':
      return `agentq bump${action.ticket === null ? '' : ` -Id ${action.ticket}`} -By ${action.by} -Reason ${q(action.reason)}`;
    case 'burst':
      return `agentq burst${opt('-Resource', action.resource)} -By ${action.by} -Minutes ${action.minutes} -Reason ${q(action.reason)}`;
    case 'land':
      return `agentq land -Branch ${action.branch} -Onto ${action.onto} -Purpose ${q(action.purpose)}${opt('-Gate', action.gate)}  (in ${action.repo})`;
    case 'run':
      return `agentq run -Resource ${action.resource} -Purpose ${q(action.purpose)}${opt('-Lane', action.lane)}${opt('-Where', action.place)}${action.command === null ? '' : ` -- ${action.command}`}${action.repo === null ? '' : `  (in ${action.repo})`}`;
    case 'commit':
      return `agentq commit -Purpose ${q(action.purpose)} -Message ${q(action.message)} -Paths ${action.paths.join(',')}  (in ${action.repo})`;
  }
}

/** A clone guessed from a leased slot, e.g. `E:\gh\.wt\codai\task-3` → `E:\gh\codai`. */
export function cloneFromSlot(slot: string | null, repo: string): string | null {
  if (slot === null) return null;
  const parts = slot.split(/[\\/]+/).filter((p) => p !== '');
  if (parts.length < 4) return null;
  if (parts[parts.length - 3] !== '.wt' || parts[parts.length - 2] !== repo) return null;
  return [...parts.slice(0, parts.length - 3), repo].join('\\');
}

/**
 * Clone directories this machine is known to work in, best guess first.
 *
 * Both sources are facts rather than guesses: a registry entry records the
 * directory a caller ran agentq from, and a lease records the slot it was
 * given. The main clone beside `.wt` is derived from the slot's shape, which
 * the worktree ADR fixes — a slot that does not have that shape yields nothing
 * rather than a plausible-looking wrong path.
 */
export function repoCandidates(s: AgentsSnapshot): readonly string[] {
  const out: string[] = [];
  const add = (p: string | null | undefined): void => {
    if (p !== null && p !== undefined && /[\\/]/.test(p) && !out.includes(p)) out.push(p);
  };
  for (const c of s.registry) {
    add(c.repo);
    add(c.cwd);
  }
  for (const l of s.leases) add(cloneFromSlot(l.path, l.repo));
  return out;
}

/** Something worth offering before the user goes looking for it. */
export interface Suggestion {
  readonly id: 'propose' | 'sweep' | 'bump' | 'clear';
  /** Values for `suggestion.<id>` interpolation. */
  readonly params: Readonly<Record<string, string | number>>;
  readonly action: AgentqAction;
}

/**
 * What this snapshot suggests doing, most urgent first.
 *
 * Deliberately narrow: every suggestion is safe to accept, and each names a
 * fact the user can see on the page — a stale holder, an expired slot, a queue
 * nobody is moving. Nothing here destroys work; the verbs that could still ask
 * first, through the same confirm dialog every other use of them goes through.
 */
export function suggestions(s: AgentsSnapshot, now: number): readonly Suggestion[] {
  const out: Suggestion[] = [];
  const tickets = s.resources.flatMap((r) => r.tickets);

  const stuck = tickets.filter((t) => t.liveness === 'stale' || t.liveness === 'dead');
  if (stuck.length > 0) {
    out.push({
      id: 'propose',
      // `holders`, not `count`: a `count` key makes i18next look for plural
      // forms (`suggest.propose_one`), and these strings read correctly for
      // any number.
      params: { holders: stuck.length },
      action: { kind: 'propose', reason: `vitals: ${stuck.length} stale holder(s)` },
    });
  }

  const expired = s.leases.filter((l) => l.stale);
  if (expired.length > 0) {
    out.push({
      id: 'sweep',
      params: { slots: expired.length },
      action: { kind: 'sweep', dryRun: false },
    });
  }

  // A queue whose front ticket has waited a while and is not moving. Fifteen
  // minutes is where a build lock stops looking like work and starts looking
  // like a serialisation that is stuck.
  const slowest = tickets
    .filter((t) => !t.held && t.createdMs !== null && t.priority !== null)
    .map((t) => ({ t, waited: now - (t.createdMs ?? now) }))
    .filter((x) => x.waited > 15 * 60_000)
    .sort((a, b) => b.waited - a.waited)[0];
  if (slowest) {
    out.push({
      id: 'bump',
      params: { resource: slowest.t.resource, minutes: Math.round(slowest.waited / 60_000) },
      action: { kind: 'bump', ticket: slowest.t.id, by: 20, reason: 'vitals: long wait' },
    });
  }

  const mark = s.marks[0];
  if (mark) {
    out.push({
      id: 'clear',
      params: { resource: mark.resource },
      action: { kind: 'clear', resource: mark.resource, reason: 'vitals: clear the mark' },
    });
  }

  return out.slice(0, 4);
}
