/**
 * Column definitions and the persisted table preferences.
 *
 * Definitions are data rather than JSX so the same list drives the header,
 * the cells, the show/hide menu and the sort comparator — four places that
 * drift apart the moment they are written out separately.
 */

import {
  formatBytes,
  formatCount,
  formatPercent,
  formatThroughput,
  formatUptime,
} from '@vitals/ui';

import { UNKNOWN } from './constants';
import type { ProcessRow, SortColumn } from './model';

export type ColumnId = SortColumn;

export interface ColumnDef {
  readonly id: ColumnId;
  /** Existing i18n key where one exists, otherwise a key from `strings.ts`. */
  readonly labelKey: string;
  readonly width: number;
  readonly minWidth: number;
  readonly align: 'start' | 'end';
  /** Text columns sort alphabetically and get no deadband. */
  readonly numeric: boolean;
  /** Whether the user may hide it. Name and PID are the row's identity. */
  readonly required: boolean;
  render(row: ProcessRow, locale: string): string;
}

export const COLUMNS: readonly ColumnDef[] = [
  {
    id: 'name',
    labelKey: 'process.name',
    width: 260,
    minWidth: 120,
    align: 'start',
    numeric: false,
    required: true,
    render: (row) => row.process.name,
  },
  {
    id: 'pid',
    labelKey: 'process.pid',
    width: 72,
    minWidth: 56,
    align: 'end',
    numeric: true,
    required: true,
    render: (row) => String(row.process.key.pid),
  },
  {
    id: 'state',
    labelKey: 'process.status',
    width: 110,
    minWidth: 80,
    align: 'start',
    numeric: false,
    required: false,
    // Translated by the cell, which has the `t` function; the raw state is
    // returned here so the definition stays free of React context.
    render: (row) => row.process.state,
  },
  {
    id: 'user',
    labelKey: 'process.user',
    width: 140,
    minWidth: 90,
    align: 'start',
    numeric: false,
    required: false,
    render: (row) => row.process.user ?? UNKNOWN,
  },
  {
    id: 'cpu',
    labelKey: 'process.column.cpu',
    width: 76,
    minWidth: 60,
    align: 'end',
    numeric: true,
    required: false,
    render: (row, locale) => formatPercent(row.rolledCpu, locale),
  },
  {
    id: 'memory',
    labelKey: 'process.column.memory',
    width: 96,
    minWidth: 72,
    align: 'end',
    numeric: true,
    required: false,
    render: (row, locale) => formatBytes(row.rolledMemory, locale),
  },
  {
    id: 'disk',
    labelKey: 'process.column.disk',
    width: 104,
    minWidth: 76,
    align: 'end',
    numeric: true,
    required: false,
    render: (row, locale) => formatThroughput(row.rolledDisk, locale),
  },
  {
    id: 'network',
    labelKey: 'process.column.network',
    width: 104,
    minWidth: 76,
    align: 'end',
    numeric: true,
    required: false,
    render: (row, locale) => formatThroughput(row.rolledNetwork, locale),
  },
  {
    id: 'gpu',
    labelKey: 'process.column.gpu',
    width: 76,
    minWidth: 60,
    align: 'end',
    numeric: true,
    required: false,
    // A machine with no GPU counter reports null. Rendering "0%" there would
    // be a claim we cannot support — the GPU may well be busy.
    render: (row, locale) =>
      row.rolledGpu === null ? UNKNOWN : formatPercent(row.rolledGpu, locale),
  },
  {
    id: 'threads',
    labelKey: 'process.threads',
    width: 76,
    minWidth: 60,
    align: 'end',
    numeric: true,
    required: false,
    render: (row, locale) => formatCount(row.process.threadCount, locale),
  },
  {
    id: 'handles',
    labelKey: 'process.handles',
    width: 84,
    minWidth: 64,
    align: 'end',
    numeric: true,
    required: false,
    render: (row, locale) =>
      row.process.handleCount === null ? UNKNOWN : formatCount(row.process.handleCount, locale),
  },
  {
    id: 'uptime',
    labelKey: 'process.uptime',
    width: 88,
    minWidth: 68,
    align: 'end',
    numeric: true,
    required: false,
    render: (row) => formatUptime(row.process.uptimeSecs),
  },
];

