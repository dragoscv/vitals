/**
 * Pure shaping for the Codwen Projects tab: the types the page reads, and the
 * ordering and formatting decisions that do not need React, kept here so they
 * are unit-tested without rendering.
 *
 * # Server-owned, never synthesised
 *
 * `status`, `lastCheckedAt`, every component's `status`/`latencyMs`/`checkedAt`
 * and every incident come from Codwen; the client renders them and probes
 * nothing (docs/codwen-projects-page.md rule 1). A project's rolled-up status
 * is *derived server-side* as worst-of-latest — it is not a column and this
 * file never invents one. The one derived value here, `worstStatus`, is used
 * only to label the components section from component data the page already
 * holds.
 *
 * # A number that cannot be measured is absent
 *
 * `latencyMs` and `value` are `null` when the source did not measure them, and
 * render as an em dash — never `0` (AGENTS.md). Nothing in this file turns a
 * `null` into a zero.
 */

/** The five possible component/service readings. */
export type HealthStatus = 'up' | 'degraded' | 'down' | 'unknown';

export type IncidentSeverity = 'info' | 'warning' | 'critical';

export type ProjectVisibility = 'public' | 'private';

/** One monitored component of a project, from its latest signal. */
export interface ProjectComponent {
  readonly key: string;
  readonly label: string | null;
  readonly status: HealthStatus;
  /** `null` when the collector did not measure a latency; never `0`. */
  readonly latencyMs: number | null;
  /** A component's own number (queue depth, count, …); `null` when unmeasured. */
  readonly value: number | null;
  readonly detail: Record<string, unknown> | null;
  /** ISO-8601, from the source's `timestamptz`. */
  readonly checkedAt: string;
}

/** One incident that has not been resolved. */
export interface ProjectIncident {
  readonly id: string;
  readonly severity: IncidentSeverity;
  readonly summary: string;
  readonly componentKey: string;
  readonly openedAt: string;
  /** `null` means the alert was not delivered, not that it was delivered late. */
  readonly notifiedAt: string | null;
}

/**
 * One Codwen project and its health.
 *
 * `kind` is a server enum whose final shape is not ratified
 * (docs/codwen-projects-page.md §3.1 "final enum to ratify"), so it is carried
 * as an opaque string and rendered as the source sent it rather than mapped to
 * a label this client would have to guess.
 */
export interface CloudProject {
  readonly id: string;
  readonly slug: string;
  readonly name: string;
  readonly kind: string;
  readonly visibility: ProjectVisibility;
  readonly status: HealthStatus;
  readonly lastCheckedAt: string | null;
  readonly components: readonly ProjectComponent[];
  readonly openIncidents: readonly ProjectIncident[];
}

/** What one read of the project list returned. */
export interface ProjectsSnapshot {
  readonly takenAtMs: number;
  readonly projects: readonly CloudProject[];
}

/* Worst-first: a lower rank sorts higher in a list that leads with problems. */
const STATUS_RANK: Record<HealthStatus, number> = { down: 0, degraded: 1, unknown: 2, up: 3 };
const SEVERITY_RANK: Record<IncidentSeverity, number> = { critical: 0, warning: 1, info: 2 };

export function statusRank(status: HealthStatus): number {
  return STATUS_RANK[status];
}

export function severityRank(severity: IncidentSeverity): number {
  return SEVERITY_RANK[severity];
}

/** Whether a reading should draw attention: anything short of a healthy `up`. */
export function isProblem(status: HealthStatus): boolean {
  return status === 'down' || status === 'degraded';
}

/**
 * The worst of a set of statuses, worst-first by the same ranks the list uses.
 *
 * An empty set is `unknown` rather than `up`: with nothing measured there is
 * nothing to call healthy. Used to label the components section from data the
 * page already holds; the project's own `status` is the server's rollup and is
 * never recomputed from this.
 */
export function worstStatus(statuses: Iterable<HealthStatus>): HealthStatus {
  let worst: HealthStatus = 'unknown';
  let rank = STATUS_RANK.unknown;
  for (const status of statuses) {
    if (STATUS_RANK[status] < rank) {
      rank = STATUS_RANK[status];
      worst = status;
    }
  }
  return worst;
}

export const projectKey = (project: CloudProject): string => project.id;
export const projectRank = (project: CloudProject): number => STATUS_RANK[project.status];

export const componentKey = (component: ProjectComponent): string => component.key;

/** Worst component first, so a failing one is not buried under healthy ones. */
export const componentRank = (component: ProjectComponent): number => STATUS_RANK[component.status];

export const incidentKey = (incident: ProjectIncident): string => incident.id;
export const incidentRank = (incident: ProjectIncident): number => SEVERITY_RANK[incident.severity];

/** Open incidents, critical first and then newest within a severity. */
export function sortIncidents(incidents: readonly ProjectIncident[]): readonly ProjectIncident[] {
  return [...incidents].sort(
    (a, b) =>
      SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity] || b.openedAt.localeCompare(a.openedAt),
  );
}

/** The headline counts the summary row draws. */
export interface ProjectsSummary {
  readonly total: number;
  readonly up: number;
  readonly degraded: number;
  readonly down: number;
  readonly unknown: number;
  readonly openIncidents: number;
}

export function summary(projects: readonly CloudProject[]): ProjectsSummary {
  let up = 0;
  let degraded = 0;
  let down = 0;
  let unknown = 0;
  let openIncidents = 0;
  for (const project of projects) {
    if (project.status === 'up') up += 1;
    else if (project.status === 'degraded') degraded += 1;
    else if (project.status === 'down') down += 1;
    else unknown += 1;
    openIncidents += project.openIncidents.length;
  }
  return { total: projects.length, up, degraded, down, unknown, openIncidents };
}

/** `12 ms`, or an em dash when the collector measured no latency. */
export function formatLatency(ms: number | null): string {
  return ms === null || !Number.isFinite(ms) ? '—' : `${Math.round(ms)} ms`;
}

/** A plain component number, or an em dash when it was not measured. */
export function formatNumber(value: number | null): string {
  return value === null || !Number.isFinite(value) ? '—' : String(value);
}
