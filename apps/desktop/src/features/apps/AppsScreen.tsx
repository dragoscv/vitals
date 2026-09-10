/**
 * Installed applications.
 *
 * # Uninstall opens the vendor's uninstaller and nothing else
 *
 * Vitals does not delete files, remove registry keys, or "clean up" leftovers.
 * Deciding which files belong to a product is guesswork, and a wrong guess is
 * unrecoverable data loss in a tool the user opened to make their computer
 * better. The confirmation dialog says so explicitly, because "uninstall" in
 * a third-party app reasonably makes people wonder what exactly is doing the
 * removing.
 *
 * # The filtered-out count is shown
 *
 * A registry scan examines thousands of subkeys and discards most of them.
 * Without the breakdown, a well-filtered scan and a broken one look identical
 * — both produce a short list. "900 Windows components" is the filter working;
 * "900 with no name" would mean the scan itself failed.
 */

import { RefreshCw, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  DialogContent,
  DialogRoot,
  EmptyState,
  SearchInput,
  SegmentedControl,
  Skeleton,
  formatBytes,
} from '@vitals/ui';

import { ExportButton } from '../../components/ExportButton';
import type { ExportColumn } from '../../lib/export';
import { oneOf, useUrlState } from '../../lib/useUrlState';
import { APPS_NS } from './strings';
import {
  appSorts,
  canUninstall,
  declaredTotal,
  filterApps,
  sortApps,
  type AppSort,
  type InstalledApp,
} from './model';
import { NO_HOST, runUninstaller, useApps, type AppsReader, type Uninstaller } from './useApps';

export interface AppsScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: AppsReader;
  readonly uninstall?: Uninstaller;
}

export function AppsScreen({
  reader,
  uninstall = runUninstaller,
}: AppsScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation(APPS_NS);
  const locale = i18n.language;

  const state = useApps(reader);
  // Search and sort live in the URL fragment so a reload restores the view.
  const [view, patchView] = useUrlState<{ q: string; sort: AppSort }>(
    'installedApps',
    { q: '', sort: 'name' },
    { sort: oneOf(appSorts) },
  );
  const query = view.q;
  const sort = view.sort;
  const setQuery = (q: string): void => {
    patchView({ q });
  };
  const [confirming, setConfirming] = useState<InstalledApp | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // The `?? []` fallback goes inside the memo. Outside it, the empty array
  // is a fresh identity on every render, so the memo's dependency changes
  // every time and it recomputes a sort of the whole list for nothing.
  const visible = useMemo(
    () => sortApps(filterApps(state.snapshot?.apps ?? [], query), sort, locale),
    [state.snapshot?.apps, query, sort, locale],
  );

  const exportColumns = useMemo(
    (): readonly ExportColumn<InstalledApp>[] => [
      { id: 'name', header: t('column.name'), value: (app) => app.name },
      { id: 'publisher', header: t('column.publisher'), value: (app) => app.publisher },
      { id: 'version', header: t('column.version'), value: (app) => app.version },
      { id: 'installDate', header: t('column.installed'), value: (app) => app.installDate },
      // Bytes as the installer wrote them, or empty. Never the formatted
      // "1.2 GB" and never 0 for "did not report".
      { id: 'estimatedSizeBytes', header: t('column.size'), value: (app) => app.estimatedSize },
      {
        id: 'installLocation',
        header: t('column.location'),
        value: (app) => app.installLocation,
      },
      { id: 'source', header: t('column.source'), value: (app) => app.source },
      { id: 'perUser', header: t('column.perUser'), value: (app) => app.perUser },
    ],
    [t],
  );

  if (state.pending) return <AppsSkeleton />;

  if (state.error === NO_HOST) {
    return (
      <EmptyState icon={<Trash2 />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  const startUninstall = (app: InstalledApp): void => {
    setConfirming(null);
    void uninstall(app.uninstallString ?? '')
      .then(() => {
        setNotice(t('uninstall.started', { name: app.name }));
      })
      .catch((cause: unknown) => {
        setNotice(
          t('uninstall.failed', {
            message: cause instanceof Error ? cause.message : String(cause),
          }),
        );
      });
  };

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('title')}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
        </div>
        <Button variant="ghost" size="sm" onClick={state.refresh}>
          <RefreshCw aria-hidden className="size-4" />
          {t('refresh')}
        </Button>
      </header>

      {state.error !== null && state.error !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('stale', { message: state.error })}
        </p>
      )}

      {notice !== null && (
        <p role="status" className="text-2xs text-[var(--color-fg-muted)]">
          {notice}
        </p>
      )}

      <Summary snapshot={state.snapshot} locale={locale} />

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
          options={appSorts.map((id) => ({ value: id, label: t(`sort.${id}`) }))}
        />
        <ExportButton name="installed-apps" rows={visible} columns={exportColumns} />
      </div>

      {visible.length === 0 ? (
        <EmptyState title={t('empty.title')} description={t('empty.body')} />
      ) : (
        <AppTable
          apps={visible}
          locale={locale}
          onUninstall={(app) => {
            setConfirming(app);
          }}
        />
      )}

      <DialogRoot
        open={confirming !== null}
        onOpenChange={(open) => {
          if (!open) setConfirming(null);
        }}
      >
        {confirming !== null && (
          <DialogContent
            title={t('uninstall.title', { name: confirming.name })}
            // Distinct from the Cancel button below. Two controls in one
            // dialog sharing an accessible name is ambiguous for a screen
            // reader — "Cancel, button. Cancel, button." gives no way to tell
            // which is the dismiss affordance.
            closeLabel={t('uninstall.close')}
          >
            <p className="text-sm">{t('uninstall.body')}</p>
            {/* Stated plainly, because "uninstall" in a third-party tool
                reasonably makes people wonder what is doing the removing. */}
            <p className="mt-1.5 text-2xs text-[var(--color-fg-muted)]">
              {t('uninstall.bodyDetail')}
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Button
                variant="ghost"
                onClick={() => {
                  setConfirming(null);
                }}
              >
                {t('uninstall.cancel')}
              </Button>
              <Button
                onClick={() => {
                  startUninstall(confirming);
                }}
              >
                {t('uninstall.confirm')}
              </Button>
            </div>
          </DialogContent>
        )}
      </DialogRoot>
    </div>
  );
}

