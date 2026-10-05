/**
 * App history screen.
 *
 * Displays per-executable resource usage accumulated over time, with
 * explicit messaging that this is Vitals' own history (not Windows' SRUM).
 */

import { Copy, Download, Globe, Info, RefreshCw, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
  DialogContent,
  DialogRoot,
  EmptyState,
  SearchInput,
  SegmentedControl,
  Skeleton,
  Tooltip,
  formatBytes,
} from '@vitals/ui';

import { ExportButton } from '../../components/ExportButton';
import type { ExportColumn, ExportKind } from '../../lib/export';
import { reportFailure } from '../../lib/reportFailure';
import { useRowMenu } from '../../lib/useRowMenu';
import { oneOf, useUrlState } from '../../lib/useUrlState';
import { HISTORY_NS } from './strings';
import {
  computeTotals,
  filterHistory,
  formatCpuTime,
  historySorts,
  sortHistory,
  type AppHistoryRecord,
  type HistorySort,
} from './model';
import { NO_HOST, useAppHistory, type HistoryClearer, type HistoryReader } from './useAppHistory';

export interface AppHistoryScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: HistoryReader;
  readonly clearer?: HistoryClearer;
}

export function AppHistoryScreen({
  reader,
  clearer,
}: AppHistoryScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation(HISTORY_NS);
  const locale = i18n.language;

  const state = useAppHistory(reader, clearer);
  const [view, patchView] = useUrlState<{ q: string; sort: HistorySort }>(
    'appHistory',
    { q: '', sort: 'cpu' },
    { sort: oneOf(historySorts) },
  );
  const query = view.q;
  const sort = view.sort;
  const setQuery = (q: string): void => {
    patchView({ q });
  };
  const [confirmingClear, setConfirmingClear] = useState(false);

  // Memoised rather than written inline as `?? []`: a fresh empty array on
  // every render is a new identity, which changes the dependency below and
  // makes the sort recompute for nothing.
  const records = useMemo(() => state.snapshot?.records ?? [], [state.snapshot?.records]);

  const visible = useMemo(
    () => sortHistory(filterHistory(records, query), sort, locale),
    [records, query, sort, locale],
  );

  // Seconds, bytes and epoch milliseconds — the units the store keeps, so a
  // spreadsheet can sum them. ISO timestamps for the two dates because a
  // 13-digit epoch is unreadable and every tool parses ISO 8601.
  const exportColumns = useMemo(
    (): readonly ExportColumn<AppHistoryRecord>[] => [
      { id: 'name', header: t('column.name'), value: (record) => record.name },
      { id: 'executable', header: t('column.executable'), value: (record) => record.executable },
      { id: 'cpuSeconds', header: t('column.cpuTime'), value: (record) => record.cpuSeconds },
      {
        id: 'diskReadBytes',
        header: t('column.diskRead'),
        value: (record) => record.diskReadBytes,
      },
      {
        id: 'diskWriteBytes',
        header: t('column.diskWrite'),
        value: (record) => record.diskWriteBytes,
      },
      {
        id: 'peakPrivateBytes',
        header: t('column.peakMemory'),
        value: (record) => record.peakPrivateBytes,
      },
      {
        id: 'firstSeen',
        header: t('column.firstSeen'),
        value: (record) => new Date(record.firstSeen).toISOString(),
      },
      {
        id: 'lastSeen',
        header: t('column.lastSeen'),
        value: (record) => new Date(record.lastSeen).toISOString(),
      },
      { id: 'sessions', header: t('column.sessions'), value: (record) => record.sessions },
    ],
    [t],
  );

  if (state.pending) return <HistorySkeleton />;

  if (state.error === NO_HOST) {
    return (
      <EmptyState icon={<Trash2 />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  const handleClear = (): void => {
    setConfirmingClear(false);
    void state.clear();
  };

  // The same serialisers and file name the toolbar's Export button uses, so
  // a file exported from a row menu is indistinguishable from the toolbar's.
  const runExport = (kind: ExportKind): void => {
    void reportFailure(
      import('../../lib/export').then((exporter) => {
        const contents =
          kind === 'csv'
            ? exporter.toCsv(visible, exportColumns)
            : exporter.toJson(visible, exportColumns);
        exporter.saveExport(exporter.exportFilename('app-history', kind), contents, kind);
      }),
      t('menu.exportFailed'),
    );
  };

  return (
    <div className="screen">
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold">{t('title')}</h2>
            <Tooltip
              className="max-w-sm"
              content={
                <>
                  <p className="mb-1.5">{t('about.ours')}</p>
                  <p className="mb-1.5">{t('about.taskManager')}</p>
                  <p>{t('about.starts')}</p>
                </>
              }
            >
              <button
                type="button"
                className="text-[var(--color-fg-muted)] transition-colors hover:text-[var(--color-fg-default)]"
                aria-label={t('about.title')}
              >
                <Info className="size-4" aria-hidden="true" />
              </button>
            </Tooltip>
          </div>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
        </div>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" onClick={state.refresh}>
            <RefreshCw aria-hidden className="size-4" />
            {t('refresh')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setConfirmingClear(true);
            }}
            disabled={records.length === 0}
          >
            <Trash2 aria-hidden className="size-4" />
            {t('clearHistory')}
          </Button>
        </div>
      </header>

      {state.error !== null && state.error !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('stale', { message: state.error })}
        </p>
      )}

      <Summary records={records} locale={locale} />

      <div className="flex flex-wrap items-center gap-2">
        <SearchInput
          value={query}
          onValueChange={setQuery}
          placeholder={t('search')}
          aria-label={t('search')}
          clearLabel={t('clear')}
          className="min-w-56 flex-1"
        />
        <SegmentedControl
          value={sort}
          ariaLabel={t('sortLabel')}
          onValueChange={(next) => {
            patchView({ sort: next });
          }}
          options={historySorts.map((id) => ({ value: id, label: t(`sort.${id}`) }))}
        />
        <ExportButton name="app-history" rows={visible} columns={exportColumns} />
      </div>

      {records.length === 0 ? (
        <EmptyState title={t('empty.title')} description={t('empty.body')} />
      ) : visible.length === 0 ? (
        <EmptyState title={t('filtered.title')} description={t('filtered.body')} />
      ) : (
        <HistoryTable
          records={visible}
          locale={locale}
          actions={{
            onExport: runExport,
            onClear: () => {
              setConfirmingClear(true);
            },
            onRefresh: state.refresh,
          }}
        />
      )}

      <DialogRoot
        open={confirmingClear}
        onOpenChange={(open) => {
          if (!open) setConfirmingClear(false);
        }}
      >
        {confirmingClear && (
          <DialogContent title={t('clearDialog.title')} closeLabel={t('clearDialog.cancel')}>
            <p className="text-sm">{t('clearDialog.body')}</p>
            <div className="mt-4 flex justify-end gap-2">
              <Button
                variant="ghost"
                onClick={() => {
                  setConfirmingClear(false);
                }}
              >
                {t('clearDialog.cancel')}
              </Button>
              <Button onClick={handleClear}>{t('clearDialog.confirm')}</Button>
            </div>
          </DialogContent>
        )}
      </DialogRoot>
    </div>
  );
}

