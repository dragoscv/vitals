/**
 * The Tracking page: the cross-repo tracker index agentq builds from every
 * repository it scans, read-only.
 *
 * # What it is for
 *
 * Every repository here keeps its own tracker, and until now the only way to
 * see them together was to open each one. The scanner writes a single
 * `index.json`; this page draws it as repositories → trackers → rows, with the
 * per-status tallies and the drift findings the scanner recorded. It never
 * writes: the tracker file each repo owns stays the source of truth, and the
 * index is a rebuildable view of it.
 *
 * # Motion
 *
 * Cards enter with a short fade-and-lift under the app's LazyMotion (ADR-0017).
 * Reduced motion is honoured twice over: the shared `MotionConfig` neutralises
 * the transitions, and this screen also reads `useReducedMotion` so the
 * durations are literally zero rather than left to the config — a list of a
 * thousand rows must not animate for anyone, and never for someone who asked
 * for still frames.
 */

import * as m from 'motion/react-m';
import {
  CircleAlert,
  CircleCheck,
  Cloud,
  FileText,
  GitBranch,
  HardDrive,
  ListChecks,
  RefreshCw,
  ShieldAlert,
  TriangleAlert,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  EmptyState,
  Separator,
  Skeleton,
  useReducedMotion,
} from '@vitals/ui';

import {
  formatAge,
  generatedAtMs,
  groupItems,
  hasDrift,
  indexTotals,
  isKnownStatus,
  openCount,
  repoItemCount,
  repoStatusCounts,
  shortRef,
  sortedStatuses,
  statusTone,
  type ItemGroup,
  type Tracker,
  type TrackerIndex,
  type TrackerItem,
  type TrackerRepo,
} from './model';
import { ResumePackPanel } from './ResumePack';
import { TRACKING_NS } from './strings';
import { useTracking, type TrackingSource } from './useTracking';

/**
 * The command that produces the index, shown when there is none to draw.
 *
 * Copied from the tool's own first usage line, and chosen against real output:
 * `--scan --all` walks `~/gh`, which on this machine is `C:\Users\…\gh` and does
 * not exist, so the tool silently falls back to scanning every directory in the
 * home folder — a scan that produces an index of the wrong thing. Naming the
 * directories is the form that works here and the one the neighbouring tool
 * (`tools/track.mjs`) prints first.
 */
const SCAN_COMMAND = 'node tools/tracker-lint.mjs --scan <dir> [<dir> ...]';

export interface TrackingScreenProps {
  readonly source?: TrackingSource;
}

export function TrackingScreen({ source }: TrackingScreenProps): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  const { state, codwen, reload, resume, resumeError, claim, pendingClaim, claimError } =
    useTracking(source);
  const reduced = useReducedMotion();
  // Zero rather than a config-dependent value: see the file header.
  const dur = reduced ? 0 : 0.18;

  const localityLabel = codwen ? t('locality.connected') : t('locality.localOnly');
  const localityHint = codwen ? t('locality.connectedHint') : t('locality.localHint');

  const header = (
    <header className="screen-header">
      <div className="min-w-0">
        <h2 className="flex items-center gap-2 text-lg font-semibold">
          <ListChecks aria-hidden className="size-5 text-[var(--color-accent)]" />
          {t('title')}
        </h2>
        <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
      </div>
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Badge
          tone={codwen ? 'accent' : 'neutral'}
          icon={codwen ? <Cloud /> : <HardDrive />}
          title={localityHint}
          aria-label={`${t('locality.label')}: ${localityLabel}. ${localityHint}`}
        >
          {localityLabel}
        </Badge>
        <Button variant="ghost" size="sm" onClick={reload}>
          <RefreshCw aria-hidden className="size-4" />
          {t('reload')}
        </Button>
      </div>
    </header>
  );

  let body: React.JSX.Element;
  // The resume pack is the fact a resume needs, so it is drawn in every state
  // where a host exists — with its own "no plan yet" and "no session" states.
  const panel =
    state.status === 'noHost' ? null : (
      <ResumePackPanel
        pack={resume}
        error={resumeError}
        pendingClaim={pendingClaim}
        claimError={claimError}
        claim={claim}
      />
    );

  switch (state.status) {
    case 'loading':
      body = (
        <div className="screen-body">
          <p className="sr-only">{t('loading')}</p>
          <div className="flex flex-col gap-3" aria-busy="true">
            <Skeleton className="h-8 w-64" />
            <Skeleton className="h-28 w-full" />
            <Skeleton className="h-28 w-full" />
          </div>
        </div>
      );
      break;
    case 'noHost':
      body = (
        <EmptyState icon={<HardDrive />} title={t('noHost.title')} description={t('noHost.body')} />
      );
      break;
    case 'empty':
      body = (
        <div className="screen-body">
          {panel}
          <EmptyState
            icon={<ListChecks />}
            title={t('empty.title')}
            description={t('empty.body')}
            action={
              <code className="block max-w-full overflow-x-auto rounded-md border border-[var(--color-border-subtle)] bg-[var(--color-bg-subtle)] px-3 py-2 font-mono text-2xs text-[var(--color-fg-muted)]">
                {SCAN_COMMAND}
              </code>
            }
          />
        </div>
      );
      break;
    case 'error':
      body = (
        <div className="screen-body">
          {panel}
          <EmptyState
            icon={<CircleAlert />}
            title={t('error.title')}
            description={t('error.body')}
            action={
              <code className="block max-w-full overflow-x-auto rounded-md border border-[var(--color-border-subtle)] bg-[var(--color-bg-subtle)] px-3 py-2 font-mono text-2xs text-[var(--color-fg-muted)]">
                {state.message}
              </code>
            }
          />
        </div>
      );
      break;
    case 'ready':
      body = <TrackingContent index={state.index} dur={dur} panel={panel} />;
      break;
  }

  return (
    <div className="screen">
      {header}
      {body}
    </div>
  );
}

