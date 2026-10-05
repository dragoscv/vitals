/**
 * Turning a flat process map into the rows the table draws.
 *
 * Kept free of React so it can be tested — and profiled — without mounting
 * anything, and so a change to the tree rules cannot accidentally depend on
 * render order.
 */

import type { Process, ProcessKind } from '@vitals/protocol';
import { displayName, matchesProcessName, processKeyId } from '@vitals/protocol';

import { ProcessFlag, hasFlag } from './constants';

export type SortColumn =
  | 'name'
  | 'pid'
  | 'state'
  | 'user'
  | 'cpu'
  | 'memory'
  | 'disk'
  | 'network'
  | 'gpu'
  | 'threads'
  | 'handles'
  | 'uptime';

export type SortDirection = 'asc' | 'desc';

/** Which slice of the process list is shown. */
export type KindFilter = 'all' | 'apps' | 'background' | 'system';

/** A node in the displayed tree. Depth is precomputed for flat rendering. */
export interface ProcessRow {
  readonly id: string;
  readonly process: Process;
  readonly depth: number;
  /** Keys of direct children, in display order. Empty for a leaf. */
  readonly childIds: readonly string[];
  /**
   * Totals including collapsed descendants.
   *
   * A collapsed parent showing only its own 0.1% CPU while hiding a child
   * burning 80% would be actively misleading — the row would say the app is
   * idle when the app is the reason the fan is loud.
   */
  readonly rolledCpu: number;
  readonly rolledMemory: number;
  readonly rolledDisk: number;
  /**
   * `null` when per-process network is not being measured.
   *
   * Windows exposes no per-process network counters, so this needs an ETW
   * kernel trace session and the elevated helper. It used to arrive as a
   * zero, which rendered "0 B/s" against every row on the machine.
   */
  readonly rolledNetwork: number | null;
  /** Null when no descendant reports GPU usage, so it renders as unknown. */
  readonly rolledGpu: number | null;
  readonly descendantCount: number;
}

/**
 * The value a column sorts on.
 *
 * Returns `Number.NaN` — not 0 — where the underlying reading is absent, so
 * the deadband comparator can push it to the end instead of ranking an
 * unmeasurable process alongside a genuinely idle one.
 */
export function sortValue(row: ProcessRow, column: SortColumn): number {
  const p = row.process;
  switch (column) {
    case 'cpu':
      return row.rolledCpu;
    case 'memory':
      return row.rolledMemory;
    case 'disk':
      return row.rolledDisk;
    case 'network':
      return row.rolledNetwork ?? Number.NaN;
    case 'gpu':
      return row.rolledGpu ?? Number.NaN;
    case 'pid':
      return p.key.pid;
    case 'threads':
      return p.threadCount;
    case 'handles':
      return p.handleCount ?? Number.NaN;
    case 'uptime':
      return p.uptimeSecs;
    default:
      return Number.NaN;
  }
}

/** Columns compared as text rather than as numbers. */
export function textValue(row: ProcessRow, column: SortColumn): string | null {
  switch (column) {
    case 'name':
      return displayName(row.process);
    case 'user':
      return row.process.user;
    case 'state':
      return row.process.state;
    default:
      return null;
  }
}

/**
 * Deadbands per column, in the column's own units.
 *
 * These are the thresholds below which two rows are declared equal and keep
 * their existing order. They are set at the level where a difference stops
 * being actionable, not at the level of measurement precision: the sampler
 * can resolve 0.01% CPU, but nobody makes a decision on it.
 */
export const DEADBAND: Readonly<Record<SortColumn, { absolute: number; relative: number }>> = {
  // 0.5 percentage points: below this the ranking is sampling noise.
  cpu: { absolute: 0.5, relative: 0.05 },
  // 4 MiB, or 5% for the large consumers where 4 MiB is invisible.
  memory: { absolute: 4 * 1024 * 1024, relative: 0.05 },
  disk: { absolute: 64 * 1024, relative: 0.15 },
  network: { absolute: 16 * 1024, relative: 0.15 },
  gpu: { absolute: 1, relative: 0.05 },
  threads: { absolute: 0, relative: 0 },
  handles: { absolute: 0, relative: 0 },
  uptime: { absolute: 0, relative: 0 },
  // Identity columns are exact by nature; a deadband on a PID is meaningless.
  pid: { absolute: 0, relative: 0 },
  name: { absolute: 0, relative: 0 },
  user: { absolute: 0, relative: 0 },
  state: { absolute: 0, relative: 0 },
};

export function matchesKind(kind: ProcessKind, filter: KindFilter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'apps':
      return kind === 'app';
    case 'background':
      return kind === 'background' || kind === 'service' || kind === 'containerized';
    case 'system':
      return kind === 'system';
  }
}