function HistorySkeleton(): React.JSX.Element {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <Skeleton className="h-6 w-32" />
          <Skeleton className="mt-1 h-4 w-96" />
        </div>
        <div className="flex gap-2">
          <Skeleton className="h-8 w-20" />
          <Skeleton className="h-8 w-28" />
        </div>
      </div>
      <Skeleton className="h-12 w-full" />
      <div className="flex gap-2">
        <Skeleton className="h-9 flex-1" />
        <Skeleton className="h-9 w-48" />
      </div>
      <div className="flex flex-col gap-2">
        {Array.from({ length: 8 }, (_, i) => (
          <Skeleton key={i} className="h-12 w-full" />
        ))}
      </div>
    </div>
  );
}

function Summary({
  records,
  locale,
}: {
  readonly records: readonly AppHistoryRecord[];
  readonly locale: string;
}) {
  const { t } = useTranslation(HISTORY_NS);
  if (records.length === 0) return null;

  const totals = computeTotals(records);

  return (
    <div>
      <p className="text-sm">
        {t('summary.count', { count: records.length })} ·{' '}
        {t('summary.totalCpu', { time: formatCpuTime(totals.totalCpuSeconds, locale) })} ·{' '}
        {t('summary.totalDisk', {
          size: formatBytes(totals.totalDiskReadBytes, locale),
          written: formatBytes(totals.totalDiskWriteBytes, locale),
        })}
      </p>
    </div>
  );
}

interface HistoryTableActions {
  readonly onExport: (kind: ExportKind) => void;
  /** Opens the same confirmation as the toolbar button; never clears directly. */
  readonly onClear: () => void;
  readonly onRefresh: () => void;
}

