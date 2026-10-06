/**
 * Developer cleanup: scan project folders for regenerable build output,
 * then choose what goes.
 *
 * # Nothing is chosen for the user except the obvious
 *
 * Only folders of projects idle 30+ days are ticked after a scan. Everything
 * else waits for a tick, and the confirmation lists every item with the
 * command that brings it back.
 *
 * # Permanent means permanent
 *
 * These folders are deleted, not recycled: a 40 GB `target` in the Recycle
 * Bin frees nothing. So the dialog says so in words and needs its own tick
 * before the button works; the backend refuses an unconfirmed run as well.
 *
 * # Unmeasured is not zero
 *
 * A folder that could not be read renders "Not measured", never "0 B", and
 * totals say how many figures they are missing.
 */

import {
  ChevronDown,
  ChevronRight,
  Copy,
  FolderPlus,
  FolderSearch,
  Search,
  Square,
  SquareCheck,
  Trash2,
  X,
} from 'lucide-react';
import { useMemo, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  BrandMark,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  Checkbox,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
  DialogContent,
  DialogRoot,
  EmptyState,
  IconButton,
  ProgressBar,
  formatBytes,
  formatCount,
} from '@vitals/ui';

import { errorMessage } from '../../../lib/commandError';
import { useRowMenu, type RowMenu } from '../../../lib/useRowMenu';
import { STORAGE_NS } from '../strings';
import {
  idleKey,
  isStale,
  measure,
  projectCheckState,
  sectionIds,
  selectableLabelKey,
  selectables,
  selectionTotal,
  sortProjects,
  stopsWsl,
  type DevCleanItem,
  type DevCleanReport,
  type DevOutcomeKey,
  type DevScan,
  type DevScanProgress,
  type DevSectionKey,
  type Project,
  type Selectable,
  type Worktree,
} from './model';
import {
  tauriDevSource,
  useDevClean,
  type DevCleanSource,
  type DevCleanState,
  type DevRun,
} from './useDevClean';

export interface DevCleanProps {
  readonly locale: string;
  /** Injectable so tests need no Tauri host. */
  readonly source?: DevCleanSource;
}

/** Typing only: the storage namespace's `t`, passed down rather than re-hooked per row. */
function useStorageT() {
  return useTranslation(STORAGE_NS).t;
}
type T = ReturnType<typeof useStorageT>;

export function DevClean({ locale, source }: DevCleanProps): React.JSX.Element {
  const t = useStorageT();
  const state = useDevClean(source);
  const [reviewing, setReviewing] = useState<readonly Selectable[] | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);
  const revealWith = (source ?? tauriDevSource).reveal;
  const reveal =
    revealWith === undefined
      ? undefined
      : (path: string) => {
          setRevealError(null);
          // Reported rather than dropped: the user asked for a window and
          // nothing appeared, which needs a reason.
          revealWith(path).catch((cause: unknown) => {
            setRevealError(errorMessage(cause));
          });
        };

  const label = useMemo(() => (kind: string) => t(selectableLabelKey(kind)), [t]);
  const items = useMemo(
    () => (state.scan === null ? [] : selectables(state.scan, (kind) => label(kind))),
    [state.scan, label],
  );

  return (
    <Card className="pane">
      <CardHeader actions={<ScanButton state={state} t={t} />}>
        <CardTitle level={3}>{t('dev.title')}</CardTitle>
      </CardHeader>
      <CardBody className="pane-scroll flex flex-col gap-3">
        <p className="text-2xs text-[var(--color-fg-muted)]">{t('dev.intro')}</p>
        {state.noHost ? (
          <p className="text-sm">{t('dev.noHost')}</p>
        ) : (
          <>
            <Roots state={state} t={t} />
            {state.scanError !== null && (
              <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
                {state.scan === null
                  ? t('dev.scan.failed', { message: state.scanError })
                  : t('dev.scan.stale', { message: state.scanError })}
              </p>
            )}
            {revealError !== null && (
              <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
                {t('explore.failed', { message: revealError })}
              </p>
            )}
            {state.scanning && <ScanRunning progress={state.progress} locale={locale} t={t} />}
            {state.scan === null ? (
              !state.scanning && (
                <EmptyState title={t('dev.title')} description={t('dev.scan.idle')} />
              )
            ) : (
              <Results
                scan={state.scan}
                state={state}
                locale={locale}
                t={t}
                {...(reveal !== undefined && { reveal })}
              />
            )}
          </>
        )}
      </CardBody>
      {state.scan !== null && (
        <Summary
          items={items}
          selected={state.selected}
          locale={locale}
          t={t}
          onReview={() => {
            state.dismissRun();
            setReviewing(items.filter((item) => state.selected.has(item.id)));
          }}
        />
      )}
      {reviewing !== null && state.scan !== null && (
        <ConfirmDialog
          scan={state.scan}
          items={reviewing}
          run={state.run}
          locale={locale}
          t={t}
          onConfirm={() => {
            void state.clean(reviewing.map((item) => item.id));
          }}
          onClose={() => {
            setReviewing(null);
            state.dismissRun();
          }}
        />
      )}
    </Card>
  );
}