/**
 * The loaded page: a fixed header line, KPI strip and warnings above the one
 * region that can grow without bound — the repository list.
 *
 * That split follows `.screen`'s rule (see styles.css): the title, summary and
 * toolbar stay put, and only `.screen-body` scrolls, so the catalogue of
 * statuses is still on screen after scrolling eight repositories down.
 */
function TrackingContent({
  index,
  dur,
  panel,
}: {
  readonly index: TrackerIndex;
  readonly dur: number;
  readonly panel: React.ReactNode;
}): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  const totals = indexTotals(index);
  const age = generatedAtMs(index);
  const blocked = totals.byStatus['blocked'] ?? 0;

  return (
    <>
      <ScanAge age={age} />

      <div className="flex flex-wrap gap-2">
        <Kpi label={t('kpi.repos')} value={totals.repos} />
        <Kpi label={t('kpi.trackers')} value={totals.trackers} />
        <Kpi label={t('kpi.items')} value={totals.items} />
        <Kpi
          label={t('kpi.open')}
          value={openCount(totals.byStatus)}
          tone={openCount(totals.byStatus) > 0 ? 'accent' : 'neutral'}
        />
        <Kpi
          label={t('kpi.drift')}
          value={totals.drift}
          tone={totals.drift > 0 ? 'danger' : 'neutral'}
        />
      </div>

      <Warnings drift={totals.drift} blocked={blocked} reposWithDrift={totals.reposWithDrift} />

      <div className="screen-body">
        {panel}
        <div className="flex flex-col gap-4">
          {index.repos.map((repo, i) => (
            <RepoCard
              key={repo.repo}
              repo={repo}
              dur={dur}
              // Staggered by index, capped so the last card of a long list is
              // not left waiting for its own entrance.
              delay={dur === 0 ? 0 : Math.min(i * 0.03, 0.24)}
            />
          ))}
        </div>
      </div>
    </>
  );
}

/**
 * How often the age of the scan is re-measured.
 *
 * One second, because the label counts seconds during the first minute: a scan
 * from a moment ago reads `12s` and would keep reading it forever, which is a
 * quieter version of the same lie the page exists to avoid.
 */
const AGE_TICK_MS = 1000;

/**
 * How long ago the scan behind this catalogue was generated.
 *
 * Its own component so a tick redraws one line instead of a list that can hold
 * a thousand rows, and the clock is read in an effect rather than during
 * render: `Date.now()` on a render path is impure (`react-hooks/purity`) and
 * React is free to draw two different ages for the same scan. Same shape as
 * the shell's `StaleBanner`.
 */
function ScanAge({ age }: { readonly age: number | null }): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const timer = setInterval(() => {
      setNow(Date.now());
    }, AGE_TICK_MS);
    return () => {
      clearInterval(timer);
    };
  }, []);

  return (
    <p className="text-2xs text-[var(--color-fg-subtle)]">
      {age === null ? t('generatedNever') : t('generated', { ago: formatAge(now - age) })}
    </p>
  );
}

