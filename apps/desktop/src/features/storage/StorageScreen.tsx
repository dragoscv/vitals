/**
 * Disk storage.
 *
 * # Nothing here deletes anything
 *
 * The cleanup section measures and explains; there is no removal backend and
 * the Delete control is rendered disabled with the reason attached. A button
 * that looks live and does nothing is worse than one that says why it cannot:
 * the first teaches the user the app is broken, the second teaches them what
 * it does.
 *
 * # Unmeasured is stated, never drawn as zero
 *
 * A cache that exists but could not be sized shows "Not measured" plus which
 * of the two causes applies — needs elevation, or unreadable for another
 * reason. Rendering it as 0 B would talk someone out of reclaiming real
 * space, and the whole point of the sizing pass is to avoid that.
 *
 * # An incomplete scan is labelled at every level
 *
 * A total that silently omits an unreadable folder is a wrong number
 * presented as a right one. So the header carries the skipped count, and each
 * affected row carries its own marker — the summary alone cannot tell the
 * user *which* of the folders in front of them is a floor.
 */

import { HardDrive, RefreshCw, Sparkles, X } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  EmptyState,
  Meter,
  ProgressBar,
  SearchInput,
  SegmentedControl,
  Skeleton,
  formatBytes,
  formatCount,
} from '@vitals/ui';
import { AnimatedValue } from '@vitals/ui';

import { ExportButton } from '../../components/ExportButton';
import type { ExportColumn } from '../../lib/export';
import { oneOf, useUrlState } from '../../lib/useUrlState';
import { STORAGE_NS } from './strings';
import {
  directorySorts,
  filterDirectories,
  groupBySafety,
  needsQualifier,
  reclaimableTotal,
  sortDirectories,
  sortVolumes,
  unmeasuredReason,
  usedBytes,
  usedPercent,
  type CleanupCandidate,
  type DirectoryEntry,
  type DirectorySort,
  type ScanProgress,
  type ScanSnapshot,
  type Volume,
} from './model';
import { NO_HOST, useStorage, type StorageSource } from './useStorage';

export interface StorageScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly source?: StorageSource;
}

