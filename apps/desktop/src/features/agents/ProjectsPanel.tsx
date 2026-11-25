/**
 * The Projects tab of the AI agents page: a signed-in user's Codwen projects
 * and their health, plus this machine's devcloud panel.
 *
 * # Read-only, and honest about absence
 *
 * The page is a reader (docs/codwen-projects-page.md rule 1). It never probes
 * a project's URL, never writes project state and never keeps the list as
 * local truth (rules 2-3); it renders what the one read returns and ages the
 * `lastCheckedAt` readings against a ticking clock. A signed-out account is a
 * sign-in state, never an empty list (rule 4). A figure the source did not
 * measure is an em dash, never `0` (`projectsModel`).
 *
 * # States
 *
 * `useProjects` answers one of five things and each has its own render:
 * `loading` (skeletons), `signedOut` (the account card), `unavailable` (the
 * endpoint does not exist yet — this build has no fixture), `ok` (the list),
 * and `error`, which keeps the last good list and marks it out of date rather
 * than blanking it.
 *
 * # Motion
 *
 * Rows enter with the `SessionCard` vocabulary under the app's `LazyMotion`
 * (ADR-0017): `m.li` under `AnimatePresence`, no layout animation. The list
 * order is held steady against the tick with `useStableOrder` + `useHoverHold`
 * so a row cannot slide out from under a pointer, and the numbers and the em
 * dashes never animate at all.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { FolderKanban, ShieldAlert } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { AgentsSnapshot } from '@vitals/protocol';
import { Badge, EmptyState, Skeleton, useHoverHold, useStableOrder } from '@vitals/ui';

import { Kpi } from './parts';
import { CloudAccount } from './CloudAccount';
import { DevcloudPanel } from './DevcloudPanel';
import { since } from './model';
import {
  componentKey,
  componentRank,
  formatLatency,
  formatNumber,
  incidentKey,
  sortIncidents,
  projectKey,
  projectRank,
  summary,
  worstStatus,
  type CloudProject,
  type HealthStatus,
  type IncidentSeverity,
  type ProjectComponent,
  type ProjectIncident,
} from './projectsModel';
import type { ProjectsSource } from './projectsApi';
import { NO_HOST, useProjects } from './useProjects';

const STATUS_TONE: Record<HealthStatus, 'ok' | 'warn' | 'danger' | 'neutral'> = {
  up: 'ok',
  degraded: 'warn',
  down: 'danger',
  unknown: 'neutral',
};

const SEVERITY_TONE: Record<IncidentSeverity, 'info' | 'warn' | 'danger'> = {
  info: 'info',
  warning: 'warn',
  critical: 'danger',
};

/** An ISO-8601 reading to a millisecond instant, or `null` when unparseable. */
function instant(iso: string | null): number | null {
  if (iso === null) return null;
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? null : ms;
}

export interface ProjectsPanelProps {
  readonly snapshot: AgentsSnapshot;
  /** Injectable source; a test supplies its own. */
  readonly source?: ProjectsSource;
}