/** A headline number. Mirrors the AI page's tile so the two pages read alike. */
function Kpi({
  label,
  value,
  tone = 'neutral',
}: {
  readonly label: string;
  readonly value: number;
  readonly tone?: 'neutral' | 'ok' | 'warn' | 'danger' | 'accent';
}): React.JSX.Element {
  const color = {
    neutral: 'var(--color-fg-default)',
    ok: 'var(--color-ok-text)',
    warn: 'var(--color-warn-text)',
    danger: 'var(--color-danger-text)',
    accent: 'var(--color-accent)',
  }[tone];
  return (
    <div className="flex min-w-[6rem] flex-1 flex-col justify-between gap-1 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--surface-card)] px-3 py-2.5">
      <span className="text-2xs text-[var(--color-fg-muted)]">{label}</span>
      <span className="text-xl font-semibold tabular-nums" style={{ color }}>
        {value}
      </span>
    </div>
  );
}

/**
 * The drift and blocked-rows banner.
 *
 * Both are warnings rather than errors, so the card is informational and says
 * so plainly when there is nothing to report — a dashboard that is silent when
 * it is healthy teaches people to distrust it.
 */
function Warnings({
  drift,
  blocked,
  reposWithDrift,
}: {
  readonly drift: number;
  readonly blocked: number;
  readonly reposWithDrift: number;
}): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  const clean = drift === 0 && blocked === 0;

  return (
    <Card regionLabel={t('warnings.title')} className="px-4 py-3">
      <div className="flex flex-wrap items-center gap-2">
        {clean ? (
          <>
            <CircleCheck aria-hidden className="size-4 text-[var(--color-ok-text)]" />
            <span className="text-sm text-[var(--color-fg-muted)]">{t('warnings.none')}</span>
          </>
        ) : (
          <>
            <TriangleAlert aria-hidden className="size-4 text-[var(--color-warn-text)]" />
            <span className="text-sm font-medium">{t('warnings.title')}</span>
            {drift > 0 && (
              <Badge tone="danger" icon={<ShieldAlert />}>
                {t('warnings.drift', { count: drift })}
              </Badge>
            )}
            {blocked > 0 && <Badge tone="warn">{t('warnings.blocked', { count: blocked })}</Badge>}
            {drift > 0 && reposWithDrift > 0 && (
              <span className="text-2xs text-[var(--color-fg-muted)]">
                {t('warnings.repos', { count: reposWithDrift })}
              </span>
            )}
            {drift > 0 && (
              <span className="text-2xs text-[var(--color-fg-subtle)]">
                {t('warnings.driftHint')}
              </span>
            )}
          </>
        )}
      </div>
    </Card>
  );
}

/** One repository: its clone facts, its trackers and their rows. */
function RepoCard({
  repo,
  dur,
  delay,
}: {
  readonly repo: TrackerRepo;
  readonly dur: number;
  readonly delay: number;
}): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  const counts = repoStatusCounts(repo);
  const statuses = sortedStatuses(counts);
  const rows = repoItemCount(repo);
  const drifted = hasDrift(repo);

  return (
    <m.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: dur, delay }}
    >
      <Card regionLabel={repo.repo}>
        <CardHeader
          actions={
            <div className="flex flex-wrap items-center justify-end gap-1.5">
              {drifted && (
                <Badge tone="danger" icon={<ShieldAlert />}>
                  {t('warnings.drift', { count: repo.drift.length })}
                </Badge>
              )}
              <span className="text-2xs text-[var(--color-fg-subtle)]">
                {t('tracker.items', { count: rows })}
              </span>
            </div>
          }
        >
          <CardTitle level={3} className="text-sm tracking-normal normal-case">
            {repo.repo}
          </CardTitle>
          <p className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-2xs text-[var(--color-fg-muted)]">
            {repo.git !== null ? (
              <>
                <span className="inline-flex items-center gap-1">
                  <GitBranch aria-hidden className="size-3" />
                  {t('repo.branch')} <span className="font-mono">{shortRef(repo.git.branch)}</span>
                </span>
                <span className="inline-flex items-center gap-1">
                  {t('repo.head')} <span className="font-mono">{repo.git.head.slice(0, 7)}</span>
                </span>
              </>
            ) : (
              <span>{t('repo.noGit')}</span>
            )}
          </p>
        </CardHeader>

        <CardBody className="flex flex-col gap-3">
          {statuses.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {statuses.map((status) => (
                <StatusChip key={status} status={status} count={counts[status] ?? 0} />
              ))}
            </div>
          )}

          {repo.trackers.length === 0 ? (
            <p className="text-2xs text-[var(--color-fg-subtle)]">{t('tracker.empty')}</p>
          ) : (
            <div className="flex flex-col gap-3">
              {repo.trackers.map((tracker) => (
                <TrackerBlock key={tracker.id} tracker={tracker} />
              ))}
            </div>
          )}

          {drifted && (
            <>
              <Separator />
              <ul className="flex flex-col gap-1.5">
                {repo.drift.map((finding, i) => (
                  <li
                    key={`${finding.tracker}-${finding.ref}-${String(i)}`}
                    className="flex flex-wrap items-baseline gap-x-2 text-2xs text-[var(--color-fg-muted)]"
                  >
                    <Badge tone="danger">{finding.kind}</Badge>
                    <span className="font-mono">{finding.tracker}</span>
                    {finding.ref !== '' && (
                      <span className="font-mono text-[var(--color-fg-subtle)]">{finding.ref}</span>
                    )}
                    {finding.detail !== '' && <span>{finding.detail}</span>}
                  </li>
                ))}
              </ul>
            </>
          )}
        </CardBody>
      </Card>
    </m.div>
  );
}

