/**
 * App history screen.
 *
 * Displays per-executable resource usage accumulated over time, with
 * explicit messaging that this is Vitals' own history (not Windows' SRUM).
 */

import { Info, RefreshCw, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Button,
  DialogContent,
  DialogRoot,
  EmptyState,
  SearchInput,
  SegmentedControl,
  Skeleton,
  Tooltip,
  formatBytes,
} from '@vitals/ui';

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
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<HistorySort>('cpu');
  const [confirmingClear, setConfirmingClear] = useState(false);

  const records = state.snapshot?.records ?? [];
  const visible = useMemo(
    () => sortHistory(filterHistory(records, query), sort, locale),
    [records, query, sort, locale],
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

  return (
    <div className="flex flex-col gap-4">
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
                className="text-[var(--color-fg-muted)] transition-colors hover:text-[var(--color-fg-base)]"
                aria-label={t('about.title')}
              >
                <Info className="size-4" />
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
            setSort(next as HistorySort);
          }}
          options={historySorts.map((id) => ({ value: id, label: t(`sort.${id}`) }))}
        />
      </div>

      {records.length === 0 ? (
        <EmptyState title={t('empty.title')} description={t('empty.body')} />
      ) : visible.length === 0 ? (
        <EmptyState title={t('filtered.title')} description={t('filtered.body')} />
      ) : (
        <HistoryTable records={visible} locale={locale} />
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

function HistoryTable({
  records,
  locale,
}: {
  readonly records: readonly AppHistoryRecord[];
  readonly locale: string;
}) {
  const { t } = useTranslation(HISTORY_NS);

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-[var(--color-border-base)]">
            <th className="px-3 py-2 text-left font-medium text-[var(--color-fg-muted)]">
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
            <tr
              key={record.executable}
              className="border-b border-[var(--color-border-subtle)] transition-colors hover:bg-[var(--color-bg-subtle)]"
            >
              <td className="px-3 py-2">
                <div className="flex flex-col">
                  <span className="font-medium">{record.name}</span>
                  <span className="text-2xs max-w-md truncate text-[var(--color-fg-muted)]">
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
              <td className="text-2xs px-3 py-2 text-right text-[var(--color-fg-muted)]">
                {new Date(record.lastSeen).toLocaleDateString(locale, {
                  month: 'short',
                  day: 'numeric',
                  hour: '2-digit',
                  minute: '2-digit',
                })}
              </td>
              <td className="px-3 py-2 text-right tabular-nums">{record.sessions}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
