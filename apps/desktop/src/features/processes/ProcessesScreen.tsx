/**
 * The Processes screen.
 *
 * Composition only: the data shape lives in `model.ts`, the ordering policy in
 * `ordering.ts`, and the risk flow in `RiskDialog.tsx`. Keeping this file to
 * wiring is what makes each of those testable without a DOM.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { EmptyState, Skeleton } from '@vitals/ui';

import type { ExportColumn } from '../../lib/export';
import { oneOf, parseHash, useUrlState } from '../../lib/useUrlState';
import {
  errorMessage,
  tauriProcessActions,
  type ActionPlan,
  type ProcessActionsApi,
  type ProcessPriority,
} from './actions';
import {
  COLUMNS,
  DEFAULT_PREFERENCES,
  loadPreferences,
  savePreferences,
  type ColumnId,
  type TablePreferences,
} from './columns';
import { buildRows, collectSubtree, type KindFilter, type ProcessRow } from './model';
import { ProcessDetails } from './ProcessDetails';
import { ProcessTable } from './ProcessTable';
import { ProcessToolbar } from './ProcessToolbar';
import { RiskDialog, type RiskAction, type RiskRequest } from './RiskDialog';
import { PROCESSES_NS, fallback } from './strings';
import { useHostFacts, type HostFactsReader } from './useHostFacts';
import {
  createTauriSnapshotSource,
  NO_SAMPLER,
  useProcessSnapshot,
  type SnapshotSource,
} from './useProcessSnapshot';
import { useStableOrder } from './useStableOrder';
import { useSettings } from '../../settings/store';

const COLUMN_IDS = COLUMNS.map((column) => column.id);
const KINDS: readonly KindFilter[] = ['all', 'apps', 'background', 'system'];
const DIRECTIONS = ['asc', 'desc'] as const;

export interface ProcessesScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly source?: SnapshotSource;
  readonly actions?: ProcessActionsApi;
  /** Overrides persistence in tests; defaults to `localStorage`. */
  readonly storage?: Storage;
  /** Host info + capability report; injectable so tests need no Tauri host. */
  readonly hostFacts?: HostFactsReader;
}

