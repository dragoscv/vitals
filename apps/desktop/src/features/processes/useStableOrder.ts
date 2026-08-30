/**
 * Applies the ordering policy in `ordering.ts` to a tick of rows.
 *
 * Separated from the table component so the policy can be exercised over a
 * synthetic sequence of ticks in a test, which is the only way to observe
 * hysteresis at all — a single frame cannot show that a row stayed put.
 */

import { useMemo, useRef } from 'react';

import { COLUMN_BY_ID, type ColumnId } from './columns';
import { DEADBAND, sortValue, textValue, type ProcessRow } from './model';
import { advanceSmoothing, deadbandCompare, reconcileOrder } from './ordering';

export interface StableOrderInput {
  readonly rows: readonly ProcessRow[];
  readonly column: ColumnId;
  readonly direction: 'asc' | 'desc';
  /** Positions are held while true; values still update. */
  readonly frozen: boolean;
  readonly locale: string;
}

/**
 * Returns the rows in a stabilised order.
 *
 * The smoothing table and the previous order live in refs rather than state:
 * writing them to state would schedule a second render for every frame, which
 * on a 1 Hz feed of 600 rows is exactly the cost this screen exists to avoid.
 */
export function useStableOrder(input: StableOrderInput): readonly ProcessRow[] {
  const { rows, column, direction, frozen, locale } = input;

  const smoothed = useRef(new Map<string, number>());
  const previousOrder = useRef<readonly string[]>([]);
  const previousColumn = useRef<ColumnId>(column);

  return useMemo(() => {
    const def = COLUMN_BY_ID.get(column);
    const byId = new Map(rows.map((row) => [row.id, row] as const));

    // Changing the sort column is an explicit request to reorder. Carrying
    // the old order forward would let hysteresis preserve a ranking the user
    // just asked to replace, which reads as the click having done nothing.
    if (previousColumn.current !== column) {
      previousOrder.current = [];
      smoothed.current.clear();
      previousColumn.current = column;
    }

    let compare: (a: string, b: string) => number;

    if (def !== undefined && def.numeric) {
      const readings = new Map<string, number>();
      for (const row of rows) readings.set(row.id, sortValue(row, column));
      advanceSmoothing(smoothed.current, readings);

      const band = DEADBAND[column];
      const table = smoothed.current;
      compare = (a, b) => {
        const av = table.get(a) ?? Number.NaN;
        const bv = table.get(b) ?? Number.NaN;
        const result = deadbandCompare(av, bv, band.absolute, band.relative);
        // An unknown value is pinned to the end in both directions rather
        // than flipping to the top when the sort is reversed: "we could not
        // measure this" is never the answer to "show me the busiest".
        if (result === 0) return 0;
        if (!Number.isFinite(av) || !Number.isFinite(bv)) return result;
        return direction === 'desc' ? -result : result;
      };
    } else {
      const collator = new Intl.Collator(locale, { sensitivity: 'base', numeric: true });
      compare = (a, b) => {
        const rowA = byId.get(a);
        const rowB = byId.get(b);
        if (rowA === undefined || rowB === undefined) return 0;
        const av = textValue(rowA, column) ?? '';
        const bv = textValue(rowB, column) ?? '';
        const result = collator.compare(av, bv);
        return direction === 'desc' ? -result : result;
      };
    }

    // Siblings are ordered within their parent, never across the whole list:
    // sorting a tree flat would separate a child from its parent and the
    // indentation would describe a structure the row order contradicts.
    const ordered = orderTree(rows, compare, previousOrder.current, frozen);
    previousOrder.current = ordered.map((row) => row.id);
    return ordered;
  }, [rows, column, direction, frozen, locale]);
}

/**
 * Reorders each sibling group independently, preserving tree order.
 *
 * The incoming `rows` are already in depth-first order with collapsed
 * subtrees omitted, so a parent's children are exactly the contiguous run of
 * deeper rows that follows it.
 */
function orderTree(
  rows: readonly ProcessRow[],
  compare: (a: string, b: string) => number,
  previous: readonly string[],
  frozen: boolean,
): readonly ProcessRow[] {
  if (rows.length === 0) return rows;

  const byId = new Map(rows.map((row) => [row.id, row] as const));
  const previousRank = new Map<string, number>();
  previous.forEach((id, index) => previousRank.set(id, index));

  const walk = (start: number, depth: number, end: number): { ids: string[]; next: number } => {
    const groupIds: string[] = [];
    const blocks = new Map<string, ProcessRow[]>();

    let i = start;
    while (i < end) {
      const row = rows[i] as ProcessRow;
      if (row.depth < depth) break;
      if (row.depth > depth) {
        i += 1;
        continue;
      }
      groupIds.push(row.id);
      const childStart = i + 1;
      const child = walk(childStart, depth + 1, end);
      blocks.set(
        row.id,
        child.ids.map((id) => byId.get(id)).filter((r): r is ProcessRow => r !== undefined),
      );
      i = child.next;
    }

    const previousGroup = groupIds
      .filter((id) => previousRank.has(id))
      .sort((a, b) => (previousRank.get(a) as number) - (previousRank.get(b) as number));

    const ordered = reconcileOrder(previousGroup, new Set(groupIds), compare, { frozen });

    const flat: string[] = [];
    for (const id of ordered) {
      flat.push(id);
      for (const childRow of blocks.get(id) ?? []) flat.push(childRow.id);
    }

    return { ids: flat, next: i };
  };

  const result = walk(0, rows[0]?.depth ?? 0, rows.length);
  return result.ids.map((id) => byId.get(id)).filter((row): row is ProcessRow => row !== undefined);
}
