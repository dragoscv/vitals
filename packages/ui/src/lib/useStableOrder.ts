/**
 * Live lists that keep their places between ticks.
 *
 * A list sorted by a live value — "most recent activity first", "busiest
 * first" — is re-sorted by its source on every snapshot, and two items whose
 * values cross swap places once a second. The order is correct each time and
 * unreadable in sequence: the card the user was about to click is somewhere
 * else by the time the pointer arrives (S22-10, the AI page's session cards).
 *
 * The source's order stays the definition of "correct". This only decides
 * WHEN the screen follows it:
 *
 * - an item already on screen keeps its place while only its values change;
 * - a new item is inserted where the source put it, relative to its
 *   neighbours, rather than appended at the end;
 * - an item that has gone simply leaves;
 * - an item whose coarse rank changes (a session going from working to idle)
 *   moves at once, because that change is the news;
 * - every `settleMs` the whole list adopts the source's order, so drift never
 *   accumulates into an order nobody would call sorted.
 * - while the list is `held` (the pointer is over it, or focus is inside it)
 *   nothing already shown moves at all — not even for a rank change or a due
 *   settle; whatever is due happens on the first tick after it is let go.
 *
 * Kept in a `.ts` module with the pure step separate from the hook, so the
 * policy is tested over a synthetic sequence of ticks — a single frame cannot
 * show that an item stayed put.
 */

import { useCallback, useMemo, useRef, useState } from 'react';

/** How long a held order may lag the source's before it is adopted whole. */
export const DEFAULT_SETTLE_MS = 30_000;

export interface StableOrderState {
  readonly order: readonly string[];
  readonly ranks: ReadonlyMap<string, number>;
  readonly settledAt: number;
  readonly resetKey: unknown;
}

export interface StableOrderStep {
  /** This tick's keys in the source's (correct) order. */
  readonly keys: readonly string[];
  /** Coarse rank per key, same length as `keys`; omitted means "no ranks". */
  readonly ranks?: readonly number[] | undefined;
  readonly now: number;
  readonly settleMs: number;
  /** A change here (a new sort column) adopts the source's order at once. */
  readonly resetKey?: unknown;
  /**
   * The user is pointing at or focused in the list: hold every shown item in
   * place. A changed `resetKey` still re-sorts, because that is their click.
   */
  readonly held?: boolean | undefined;
}

/**
 * One tick of the policy: the order to show, and the state for the next tick.
 *
 * Pure, so a test can replay any sequence of snapshots and clock readings.
 */
export function stableOrderStep(
  previous: StableOrderState | null,
  step: StableOrderStep,
): StableOrderState {
  const { keys, ranks, now, settleMs, resetKey, held = false } = step;
  const rankOf = new Map<string, number>();
  if (ranks !== undefined) keys.forEach((key, i) => rankOf.set(key, ranks[i] ?? 0));

  const due =
    previous === null ||
    previous.order.length === 0 ||
    previous.resetKey !== resetKey ||
    (!held && now - previous.settledAt >= settleMs);
  if (due) return { order: [...keys], ranks: rankOf, settledAt: now, resetKey };

  const incoming = new Map<string, number>();
  keys.forEach((key, i) => incoming.set(key, i));

  // Survivors in their held order, minus any whose rank changed: those are
  // re-placed below like a new arrival, which puts them in their new group.
  // While held, a rank change waits too; the rank remembered is the old one,
  // so the move still happens on the first tick after release.
  const order: string[] = [];
  const placed = new Set<string>();
  const nextRanks = new Map(rankOf);
  for (const key of previous.order) {
    if (!incoming.has(key) || placed.has(key)) continue;
    const before = previous.ranks.get(key);
    const after = rankOf.get(key);
    if (before !== undefined && after !== undefined && before !== after) {
      if (!held) continue;
      nextRanks.set(key, before);
    }
    order.push(key);
    placed.add(key);
  }

  for (const key of keys) {
    if (placed.has(key)) continue;
    const at = incoming.get(key) ?? 0;
    // Before the first held item the source puts after this one: the
    // nearest thing to "where it belongs" that moves nothing else.
    let index = order.findIndex((other) => (incoming.get(other) ?? 0) > at);
    if (index < 0) index = order.length;
    order.splice(index, 0, key);
    placed.add(key);
  }

  return { order, ranks: nextRanks, settledAt: previous.settledAt, resetKey };
}

export interface StableOrderOptions<T> {
  /**
   * A coarse category whose change should move the item at once — a session
   * state, "running" vs "finished". Never a live value: that is exactly the
   * churn this hook exists to ignore.
   */
  readonly rankOf?: (item: T) => number;
  /** Adopt the source's order whole at most this often. Default 30 s. */
  readonly settleMs?: number;
  /**
   * The clock. Pass the screen's own ticking `now` where it has one, so the
   * settle happens on the same beat as the rest of the screen.
   */
  readonly now?: number;
  /** Changing this (sort column, filter) re-sorts immediately. */
  readonly resetKey?: string | number;
  /** Hold every shown item in place — see {@link useHoverHold}. */
  readonly held?: boolean;
}

/**
 * `items` in the order they were last shown, updated by {@link stableOrderStep}.
 *
 * The previous order lives in a ref, not state: storing it in state would
 * schedule a second render for every snapshot. The step is idempotent for the
 * same input, so a repeated render (StrictMode, an unrelated parent update)
 * yields the same order.
 */