export function StorageScreen({ source }: StorageScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation(STORAGE_NS);
  const locale = i18n.language;

  const state = useStorage(source);
  const [selected, setSelected] = useState<string | null>(null);
  // Only the result table's view goes in the URL. The chosen volume is a scan
  // parameter, not view state: restoring it from a link would imply the scan
  // itself was restored, and it is not.
  const [view, patchView] = useUrlState<{ q: string; sort: DirectorySort }>(
    'storage',
    { q: '', sort: 'allocated' },
    { sort: oneOf(directorySorts) },
  );
  const query = view.q;
  const sort = view.sort;

  const volumes = useMemo(() => sortVolumes(state.volumes), [state.volumes]);
  // Falls back to the first volume so the scan button is usable immediately,
  // rather than requiring a click that only reselects what is already shown.
  const activeMount = selected ?? volumes[0]?.mount ?? null;

  const rows = useMemo(
    () => sortDirectories(filterDirectories(state.snapshot?.largest ?? [], query), sort, locale),
    [state.snapshot, query, sort, locale],
  );

  if (state.pending) return <StorageSkeleton />;

  if (state.volumeError === NO_HOST) {
    return (
      <EmptyState icon={<HardDrive />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  return (
    <div className="screen">
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('title')}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
        </div>
        <Button variant="ghost" size="sm" onClick={state.refreshVolumes}>
          <RefreshCw aria-hidden className="size-4" />
          {/* Its own label, not `scan.rescan`: this re-reads the drive list
              while the other button re-walks the tree, and two buttons with
              one accessible name is ambiguous for a screen reader. */}
          {t('volumes.refresh')}
        </Button>
      </header>

      {state.volumeError !== null && state.volumeError !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('scan.stale', { message: state.volumeError })}
        </p>
      )}

      {/*
       * Drives and the scan controls stay at the top; below them the scan
       * result and the clean-up list sit side by side, each scrolling inside
       * its own card (S12-26). Before, the three scrolled together, so the
       * Clean-up button was below the fold as soon as a scan had results.
       * Small windows stack everything and the body scrolls instead.
       */}
      <div className="screen-body grid-rows-[auto_minmax(0,1fr)_minmax(0,1fr)] @5xl/main:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] @5xl/main:grid-rows-[auto_minmax(0,1fr)]">
        <div className="flex flex-col gap-3 @5xl/main:col-span-2">
          <Volumes
            volumes={volumes}
            activeMount={activeMount}
            locale={locale}
            onSelect={setSelected}
            disabled={state.scanning}
          />

          <div className="flex flex-wrap items-center gap-2">
            {state.scanning ? (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  state.cancelScan();
                }}
              >
                <X aria-hidden className="size-4" />
                {t('scan.cancel')}
              </Button>
            ) : (
              <Button
                size="sm"
                disabled={activeMount === null}
                onClick={() => {
                  if (activeMount !== null) state.scan(activeMount);
                }}
              >
                {state.snapshot === null ? t('scan.start') : t('scan.rescan')}
              </Button>
            )}
          </div>

          {state.scanError !== null && (
            <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
              {/* Phrased as "the last completed scan" when one survives, because
              the previous snapshot is deliberately still on screen. */}
              {state.snapshot === null
                ? t('scan.failed', { message: state.scanError })
                : t('scan.stale', { message: state.scanError })}
            </p>
          )}

          {state.scanning && (
            <ScanRunning
              root={state.scanRoot ?? ''}
              progress={state.progress}
              volume={volumes.find((volume) => volume.mount === state.scanRoot) ?? null}
              locale={locale}
            />
          )}
        </div>

        {state.snapshot === null ? (
          !state.scanning && (
            <div className="pane">
              <EmptyState title={t('scan.idleTitle')} description={t('scan.idleBody')} />
            </div>
          )
        ) : (
          <ScanResult
            snapshot={state.snapshot}
            locale={locale}
            query={query}
            onQueryChange={(q) => {
              patchView({ q });
            }}
            sort={sort}
            onSortChange={(next) => {
              patchView({ sort: next });
            }}
            rows={rows}
          />
        )}

        <Cleanup
          candidates={state.candidates}
          running={state.cleanupRunning}
          error={state.cleanupError}
          locale={locale}
          onScan={state.findCleanup}
          onCancel={state.cancelCleanup}
        />
      </div>
    </div>
  );
}

/**
 * The running scan: files, bytes and rate, climbing as they arrive.
 *
 * Measured against the drive's used space when the scan root is a whole
 * drive, so the bar is real. For a folder there is no known total, and the
 * bar stays indeterminate rather than invent one. The fraction can overshoot
 * a little — hard links and compression make the walk's figure differ from
 * the volume's — so it is capped rather than trusted to the last percent.
 */
function ScanRunning({
  root,
  progress,
  volume,
  locale,
}: {
  readonly root: string;
  readonly progress: ScanProgress | null;
  readonly volume: Volume | null;
  readonly locale: string;
}) {
  const { t } = useTranslation(STORAGE_NS);
  const used = volume === null ? 0 : usedBytes(volume);
  const fraction = progress !== null && used > 0 ? Math.min(0.99, progress.bytesSeen / used) : null;
  const rate =
    progress !== null && progress.elapsedMs > 0
      ? Math.round((progress.filesSeen * 1000) / progress.elapsedMs)
      : null;

  return (
    <div role="status" className="flex flex-col gap-1.5">
      <p className="text-sm">{t('scan.running', { root })}</p>
      {/* Visible figures roll; the live region above names only the root so
          it is announced once, not ten times a second. */}
      <p aria-hidden className="tnum text-2xs text-[var(--color-fg-muted)]">
        {progress === null || rate === null ? (
          t('scan.progressStarting')
        ) : (
          <AnimatedValue
            value={t('scan.progress', {
              files: formatCount(progress.filesSeen, locale),
              size: formatBytes(progress.bytesSeen, locale),
              rate: formatCount(rate, locale),
            })}
          />
        )}
      </p>
      <ProgressBar
        {...(fraction === null ? { indeterminate: true } : { value: fraction * 100 })}
        label={t('scan.running', { root })}
      />
      {progress !== null && (
        <p
          aria-hidden
          className="truncate font-mono text-2xs text-[var(--color-fg-subtle)]"
          title={progress.currentPath}
        >
          {t('scan.now', { path: progress.currentPath })}
        </p>
      )}
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('scan.runningDetail')}</p>
    </div>
  );
}