export function ProcessesScreen({
  source,
  actions = tauriProcessActions,
  storage,
  hostFacts,
}: ProcessesScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const { t: tp } = useTranslation(PROCESSES_NS);
  const locale = i18n.language;
  const confirmEndTask = useSettings((state) => state.settings.confirmEndTask);
  const facts = useHostFacts(hostFacts);

  // A lazy state initialiser rather than a ref written during render: the
  // source must be created exactly once, and a ref read during render is a
  // correctness hazard the compiler rightly rejects.
  const [defaultSource] = useState<SnapshotSource | null>(() =>
    source === undefined ? createTauriSnapshotSource() : null,
  );
  const snapshot = useProcessSnapshot(source ?? (defaultSource as SnapshotSource));

  // The search box is URL-only. Sort, direction and kind are preferences that
  // already persist in localStorage; the URL mirrors them so a reload or a
  // pasted link restores the same view, and when a link names them it wins
  // over what is stored — a link is an explicit request, storage is a habit.
  const [view, patchView] = useUrlState<{
    q: string;
    sort: ColumnId;
    dir: 'asc' | 'desc';
    kind: KindFilter;
  }>(
    'processes',
    {
      q: '',
      sort: DEFAULT_PREFERENCES.sortColumn,
      dir: DEFAULT_PREFERENCES.sortDirection,
      kind: DEFAULT_PREFERENCES.kind,
    },
    { sort: oneOf(COLUMN_IDS), dir: oneOf(DIRECTIONS), kind: oneOf(KINDS) },
  );
  const query = view.q;
  const setQuery = useCallback(
    (q: string) => {
      patchView({ q });
    },
    [patchView],
  );

  const [preferences, setPreferences] = useState<TablePreferences>(() => {
    const stored = storage === undefined ? loadPreferences() : loadPreferences(storage);
    // Only keys the fragment actually names override storage. `view` alone
    // cannot tell "the URL said cpu" from "the URL said nothing and cpu is
    // the default", and the latter must not clobber a stored choice.
    const named = parseHash(window.location.hash);
    if (named.route !== 'processes') return stored;
    return {
      ...stored,
      ...(named.params.has('sort') && { sortColumn: view.sort }),
      ...(named.params.has('dir') && { sortDirection: view.dir }),
      ...(named.params.has('kind') && { kind: view.kind }),
    };
  });
  useEffect(() => {
    savePreferences(preferences, storage);
  }, [preferences, storage]);
  useEffect(() => {
    patchView({
      sort: preferences.sortColumn,
      dir: preferences.sortDirection,
      kind: preferences.kind,
    });
  }, [preferences.sortColumn, preferences.sortDirection, preferences.kind, patchView]);

  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set<string>());
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set<string>());
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const [hovering, setHovering] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [request, setRequest] = useState<RiskRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const pendingRef = useRef<{ action: RiskAction; ids: readonly string[] } | null>(null);

  const built = useMemo(
    () =>
      buildRows({
        processes: snapshot.processes,
        query,
        kind: preferences.kind,
        grouped: preferences.grouped,
        expanded,
      }),
    [snapshot.processes, query, preferences.kind, preferences.grouped, expanded],
  );

  // Positions are held whenever a pointer is over the table or a menu is
  // open. Those are precisely the moments the user is aiming at a row, and a
  // row that moves then is a row you kill by accident.
  const frozen = hovering || menuOpen || request !== null;

  const rows = useStableOrder({
    rows: built.rows,
    column: preferences.sortColumn,
    direction: preferences.sortDirection,
    frozen,
    locale,
  });

  const rowIndex = useMemo(() => {
    const index = new Map<string, number>();
    rows.forEach((row, i) => index.set(row.id, i));
    return index;
  }, [rows]);

  // Read by the action executor. A ref rather than a dependency because the
  // executor must see the rows as they are when the user confirms — the map
  // is rebuilt every tick, and depending on it would recreate every row
  // menu callback once a second.
  const rowsById = useRef(built.byId);
  useEffect(() => {
    rowsById.current = built.byId;
  }, [built.byId]);

  const focusedRow = focusedId === null ? null : (built.byId.get(focusedId) ?? null);

  // The executable path for the focused row. Read once per process identity,
  // not per tick: the sampler carries only the file name because the path
  // costs a handle per process, and this is the one place that cost is paid.
  // `undefined` while unknown so the menu can disable the shell actions
  // rather than offering them and failing.
  const [paths, setPaths] = useState<ReadonlyMap<string, string | null>>(() => new Map());
  const focusedKey = focusedRow === null ? null : focusedRow.id;
  useEffect(() => {
    if (focusedKey === null || paths.has(focusedKey)) return;
    // Through the ref, not `focusedRow`: the snapshot replaces every row
    // object each tick, and depending on it would cancel this read every
    // second before it could resolve.
    const row = rowsById.current.get(focusedKey);
    if (row === undefined) return;
    let live = true;
    void actions
      .getExecutablePath(row.process)
      .catch(() => null)
      .then((path) => {
        if (live) setPaths((current) => new Map(current).set(focusedKey, path));
      });
    return () => {
      live = false;
    };
  }, [focusedKey, paths, actions]);
  const pathOf = useCallback((id: string): string | null => paths.get(id) ?? null, [paths]);

  const onSort = useCallback((column: ColumnId) => {
    setPreferences((current) => ({
      ...current,
      sortColumn: column,
      sortDirection:
        current.sortColumn === column && current.sortDirection === 'desc' ? 'asc' : 'desc',
    }));
  }, []);

  const onResize = useCallback((column: ColumnId, width: number) => {
    setPreferences((current) => ({
      ...current,
      widths: { ...current.widths, [column]: width },
    }));
  }, []);

  const onToggleColumn = useCallback((column: ColumnId) => {
    setPreferences((current) => {
      const set = new Set(current.visible);
      if (set.has(column)) set.delete(column);
      else set.add(column);
      return {
        ...current,
        visible: DEFAULT_PREFERENCES.visible
          .concat(current.visible)
          .filter((id, i, all) => all.indexOf(id) === i && set.has(id)),
      };
    });
  }, []);

  const onToggleExpand = useCallback((id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const selectRow = useCallback(
    (id: string, modifiers: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) => {
      setFocusedId(id);
      setSelected((current) => {
        if (modifiers.ctrlKey || modifiers.metaKey) {
          const next = new Set(current);
          if (next.has(id)) next.delete(id);
          else next.add(id);
          return next;
        }
        if (modifiers.shiftKey && focusedId !== null) {
          const from = rowIndex.get(focusedId);
          const to = rowIndex.get(id);
          if (from !== undefined && to !== undefined) {
            const next = new Set<string>();
            for (let i = Math.min(from, to); i <= Math.max(from, to); i += 1) {
              const row = rows[i];
              if (row !== undefined) next.add(row.id);
            }
            return next;
          }
        }
        return new Set([id]);
      });
    },
    [focusedId, rowIndex, rows],
  );

  const runPending = useCallback(async () => {
    const pending = pendingRef.current;
    if (pending === null) return;
    setBusy(true);
    try {
      // Children first: ending a parent can orphan a child that then
      // re-parents to the session manager and survives the tree kill.
      const ordered = [...pending.ids].reverse();
      for (const id of ordered) {
        const row = rowsById.current.get(id);
        if (row === undefined) continue;
        if (pending.action === 'suspend') await actions.suspend(row.process);
        else await actions.terminate(row.process);
      }
    } catch (error) {
      setFailure(errorMessage(error));
    } finally {
      setBusy(false);
      setRequest(null);
      pendingRef.current = null;
    }
  }, [actions]);

  const beginAction = useCallback(
    async (action: RiskAction, row: ProcessRow) => {
      setFailure(null);
      const ids = action === 'terminate-tree' ? collectSubtree(built.byId, row.id) : [row.id];
      pendingRef.current = { action, ids };

      let plan: ActionPlan;
      try {
        plan =
          action === 'suspend'
            ? await actions.planSuspend(row.process)
            : await actions.planTerminate(row.process);
      } catch (error) {
        setFailure(errorMessage(error));
        pendingRef.current = null;
        return;
      }

      // A `forbidden` plan is still shown, and deliberately so: silently
      // doing nothing would look like a broken menu item, and the user
      // deserves to be told that Windows — not a missing privilege — is what
      // stops this. The dialog renders no confirm button in that case.
      //
      // "Confirm before ending a task" off means the low-risk confirmation is
      // skipped; a plan the backend marks high-risk or forbidden is still
      // shown, because that setting is about convenience, not about
      // suppressing a warning that the process is critical.
      const skipConfirm = !confirmEndTask && plan.risk !== 'critical';
      if ((!plan.needsConfirmation || skipConfirm) && plan.risk !== 'forbidden') {
        await runPending();
        return;
      }

      setRequest({
        action,
        plan,
        processName: row.process.name,
        childCount: Math.max(0, ids.length - 1),
      });
    },
    [actions, built.byId, confirmEndTask, runPending],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      if (rows.length === 0) return;
      const currentIndex = focusedId === null ? -1 : (rowIndex.get(focusedId) ?? -1);

      const move = (next: number): void => {
        const clamped = Math.max(0, Math.min(rows.length - 1, next));
        const row = rows[clamped];
        if (row === undefined) return;
        event.preventDefault();
        setFocusedId(row.id);
        if (!event.ctrlKey) setSelected(new Set([row.id]));
      };

      switch (event.key) {
        case 'ArrowDown':
          move(currentIndex + 1);
          break;
        case 'ArrowUp':
          move(currentIndex - 1);
          break;
        case 'Home':
          move(0);
          break;
        case 'End':
          move(rows.length - 1);
          break;
        case 'PageDown':
          move(currentIndex + 20);
          break;
        case 'PageUp':
          move(currentIndex - 20);
          break;
        case 'ArrowRight':
          if (
            focusedRow !== null &&
            focusedRow.childIds.length > 0 &&
            !expanded.has(focusedRow.id)
          ) {
            event.preventDefault();
            onToggleExpand(focusedRow.id);
          }
          break;
        case 'ArrowLeft':
          if (focusedRow !== null && expanded.has(focusedRow.id)) {
            event.preventDefault();
            onToggleExpand(focusedRow.id);
          }
          break;
        case 'Delete':
          // Delete goes through the same plan-then-confirm path as the menu.
          // A destructive shortcut with its own shorter path is how people
          // end up bugchecking a machine by leaning on a key.
          if (focusedRow !== null) {
            event.preventDefault();
            void beginAction('terminate', focusedRow);
          }
          break;
        default:
          break;
      }
    },
    [rows, rowIndex, focusedId, focusedRow, expanded, onToggleExpand, beginAction],
  );

  const menuProps = useCallback(
    (row: ProcessRow) => ({
      process: row.process,
      descendantCount: row.descendantCount,
      executablePath: pathOf(row.id),
      onTerminate: () => void beginAction('terminate', row),
      onTerminateTree: () => void beginAction('terminate-tree', row),
      onSuspend: () => void beginAction('suspend', row),
      onResume: () => {
        void actions.resume(row.process).catch((error: unknown) => setFailure(errorMessage(error)));
      },
      // No confirmation. Priority is reversible, takes effect immediately and
      // is undone by choosing another — the plan-then-confirm path exists for
      // actions that destroy work, and putting a dialog in front of this one
      // would dilute the ones that matter.
      //
      // Realtime is the arguable exception, but its own menu label carries
      // the warning, which is a better place for it than a modal the user
      // learns to dismiss.
      onSetPriority: (priority: ProcessPriority) => {
        void actions
          .setPriority(row.process, priority)
          .catch((error: unknown) => setFailure(errorMessage(error)));
      },
      onOpenFileLocation: () => {
        const path = pathOf(row.id);
        if (path === null) return;
        void actions
          .openFileLocation(path)
          .catch((error: unknown) =>
            setFailure(tp('detail.file.openFailed', { message: errorMessage(error) })),
          );
      },
      onShowProperties: () => {
        const path = pathOf(row.id);
        if (path === null) return;
        void actions
          .showFileProperties(path)
          .catch((error: unknown) =>
            setFailure(tp('detail.file.propertiesFailed', { message: errorMessage(error) })),
          );
      },
      onSearchOnline: () => {
        globalThis.open?.(
          `https://duckduckgo.com/?q=${encodeURIComponent(row.process.name)}`,
          '_blank',
          'noopener,noreferrer',
        );
      },
      onCopyDetails: () => {
        void globalThis.navigator?.clipboard?.writeText(
          `${row.process.name}\tPID ${row.process.key.pid}\t${row.process.user ?? '—'}`,
        );
      },
    }),
    [actions, beginAction, pathOf, tp],
  );

  const total = snapshot.processes.size;

  // Which counter feeds the Disk column. `null` before the first sample has
  // told us; the tooltip then says so rather than picking a label.
  const diskTitle = useMemo(() => {
    const source = facts.capabilities?.diskCounterSource ?? null;
    if (source === 'storageStack') return tp('disk.storageStack');
    if (source === 'allIo') return tp('disk.allIo');
    return tp('disk.unknown');
  }, [facts.capabilities, tp]);
  const columnTitles = useMemo(() => ({ disk: diskTitle }), [diskTitle]);

  // Raw units: CPU as a fraction of the machine, bytes, bytes per second,
  // seconds. The rolled-up figures are what the row displays, so a collapsed
  // parent exports the same total the user was looking at. Network, GPU and
  // handles stay `null` where they were not measured.
  const exportColumns = useMemo(
    (): readonly ExportColumn<ProcessRow>[] => [
      { id: 'name', header: t('process.name'), value: (row) => row.process.name },
      { id: 'pid', header: t('process.pid'), value: (row) => row.process.key.pid },
      { id: 'state', header: t('process.status'), value: (row) => row.process.state },
      { id: 'user', header: t('process.user'), value: (row) => row.process.user },
      {
        id: 'cpuFraction',
        header: t('process.column.cpu', fallback('process.column.cpu')),
        value: (row) => row.rolledCpu,
      },
      {
        id: 'memoryBytes',
        header: t('process.column.memory', fallback('process.column.memory')),
        value: (row) => row.rolledMemory,
      },
      {
        id: 'diskBytesPerSecond',
        header: t('process.column.disk', fallback('process.column.disk')),
        value: (row) => row.rolledDisk,
      },
      {
        id: 'networkBytesPerSecond',
        header: t('process.column.network', fallback('process.column.network')),
        value: (row) => row.rolledNetwork,
      },
      {
        id: 'gpuFraction',
        header: t('process.column.gpu', fallback('process.column.gpu')),
        value: (row) => row.rolledGpu,
      },
      { id: 'threads', header: t('process.threads'), value: (row) => row.process.threadCount },
      { id: 'handles', header: t('process.handles'), value: (row) => row.process.handleCount },
      { id: 'uptimeSeconds', header: t('process.uptime'), value: (row) => row.process.uptimeSecs },
    ],
    [t],
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <ProcessToolbar
        query={query}
        onQueryChange={setQuery}
        kind={preferences.kind}
        onKindChange={(kind: KindFilter) => setPreferences((c) => ({ ...c, kind }))}
        grouped={preferences.grouped}
        onGroupedChange={(grouped) => setPreferences((c) => ({ ...c, grouped }))}
        visible={preferences.visible}
        onToggleColumn={onToggleColumn}
        shown={built.matchCount}
        total={total}
        orderHeld={frozen}
        exportRows={rows}
        exportColumns={exportColumns}
      />

      {failure !== null && (
        <p role="alert" className="px-3 py-1.5 text-2xs text-[var(--color-status-danger)]">
          {t('process.error.failed', fallback('process.error.failed'), { message: failure })}
        </p>
      )}

      <div className="flex min-h-0 flex-1">
        {snapshot.pending ? (
          <div className="flex-1 space-y-1 p-3" aria-busy="true">
            {Array.from({ length: 12 }, (_, i) => (
              <Skeleton key={i} className="h-6 w-full" />
            ))}
          </div>
        ) : rows.length === 0 ? (
          <div className="flex flex-1 items-center justify-center">
            {/*
             * "No process matches" is only true when a filter excluded them.
             * With no sampler there are no rows to match in the first place,
             * and telling the user to clear their search sends them looking
             * for a mistake they did not make.
             */}
            {snapshot.error === NO_SAMPLER ? (
              <EmptyState
                title={t('process.noSampler.title', fallback('process.noSampler.title'))}
                description={t('process.noSampler.body', fallback('process.noSampler.body'))}
              />
            ) : (
              <EmptyState
                title={t('process.empty.title', fallback('process.empty.title'))}
                description={t('process.empty.body', fallback('process.empty.body'))}
              />
            )}
          </div>
        ) : (
          <ProcessTable
            rows={rows}
            columns={preferences.visible}
            widths={preferences.widths}
            sortColumn={preferences.sortColumn}
            sortDirection={preferences.sortDirection}
            onSort={onSort}
            onResize={onResize}
            selected={selected}
            focusedId={focusedId}
            expanded={expanded}
            onToggleExpand={onToggleExpand}
            onRowPointerDown={selectRow}
            onFocusRow={setFocusedId}
            onKeyDown={onKeyDown}
            onHoverChange={setHovering}
            onMenuOpenChange={setMenuOpen}
            menuProps={menuProps}
            locale={locale}
            columnTitles={columnTitles}
          />
        )}

        <ProcessDetails
          row={focusedRow}
          locale={locale}
          actions={actions}
          host={facts.host}
          executablePath={focusedRow === null ? null : pathOf(focusedRow.id)}
          onFailure={setFailure}
        />
      </div>

      <RiskDialog
        request={request}
        busy={busy}
        onCancel={() => {
          setRequest(null);
          pendingRef.current = null;
        }}
        onConfirm={() => {
          void runPending();
        }}
      />
    </div>
  );
}