function ScanButton({ state, t }: { readonly state: DevCleanState; readonly t: T }) {
  if (state.noHost) return null;
  if (state.scanning) {
    return (
      <Button variant="ghost" size="sm" onClick={state.cancelScan}>
        <X aria-hidden className="size-4" />
        {t('dev.scan.stop')}
      </Button>
    );
  }
  return (
    <Button
      variant="ghost"
      size="sm"
      disabled={state.rootsPending || state.roots.length === 0 || state.run?.running === true}
      onClick={state.startScan}
    >
      <Search aria-hidden className="size-4" />
      {state.scan === null ? t('dev.scan.start') : t('dev.scan.rescan')}
    </Button>
  );
}

function Roots({ state, t }: { readonly state: DevCleanState; readonly t: T }) {
  return (
    <section aria-label={t('dev.roots.heading')} className="flex flex-col gap-1.5">
      <h4 className="text-2xs font-medium text-[var(--color-fg-muted)]">
        {t('dev.roots.heading')}
      </h4>
      {state.rootsError !== null && (
        <p role="alert" className="text-2xs text-[var(--color-status-warn)]">
          {t('dev.roots.failed', { message: state.rootsError })}
        </p>
      )}
      {state.rootsPending ? (
        <p role="status" className="text-2xs">
          {t('dev.roots.loading')}
        </p>
      ) : (
        <ul className="flex flex-wrap items-center gap-1.5">
          {state.roots.length === 0 && <li className="text-2xs">{t('dev.roots.empty')}</li>}
          {state.roots.map((root) => (
            <li
              key={root}
              className="flex items-center gap-1 rounded-md border border-[var(--color-border-subtle)] py-0.5 pr-0.5 pl-2 font-mono text-2xs"
            >
              {root}
              <IconButton
                size="sm"
                icon={<X aria-hidden />}
                label={t('dev.roots.remove', { path: root })}
                disabled={state.scanning}
                onClick={() => {
                  state.removeRoot(root);
                }}
              />
            </li>
          ))}
          <li>
            <Button variant="ghost" size="sm" disabled={state.scanning} onClick={state.addFolders}>
              <FolderPlus aria-hidden className="size-4" />
              {t('dev.roots.add')}
            </Button>
          </li>
        </ul>
      )}
    </section>
  );
}

function ScanRunning({
  progress,
  locale,
  t,
}: {
  readonly progress: DevScanProgress | null;
  readonly locale: string;
  readonly t: T;
}) {
  return (
    <div role="status" className="flex flex-col gap-1">
      <p className="flex items-center gap-2 text-sm">
        <BrandMark size={20} state="thinking" />
        {progress === null ? t('dev.scan.running') : t(`dev.phase.${progress.phase}`)}
      </p>
      <ProgressBar indeterminate label={t('dev.scan.running')} />
      {progress !== null && (
        <p className="tnum text-2xs text-[var(--color-fg-muted)]">
          {t('dev.scan.found', { count: progress.found, n: formatCount(progress.found, locale) })}
        </p>
      )}
      {progress?.currentPath !== null && progress?.currentPath !== undefined && (
        <p className="truncate font-mono text-2xs text-[var(--color-fg-subtle)]">
          {t('dev.scan.now', { path: progress.currentPath })}
        </p>
      )}
    </div>
  );
}

/** "12 GB", "12 GB plus 2 not measured", or "Not measured" — never "0 B" for unknown. */
function totalText(sizes: readonly (number | null)[], locale: string, t: T): string {
  const m = measure(sizes);
  if (m.count > 0 && m.unmeasured === m.count) return t('dev.notMeasured');
  const size = formatBytes(m.bytes, locale);
  return m.unmeasured > 0
    ? t('dev.totalFloor', { size, n: formatCount(m.unmeasured, locale) })
    : t('dev.total', { size });
}

function sizeText(size: number | null, locale: string, t: T): string {
  return size === null ? t('dev.notMeasured') : formatBytes(size, locale);
}

interface ResultsProps {
  readonly scan: DevScan;
  readonly state: DevCleanState;
  readonly locale: string;
  readonly t: T;
  readonly reveal?: (path: string) => void;
}