export function ProjectsPanel({ snapshot, source }: ProjectsPanelProps): React.JSX.Element {
  // The Projects strings live in the shared locale files, so the default
  // namespace is the one to translate through.
  const { t } = useTranslation();
  const { state, now } = useProjects(source);
  const { held, bind } = useHoverHold();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Hooks run before any early return. The list is empty in every non-`ok`
  // state, which the ordering helper handles without a special case.
  const projects =
    state.status === 'ok'
      ? state.data.projects
      : state.status === 'error'
        ? (state.data?.projects ?? [])
        : [];
  const ordered = useStableOrder(projects, projectKey, { rankOf: projectRank, now, held });
  const selected = ordered.find((p) => p.id === selectedId) ?? ordered[0] ?? null;
  const totals = summary(projects);

  function body(): React.JSX.Element {
    if (state.status === 'loading') return <ProjectsSkeleton label={t('projects.loading')} />;
    if (state.status === 'error' && state.error === NO_HOST) {
      return (
        <EmptyState
          icon={<ShieldAlert />}
          title={t('projects.noHost.title')}
          description={t('projects.noHost.body')}
        />
      );
    }
    if (state.status === 'signedOut') {
      // The account card is the sign-in UI: its own empty state, its own
      // button, its own read-only note. Never an empty project list.
      return <CloudAccount />;
    }
    if (state.status === 'unavailable') {
      return (
        <EmptyState
          icon={<FolderKanban />}
          title={t('projects.unavailable.title')}
          description={t('projects.unavailable.body')}
        />
      );
    }
    if (state.status === 'error' && state.data === null) {
      return (
        <EmptyState
          icon={<ShieldAlert />}
          title={t('projects.error.title')}
          description={t('projects.error.body')}
        />
      );
    }

    return (
      <>
        {state.status === 'error' && (
          <p role="status" className="text-2xs text-[var(--color-warn-text)]">
            {t('projects.stale')}
          </p>
        )}

        <div className="flex flex-wrap gap-2">
          <Kpi label={t('projects.kpi.total')} value={totals.total} />
          <Kpi
            label={t('projects.kpi.down')}
            value={totals.down}
            tone={totals.down > 0 ? 'danger' : 'neutral'}
          />
          <Kpi
            label={t('projects.kpi.degraded')}
            value={totals.degraded}
            tone={totals.degraded > 0 ? 'warn' : 'neutral'}
          />
          <Kpi
            label={t('projects.kpi.up')}
            value={totals.up}
            tone={totals.up > 0 ? 'ok' : 'neutral'}
          />
          <Kpi
            label={t('projects.kpi.incidents')}
            value={totals.openIncidents}
            tone={totals.openIncidents > 0 ? 'warn' : 'neutral'}
          />
        </div>

        {ordered.length === 0 ? (
          <EmptyState
            icon={<FolderKanban />}
            title={t('projects.empty.title')}
            description={t('projects.empty.body')}
          />
        ) : (
          <div className="grid gap-4 lg:grid-cols-2 lg:items-start">
            <ul {...bind} className="flex flex-col gap-2">
              <AnimatePresence initial={false}>
                {ordered.map((project) => (
                  <m.li
                    key={project.id}
                    initial={{ opacity: 0, y: 8 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.25 }}
                  >
                    <ProjectRow
                      project={project}
                      selected={project.id === selected?.id}
                      onSelect={() => setSelectedId(project.id)}
                      now={now}
                    />
                  </m.li>
                ))}
              </AnimatePresence>
            </ul>
            {selected !== null && <ProjectDetail project={selected} now={now} />}
          </div>
        )}

        <p className="text-2xs text-[var(--color-fg-subtle)]">{t('projects.readOnly')}</p>
      </>
    );
  }

  return (
    <section aria-label={t('projects.title')} className="flex flex-col gap-4">
      {body()}
      <DevcloudPanel snapshot={snapshot} />
    </section>
  );
}

function ProjectRow({
  project,
  selected,
  onSelect,
  now,
}: {
  readonly project: CloudProject;
  readonly selected: boolean;
  readonly onSelect: () => void;
  readonly now: number;
}): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onSelect}
      className={`flex w-full flex-col gap-1 rounded-xl border p-3 text-left transition-colors ${
        selected
          ? 'border-[var(--color-accent-border)] bg-[var(--color-accent-subtle)]'
          : 'border-[var(--color-border-subtle)] bg-[var(--surface-card)] hover:border-[var(--color-border-default)]'
      }`}
    >
      <span className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-baseline gap-2">
          <span className="truncate text-sm font-medium">{project.name}</span>
          <span className="truncate font-mono text-2xs text-[var(--color-fg-subtle)]">
            {project.slug}
          </span>
        </span>
        <StatusChip status={project.status} />
      </span>
      <span className="flex flex-wrap items-center gap-x-2 text-2xs text-[var(--color-fg-muted)]">
        <span>{project.kind}</span>
        <span aria-hidden>·</span>
        <span>{t(`projects.visibility.${project.visibility}`)}</span>
        <span aria-hidden>·</span>
        <span>
          {project.lastCheckedAt === null
            ? t('projects.neverChecked')
            : t('projects.lastChecked', { ago: since(instant(project.lastCheckedAt), now) })}
        </span>
        {project.openIncidents.length > 0 && (
          <>
            <span aria-hidden>·</span>
            <span className="text-[var(--color-warn-text)]">
              {t('projects.incidents', { count: project.openIncidents.length })}
            </span>
          </>
        )}
      </span>
    </button>
  );
}