function Summary({
  snapshot,
  locale,
}: {
  readonly snapshot: ReturnType<typeof useApps>['snapshot'];
  readonly locale: string;
}) {
  const { t } = useTranslation(APPS_NS);
  if (snapshot === null) return null;

  const totals = declaredTotal(snapshot.apps);
  const breakdown = snapshot.rejectedByReason
    .map((rejection) => t(`reject.${rejection.reason}`, { count: rejection.count }))
    .join(', ');

  return (
    <div>
      <p className="text-sm">
        {t('summary.count', { count: snapshot.apps.length })} ·{' '}
        {t('summary.declared', {
          size: formatBytes(totals.bytes, locale),
          withSize: totals.withSize,
        })}
      </p>
      {totals.withoutSize > 0 && (
        <p className="mt-0.5 text-2xs text-[var(--color-fg-muted)]">
          {t('summary.noSize', { count: totals.withoutSize })} — {t('summary.sizeHint')}
        </p>
      )}
      {/* Without this a well-filtered scan and a broken one look identical:
          both produce a short list. The breakdown is what tells them apart. */}
      {snapshot.rejected > 0 && (
        <p className="mt-0.5 text-2xs text-[var(--color-fg-subtle)]">
          {t('summary.filtered', {
            rejected: snapshot.rejected,
            examined: snapshot.examined,
            breakdown,
          })}
          {snapshot.duplicatesCollapsed > 0 &&
            ` ${t('summary.duplicates', { count: snapshot.duplicatesCollapsed })}.`}
        </p>
      )}
    </div>
  );
}

function AppTable({
  apps,
  locale,
  onUninstall,
}: {
  readonly apps: readonly InstalledApp[];
  readonly locale: string;
  readonly onUninstall: (app: InstalledApp) => void;
}) {
  const { t } = useTranslation(APPS_NS);
  const dateFormat = useMemo(
    () => new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }),
    [locale],
  );

  return (
    <div className="overflow-x-auto rounded-md border border-[var(--color-border-subtle)]">
      <table className="w-full text-left">
        <thead>
          <tr className="text-2xs text-[var(--color-fg-muted)]">
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.name')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.version')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.installed')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 text-right font-normal">
              {t('column.size')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              <span className="sr-only">{t('uninstall.action')}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {apps.map((app) => (
            <tr key={app.keyName} className="border-t border-[var(--color-border-subtle)]">
              <td className="px-2.5 py-1.5">
                <span className="block truncate text-sm">{app.name}</span>
                <span className="block truncate text-2xs text-[var(--color-fg-subtle)]">
                  {app.publisher ?? t(`source.${app.source}`)}
                  {app.perUser && (
                    <Badge tone="neutral" className="ml-1.5">
                      {t('perUser')}
                    </Badge>
                  )}
                </span>
              </td>
              <td className="px-2.5 py-1.5 font-mono text-2xs">{app.version ?? '—'}</td>
              <td className="px-2.5 py-1.5 text-2xs">
                {app.installDate === null
                  ? t('unknownDate')
                  : // Parsed as a local date. The registry records no time and
                    // no zone, so anything richer would invent precision.
                    dateFormat.format(new Date(`${app.installDate}T00:00:00`))}
              </td>
              <td className="tnum px-2.5 py-1.5 text-right font-mono text-2xs">
                {app.estimatedSize === null
                  ? t('unknownSize')
                  : formatBytes(app.estimatedSize, locale)}
              </td>
              <td className="px-2.5 py-1.5 text-right">
                <Button
                  variant="ghost"
                  size="sm"
                  // Disabled rather than hidden: the user can see the option
                  // exists and that this product does not offer it, instead of
                  // wondering why some rows have a button and others do not.
                  disabled={!canUninstall(app)}
                  title={canUninstall(app) ? undefined : t('uninstall.unavailable')}
                  onClick={() => {
                    onUninstall(app);
                  }}
                >
                  {t('uninstall.action')}
                </Button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AppsSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="space-y-2">
      <Skeleton className="h-7 w-64" />
      <Skeleton className="h-9 w-full" />
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <Skeleton key={index} className="h-10" />
      ))}
    </div>
  );
}
