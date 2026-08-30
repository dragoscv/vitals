/**
 * Row-order stabilisation.
 *
 * The problem this solves is the single loudest complaint about Task
 * Manager: sort by CPU, try to right-click the process at the top, and by the
 * time the pointer arrives a different process is under it. At 1 Hz with ~600
 * rows, dozens of adjacent pairs swap every tick purely from sampling noise —
 * 0.4% vs 0.3% CPU is not a meaningful difference, but a naive comparator
 * treats it as one and the row moves. The user's target is then gone, and in a
 * tool whose main verb is "end task" that is not a cosmetic annoyance: it is
 * how you kill the wrong process.
 *
 * Three mechanisms stack here, each fixing a different failure:
 *
 * 1. **Exponential smoothing** of the sort value removes single-tick spikes.
 *    A process that briefly touches 40% CPU should not vault to the top and
 *    fall back a second later, dragging every row below it up and down.
 * 2. **A deadband comparator plus a stable insertion sort.** Values closer
 *    together than the deadband compare as *equal*, and equal elements keep
 *    their previous relative order. This is what actually gives hysteresis:
 *    a row must beat its neighbour by a real margin to overtake it, and once
 *    it has, it must lose by a real margin to fall back. Insertion sort is
 *    used deliberately — it is O(n) on the nearly-sorted input we always
 *    have, and unlike `Array.prototype.sort` it is well defined under a
 *    comparator that is not transitive, which a deadband comparator is not.
 * 3. **Freezing** while the pointer is over the table or a menu is open.
 *    Smoothing narrows the window in which a row can move; freezing closes
 *    it. Numbers keep updating in place — only positions are held — so the
 *    table never lies about what is happening, it just stops rearranging
 *    itself while someone is aiming at it.
 */

/**
 * How much of the new reading is folded in each tick.
 *
 * 0.4 settles to within a few percent of a step change in about four seconds,
 * which is fast enough that a process spinning up still climbs visibly, and
 * slow enough that one noisy sample cannot reorder the list.
 */
export const DEFAULT_SMOOTHING = 0.4;

/**
 * Folds a new reading into a smoothed value.
 *
 * A previously unseen row adopts its reading outright rather than easing up
 * from zero: a process that starts at 30% CPU should appear at 30%, not climb
 * there over several seconds while the user wonders why the number on screen
 * disagrees with the one in the row.
 */
export function smooth(
  previous: number | undefined,
  next: number,
  alpha = DEFAULT_SMOOTHING,
): number {
  if (previous === undefined || !Number.isFinite(previous)) return next;
  return previous + (next - previous) * alpha;
}

/**
 * Updates a smoothing table in place for the keys present this tick.
 *
 * Keys absent from `values` are dropped, so the table cannot grow without
 * bound on a machine that churns through short-lived processes — a build
 * server can create tens of thousands in an hour.
 */
export function advanceSmoothing(
  table: Map<string, number>,
  values: ReadonlyMap<string, number>,
  alpha = DEFAULT_SMOOTHING,
): void {
  for (const key of table.keys()) {
    if (!values.has(key)) table.delete(key);
  }
  for (const [key, value] of values) {
    table.set(key, smooth(table.get(key), value, alpha));
  }
}

/**
 * A comparator that reports "indistinguishable" instead of splitting hairs.
 *
 * Both an absolute and a relative floor are needed. Absolute alone would
 * still shuffle the top of a memory-sorted list, where the values are in
 * gigabytes and a 64 KB deadband is nothing; relative alone would treat 0.01%
 * and 0.02% CPU as a twofold difference worth reordering for.
 */
export function deadbandCompare(a: number, b: number, absolute: number, relative: number): number {
  if (!Number.isFinite(a) || !Number.isFinite(b)) {
    // A missing value sorts last regardless of direction: "unknown" is not
    // "zero", and floating it to the top of a descending sort would put the
    // rows we know least about where the user looks first.
    if (!Number.isFinite(a) && !Number.isFinite(b)) return 0;
    return Number.isFinite(a) ? -1 : 1;
  }

  const margin = Math.max(absolute, relative * Math.max(Math.abs(a), Math.abs(b)));
  if (Math.abs(a - b) <= margin) return 0;
  return a < b ? -1 : 1;
}

export interface ReconcileOptions {
  /**
   * Suppresses reordering entirely.
   *
   * Rows that have exited still leave, because a dead process must not be
   * left on screen for someone to try to kill; new rows are appended rather
   * than inserted, because inserting shifts everything below the insertion
   * point, which is exactly the movement being prevented.
   */
  readonly frozen?: boolean;
}

/**
 * Produces this tick's row order from the last one.
 *
 * Deriving from the previous order rather than sorting from scratch is the
 * whole point: a sort from scratch has no memory, so it cannot preserve a
 * position, and hysteresis is impossible without one.
 *
 * `compare` must return 0 for values the user should not see reorder.
 */
export function reconcileOrder(
  previous: readonly string[],
  keys: ReadonlySet<string>,
  compare: (a: string, b: string) => number,
  options: ReconcileOptions = {},
): string[] {
  const surviving: string[] = [];
  const seen = new Set<string>();
  for (const key of previous) {
    if (keys.has(key) && !seen.has(key)) {
      surviving.push(key);
      seen.add(key);
    }
  }

  const added: string[] = [];
  for (const key of keys) {
    if (!seen.has(key)) added.push(key);
  }

  if (options.frozen === true) {
    // Appending is the only placement that moves nothing. New rows land at
    // the bottom and slot into place on the next unfrozen tick.
    added.sort(compare);
    return surviving.concat(added);
  }

  const order = surviving.concat(added);

  // Insertion sort, not `Array.prototype.sort`. The deadband comparator is
  // intransitive by construction (a≈b and b≈c does not imply a≈c), and
  // V8's TimSort is only defined for a consistent comparator — it will
  // happily produce a different order for the same input. Insertion sort
  // gives a deterministic result: each element moves left only past
  // elements it strictly beats, so a row keeps its place unless something
  // genuinely overtakes it.
  for (let i = 1; i < order.length; i += 1) {
    const key = order[i] as string;
    let j = i - 1;
    while (j >= 0 && compare(order[j] as string, key) > 0) {
      order[j + 1] = order[j] as string;
      j -= 1;
    }
    order[j + 1] = key;
  }

  return order;
}