/** One tracker file and its rows, grouped as the file grouped them. */
function TrackerBlock({ tracker }: { readonly tracker: Tracker }): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  const groups = groupItems(tracker.items);
  // A heading only earns its line when the items are actually split; a single
  // ungrouped bucket would otherwise label everything "Ungrouped" for nothing.
  const showHeadings = groups.length > 1 || (groups[0]?.group ?? null) !== null;
  const rowCount = tracker.items.length;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <FileText aria-hidden className="size-3.5 text-[var(--color-fg-subtle)]" />
        <span className="font-mono text-2xs text-[var(--color-fg-default)]">{tracker.file}</span>
        <span className="text-2xs text-[var(--color-fg-subtle)]">
          {t('tracker.items', { count: rowCount })}
        </span>
      </div>

      {rowCount === 0 ? (
        <p className="text-2xs text-[var(--color-fg-subtle)]">{t('tracker.empty')}</p>
      ) : (
        <div className="flex flex-col gap-2">
          {groups.map((group) => (
            <ItemGroupBlock
              key={group.group ?? '\u0000ungrouped'}
              group={group}
              headings={showHeadings}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ItemGroupBlock({
  group,
  headings,
}: {
  readonly group: ItemGroup;
  readonly headings: boolean;
}): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  return (
    <div className="flex flex-col gap-1">
      {headings && (
        <h4 className="text-2xs font-semibold tracking-[0.06em] text-[var(--color-fg-subtle)] uppercase">
          {group.group ?? t('group.ungrouped')}
        </h4>
      )}
      <ul className="flex flex-col divide-y divide-[var(--color-border-subtle)]">
        {group.items.map((item) => (
          <ItemRow key={item.id} item={item} />
        ))}
      </ul>
    </div>
  );
}

function ItemRow({ item }: { readonly item: TrackerItem }): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  return (
    <li className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 py-1.5">
      <span className="font-mono text-2xs text-[var(--color-fg-subtle)]">{item.id}</span>
      <span className="min-w-0 flex-1 text-2xs text-[var(--color-fg-default)]">{item.title}</span>
      <Badge tone={statusTone(item.status)}>
        {isKnownStatus(item.status) ? t(`status.${item.status}`) : item.status}
      </Badge>
      {item.evidence !== undefined && (
        <span className="w-full text-2xs text-[var(--color-fg-subtle)]">
          <span className="text-[var(--color-fg-muted)]">{t('item.evidence')}: </span>
          {item.evidence}
        </span>
      )}
      {item.notes !== undefined && (
        <span className="w-full text-2xs text-[var(--color-fg-subtle)]">
          <span className="text-[var(--color-fg-muted)]">{t('item.notes')}: </span>
          {item.notes}
        </span>
      )}
    </li>
  );
}

/** A status with its row count. The label carries the meaning, not the tint. */
function StatusChip({
  status,
  count,
}: {
  readonly status: string;
  readonly count: number;
}): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  return (
    <Badge tone={statusTone(status)}>
      {isKnownStatus(status) ? t(`status.${status}`) : status}
      <span className="text-[var(--color-fg-muted)] tabular-nums">{count}</span>
    </Badge>
  );
}
