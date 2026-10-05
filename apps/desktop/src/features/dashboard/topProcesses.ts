/**
 * The "which program is doing this" widgets.
 *
 * # Grouped by name, not listed per PID
 *
 * A raw top-five by CPU on a machine running a browser is five rows all called
 * `chrome.exe`, which answers nothing — the user wanted to know that Chrome is
 * using 40%, not that one of its ninety renderers is using 8%. Rows are
 * therefore aggregated by executable name, with the contributing process count
 * carried so the grouping is visible rather than a silent lie about how many
 * processes exist.
 *
 * The Processes screen deliberately does the opposite and shows every PID:
 * that is the screen for acting on one process, this is the widget for
 * spotting which application to blame.
 *
 * # Partial selection, not a full sort
 *
 * Finding the top five of ~600 rows by sorting is O(n log n) and allocates a
 * copy of the array every tick. {@link topBy} keeps a bounded insertion list
 * instead: one pass, no allocation proportional to input, and the constant is
 * tiny because five comparisons is the worst case per row. On a 1 Hz feed this
 * is the difference between a widget that costs nothing and one that shows up
 * in a profile.
 */

import { displayName, type Process } from '@vitals/protocol';

/** How many rows each widget shows. */
export const TOP_COUNT = 5;

export interface TopEntry {
  /** The app's name ("Google Chrome"), else the executable name. The group key. */
  readonly name: string;
  /** Summed metric across every process with this name. */
  readonly value: number;
  /** How many processes contributed. Shown when greater than one. */
  readonly count: number;
  /**
   * The largest single contributor, so the widget can offer a useful action.
   *
   * Selecting a *group* is meaningless to the Processes screen, which acts on
   * one race-free key. The dominant PID is the one a user clicking through
   * almost certainly means.
   */
  readonly representativeKey: string;
}

/**
 * Sums a metric per executable name.
 *
 * Processes reporting zero are skipped rather than grouped: they cannot appear
 * in a top-five and including them would make the map proportional to the
 * process count instead of to the number of *active* applications.
 */
export function groupByName(
  processes: Iterable<readonly [string, Process]>,
  metric: (process: Process) => number,
): Map<string, { value: number; count: number; bestKey: string; best: number }> {
  const groups = new Map<string, { value: number; count: number; bestKey: string; best: number }>();

  for (const [key, process] of processes) {
    const value = metric(process);
    if (value <= 0) continue;

    // By the name people recognise: the widget answers "which app", and the
    // description is per image, so it groups exactly as the file name did.
    const name = displayName(process);
    const existing = groups.get(name);
    if (existing === undefined) {
      groups.set(name, { value, count: 1, bestKey: key, best: value });
      continue;
    }

    existing.value += value;
    existing.count += 1;
    if (value > existing.best) {
      existing.best = value;
      existing.bestKey = key;
    }
  }

  return groups;
}

/**
 * Selects the `limit` largest entries, descending.
 *
 * Ties break on name so the order is deterministic. Without that, two
 * processes sitting at the same value would swap position on every tick purely
 * from map iteration order — the same flicker the Processes screen's ordering
 * policy exists to prevent, arriving through a different door.
 */
export function topBy(
  groups: ReadonlyMap<string, { value: number; count: number; bestKey: string }>,
  limit: number = TOP_COUNT,
): readonly TopEntry[] {
  const best: TopEntry[] = [];

  for (const [name, group] of groups) {
    const entry: TopEntry = {
      name,
      value: group.value,
      count: group.count,
      representativeKey: group.bestKey,
    };

    // Cheap rejection first: once the list is full, most rows lose immediately
    // against the smallest retained value and cost a single comparison.
    if (best.length === limit) {
      const weakest = best[limit - 1];
      if (weakest !== undefined && !outranks(entry, weakest)) continue;
      best.pop();
    }

    let index = best.length;
    while (index > 0) {
      const candidate = best[index - 1];
      if (candidate === undefined || outranks(candidate, entry)) break;
      best[index] = candidate;
      index -= 1;
    }
    best[index] = entry;
  }

  return best;
}

function outranks(a: TopEntry, b: TopEntry): boolean {
  if (a.value !== b.value) return a.value > b.value;
  return a.name.localeCompare(b.name) < 0;
}

export function topByCpu(processes: ReadonlyMap<string, Process>): readonly TopEntry[] {
  return topBy(groupByName(processes, (process) => process.cpu));
}

export function topByMemory(processes: ReadonlyMap<string, Process>): readonly TopEntry[] {
  // Private bytes, matching the Processes screen's default and the honest
  // answer to "how much RAM would I get back". Working set would double-count
  // shared pages across every process that maps them.
  return topBy(groupByName(processes, (process) => process.memoryPrivate));
}