/**
 * Whether a process matches the search query.
 *
 * Matching on the PID as well as the name because "which process is 4242?" is
 * a question people arrive with, having read the number out of a crash log.
 */
export function matchesQuery(process: Process, query: string): boolean {
  if (query === '') return true;
  const needle = query.toLowerCase();
  if (matchesProcessName(process, needle)) return true;
  if (String(process.key.pid).includes(needle)) return true;
  const user = process.user;
  return user !== null && user.toLowerCase().includes(needle);
}

interface BuildOptions {
  readonly processes: ReadonlyMap<string, Process>;
  readonly query: string;
  readonly kind: KindFilter;
  readonly grouped: boolean;
  readonly expanded: ReadonlySet<string>;
}

interface BuildResult {
  /** Rows in tree order, collapsed subtrees omitted. */
  readonly rows: readonly ProcessRow[];
  /** Every row including hidden descendants, keyed by id. */
  readonly byId: ReadonlyMap<string, ProcessRow>;
  /** Rows that survived filtering, before collapsing. */
  readonly matchCount: number;
}

interface Node {
  readonly process: Process;
  readonly children: string[];
}

/**
 * Builds the visible row list.
 *
 * A filter hides a row but keeps its subtree reachable: hiding a matching
 * child because its parent did not match would make searching for "chrome"
 * return nothing when Chrome's renderers are nested under a launcher that is
 * named something else. Any process on the path to a match is therefore
 * retained as scaffolding.
 */
export function buildRows(options: BuildOptions): BuildResult {
  const { processes, query, kind, grouped, expanded } = options;

  const directMatches = new Set<string>();
  for (const [id, process] of processes) {
    if (matchesKind(process.kind, kind) && matchesQuery(process, query)) {
      directMatches.add(id);
    }
  }

  if (!grouped) {
    const rows: ProcessRow[] = [];
    const byId = new Map<string, ProcessRow>();
    for (const id of directMatches) {
      const process = processes.get(id);
      if (process === undefined) continue;
      const row = leafRow(id, process, 0);
      rows.push(row);
      byId.set(id, row);
    }
    return { rows, byId, matchCount: rows.length };
  }

  // PID → key index, because `Process.parent` is a bare PID while our
  // identity is pid+startTime. A parent PID that has been recycled resolves
  // to whichever live process now owns it; that is unavoidable with the data
  // we have, and is why the roots check below also guards against cycles.
  const byPid = new Map<number, string>();
  for (const [id, process] of processes) byPid.set(process.key.pid, id);

  const nodes = new Map<string, Node>();
  for (const [id, process] of processes) nodes.set(id, { process, children: [] });

  const roots: string[] = [];
  const parentOf = new Map<string, string>();
  for (const [id, node] of nodes) {
    const parentPid = node.process.parent;
    const parentId = parentPid === null ? undefined : byPid.get(parentPid);
    if (parentId === undefined || parentId === id || !nodes.has(parentId)) {
      roots.push(id);
      continue;
    }
    // Self-parenting and PID-recycling can produce a cycle. A cycle here
    // would recurse until the stack blows, taking the whole window with it,
    // so anything not reachable from a root is re-rooted below.
    nodes.get(parentId)?.children.push(id);
    parentOf.set(id, parentId);
  }

  detachCycles(nodes, roots, parentOf);

  // Which subtrees contain a match. Computed bottom-up so a deep match keeps
  // its whole ancestor chain visible.
  const keep = new Set<string>();
  const markKept = (id: string): boolean => {
    const node = nodes.get(id);
    if (node === undefined) return false;
    let kept = directMatches.has(id);
    for (const child of node.children) {
      if (markKept(child)) kept = true;
    }
    if (kept) keep.add(id);
    return kept;
  };
  for (const root of roots) markKept(root);

  const byId = new Map<string, ProcessRow>();
  const rows: ProcessRow[] = [];
  let matchCount = 0;

  interface Totals {
    cpu: number;
    memory: number;
    disk: number;
    network: number | null;
    gpu: number | null;
    count: number;
  }

  const emit = (id: string, depth: number): Totals => {
    const node = nodes.get(id);
    if (node === undefined) {
      return { cpu: 0, memory: 0, disk: 0, network: null, gpu: null, count: 0 };
    }
    const p = node.process;
    const visibleChildren = node.children.filter((child) => keep.has(child));

    const placeholder = leafRow(id, p, depth);
    const index = rows.length;
    rows.push(placeholder);
    byId.set(id, placeholder);
    matchCount += 1;

    const totals: Totals = {
      cpu: p.cpu,
      memory: p.memoryPrivate,
      disk: p.diskRead + p.diskWrite,
      // Both halves come from the same ETW session, so either both are
      // measured or neither is.
      network: p.netRx === null || p.netTx === null ? null : p.netRx + p.netTx,
      gpu: p.gpu,
      count: 0,
    };

    const childrenVisible = expanded.has(id);
    for (const child of visibleChildren) {
      const before = rows.length;
      const sub = emit(child, depth + 1);
      totals.cpu += sub.cpu;
      totals.memory += sub.memory;
      totals.disk += sub.disk;
      if (sub.network !== null) totals.network = (totals.network ?? 0) + sub.network;
      if (sub.gpu !== null) totals.gpu = (totals.gpu ?? 0) + sub.gpu;
      totals.count += sub.count + 1;
      if (!childrenVisible) {
        // Emitted so the subtree is measured and indexed, then removed from
        // the drawn list. Building the totals requires the walk either way.
        rows.length = before;
      }
    }

    const row: ProcessRow = {
      id,
      process: p,
      depth,
      childIds: visibleChildren,
      rolledCpu: totals.cpu,
      rolledMemory: totals.memory,
      rolledDisk: totals.disk,
      rolledNetwork: totals.network,
      rolledGpu: totals.gpu,
      descendantCount: totals.count,
    };
    rows[index] = row;
    byId.set(id, row);
    return totals;
  };

  for (const root of roots) {
    if (keep.has(root)) emit(root, 0);
  }

  return { rows, byId, matchCount };
}