function Results({ scan, state, locale, t, reveal }: ResultsProps) {
  const [onlyStale, setOnlyStale] = useState(false);
  const menu = useRowMenu();
  const projects = useMemo(() => {
    const sorted = sortProjects(scan.projects);
    return onlyStale ? sorted.filter(isStale) : sorted;
  }, [scan.projects, onlyStale]);
  const ids = useMemo(() => sectionIds(scan, projects), [scan, projects]);
  const empty =
    scan.projects.length === 0 &&
    scan.worktrees.length === 0 &&
    scan.caches.length === 0 &&
    scan.docker.items.length === 0 &&
    scan.docker.state === 'ok' &&
    scan.vdisks.length === 0;

  const report = state.lastReport;
  const line = (id: string | null) =>
    id === null || report === null ? undefined : report.items.find((item) => item.id === id);
  const row: RowContext = { state, locale, t, line, menu, ...(reveal !== undefined && { reveal }) };

  return (
    <div className="flex flex-col gap-4" onKeyDown={menu.onKeyDown}>
      <p className="text-2xs text-[var(--color-fg-subtle)]">
        {t('dev.scan.elapsed', { seconds: Math.round(scan.elapsedMs / 1000) })}
      </p>
      {report !== null && <DriveGains report={report} locale={locale} t={t} />}
      {empty && <EmptyState title={t('dev.title')} description={t('dev.scan.nothing')} />}

      <Section
        section="projects"
        ids={ids.projects}
        total={projects.flatMap((p) => p.artefacts.map((a) => a.size))}
        row={row}
        extra={
          <Checkbox
            checked={onlyStale}
            onCheckedChange={(next) => {
              setOnlyStale(next === true);
            }}
            label={t('dev.project.onlyStale')}
          />
        }
      >
        {projects.length === 0 ? (
          <p className="text-2xs">
            {onlyStale && scan.projects.length > 0
              ? t('dev.project.filteredOut')
              : t('dev.project.none')}
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {projects.map((project) => (
              <ProjectRow key={project.path} project={project} row={row} />
            ))}
          </ul>
        )}
      </Section>

      <Section
        section="worktrees"
        ids={ids.worktrees}
        total={scan.worktrees.filter((w) => w.id !== null).map((w) => w.size)}
        row={row}
      >
        {scan.worktrees.length === 0 ? (
          <p className="text-2xs">{t('dev.worktree.none')}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {scan.worktrees.map((worktree) => (
              <WorktreeRow key={worktree.path} worktree={worktree} row={row} />
            ))}
          </ul>
        )}
      </Section>

      <Section
        section="caches"
        ids={ids.caches}
        total={scan.caches.filter((c) => c.id !== null).map((c) => c.size)}
        row={row}
      >
        {scan.caches.length === 0 ? (
          <p className="text-2xs">{t('dev.cacheNone')}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {scan.caches.map((cache) => (
              <Row
                key={cache.path}
                id={cache.id}
                ariaLabel={t('dev.cacheSelect', { label: t(`dev.cache.${cache.kind}`) })}
                title={t(`dev.cache.${cache.kind}`)}
                path={cache.path}
                revealable
                size={cache.id === null ? null : sizeText(cache.size, locale, t)}
                row={row}
              >
                {cache.id === null ? (
                  <p className="text-2xs text-[var(--color-fg-muted)]">{t('dev.cacheMissing')}</p>
                ) : (
                  <>
                    <Command label={t('dev.runs')} command={cache.command} />
                    <Command label={t('dev.restore')} command={cache.restore} />
                  </>
                )}
              </Row>
            ))}
          </ul>
        )}
      </Section>

      <Section
        section="docker"
        ids={ids.docker}
        total={scan.docker.items.filter((d) => d.id !== null).map((d) => d.reclaimable)}
        row={row}
      >
        <DockerBody scan={scan} row={row} />
      </Section>

      <Section section="vdisks" ids={ids.vdisks} total={scan.vdisks.map((v) => v.size)} row={row}>
        {scan.vdisks.length === 0 ? (
          <p className="text-2xs">{t('dev.vdiskNone')}</p>
        ) : (
          <>
            <p className="text-2xs text-[var(--color-fg-muted)]">{t('dev.vdiskNote')}</p>
            <ul className="flex flex-col gap-1">
              {scan.vdisks.map((disk) => (
                <Row
                  key={disk.id}
                  id={disk.id}
                  ariaLabel={t('dev.vdiskSelect', {
                    label: t(`dev.vdisk.${disk.kind}`),
                    path: disk.path,
                  })}
                  title={t(`dev.vdisk.${disk.kind}`)}
                  path={disk.path}
                  revealable
                  size={t('dev.vdiskSize', { size: formatBytes(disk.size, locale) })}
                  row={row}
                >
                  {disk.distro !== null && (
                    <p className="text-2xs">{t('dev.vdiskDistro', { distro: disk.distro })}</p>
                  )}
                </Row>
              ))}
            </ul>
          </>
        )}
      </Section>
    </div>
  );
}