function HistoryTable({
  records,
  locale,
  actions,
}: {
  readonly records: readonly AppHistoryRecord[];
  readonly locale: string;
  readonly actions: HistoryTableActions;
}) {
  const { t } = useTranslation(HISTORY_NS);
  const menu = useRowMenu();

  // Rows carry a top border and the header none, like every other table: the
  // `.table-scroll` frame already draws the outer edge, so a bottom border on
  // the last row doubled it.
  return (
    <div className="table-scroll" onKeyDown={menu.onKeyDown}>
      <table className="w-full text-sm">
        <thead>
          <tr>
            <th className="cell-fill px-3 py-2 text-left font-medium text-[var(--color-fg-muted)]">
              {t('column.name')}
            </th>
            <th className="px-3 py-2 text-right font-medium text-[var(--color-fg-muted)]">
              {t('column.cpuTime')}
            </th>
            <th className="px-3 py-2 text-right font-medium text-[var(--color-fg-muted)]">
              {t('column.diskRead')}
            </th>
            <th className="px-3 py-2 text-right font-medium text-[var(--color-fg-muted)]">
              {t('column.diskWrite')}
            </th>
            <th className="px-3 py-2 text-right font-medium text-[var(--color-fg-muted)]">
              {t('column.peakMemory')}
            </th>
            <th className="px-3 py-2 text-right font-medium text-[var(--color-fg-muted)]">
              {t('column.lastSeen')}
            </th>
            <th className="px-3 py-2 text-right font-medium text-[var(--color-fg-muted)]">
              {t('column.sessions')}
            </th>
          </tr>
        </thead>
        <tbody>
          {records.map((record) => (
            <ContextMenu key={record.executable} {...menu.rootProps(record.executable)}>
              <ContextMenuTrigger asChild>
                <tr
                  className="border-t border-[var(--color-border-subtle)] transition-colors hover:bg-[var(--color-bg-subtle)]"
                  onContextMenu={menu.onContextMenu}
                >
                  <td className="cell-fill px-3 py-2">
                    <div className="flex min-w-0 flex-col">
                      <span className="truncate font-medium">{record.name}</span>
                      <span
                        className="truncate text-2xs text-[var(--color-fg-muted)]"
                        title={record.executable}
                      >
                        {record.executable}
                      </span>
                    </div>
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">
                    {formatCpuTime(record.cpuSeconds, locale)}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">
                    {formatBytes(record.diskReadBytes, locale)}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">
                    {formatBytes(record.diskWriteBytes, locale)}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">
                    {formatBytes(record.peakPrivateBytes, locale)}
                  </td>
                  <td className="px-3 py-2 text-right text-2xs text-[var(--color-fg-muted)]">
                    {new Date(record.lastSeen).toLocaleDateString(locale, {
                      month: 'short',
                      day: 'numeric',
                      hour: '2-digit',
                      minute: '2-digit',
                    })}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums">{record.sessions}</td>
                </tr>
              </ContextMenuTrigger>
              <HistoryMenu record={record} locale={locale} actions={actions} />
            </ContextMenu>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function HistoryMenu({
  record,
  locale,
  actions,
}: {
  readonly record: AppHistoryRecord;
  readonly locale: string;
  readonly actions: HistoryTableActions;
}) {
  const { t } = useTranslation(HISTORY_NS);
  const details = [
    record.name,
    record.executable,
    `${t('column.cpuTime')}: ${formatCpuTime(record.cpuSeconds, locale)}`,
    `${t('column.diskRead')}: ${formatBytes(record.diskReadBytes, locale)}`,
    `${t('column.diskWrite')}: ${formatBytes(record.diskWriteBytes, locale)}`,
    `${t('column.peakMemory')}: ${formatBytes(record.peakPrivateBytes, locale)}`,
    `${t('column.sessions')}: ${String(record.sessions)}`,
  ].join('\n');
  return (
    <ContextMenuContent>
      <ContextMenuLabel>{record.name}</ContextMenuLabel>
      <ContextMenuItem
        onSelect={() => {
          void globalThis.navigator?.clipboard?.writeText(details);
        }}
      >
        <Copy className="size-4" aria-hidden="true" />
        {t('menu.copyDetails')}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => {
          globalThis.open?.(
            `https://duckduckgo.com/?q=${encodeURIComponent(record.name)}`,
            '_blank',
            'noopener,noreferrer',
          );
        }}
      >
        <Globe className="size-4" aria-hidden="true" />
        {t('menu.searchOnline')}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem
        onSelect={() => {
          actions.onExport('csv');
        }}
      >
        <Download className="size-4" aria-hidden="true" />
        {t('menu.exportCsv')}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => {
          actions.onExport('json');
        }}
      >
        <Download className="size-4" aria-hidden="true" />
        {t('menu.exportJson')}
      </ContextMenuItem>
      <ContextMenuItem onSelect={actions.onRefresh}>
        <RefreshCw className="size-4" aria-hidden="true" />
        {t('refresh')}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem destructive onSelect={actions.onClear}>
        <Trash2 className="size-4" aria-hidden="true" />
        {t('clearHistory')}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}