function leafRow(id: string, process: Process, depth: number): ProcessRow {
  return {
    id,
    process,
    depth,
    childIds: [],
    rolledCpu: process.cpu,
    rolledMemory: process.memoryPrivate,
    rolledDisk: process.diskRead + process.diskWrite,
    rolledNetwork:
      process.netRx === null || process.netTx === null ? null : process.netRx + process.netTx,
    rolledGpu: process.gpu,
    descendantCount: 0,
  };
}

/**
 * Re-roots any node not reachable from a root, severing the edge that made it
 * unreachable.
 *
 * Recycled PIDs genuinely produce cycles: a process whose recorded parent PID
 * now belongs to one of its own descendants. Merely adding such a node to the
 * root list is not enough — it would still be its parent's child, so the
 * depth-first walk would visit it twice and recurse forever. The back-edge
 * has to actually be cut.
 */
function detachCycles(
  nodes: Map<string, Node>,
  roots: string[],
  parentOf: ReadonlyMap<string, string>,
): void {
  const reachable = new Set<string>();
  const visit = (start: string): void => {
    const stack = [start];
    while (stack.length > 0) {
      const id = stack.pop() as string;
      if (reachable.has(id)) continue;
      reachable.add(id);
      const node = nodes.get(id);
      if (node !== undefined) stack.push(...node.children);
    }
  };

  for (const root of roots) visit(root);

  for (const id of nodes.keys()) {
    if (reachable.has(id)) continue;

    const parentId = parentOf.get(id);
    const parent = parentId === undefined ? undefined : nodes.get(parentId);
    if (parent !== undefined) {
      const index = parent.children.indexOf(id);
      if (index >= 0) parent.children.splice(index, 1);
    }

    roots.push(id);
    visit(id);
  }
}

/** Every descendant of a row, for "end process tree". */
export function collectSubtree(
  byId: ReadonlyMap<string, ProcessRow>,
  id: string,
): readonly string[] {
  const out: string[] = [];
  const stack = [id];
  const seen = new Set<string>();
  while (stack.length > 0) {
    const current = stack.pop() as string;
    if (seen.has(current)) continue;
    seen.add(current);
    out.push(current);
    const row = byId.get(current);
    if (row !== undefined) stack.push(...row.childIds);
  }
  return out;
}

/** Human-readable attributes of a process, as translation key suffixes. */
export function flagKeys(flags: number): readonly string[] {
  const out: string[] = [];
  if (hasFlag(flags, ProcessFlag.Signed)) out.push('signed');
  if (hasFlag(flags, ProcessFlag.SignatureBroken)) out.push('signatureBroken');
  if (hasFlag(flags, ProcessFlag.Elevated)) out.push('elevated');
  if (hasFlag(flags, ProcessFlag.EfficiencyMode)) out.push('efficiencyMode');
  if (hasFlag(flags, ProcessFlag.Wow64)) out.push('wow64');
  if (hasFlag(flags, ProcessFlag.Critical)) out.push('critical');
  if (hasFlag(flags, ProcessFlag.HasWindow)) out.push('hasWindow');
  if (hasFlag(flags, ProcessFlag.Packaged)) out.push('packaged');
  if (hasFlag(flags, ProcessFlag.Preexisting)) out.push('preexisting');
  if (hasFlag(flags, ProcessFlag.ShortLived)) out.push('shortLived');
  if (hasFlag(flags, ProcessFlag.Debugged)) out.push('debugged');
  if (hasFlag(flags, ProcessFlag.Managed)) out.push('managed');
  return out;
}

export { processKeyId };
