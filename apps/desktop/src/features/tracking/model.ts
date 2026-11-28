/**
 * Pure logic for the Tracking page: reading the cross-repo tracker index that
 * agentq's `node tools/tracker-lint.mjs --scan` writes, and the sums, groupings
 * and warnings the screen draws from it.
 *
 * # Why the index is typed here rather than in `@vitals/protocol`
 *
 * The protocol package holds the wire types this backend *owns* — generated
 * from Rust with ts-rs so the two sides cannot drift. This index is not ours:
 * it is a rebuildable view written by a JavaScript tool on the machine, whose
 * single source of truth is the per-repo tracker file it was built from. So the
 * backend hands the file through verbatim (`agents::get_tracker_index`) and the
 * shape is parsed here, tolerantly, exactly as `get_cloud_status`'s answer is
 * typed at its call site.
 *
 * # Tolerant by construction
 *
 * The reader refuses a malformed file rather than inventing a plausible one: a
 * missing `id` drops that item, a tracker with no items is still shown, and a
 * body that is not an index at all parses to `null` (the page's empty state).
 * A viewer that rendered a half-parsed file as if it were whole would be worse
 * than one that said "no scan yet".
 */

/** The status vocabulary tracker-lint maps every tracker onto (`CANONICAL`). */
export const trackerStatuses = [
  'todo',
  'in-progress',
  'blocked',
  'review',
  'done',
  'dropped',
  'deferred',
] as const;

export type TrackerStatus = (typeof trackerStatuses)[number];

/**
 * Statuses that mean "finished with", so an open count can be derived from the
 * counts without listing them. Anything off-vocabulary counts as open: a
 * status nobody recognises is a row a human still has to act on.
 */
const CLOSED_STATUSES: ReadonlySet<string> = new Set(['done', 'dropped', 'deferred']);

/** One row of a tracker. */
export interface TrackerItem {
  readonly id: string;
  readonly title: string;
  readonly status: string;
  /** Optional in the schema; only present when the source had a non-empty one. */
  readonly group?: string;
  readonly evidence?: string;
  readonly notes?: string;
}

/** One tracker file of one repo, with its pre-computed per-status counts. */
export interface Tracker {
  readonly id: string;
  readonly file: string;
  readonly countsByStatus: Readonly<Record<string, number>>;
  readonly items: readonly TrackerItem[];
}

/** One drift finding: a tracker that no longer matches the conventions. */
export interface TrackerDrift {
  readonly kind: string;
  readonly tracker: string;
  readonly ref: string;
  readonly detail: string;
}

/** The branch and commit head the scan recorded for a repo, if it is a clone. */
export interface TrackerRepoGit {
  readonly branch: string;
  readonly head: string;
}

/** One repository in the index. */
export interface TrackerRepo {
  readonly repo: string;
  readonly root: string;
  readonly git: TrackerRepoGit | null;
  readonly trackers: readonly Tracker[];
  readonly drift: readonly TrackerDrift[];
}

/** The whole cross-repo index. */
export interface TrackerIndex {
  readonly version: number;
  readonly generatedAt: string;
  readonly repos: readonly TrackerRepo[];
}

