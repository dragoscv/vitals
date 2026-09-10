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

import {
  errorMessage,
  tauriProcessActions,
  type ActionPlan,
  type ProcessActionsApi,
  type ProcessPriority,
} from './actions';
import {
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
import { fallback } from './strings';
import {
  createTauriSnapshotSource,
  NO_SAMPLER,
  useProcessSnapshot,
  type SnapshotSource,
} from './useProcessSnapshot';
import { useStableOrder } from './useStableOrder';

export interface ProcessesScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly source?: SnapshotSource;
  readonly actions?: ProcessActionsApi;
  /** Overrides persistence in tests; defaults to `localStorage`. */
  readonly storage?: Storage;
}

export function ProcessesScreen({
  source,
  actions = tauriProcessActions,
  storage,
}: ProcessesScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;

  // A lazy state initialiser rather than a ref written during render: the
  // source must be created exactly once, and a ref read during render is a
  // correctness hazard the compiler rightly rejects.
  const [defaultSource] = useState<SnapshotSource | null>(() =>
    source === undefined ? createTauriSnapshotSource() : null,
  );
  const snapshot = useProcessSnapshot(source ?? (defaultSource as SnapshotSource));

  const [preferences, setPreferences] = useState<TablePreferences>(() =>
    storage === undefined ? loadPreferences() : loadPreferences(storage),
  );
  useEffect(() => {
    savePreferences(preferences, storage);
  }, [preferences, storage]);

  const [query, setQuery] = useState('');
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
      if (!plan.needsConfirmation && plan.risk !== 'forbidden') {
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
    [actions, built.byId, runPending],
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
    [actions, beginAction],
  );

  const total = snapshot.processes.size;

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
          />
        )}

        <ProcessDetails row={focusedRow} locale={locale} />
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