export function useStableOrder<T>(
  items: readonly T[],
  keyOf: (item: T) => string,
  options: StableOrderOptions<T> = {},
): readonly T[] {
  const { rankOf, settleMs = DEFAULT_SETTLE_MS, now, resetKey, held = false } = options;
  const state = useRef<StableOrderState | null>(null);

  return useMemo(() => {
    const byKey = new Map<string, T>();
    const keys: string[] = [];
    for (const item of items) {
      const key = keyOf(item);
      if (byKey.has(key)) continue;
      byKey.set(key, item);
      keys.push(key);
    }
    const next = stableOrderStep(state.current, {
      keys,
      ranks: rankOf === undefined ? undefined : keys.map((key) => rankOf(byKey.get(key) as T)),
      now: now ?? Date.now(),
      settleMs,
      resetKey,
      held,
    });
    state.current = next;
    return next.order.map((key) => byKey.get(key) as T);
  }, [items, keyOf, rankOf, settleMs, now, resetKey, held]);
}

export interface HoverHold {
  /** True while the pointer is over the list or keyboard focus is inside it. */
  readonly held: boolean;
  /** Spread onto the list's container element. */
  readonly bind: {
    readonly onPointerEnter: () => void;
    readonly onPointerLeave: () => void;
    readonly onFocus: () => void;
    readonly onBlur: (event: React.FocusEvent<HTMLElement>) => void;
  };
}

/**
 * Whether the user is reaching for something in a list. A row must never
 * slide out from under a pointer on its way to a click, nor from under the
 * keyboard focus (S22-10); pass `held` to {@link useStableOrder}.
 */
export function useHoverHold(): HoverHold {
  const [pointer, setPointer] = useState(false);
  const [focus, setFocus] = useState(false);
  const onPointerEnter = useCallback(() => setPointer(true), []);
  const onPointerLeave = useCallback(() => setPointer(false), []);
  const onFocus = useCallback(() => setFocus(true), []);
  const onBlur = useCallback((event: React.FocusEvent<HTMLElement>) => {
    // Focus moving between two rows of the same list is still "inside".
    if (!event.currentTarget.contains(event.relatedTarget)) setFocus(false);
  }, []);
  const bind = useMemo(
    () => ({ onPointerEnter, onPointerLeave, onFocus, onBlur }),
    [onPointerEnter, onPointerLeave, onFocus, onBlur],
  );
  return { held: pointer || focus, bind };
}

/**
 * Every key that has been present in this mounted component, ever.
 *
 * For lists that drop unmeasured rows: a reading that flickers between a value
 * and `null` would otherwise make its row appear and vanish each tick, and
 * every row after it would change place. Once a key has been seen its slot is
 * kept (the caller renders the em dash); a key never seen stays omitted, so a
 * machine without the sensor still shows no column of dashes.
 */
export function useSeenKeys(present: readonly string[]): ReadonlySet<string> {
  const seen = useRef<Set<string>>(new Set());
  for (const key of present) seen.current.add(key);
  return seen.current;
}

export interface StickyRow<T> {
  readonly key: string;
  /**
   * This tick's row, or — when `present` is false — the last one seen, kept
   * only so the caller can still print its label. Its readings are stale and
   * must not be shown as current: render the em dash instead.
   */
  readonly row: T;
  readonly present: boolean;
}

/**
 * The pure step behind {@link useStickyRows}: every key ever seen, in place.
 *
 * A new key goes before the first known key the source lists after it, so a
 * sensor that appears late lands next to its neighbours rather than at the end.
 */
export function stickyRowsStep<T>(
  known: ReadonlyMap<string, T>,
  order: readonly string[],
  rows: readonly T[],
  keyOf: (row: T) => string,
): { readonly order: string[]; readonly known: Map<string, T>; readonly out: StickyRow<T>[] } {
  const nextKnown = new Map(known);
  const nextOrder = [...order];
  const incoming = new Map<string, number>();
  const present = new Map<string, T>();
  rows.forEach((row, i) => {
    const key = keyOf(row);
    if (present.has(key)) return;
    present.set(key, row);
    incoming.set(key, i);
    nextKnown.set(key, row);
  });
  for (const [key, at] of incoming) {
    if (nextOrder.includes(key)) continue;
    let index = nextOrder.findIndex((other) => (incoming.get(other) ?? -1) > at);
    if (index < 0) index = nextOrder.length;
    nextOrder.splice(index, 0, key);
  }
  const out = nextOrder.map((key) => {
    const now = present.get(key);
    return now === undefined
      ? { key, row: nextKnown.get(key) as T, present: false }
      : { key, row: now, present: true };
  });
  return { order: nextOrder, known: nextKnown, out };
}

/**
 * Rows that keep their slot once seen, for lists built only from what is
 * measured this tick (temperatures, fans). A reading that drops out leaves an
 * em dash in its place instead of pulling every row below it up a line.
 */
export function useStickyRows<T>(
  rows: readonly T[],
  keyOf: (row: T) => string,
): readonly StickyRow<T>[] {
  const state = useRef<{ order: readonly string[]; known: ReadonlyMap<string, T> }>({
    order: [],
    known: new Map(),
  });
  return useMemo(() => {
    const next = stickyRowsStep(state.current.known, state.current.order, rows, keyOf);
    state.current = { order: next.order, known: next.known };
    return next.out;
  }, [rows, keyOf]);
}