/** A per-status tally, keyed by the raw status string the tracker used. */
export type StatusCounts = Record<string, number>;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function str(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

function num(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function parseCounts(raw: unknown): StatusCounts {
  const out: StatusCounts = {};
  if (!isRecord(raw)) return out;
  for (const [key, value] of Object.entries(raw)) {
    const n = num(value);
    // A zero is not a fact worth drawing; the chip is only there when a status
    // actually has rows, so a status that fell to zero leaves no trace.
    if (n > 0) out[key] = n;
  }
  return out;
}

function parseItem(raw: unknown): TrackerItem | null {
  if (!isRecord(raw)) return null;
  const id = str(raw['id']);
  if (id === '') return null;
  const group = str(raw['group']);
  const evidence = str(raw['evidence']);
  const notes = str(raw['notes']);
  return {
    id,
    title: str(raw['title']),
    status: str(raw['status'], 'todo'),
    // `exactOptionalPropertyTypes` is on: an optional key is added only when it
    // has a value, never set to `undefined`.
    ...(group !== '' && { group }),
    ...(evidence !== '' && { evidence }),
    ...(notes !== '' && { notes }),
  };
}

function parseTracker(raw: unknown): Tracker | null {
  if (!isRecord(raw)) return null;
  const id = str(raw['id']);
  if (id === '') return null;
  const items = Array.isArray(raw['items'])
    ? raw['items'].map(parseItem).filter((item): item is TrackerItem => item !== null)
    : [];
  return {
    id,
    file: str(raw['file']),
    countsByStatus: parseCounts(raw['countsByStatus']),
    items,
  };
}

function parseDrift(raw: unknown): TrackerDrift | null {
  if (!isRecord(raw)) return null;
  return {
    kind: str(raw['kind']),
    tracker: str(raw['tracker']),
    ref: str(raw['ref']),
    detail: str(raw['detail']),
  };
}

function parseRepo(raw: unknown): TrackerRepo | null {
  if (!isRecord(raw)) return null;
  const repo = str(raw['repo']);
  if (repo === '') return null;
  const gitRaw = raw['git'];
  const git =
    isRecord(gitRaw) && str(gitRaw['branch']) !== ''
      ? { branch: str(gitRaw['branch']), head: str(gitRaw['head']) }
      : null;
  const trackers = Array.isArray(raw['trackers'])
    ? raw['trackers'].map(parseTracker).filter((t): t is Tracker => t !== null)
    : [];
  const drift = Array.isArray(raw['drift'])
    ? raw['drift'].map(parseDrift).filter((d): d is TrackerDrift => d !== null)
    : [];
  return { repo, root: str(raw['root']), git, trackers, drift };
}

/**
 * Reads one index body into the typed shape, or `null` when it is not an index.
 *
 * `null` is deliberately the same answer for "the file was absent" (the host
 * command returns `null`) and "the file was there but was not an index": both
 * mean the page has nothing trustworthy to draw, so both show the empty state
 * that names the command which produces one.
 */
export function parseIndex(raw: unknown): TrackerIndex | null {
  if (!isRecord(raw)) return null;
  const reposRaw = raw['repos'];
  if (!Array.isArray(reposRaw)) return null;
  const repos = reposRaw.map(parseRepo).filter((r): r is TrackerRepo => r !== null);
  return {
    version: num(raw['version']),
    generatedAt: str(raw['generatedAt']),
    repos,
  };
}

/** Every tracker in the index, flattened. */
export function allTrackers(index: TrackerIndex): readonly Tracker[] {
  return index.repos.flatMap((repo) => repo.trackers);
}

/** Adds a tally into an accumulator, in place. */
function addInto(into: StatusCounts, counts: Readonly<Record<string, number>>): void {
  for (const [status, n] of Object.entries(counts)) {
    into[status] = (into[status] ?? 0) + n;
  }
}

/** The per-status totals for one repo, summed across its trackers. */
export function repoStatusCounts(repo: TrackerRepo): StatusCounts {
  const out: StatusCounts = {};
  for (const tracker of repo.trackers) addInto(out, tracker.countsByStatus);
  return out;
}

/** The per-status totals across the whole index. */
export function indexStatusCounts(index: TrackerIndex): StatusCounts {
  const out: StatusCounts = {};
  for (const repo of index.repos) addInto(out, repoStatusCounts(repo));
  return out;
}

/** Rows still to act on: every status that is not done, dropped or deferred. */
export function openCount(counts: Readonly<Record<string, number>>): number {
  let total = 0;
  for (const [status, n] of Object.entries(counts)) {
    if (!CLOSED_STATUSES.has(status)) total += n;
  }
  return total;
}

/** The statuses present in a tally, most rows first, then alphabetically. */
export function sortedStatuses(counts: Readonly<Record<string, number>>): readonly string[] {
  return Object.keys(counts).sort((a, b) => {
    const delta = (counts[b] ?? 0) - (counts[a] ?? 0);
    return delta !== 0 ? delta : a.localeCompare(b);
  });
}

/** How many rows a repo holds, across its trackers. */
export function repoItemCount(repo: TrackerRepo): number {
  return repo.trackers.reduce((n, tracker) => n + tracker.items.length, 0);
}

/** Whether a repo has any drift finding — the guard the warnings section reads. */
export function hasDrift(repo: TrackerRepo): boolean {
  return repo.drift.length > 0;
}

export interface IndexTotals {
  readonly repos: number;
  readonly reposWithDrift: number;
  readonly trackers: number;
  readonly items: number;
  readonly drift: number;
  readonly byStatus: StatusCounts;
}

/** The headline numbers the KPI strip and the warnings draw from. */
export function indexTotals(index: TrackerIndex): IndexTotals {
  let trackers = 0;
  let items = 0;
  let drift = 0;
  let reposWithDrift = 0;
  for (const repo of index.repos) {
    trackers += repo.trackers.length;
    items += repoItemCount(repo);
    drift += repo.drift.length;
    if (hasDrift(repo)) reposWithDrift += 1;
  }
  return {
    repos: index.repos.length,
    reposWithDrift,
    trackers,
    items,
    drift,
    byStatus: indexStatusCounts(index),
  };
}

/** One group of items under a shared group label, or under no label at all. */
export interface ItemGroup {
  /** The tracker's `group` value, or `null` for items that had none. */
  readonly group: string | null;
  readonly items: readonly TrackerItem[];
}

/**
 * Groups a tracker's items by their `group` field, preserving first-seen order.
 *
 * Order-preserving rather than sorted: the source lists groups in the order a
 * human wrote them, and re-sorting would silently reorder someone's plan.
 * Ungrouped items collect under a single `null` group so the caller can decide
 * whether to render a heading for them.
 */
export function groupItems(items: readonly TrackerItem[]): readonly ItemGroup[] {
  const order: (string | null)[] = [];
  const buckets = new Map<string | null, TrackerItem[]>();
  for (const item of items) {
    const key = item.group ?? null;
    let bucket = buckets.get(key);
    if (bucket === undefined) {
      bucket = [];
      buckets.set(key, bucket);
      order.push(key);
    }
    bucket.push(item);
  }
  return order.map((group) => ({ group, items: buckets.get(group) ?? [] }));
}

/** The Badge tone a status maps onto; the label always carries the meaning too. */
export type StatusTone = 'neutral' | 'accent' | 'ok' | 'warn' | 'danger' | 'info';

const STATUS_TONE: Readonly<Record<string, StatusTone>> = {
  done: 'ok',
  'in-progress': 'accent',
  claimed: 'accent',
  review: 'info',
  blocked: 'danger',
  todo: 'warn',
  dropped: 'neutral',
  deferred: 'neutral',
};

/** A tone per status; an unknown status is neutral rather than a colour claim. */
export function statusTone(status: string): StatusTone {
  return STATUS_TONE[status] ?? 'neutral';
}

/** Whether a status is one we have a label for (so the raw value is not shown). */
export function isKnownStatus(status: string): status is TrackerStatus {
  return (trackerStatuses as readonly string[]).includes(status);
}

/** Parses the index's ISO `generatedAt` into epoch ms, or `null` if unusable. */
export function generatedAtMs(index: TrackerIndex): number | null {
  if (index.generatedAt === '') return null;
  const ms = Date.parse(index.generatedAt);
  return Number.isFinite(ms) ? ms : null;
}

/**
 * The branch name as a person knows it, from the ref the scanner recorded.
 *
 * The real index holds `refs/heads/main`, because that is what git prints for a
 * symbolic ref; eleven characters of prefix on every card is noise, and nobody
 * calls their branch `refs/heads/main`. Only that prefix is removed, and only
 * when something is left behind — a detached head is a sha, and anything else
 * the scanner might write is shown exactly as it was recorded rather than
 * guessed at. Display only: the index still holds the full ref.
 */
export function shortRef(branch: string): string {
  const stripped = branch.replace(/^refs\/heads\//, '');
  return stripped === '' ? branch : stripped;
}

/** `1h 04m`, `12m 30s`, `45s` — the age of a scan, for the header line. */
export function formatAge(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms) || ms < 0) return '—';
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`;
  if (m > 0) return `${m}m ${String(sec).padStart(2, '0')}s`;
  return `${sec}s`;
}

// ---------------------------------------------------------------------------
// The resume pack: a session's plan and the claimable task store
// ---------------------------------------------------------------------------
//
// Where the tracker index is a snapshot of what *repositories* say, the resume
// pack is what a *session* is carrying: its goal, the steps still open, and the
// tasks it (and its subagents) have claimed. The shapes belong to `track.mjs`,
// so — exactly as for the index — they are parsed here tolerantly rather than
// trusted, and a body that is not a plan or a task drops out rather than
// rendering a half-parsed row.

/** The status vocabulary a task uses (`track.mjs`'s planTask.status). */
export const taskStatuses = [
  'todo',
  'claimed',
  'in-progress',
  'blocked',
  'review',
  'done',
  'dropped',
] as const;

export type TaskStatus = (typeof taskStatuses)[number];

/** Whether a task status is one we have a label for (so the raw value is not shown). */
export function isKnownTaskStatus(status: string): status is TaskStatus {
  return (taskStatuses as readonly string[]).includes(status);
}

/** Statuses a claim may move out of, mirroring `track.mjs`'s CLAIMABLE. */
const CLAIMABLE: ReadonlySet<string> = new Set(['todo', 'claimed', 'in-progress']);
/** Statuses that mean "finished with", so a task is no longer open. */
const TASK_CLOSED: ReadonlySet<string> = new Set(['done', 'dropped']);

/** One step in a session plan's `next` list. */
export interface PlanNextStep {
  readonly title: string;
  /** '' when the step carried no status, which `track.mjs` reads as open. */
  readonly status: string;
  readonly owner?: string;
  readonly note?: string;
}

/** A subagent a session dispatched (or planned to). */
export interface PlanRosterEntry {
  readonly agentName: string;
  readonly purpose: string;
  readonly status: string;
}

/** One claimable task (`<state>/tasks/<id>.json`). */
export interface PlanTask {
  readonly id: string;
  readonly title: string;
  readonly status: string;
  /** The session that claimed it, absent until someone does. */
  readonly owner?: string;
  readonly group?: string;
  readonly evidence?: string;
}

/** A session plan (`<state>/chats/<session>.plan.json`). */
export interface ResumePlan {
  readonly goal: string;
  readonly doneMeans: readonly string[];
  readonly next: readonly PlanNextStep[];
  readonly roster: readonly PlanRosterEntry[];
  readonly handoff?: string;
  readonly handoffPath?: string;
}

/**
 * The resume pack the host reads: the session in hand, its plan, and every
 * task in the store. Every part is optional-shaped, because a machine that has
 * written nothing is a normal state with its own screen.
 */
export interface ResumePack {
  readonly sessionId: string | null;
  readonly plan: ResumePlan | null;
  readonly tasks: readonly PlanTask[];
}

function strList(raw: unknown): readonly string[] {
  return Array.isArray(raw) ? raw.filter((x): x is string => typeof x === 'string') : [];
}

function parseNextStep(raw: unknown): PlanNextStep | null {
  if (!isRecord(raw)) return null;
  const title = str(raw['title']);
  if (title === '') return null;
  const owner = str(raw['owner']);
  const note = str(raw['note']);
  return {
    title,
    status: str(raw['status']),
    ...(owner !== '' && { owner }),
    ...(note !== '' && { note }),
  };
}

function parseRosterEntry(raw: unknown): PlanRosterEntry | null {
  if (!isRecord(raw)) return null;
  const agentName = str(raw['agentName']);
  if (agentName === '') return null;
  return { agentName, purpose: str(raw['purpose']), status: str(raw['status']) };
}

function parseResumePlan(raw: unknown): ResumePlan | null {
  if (!isRecord(raw)) return null;
  const goal = str(raw['goal']);
  // A plan with no goal is not resumable — the one field a Continue must not
  // lose is the one this view exists to show, so it is required.
  if (goal === '') return null;
  const next = Array.isArray(raw['next'])
    ? raw['next'].map(parseNextStep).filter((s): s is PlanNextStep => s !== null)
    : [];
  const roster = Array.isArray(raw['roster'])
    ? raw['roster'].map(parseRosterEntry).filter((r): r is PlanRosterEntry => r !== null)
    : [];
  const handoff = str(raw['handoff']);
  const handoffPath = str(raw['handoffPath']);
  return {
    goal,
    doneMeans: strList(raw['doneMeans']),
    next,
    roster,
    ...(handoff !== '' && { handoff }),
    ...(handoffPath !== '' && { handoffPath }),
  };
}

function parsePlanTask(raw: unknown): PlanTask | null {
  if (!isRecord(raw)) return null;
  const id = str(raw['id']);
  if (id === '') return null;
  const owner = str(raw['owner']);
  const group = str(raw['group']);
  const evidence = str(raw['evidence']);
  return {
    id,
    title: str(raw['title']),
    status: str(raw['status'], 'todo'),
    ...(owner !== '' && { owner }),
    ...(group !== '' && { group }),
    ...(evidence !== '' && { evidence }),
  };
}

/**
 * Reads a resume-pack body, or `null` when it is not one. `null` is the same
 * answer for "the host returned nothing" and "the body was not a pack": both
 * mean there is nothing trustworthy to draw.
 */
export function parseResumePack(raw: unknown): ResumePack | null {
  if (!isRecord(raw)) return null;
  const sessionRaw = raw['sessionId'];
  const sessionId = typeof sessionRaw === 'string' && sessionRaw !== '' ? sessionRaw : null;
  const plan = parseResumePlan(raw['plan']);
  const tasksRaw = raw['tasks'];
  const tasks = Array.isArray(tasksRaw)
    ? tasksRaw.map(parsePlanTask).filter((t): t is PlanTask => t !== null)
    : [];
  return { sessionId, plan, tasks };
}

/** The steps still open: everything the plan has not marked done. */
export function openNextSteps(plan: ResumePlan | null): readonly PlanNextStep[] {
  if (plan === null) return [];
  return plan.next.filter((step) => step.status !== 'done');
}

/** Whether a task is still to act on — not done and not dropped. */
export function taskOpen(task: PlanTask): boolean {
  return !TASK_CLOSED.has(task.status);
}

/** Whether `sessionId` may claim this task, matching `track.mjs` exactly. */
export function canClaim(task: PlanTask, sessionId: string | null): boolean {
  if (sessionId === null || !CLAIMABLE.has(task.status)) return false;
  return task.owner === undefined || task.owner === sessionId;
}

/** Whether `sessionId` may complete this task (the owner, before it is done). */
export function canComplete(task: PlanTask, sessionId: string | null): boolean {
  if (sessionId === null || TASK_CLOSED.has(task.status)) return false;
  return task.owner === sessionId;
}

/**
 * The owners whose claims belong on this session's resume pack: the session
 * itself, plus every subagent it dispatched. A subagent's claim is part of the
 * session's picture — a resume must re-dispatch it or wait on it, and either
 * way must be able to see it.
 */
export function sessionOwners(
  plan: ResumePlan | null,
  sessionId: string | null,
): ReadonlySet<string> {
  const owners = new Set<string>();
  if (sessionId !== null) owners.add(sessionId);
  for (const entry of plan?.roster ?? []) owners.add(entry.agentName);
  return owners;
}

/** Every task owned by this session or one of its subagents. */
export function sessionClaims(
  tasks: readonly PlanTask[],
  plan: ResumePlan | null,
  sessionId: string | null,
): readonly PlanTask[] {
  const owners = sessionOwners(plan, sessionId);
  if (owners.size === 0) return [];
  return tasks.filter((task) => task.owner !== undefined && owners.has(task.owner));
}

/** The Badge tone a task status maps onto; an unknown status is neutral. */
export function taskTone(status: string): StatusTone {
  return statusTone(status);
}