function ProjectDetail({
  project,
  now,
}: {
  readonly project: CloudProject;
  readonly now: number;
}): React.JSX.Element {
  const { t } = useTranslation();
  const { held, bind } = useHoverHold();
  const components = useStableOrder(project.components, componentKey, {
    rankOf: componentRank,
    now,
    held,
  });
  const incidents = sortIncidents(project.openIncidents);
  const worst = worstStatus(project.components.map((c) => c.status));

  return (
    <section
      aria-label={t('projects.detail')}
      className="flex flex-col gap-3 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--surface-card)] p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="truncate text-sm font-semibold">{project.name}</h3>
        <StatusChip status={project.status} />
      </div>

      <div className="flex flex-col gap-1.5">
        <h4 className="flex items-center gap-2 text-2xs font-semibold text-[var(--color-fg-muted)]">
          {t('projects.components')}
          <span className="font-normal">
            {t('projects.worst', { status: t(`projects.status.${worst}`) })}
          </span>
        </h4>
        <ul {...bind} className="flex flex-col gap-1">
          <AnimatePresence initial={false}>
            {components.map((component) => (
              <m.li
                key={component.key}
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.25 }}
              >
                <ComponentRow component={component} />
              </m.li>
            ))}
          </AnimatePresence>
        </ul>
      </div>

      <div className="flex flex-col gap-1.5">
        <h4 className="text-2xs font-semibold text-[var(--color-fg-muted)]">
          {t('projects.openIncidents')}
        </h4>
        {incidents.length === 0 ? (
          <p className="text-2xs text-[var(--color-fg-subtle)]">{t('projects.noIncidents')}</p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {incidents.map((incident) => (
              <IncidentRow key={incidentKey(incident)} incident={incident} now={now} />
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

function ComponentRow({ component }: { readonly component: ProjectComponent }): React.JSX.Element {
  const { t } = useTranslation();
  return (
    // A component row is the content of the animated list item above it, not a
    // list item itself — an `<li>` inside an `<li>` is invalid HTML.
    <div className="flex items-center justify-between gap-2 rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] px-3 py-1.5 text-2xs">
      <span className="flex min-w-0 items-center gap-2">
        <StatusChip status={component.status} />
        <span className="truncate">{component.label ?? component.key}</span>
      </span>
      <span className="flex shrink-0 items-center gap-2 text-[var(--color-fg-muted)] tabular-nums">
        <span>
          {t('projects.latency')}: {formatLatency(component.latencyMs)}
        </span>
        {component.value !== null && (
          <span>
            {t('projects.value')}: {formatNumber(component.value)}
          </span>
        )}
      </span>
    </div>
  );
}

function IncidentRow({
  incident,
  now,
}: {
  readonly incident: ProjectIncident;
  readonly now: number;
}): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <li className="flex flex-col gap-0.5 rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] px-3 py-1.5 text-2xs">
      <span className="flex items-center gap-2">
        <Badge tone={SEVERITY_TONE[incident.severity]}>
          {t(`projects.severity.${incident.severity}`)}
        </Badge>
        <span className="text-[var(--color-fg-muted)]">
          {t('projects.opened', { ago: since(instant(incident.openedAt), now) })}
        </span>
      </span>
      <span className="text-[var(--color-fg)]">{incident.summary}</span>
      <span className="text-[var(--color-fg-subtle)]">
        {incident.notifiedAt === null ? t('projects.notNotified') : t('projects.notified')}
      </span>
    </li>
  );
}

function StatusChip({ status }: { readonly status: HealthStatus }): React.JSX.Element {
  const { t } = useTranslation();
  return <Badge tone={STATUS_TONE[status]}>{t(`projects.status.${status}`)}</Badge>;
}

function ProjectsSkeleton({ label }: { readonly label: string }): React.JSX.Element {
  return (
    <div aria-busy="true" aria-label={label} className="flex flex-col gap-3">
      <div className="flex gap-2">
        {Array.from({ length: 5 }, (_, i) => (
          <Skeleton key={i} className="h-16 flex-1" />
        ))}
      </div>
      <Skeleton className="h-40 w-full" />
    </div>
  );
}