function Volumes({
  volumes,
  activeMount,
  locale,
  onSelect,
  disabled,
}: {
  readonly volumes: readonly Volume[];
  readonly activeMount: string | null;
  readonly locale: string;
  readonly onSelect: (mount: string) => void;
  /** While a scan runs: changing the selection would detach it from the scan. */
  readonly disabled: boolean;
}) {
  const { t } = useTranslation(STORAGE_NS);

  if (volumes.length === 0) {
    return <EmptyState title={t('volumes.heading')} description={t('volumes.empty')} />;
  }

  return (
    <section aria-label={t('volumes.pick')}>
      <h3 className="mb-2 text-sm font-medium">{t('volumes.heading')}</h3>
      <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
        {volumes.map((volume) => {
          const percent = usedPercent(volume);
          const active = volume.mount === activeMount;

          return (
            <button
              key={volume.mount}
              type="button"
              aria-pressed={active}
              disabled={disabled}
              onClick={() => {
                onSelect(volume.mount);
              }}
              className={`rounded-md border p-2.5 text-left ${
                active
                  ? 'border-[var(--color-accent-border)] bg-[var(--color-accent-subtle)]'
                  : 'border-[var(--color-border-subtle)]'
              }`}
            >
              <div className="flex items-baseline justify-between gap-2">
                <span className="truncate text-sm font-medium">
                  {volume.label === null
                    ? volume.mount
                    : t('volumes.label', { mount: volume.mount, label: volume.label })}
                </span>
                <span className="text-2xs text-[var(--color-fg-subtle)]">
                  {t(`kind.${volume.kind}`)}
                </span>
              </div>

              {/* Not a meter at 0%: an unreported capacity and a genuinely
                  empty drive are different facts, and a full-looking or
                  empty-looking bar asserts one of them without evidence. */}
              {percent === null ? (
                <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">
                  {t('volumes.unknownCapacity')}
                </p>
              ) : (
                <Meter
                  className="mt-1.5"
                  label={t('volumes.free', { free: formatBytes(volume.available, locale) })}
                  accessibleLabel={t('volumes.used', {
                    used: formatBytes(usedBytes(volume), locale),
                    total: formatBytes(volume.total, locale),
                  })}
                  value={percent}
                  valueText={t('volumes.used', {
                    used: formatBytes(usedBytes(volume), locale),
                    total: formatBytes(volume.total, locale),
                  })}
                  tone={percent >= 90 ? 'danger' : percent >= 75 ? 'warn' : 'accent'}
                />
              )}

              {volume.strategy === 'mftAssisted' && (
                <Badge tone="info" className="mt-1.5" title={t('volumes.fastHint')}>
                  {t('volumes.fast')}
                </Badge>
              )}
            </button>
          );
        })}
      </div>
      <p className="mt-1.5 text-2xs text-[var(--color-fg-subtle)]">{t('volumes.scanHint')}</p>
    </section>
  );
}