interface RowContext {
  readonly state: DevCleanState;
  readonly locale: string;
  readonly t: T;
  readonly line: (id: string | null) => DevCleanItem | undefined;
  /** One menu for the whole card, so only one row's menu is ever open. */
  readonly menu: RowMenu;
  readonly reveal?: (path: string) => void;
}

/**
 * The right-click menu of a row: the same tick the checkbox offers, then
 * where the thing is. Built here once so every section's rows agree.
 */
function RowMenuContent({
  label,
  row,
  check,
  path,
  revealable,
  children,
}: {
  readonly label: string;
  readonly row: RowContext;
  /** Absent when the row cannot be ticked (missing cache, Docker volumes). */
  readonly check?: { readonly on: boolean; readonly set: (on: boolean) => void };
  readonly path: string;
  /** False when `path` is a command rather than a place on disk. */
  readonly revealable: boolean;
  readonly children?: ReactNode;
}) {
  const { state, t, reveal } = row;
  return (
    <ContextMenuContent className="min-w-52">
      <ContextMenuLabel>{label}</ContextMenuLabel>
      <ContextMenuSeparator />
      {children}
      {check !== undefined && (
        <ContextMenuItem
          disabled={state.run?.running === true}
          onSelect={() => {
            check.set(!check.on);
          }}
        >
          {check.on ? (
            <Square className="size-4" aria-hidden="true" />
          ) : (
            <SquareCheck className="size-4" aria-hidden="true" />
          )}
          {check.on ? t('dev.menu.deselect') : t('dev.menu.select')}
        </ContextMenuItem>
      )}
      {(check !== undefined || children !== undefined) && <ContextMenuSeparator />}
      {revealable && reveal !== undefined && (
        <ContextMenuItem
          onSelect={() => {
            reveal(path);
          }}
        >
          <FolderSearch className="size-4" aria-hidden="true" />
          {t('explore.menu.reveal')}
        </ContextMenuItem>
      )}
      <ContextMenuItem
        onSelect={() => {
          void globalThis.navigator?.clipboard?.writeText(path);
        }}
      >
        <Copy className="size-4" aria-hidden="true" />
        {revealable ? t('explore.menu.copy') : t('dev.menu.copyCommand')}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

function Section({
  section,
  ids,
  total,
  row,
  extra,
  children,
}: {
  readonly section: DevSectionKey;
  readonly ids: readonly string[];
  readonly total: readonly (number | null)[];
  readonly row: RowContext;
  readonly extra?: ReactNode;
  readonly children: ReactNode;
}) {
  const { state, locale, t } = row;
  const name = t(`dev.section.${section}`);
  const busy = state.run?.running === true;
  return (
    <section aria-label={name} className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h4 className="text-sm font-medium">
          {name}
          <span className="tnum ml-2 text-2xs font-normal text-[var(--color-fg-muted)]">
            {totalText(total, locale, t)}
          </span>
        </h4>
        <div className="flex flex-wrap items-center gap-1">
          {extra}
          {ids.length > 0 && (
            <>
              <Button
                variant="ghost"
                size="sm"
                disabled={busy}
                onClick={() => {
                  state.setMany(ids, true);
                }}
              >
                {t('dev.selectAll', { section: name })}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                disabled={busy}
                onClick={() => {
                  state.setMany(ids, false);
                }}
              >
                {t('dev.selectNone', { section: name })}
              </Button>
            </>
          )}
        </div>
      </div>
      {children}
    </section>
  );
}

/** One line of a section: an optional tick, what it is, where, and how big. */
function Row({
  id,
  ariaLabel,
  title,
  path,
  revealable = false,
  size,
  row,
  children,
}: {
  readonly id: string | null;
  readonly ariaLabel: string;
  readonly title: string;
  readonly path: string;
  /** True when `path` is a place on disk rather than a command. */
  readonly revealable?: boolean;
  readonly size: string | null;
  readonly row: RowContext;
  readonly children?: ReactNode;
}) {
  const { state, locale, t, line, menu } = row;
  const report = line(id);
  return (
    <ContextMenu {...menu.rootProps(`row:${id ?? path}`)}>
      <ContextMenuTrigger asChild>
        <li
          tabIndex={0}
          data-testid="dev-row"
          className="flex items-start gap-2 rounded-md border border-[var(--color-border-subtle)] p-2 focus-visible:outline-2 focus-visible:outline-[var(--color-accent)]"
          onContextMenu={menu.onContextMenu}
        >
          {id === null ? (
            <span aria-hidden className="size-4 shrink-0" />
          ) : (
            <Checkbox
              className="mt-0.5"
              ariaLabel={ariaLabel}
              checked={state.selected.has(id)}
              disabled={state.run?.running === true}
              onCheckedChange={(next) => {
                state.toggle(id, next === true);
              }}
            />
          )}
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <div className="flex items-baseline justify-between gap-2">
              <span className="text-sm">{title}</span>
              {size !== null && <span className="tnum shrink-0 text-2xs">{size}</span>}
            </div>
            <p className="font-mono text-2xs break-all text-[var(--color-fg-subtle)]">{path}</p>
            {children}
            {report !== undefined && <ReportLine item={report} locale={locale} t={t} />}
          </div>
        </li>
      </ContextMenuTrigger>
      <RowMenuContent
        label={title}
        row={row}
        path={path}
        revealable={revealable}
        {...(id !== null && {
          check: {
            on: state.selected.has(id),
            set: (on: boolean) => {
              state.toggle(id, on);
            },
          },
        })}
      />
    </ContextMenu>
  );
}

function Command({ label, command }: { readonly label: string; readonly command: string }) {
  return (
    <p className="text-2xs text-[var(--color-fg-muted)]">
      {label} <code className="font-mono text-[var(--color-fg-default)]">{command}</code>
    </p>
  );
}

function idleText(project: Project, t: T): string {
  switch (idleKey(project)) {
    case 'unknown':
      return t('dev.project.idleUnknown');
    case 'active':
      return t('dev.project.active');
    case 'idle':
      return t('dev.project.idle', { count: project.idleDays ?? 0 });
  }
}

function ProjectRow({ project, row }: { readonly project: Project; readonly row: RowContext }) {
  const { state, locale, t, menu } = row;
  const [open, setOpen] = useState(false);
  const check = projectCheckState(project, state.selected);
  const ids = project.artefacts.map((a) => a.id);
  return (
    <li className="rounded-md border border-[var(--color-border-subtle)]">
      {/* The header, not the whole item, is the trigger: the artefact rows
          inside have menus of their own, and a right click on one must not
          bubble up and open this one as well. */}
      <ContextMenu {...menu.rootProps(`project:${project.path}`)}>
        <ContextMenuTrigger asChild>
          <div
            tabIndex={0}
            data-testid="dev-project"
            className="flex items-center gap-2 rounded-md p-2 focus-visible:outline-2 focus-visible:outline-[var(--color-accent)]"
            onContextMenu={menu.onContextMenu}
          >
            <IconButton
              size="sm"
              icon={open ? <ChevronDown aria-hidden /> : <ChevronRight aria-hidden />}
              aria-expanded={open}
              label={
                open
                  ? t('dev.project.hide', { name: project.name })
                  : t('dev.project.show', { name: project.name })
              }
              onClick={() => {
                setOpen((v) => !v);
              }}
            />
            <Checkbox
              ariaLabel={t('dev.project.select', { name: project.name })}
              checked={check}
              disabled={state.run?.running === true}
              onCheckedChange={() => {
                state.setMany(ids, check !== true);
              }}
            />
            <div className="flex min-w-0 flex-1 flex-col">
              <div className="flex items-baseline justify-between gap-2">
                <span className="truncate text-sm font-medium">{project.name}</span>
                <span className="tnum shrink-0 text-2xs">
                  {totalText(
                    project.artefacts.map((a) => a.size),
                    locale,
                    t,
                  )}
                </span>
              </div>
              <p className="flex flex-wrap gap-x-2 text-2xs text-[var(--color-fg-muted)]">
                <span>{idleText(project, t)}</span>
                <span>{project.isGit ? t('dev.project.git') : t('dev.project.notGit')}</span>
                <span className="font-mono break-all text-[var(--color-fg-subtle)]">
                  {project.path}
                </span>
              </p>
            </div>
          </div>
        </ContextMenuTrigger>
        <RowMenuContent
          label={project.name}
          row={row}
          path={project.path}
          revealable
          check={{
            on: check === true,
            set: (on) => {
              state.setMany(ids, on);
            },
          }}
        >
          <ContextMenuItem
            onSelect={() => {
              setOpen((v) => !v);
            }}
          >
            {open ? (
              <ChevronDown className="size-4" aria-hidden="true" />
            ) : (
              <ChevronRight className="size-4" aria-hidden="true" />
            )}
            {open ? t('dev.menu.collapse') : t('dev.menu.expand')}
          </ContextMenuItem>
        </RowMenuContent>
      </ContextMenu>
      {open && (
        <ul className="flex flex-col gap-1 px-2 pb-2 pl-10">
          {project.artefacts.map((artefact) => {
            const kind = t(`dev.artefact.${artefact.kind}`);
            return (
              <Row
                key={artefact.id}
                id={artefact.id}
                ariaLabel={t('dev.artefactSelect', { label: kind, path: artefact.path })}
                title={kind}
                path={artefact.path}
                revealable
                size={sizeText(artefact.size, locale, t)}
                row={row}
              >
                <Command label={t('dev.restore')} command={artefact.restore} />
                {artefact.sharedWithPnpmStore && (
                  <p className="text-2xs text-[var(--color-status-warn)]">{t('dev.sharedPnpm')}</p>
                )}
              </Row>
            );
          })}
        </ul>
      )}
    </li>
  );
}

function worktreeReason(worktree: Worktree, t: T): string {
  switch (worktree.state) {
    case 'removable':
      return t('dev.worktree.removable');
    case 'prunable':
      return t('dev.worktree.prunable');
    case 'dirty':
      return worktree.changes === null
        ? t('dev.worktree.dirtyUnknown')
        : t('dev.worktree.dirty', { count: worktree.changes });
    case 'unpushed':
      return t('dev.worktree.unpushed');
    case 'active':
      return worktree.idleHours === null
        ? t('dev.worktree.activeUnknown')
        : t('dev.worktree.active', { count: Math.max(0, Math.round(worktree.idleHours)) });
    case 'locked':
      return t('dev.worktree.locked');
  }
}

function WorktreeRow({ worktree, row }: { readonly worktree: Worktree; readonly row: RowContext }) {
  const { locale, t } = row;
  const branch =
    worktree.branch === null
      ? t('dev.worktree.detached')
      : t('dev.worktree.branch', { branch: worktree.branch });
  return (
    <Row
      id={worktree.id}
      ariaLabel={t('dev.worktree.select', { path: worktree.path })}
      title={`${worktree.repo} · ${branch}`}
      path={worktree.path}
      revealable
      size={worktree.state === 'prunable' ? null : sizeText(worktree.size, locale, t)}
      row={row}
    >
      <p
        className={
          worktree.id === null
            ? 'text-2xs text-[var(--color-fg-muted)]'
            : 'text-2xs text-[var(--color-status-ok)]'
        }
      >
        {worktreeReason(worktree, t)}
      </p>
    </Row>
  );
}

function DockerBody({ scan, row }: { readonly scan: DevScan; readonly row: RowContext }) {
  const { locale, t } = row;
  const docker = scan.docker;
  if (docker.state !== 'ok') {
    return <p className="text-2xs">{t(`dev.dockerState.${docker.state}`)}</p>;
  }
  return (
    <>
      {docker.items.length === 0 ? (
        <p className="text-2xs">{t('dev.dockerNone')}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {docker.items.map((item) => {
            const name = t(`dev.docker.${item.kind}`);
            const neverOffered = item.id === null || item.kind === 'volumes';
            return (
              <Row
                key={item.kind}
                id={neverOffered ? null : item.id}
                ariaLabel={t('dev.dockerSelect', { label: name })}
                title={
                  item.count === null
                    ? name
                    : `${name} · ${t('dev.dockerCount', { count: item.count, n: formatCount(item.count, locale) })}`
                }
                path={item.command}
                size={neverOffered ? null : sizeText(item.reclaimable, locale, t)}
                row={row}
              >
                {neverOffered ? (
                  <p className="text-2xs text-[var(--color-fg-muted)]">{t('dev.dockerVolumes')}</p>
                ) : (
                  item.reclaimable !== null && (
                    <p className="text-2xs text-[var(--color-fg-muted)]">
                      {t('dev.dockerReclaimable', { size: formatBytes(item.reclaimable, locale) })}
                    </p>
                  )
                )}
              </Row>
            );
          })}
        </ul>
      )}
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('dev.dockerDisk')}</p>
    </>
  );
}

function Summary({
  items,
  selected,
  locale,
  t,
  onReview,
}: {
  readonly items: readonly Selectable[];
  readonly selected: ReadonlySet<string>;
  readonly locale: string;
  readonly t: T;
  readonly onReview: () => void;
}) {
  const total = selectionTotal(items, selected);
  const size =
    total.bytes === 0 && total.unmeasured > 0
      ? t('dev.notMeasured')
      : formatBytes(total.bytes, locale);
  return (
    // Spacing, not a rule, separates it from the list: inside a card a rule
    // under the scroll area read as a second card edge.
    <div className="flex flex-wrap items-center justify-between gap-2 px-4 pt-1 pb-3">
      <p className="tnum text-sm" aria-live="polite">
        {t('dev.summary', { count: total.count, n: formatCount(total.count, locale), size })}
        {total.unmeasured > 0 && total.bytes > 0 && (
          <> {t('dev.summaryUnmeasured', { n: formatCount(total.unmeasured, locale) })}</>
        )}
      </p>
      <Button variant="danger" size="sm" disabled={total.count === 0} onClick={onReview}>
        <Trash2 aria-hidden className="size-4" />
        {t('dev.review')}
      </Button>
    </div>
  );
}

function ConfirmDialog({
  scan,
  items,
  run,
  locale,
  t,
  onConfirm,
  onClose,
}: {
  readonly scan: DevScan;
  readonly items: readonly Selectable[];
  readonly run: DevRun | null;
  readonly locale: string;
  readonly t: T;
  readonly onConfirm: () => void;
  readonly onClose: () => void;
}) {
  const [understood, setUnderstood] = useState(false);
  const running = run?.running === true;
  const finished = run !== null && !run.running;
  const ids = useMemo(() => new Set(items.map((item) => item.id)), [items]);
  const wsl = stopsWsl(scan, ids);
  const title = !finished
    ? t('dev.confirm.title')
    : run.report !== null
      ? t('dev.confirm.reportTitle')
      : t('dev.confirm.notRunTitle');

  return (
    <DialogRoot
      open
      onOpenChange={(next) => {
        // Not closable while it runs: the report is the only place the user
        // learns what was removed.
        if (!next && !running) onClose();
      }}
    >
      <DialogContent
        size="lg"
        title={title}
        closeLabel={t('dev.confirm.close')}
        footer={
          finished ? (
            <Button variant="primary" size="sm" onClick={onClose}>
              {t('dev.confirm.done')}
            </Button>
          ) : (
            <>
              <Button variant="ghost" size="sm" disabled={running} onClick={onClose}>
                {t('dev.confirm.cancel')}
              </Button>
              <Button
                variant="danger"
                size="sm"
                disabled={running || !understood}
                onClick={onConfirm}
              >
                <Trash2 aria-hidden className="size-4" />
                {running ? t('dev.confirm.running') : t('dev.confirm.run')}
              </Button>
            </>
          )
        }
      >
        {finished ? (
          <RunOutcome run={run} items={items} locale={locale} t={t} />
        ) : (
          <div className="flex flex-col gap-2 text-sm">
            <ul className="flex max-h-72 flex-col gap-1 overflow-auto">
              {items.map((item) => (
                <li
                  key={item.id}
                  className="rounded-md border border-[var(--color-border-subtle)] p-2"
                >
                  <div className="flex items-baseline justify-between gap-2">
                    <span>
                      {item.label}
                      {item.permanent && (
                        <Badge tone="danger" className="ml-2">
                          {t('dev.confirm.permanentBadge')}
                        </Badge>
                      )}
                    </span>
                    <span className="tnum shrink-0 text-2xs">{sizeText(item.size, locale, t)}</span>
                  </div>
                  <p className="font-mono text-2xs break-all text-[var(--color-fg-subtle)]">
                    {item.path}
                  </p>
                  {item.restore !== null && (
                    <Command label={t('dev.restore')} command={item.restore} />
                  )}
                </li>
              ))}
            </ul>
            <div
              role="note"
              className="flex flex-col gap-2 rounded-md border border-[var(--color-status-danger)] p-2 text-2xs"
            >
              <p>{t('dev.confirm.permanent')}</p>
              {wsl && (
                <>
                  <p>{t('dev.confirm.stopsWsl')}</p>
                  <p>{t('dev.confirm.elevation')}</p>
                </>
              )}
              <Checkbox
                checked={understood}
                disabled={running}
                onCheckedChange={(next) => {
                  setUnderstood(next === true);
                }}
                label={t('dev.confirm.understood')}
              />
            </div>
            {running && <CleanRunning run={run} items={items} t={t} />}
          </div>
        )}
      </DialogContent>
    </DialogRoot>
  );
}

function CleanRunning({
  run,
  items,
  t,
}: {
  readonly run: DevRun;
  readonly items: readonly Selectable[];
  readonly t: T;
}) {
  const progress = run.progress;
  const current =
    progress === null ? undefined : items.find((item) => item.id === progress.id)?.path;
  // `index` is zero-based: the item being worked on now.
  const text =
    progress === null
      ? t('dev.confirm.running')
      : t('dev.confirm.progress', {
          index: Math.min(progress.index + 1, progress.total),
          total: progress.total,
        });
  return (
    <div role="status" className="flex flex-col gap-1">
      <p>{text}</p>
      {progress === null ? (
        <ProgressBar indeterminate label={text} />
      ) : (
        <ProgressBar
          value={Math.min(progress.index, progress.total)}
          max={Math.max(1, progress.total)}
          label={t('dev.confirm.running')}
          valueText={text}
        />
      )}
      {current !== undefined && (
        <p className="font-mono text-2xs break-all text-[var(--color-fg-subtle)]">{current}</p>
      )}
    </div>
  );
}

function RunOutcome({
  run,
  items,
  locale,
  t,
}: {
  readonly run: DevRun;
  readonly items: readonly Selectable[];
  readonly locale: string;
  readonly t: T;
}) {
  if (run.report === null) {
    return (
      <p role="alert" className="text-sm text-[var(--color-status-warn)]">
        {run.refused
          ? t('dev.report.refusedAll', { message: run.error ?? '' })
          : t('dev.report.failed', { message: run.error ?? '' })}
      </p>
    );
  }
  const labels = new Map(items.map((item) => [item.id, item.label]));
  return (
    <div className="flex flex-col gap-2 text-sm">
      <DriveGains report={run.report} locale={locale} t={t} />
      <ul className="flex max-h-80 flex-col gap-1 overflow-auto">
        {run.report.items.map((item) => (
          <li key={item.id} className="rounded-md border border-[var(--color-border-subtle)] p-2">
            <p>{labels.get(item.id) ?? item.path}</p>
            <p className="font-mono text-2xs break-all text-[var(--color-fg-subtle)]">
              {item.path}
            </p>
            <ReportLine item={item} locale={locale} t={t} />
          </li>
        ))}
      </ul>
    </div>
  );
}

const OUTCOME_TONE: Record<DevOutcomeKey, 'ok' | 'warn' | 'danger' | 'neutral'> = {
  done: 'ok',
  partial: 'warn',
  failed: 'danger',
  refused: 'neutral',
  unavailable: 'neutral',
};

function ReportLine({
  item,
  locale,
  t,
}: {
  readonly item: DevCleanItem;
  readonly locale: string;
  readonly t: T;
}) {
  const showHolders =
    (item.outcome === 'partial' || item.outcome === 'failed') &&
    item.holders !== null &&
    item.holders.length > 0;
  return (
    <div className="flex flex-col gap-0.5 text-2xs">
      <p className="flex flex-wrap items-center gap-2">
        <Badge tone={OUTCOME_TONE[item.outcome]}>{t(`dev.outcome.${item.outcome}`)}</Badge>
        {item.outcome !== 'refused' && (
          <span>
            {item.freed === null
              ? t('dev.report.freedUnknown')
              : t('dev.report.freed', { size: formatBytes(Math.max(0, item.freed), locale) })}
          </span>
        )}
        {item.toolFreed !== null && (
          <span className="text-[var(--color-fg-muted)]">
            {t('dev.report.toolFreed', { size: formatBytes(item.toolFreed, locale) })}
          </span>
        )}
      </p>
      {item.outcome === 'refused' && <p>{t('dev.report.refused')}</p>}
      {item.message !== null && <p className="text-[var(--color-fg-muted)]">{item.message}</p>}
      {showHolders && (
        <div>
          <p>{t('dev.report.holders')}</p>
          <ul className="list-disc pl-4">
            {item.holders?.map((holder) => (
              <li key={holder.pid}>
                {t('dev.report.holder', { name: holder.name, pid: holder.pid })}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function DriveGains({
  report,
  locale,
  t,
}: {
  readonly report: DevCleanReport;
  readonly locale: string;
  readonly t: T;
}) {
  if (report.drives.length === 0) return null;
  return (
    <ul className="flex flex-col gap-0.5 text-sm font-medium">
      {report.drives.map((drive) => (
        <li key={drive.drive}>
          {drive.freed >= 0
            ? t('dev.report.drive', {
                drive: drive.drive,
                size: formatBytes(drive.freed, locale),
              })
            : t('dev.report.driveLost', {
                drive: drive.drive,
                size: formatBytes(-drive.freed, locale),
              })}
        </li>
      ))}
    </ul>
  );
}
