/**
 * Data read ahead of the screen that needs it.
 *
 * Every section used to start loading only when it was first opened, so the
 * first visit to Startup was a skeleton for the length of a registry-and-SCM
 * walk. The shell now reads each section's data once, in the background,
 * after the window has painted (`routes.tsx`, `preloadRoutes`), and a hook
 * starts from that result instead of from nothing.
 *
 * Two operations, because a hook needs two different things:
 *
 * - {@link peekPrefetched} at render time, synchronously, to seed its
 *   initial state. That is what removes the skeleton: the first render
 *   already has data, so there is no frame without it — which matters most
 *   inside a View Transition, where that frame would be what the browser
 *   snapshots and animates.
 * - {@link takePrefetched} from its first load, to reuse the read rather than
 *   repeat it — but only while it is younger than {@link REUSE_WITHIN_MS}.
 *   Older, the seeded data stays on screen and the hook reads again, so a
 *   list read at launch is never presented as current an hour later. Taking
 *   removes the entry, so a later refresh is always a real read.
 *
 * Entries expire. A connection table from five minutes ago is not a
 * connection table; past `maxAgeMs` an entry is ignored and the hook loads
 * normally, exactly as it did before this existed.
 */

/** How young a background read must be for a screen's first load to reuse it outright. */
export const REUSE_WITHIN_MS = 15_000;

interface Entry {
  promise: Promise<unknown>;
  readonly maxAgeMs: number;
  /** When the read started; age counts from here so an in-flight read can expire too. */
  readonly startedAt: number;
  settled?: { readonly value: unknown; readonly at: number } | undefined;
}

const entries = new Map<string, Entry>();

const now = (): number => Date.now();

function fresh(entry: Entry): boolean {
  return now() - (entry.settled?.at ?? entry.startedAt) <= entry.maxAgeMs;
}

/**
 * Starts a background read under `key`, unless a fresh one already exists.
 *
 * A failed read leaves no entry: the screen will load for itself and show
 * the error then, where it can be explained, rather than inheriting a
 * failure it never saw happen.
 */
export function prefetch<T>(key: string, load: () => Promise<T>, maxAgeMs: number): Promise<void> {
  const existing = entries.get(key);
  if (existing !== undefined && fresh(existing)) return existing.promise.then(noop, noop);

  const entry: Entry = { promise: Promise.resolve(), maxAgeMs, startedAt: now() };
  entry.promise = load().then(
    (value) => {
      entry.settled = { value, at: now() };
      return value;
    },
    (error: unknown) => {
      if (entries.get(key) === entry) entries.delete(key);
      throw error;
    },
  );
  entries.set(key, entry);
  return entry.promise.then(noop, noop);
}

/** The settled value under `key` and when it was read, if fresh. Does not consume it. */
export function peekPrefetched<T>(
  key: string,
): { readonly value: T; readonly at: number } | undefined {
  const entry = entries.get(key);
  if (entry?.settled === undefined || !fresh(entry)) return undefined;
  return { value: entry.settled.value as T, at: entry.settled.at };
}

/**
 * The read under `key`, settled or still in flight, if younger than
 * `reuseWithinMs` — removed so it is used once. `undefined` means "read it
 * yourself" (and whatever was seeded stays on screen meanwhile).
 */
export function takePrefetched<T>(
  key: string,
  reuseWithinMs: number = REUSE_WITHIN_MS,
): Promise<T> | undefined {
  const entry = entries.get(key);
  if (entry === undefined) return undefined;
  entries.delete(key);
  const age = now() - (entry.settled?.at ?? entry.startedAt);
  return age <= Math.min(reuseWithinMs, entry.maxAgeMs) ? (entry.promise as Promise<T>) : undefined;
}

/** Forgets everything. For tests. */
export function clearPrefetched(): void {
  entries.clear();
}

function noop(): void {}