function ScanResult({
  snapshot,
  locale,
  query,
  onQueryChange,
  sort,
  onSortChange,
  rows,
}: {
  readonly snapshot: ScanSnapshot;
  readonly locale: string;
  readonly query: string;
  readonly onQueryChange: (value: string) => void;
  readonly sort: DirectorySort;
  readonly onSortChange: (value: DirectorySort) => void;
  readonly rows: readonly DirectoryEntry[];
}) {
  const { t } = useTranslation(STORAGE_NS);
  const elevationFixable = snapshot.skipped.filter((entry) => entry.elevationFixable).length;

  // Bytes and counts, not "1.4 GB": the point of exporting a disk scan is to
  // sort and sum it somewhere else. `incomplete` travels as the reason key so
  // a floor is never mistaken for a measurement in the spreadsheet either.
  const exportColumns = useMemo(
    (): readonly ExportColumn<DirectoryEntry>[] => [
      { id: 'path', header: t('column.path'), value: (entry) => entry.path },
      { id: 'allocatedBytes', header: t('column.allocated'), value: (entry) => entry.allocated },
      { id: 'logicalBytes', header: t('column.logical'), value: (entry) => entry.logical },
      { id: 'files', header: t('column.files'), value: (entry) => entry.files },
      { id: 'incomplete', header: t('incomplete'), value: (entry) => entry.incomplete },
    ],
    [t],
  );

  return (
    <Card className="pane">
      <CardHeader>
        <CardTitle level={3}>{t('result.heading')}</CardTitle>
      </CardHeader>
      <CardBody className="flex min-h-0 flex-1 flex-col gap-3">
        <div>
          <p className="text-sm">
            {t('result.total', {
              allocated: formatBytes(snapshot.allocated, locale),
              files: formatCount(snapshot.filesScanned, locale),
            })}
          </p>
          {/* Both figures, always. Showing only one and letting the user
              compare it against Explorer is how they conclude we are wrong. */}
          <p className="text-2xs text-[var(--color-fg-muted)]" title={t('result.logicalHint')}>
            {t('result.logical', { logical: formatBytes(snapshot.logical, locale) })} ·{' '}
            {t('result.elapsed', { seconds: Math.round(snapshot.elapsedMs / 100) / 10 })}
          </p>
        </div>

        {needsQualifier(snapshot) && (
          <div className="flex flex-col gap-1 text-2xs text-[var(--color-status-warn)]">
            {snapshot.cancelled && <p>{t('result.cancelled')}</p>}
            {snapshot.clusterBytes === null && <p>{t('result.noCluster')}</p>}
            {snapshot.skippedTotal > 0 && (
              <p>
                {t('result.incompleteScan', { count: snapshot.skippedTotal })}
                {elevationFixable > 0 &&
                  ` ${t('result.elevationWould', { count: elevationFixable })}`}
              </p>
            )}
          </div>
        )}

        {snapshot.linksNotFollowed > 0 && (
          <p className="text-2xs text-[var(--color-fg-subtle)]" title={t('result.linksHint')}>
            {t('result.links', {
              count: snapshot.linksNotFollowed,
              n: formatCount(snapshot.linksNotFollowed, locale),
            })}
          </p>
        )}

        {snapshot.hardLinkDuplicates > 0 && (
          <p className="text-2xs text-[var(--color-fg-subtle)]">
            {t('result.dedup', {
              count: snapshot.hardLinkDuplicates,
              size: formatBytes(snapshot.hardLinkBytesSaved, locale),
            })}
          </p>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <SearchInput
            value={query}
            onValueChange={onQueryChange}
            placeholder={t('search')}
            aria-label={t('search')}
            clearLabel={t('clear')}
            className="min-w-56 flex-1"
          />
          <SegmentedControl
            value={sort}
            ariaLabel={t('sortLabel')}
            onValueChange={(next) => {
              onSortChange(next);
            }}
            options={directorySorts.map((id) => ({ value: id, label: t(`sort.${id}`) }))}
          />
          <ExportButton name="storage" rows={rows} columns={exportColumns} />
        </div>

        {rows.length === 0 ? (
          <EmptyState
            title={query === '' ? t('result.emptyTitle') : t('result.filterEmpty')}
            description={query === '' ? t('result.emptyBody') : t('result.filterEmptyBody')}
          />
        ) : (
          <div className="pane-scroll rounded-md border border-[var(--color-border-subtle)]">
            <table className="w-full text-left">
              <thead>
                <tr className="text-2xs text-[var(--color-fg-muted)]">
                  <th scope="col" className="cell-fill px-2.5 py-1.5 font-normal">
                    {t('column.path')}
                  </th>
                  <th scope="col" className="px-2.5 py-1.5 text-right font-normal">
                    {t('column.allocated')}
                  </th>
                  <th scope="col" className="px-2.5 py-1.5 text-right font-normal">
                    {t('column.logical')}
                  </th>
                  <th scope="col" className="px-2.5 py-1.5 text-right font-normal">
                    {t('column.files')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {rows.map((entry) => (
                  <tr key={entry.path} className="border-t border-[var(--color-border-subtle)]">
                    <td className="cell-fill px-2.5 py-1.5">
                      <span className="block truncate text-sm" title={entry.path}>
                        {entry.path}
                      </span>
                      {/* Per-row, not just in the header count: the summary
                          cannot tell the user WHICH figure is a floor. */}
                      {entry.incomplete !== null && (
                        <Badge
                          tone="warn"
                          className="mt-0.5"
                          title={t('incompleteHint', {
                            reason: t(`skip.${entry.incomplete}`),
                          })}
                        >
                          {t('incomplete')}
                        </Badge>
                      )}
                    </td>
                    <td className="tnum px-2.5 py-1.5 text-right font-mono text-2xs">
                      {formatBytes(entry.allocated, locale)}
                    </td>
                    <td className="tnum px-2.5 py-1.5 text-right font-mono text-2xs text-[var(--color-fg-subtle)]">
                      {formatBytes(entry.logical, locale)}
                    </td>
                    <td className="tnum px-2.5 py-1.5 text-right font-mono text-2xs">
                      {formatCount(entry.files, locale)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardBody>
    </Card>
  );
}

function Cleanup({
  candidates,
  running,
  error,
  locale,
  onScan,
  onCancel,
}: {
  readonly candidates: readonly CleanupCandidate[] | null;
  readonly running: boolean;
  readonly error: string | null;
  readonly locale: string;
  readonly onScan: () => void;
  readonly onCancel: () => void;
}) {
  const { t } = useTranslation(STORAGE_NS);
  const total = useMemo(() => reclaimableTotal(candidates ?? []), [candidates]);
  const groups = useMemo(() => groupBySafety(candidates ?? []), [candidates]);

  return (
    <Card className="pane">
      <CardHeader
        actions={
          running ? (
            <Button variant="ghost" size="sm" onClick={onCancel}>
              <X aria-hidden className="size-4" />
              {t('cleanup.cancel')}
            </Button>
          ) : (
            <Button variant="ghost" size="sm" onClick={onScan}>
              <Sparkles aria-hidden className="size-4" />
              {candidates === null ? t('cleanup.scan') : t('cleanup.rescan')}
            </Button>
          )
        }
      >
        <CardTitle level={3}>{t('cleanup.heading')}</CardTitle>
      </CardHeader>
      <CardBody className="pane-scroll flex flex-col gap-3">
        {error !== null && (
          <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
            {t('cleanup.failed', { message: error })}
          </p>
        )}

        {running && (
          <div role="status">
            <p className="text-sm">{t('cleanup.running')}</p>
            {/* Labelled by the line above: one announcement, not two. */}
            <ProgressBar indeterminate label={t('cleanup.heading')} className="mt-1.5" />
          </div>
        )}

        {candidates === null ? (
          !running && (
            <EmptyState title={t('cleanup.idleTitle')} description={t('cleanup.idleBody')} />
          )
        ) : candidates.length === 0 ? (
          <EmptyState title={t('cleanup.emptyTitle')} description={t('cleanup.emptyBody')} />
        ) : (
          <>
            <div>
              {/* "At least" whenever something could not be measured — the
                  headline must not read as a complete figure when it is a
                  floor by an unknown amount. */}
              <p className="text-sm font-medium">
                {t(total.unmeasured > 0 ? 'cleanup.totalFloor' : 'cleanup.total', {
                  size: formatBytes(total.bytes, locale),
                })}
              </p>
              <p className="text-2xs text-[var(--color-fg-muted)]">{t('cleanup.totalScope')}</p>
              {total.unmeasured > 0 && (
                <p className="text-2xs text-[var(--color-status-warn)]">
                  {t('cleanup.unmeasured', { count: total.unmeasured })}
                </p>
              )}
            </div>

            {groups.map((group) => (
              <section key={group.safety} aria-label={t(`safety.${group.safety}`)}>
                <h4 className="text-sm font-medium">{t(`safety.${group.safety}`)}</h4>
                <p className="mb-1.5 text-2xs text-[var(--color-fg-muted)]">
                  {t(`safety.${group.safety}Body`)}
                </p>
                <ul className="flex flex-col gap-1.5">
                  {group.items.map((candidate) => (
                    <CandidateRow key={candidate.path} candidate={candidate} locale={locale} />
                  ))}
                </ul>
              </section>
            ))}
          </>
        )}
      </CardBody>
    </Card>
  );
}

function CandidateRow({
  candidate,
  locale,
}: {
  readonly candidate: CleanupCandidate;
  readonly locale: string;
}) {
  const { t } = useTranslation(STORAGE_NS);
  const reason = unmeasuredReason(candidate);

  return (
    <li className="flex flex-wrap items-start justify-between gap-2 rounded-md border border-[var(--color-border-subtle)] p-2">
      <div className="min-w-0 flex-1">
        <p className="text-sm">{t(`kindLabel.${candidate.kind}`)}</p>
        <p className="truncate font-mono text-2xs text-[var(--color-fg-subtle)]">
          {candidate.path}
        </p>
        <p className="mt-0.5 text-2xs text-[var(--color-fg-muted)]">
          {t(`kindReason.${candidate.kind}`)}
        </p>
        {/* Which of the two causes applies, not merely that one does: without
            it the user has no idea whether elevating would help. */}
        {reason !== null && (
          <p className="mt-0.5 text-2xs text-[var(--color-status-warn)]">
            {reason === 'needsElevation'
              ? t('cleanup.reasonNeedsElevation')
              : t('cleanup.reasonUnreadable')}
          </p>
        )}
      </div>

      <div className="flex items-center gap-2">
        <span className="tnum font-mono text-2xs">
          {candidate.size === null ? t('cleanup.unknownSize') : formatBytes(candidate.size, locale)}
        </span>
        {candidate.needsElevation && <Badge tone="warn">{t('cleanup.needsElevation')}</Badge>}
        {/* Disabled with the reason attached rather than hidden or faked.
            There is no deletion backend, and a live-looking button that does
            nothing teaches the user the app is broken. */}
        <Button variant="ghost" size="sm" disabled title={t('cleanup.deleteUnavailable')}>
          {t('cleanup.delete')}
        </Button>
      </div>
    </li>
  );
}

function StorageSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="space-y-2">
      <Skeleton className="h-7 w-48" />
      <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
        {[0, 1, 2].map((index) => (
          <Skeleton key={index} className="h-20" />
        ))}
      </div>
      <Skeleton className="h-9 w-full" />
      {[0, 1, 2, 3].map((index) => (
        <Skeleton key={index} className="h-10" />
      ))}
    </div>
  );
}
