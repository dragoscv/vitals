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