export const COLUMN_BY_ID: ReadonlyMap<ColumnId, ColumnDef> = new Map(
  COLUMNS.map((column) => [column.id, column]),
);

export interface TablePreferences {
  readonly visible: readonly ColumnId[];
  readonly widths: Readonly<Partial<Record<ColumnId, number>>>;
  readonly sortColumn: ColumnId;
  readonly sortDirection: 'asc' | 'desc';
  readonly grouped: boolean;
  readonly kind: 'all' | 'apps' | 'background' | 'system';
}

export const DEFAULT_PREFERENCES: TablePreferences = {
  visible: ['name', 'pid', 'state', 'user', 'cpu', 'memory', 'disk', 'network', 'gpu'],
  widths: {},
  // CPU descending is what someone opening a task manager is nearly always
  // there to find out.
  sortColumn: 'cpu',
  sortDirection: 'desc',
  grouped: true,
  kind: 'all',
};

const STORAGE_KEY = 'vitals.processes.preferences';

/**
 * Loads preferences, tolerating anything.
 *
 * A malformed or stale value must never prevent the screen from opening: the
 * process list is the one thing a user reaches for when their machine is
 * misbehaving, and refusing to render because a column width is a string
 * would be a spectacularly bad time to fail.
 */
export function loadPreferences(storage: Storage | undefined = safeStorage()): TablePreferences {
  if (storage === undefined) return DEFAULT_PREFERENCES;
  try {
    const raw = storage.getItem(STORAGE_KEY);
    if (raw === null) return DEFAULT_PREFERENCES;
    const parsed: unknown = JSON.parse(raw);
    return mergePreferences(parsed);
  } catch {
    return DEFAULT_PREFERENCES;
  }
}

export function savePreferences(
  preferences: TablePreferences,
  storage: Storage | undefined = safeStorage(),
): void {
  if (storage === undefined) return;
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(preferences));
  } catch {
    // A full or blocked quota must not break the table. Losing a column
    // width is an acceptable outcome; throwing during a render is not.
  }
}

export function mergePreferences(value: unknown): TablePreferences {
  if (typeof value !== 'object' || value === null) return DEFAULT_PREFERENCES;
  const candidate = value as Partial<Record<keyof TablePreferences, unknown>>;

  const visible = Array.isArray(candidate.visible)
    ? candidate.visible.filter((id): id is ColumnId => COLUMN_BY_ID.has(id as ColumnId))
    : DEFAULT_PREFERENCES.visible;

  const widths: Partial<Record<ColumnId, number>> = {};
  if (typeof candidate.widths === 'object' && candidate.widths !== null) {
    for (const [id, width] of Object.entries(candidate.widths)) {
      const column = COLUMN_BY_ID.get(id as ColumnId);
      if (column !== undefined && typeof width === 'number' && Number.isFinite(width)) {
        widths[column.id] = Math.max(column.minWidth, width);
      }
    }
  }

  const sortColumn = COLUMN_BY_ID.has(candidate.sortColumn as ColumnId)
    ? (candidate.sortColumn as ColumnId)
    : DEFAULT_PREFERENCES.sortColumn;

  return {
    // Required columns are re-added rather than trusted from storage: a
    // persisted state with no name column is an unusable table with no
    // in-app way back.
    visible: ensureRequired(visible),
    widths,
    sortColumn,
    sortDirection: candidate.sortDirection === 'asc' ? 'asc' : 'desc',
    grouped: candidate.grouped !== false,
    kind:
      candidate.kind === 'apps' || candidate.kind === 'background' || candidate.kind === 'system'
        ? candidate.kind
        : 'all',
  };
}

function ensureRequired(visible: readonly ColumnId[]): readonly ColumnId[] {
  const set = new Set(visible);
  for (const column of COLUMNS) {
    if (column.required) set.add(column.id);
  }
  // Ordered by the canonical definition list so hiding and re-showing a
  // column returns it to its original position rather than the end.
  return COLUMNS.filter((column) => set.has(column.id)).map((column) => column.id);
}

function safeStorage(): Storage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}
