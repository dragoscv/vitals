/**
 * The Projects tab's data source — and the ONE seam where the Codwen
 * project-health API will replace a fixture.
 *
 * # Why a fixture is here at all
 *
 * `GET /v1/platform/projects` (NOT `/v1/projects`) is the real route; it exists
 * in codai main (`apps/gateway/src/routes/projects.ts`, schema
 * `projects-health.ts`, migrations `0222`/`0223`), but is not yet confirmed
 * deployed, and the `projects:read` scope is not yet on this client's key. The
 * page is a read-client for a contract written down but not yet served live.
 *
 * So this file stands in for that read. It is deliberately the *only* place
 * that knows the shape of the answer, so swapping the fixture for the real
 * endpoint is a single edit to `readProjects` below — the hook, the model and
 * the panel never change.
 *
 * # What is real and what is faked
 *
 * The account is real: whether a Codwen key is stored comes from the existing
 * `get_cloud_status` command, the same read the account card on the Cloud tab
 * makes. Only the project list is faked, and only while the endpoint is
 * absent. A signed-out user sees the real sign-in state, never a fixture.
 *
 * The fixture is shipped **only in a dev build** (`import.meta.env.DEV`). A
 * production build with a signed-in account returns `unavailable` and the page
 * says so, rather than showing invented projects as if they were the user's.
 *
 * # Do not invent an endpoint or a field
 *
 * The fixture's fields are exactly the brief's §3.1 shape, camelCase (this
 * repo generates wire types from Rust with `rename_all = "camelCase"` and
 * `check-drift` enforces it). Nothing here probes a project's URL, writes
 * project state, or persists the list as local truth (rules 1-3).
 */

import {
  projectRank,
  type CloudProject,
  type HealthStatus,
  type ProjectsSnapshot,
} from './projectsModel';

/** What one read of the project list answered. */
export type ProjectsRead =
  /** No Codwen account is stored — the page shows the sign-in state. */
  | { readonly state: 'signedOut' }
  /** Signed in, but the endpoint that would answer does not exist yet. */
  | { readonly state: 'unavailable' }
  | { readonly state: 'ok'; readonly snapshot: ProjectsSnapshot };

/** The seam the panel depends on. One method, swappable in tests. */
export interface ProjectsSource {
  read(): Promise<ProjectsRead>;
}

/**
 * Whether the fixture stands in for the absent endpoint.
 *
 * `import.meta.env.DEV` is true under `vite dev` (which is what the e2e suite
 * and local review run) and false in a packaged desktop build, so a real user
 * never sees fabricated projects.
 */
const FIXTURE_ENABLED = import.meta.env.DEV;

/** Whether a Codwen key is stored. Real — the account works today. */
async function cloudSignedIn(): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  const status = await invoke<{ signedIn: boolean }>('get_cloud_status');
  return status.signedIn;
}

/**
 * THE SEAM.
 *
 * When `GET /v1/platform/projects` is live, this function's smoke-and-fixture middle
 * becomes one call — read the key, request the list, parse it against the §3.1
 * shape — and `fixtureProjects` is deleted. Nothing else in the tab changes.
 */
export async function readProjects(): Promise<ProjectsRead> {
  if (!(await cloudSignedIn())) return { state: 'signedOut' };
  if (!FIXTURE_ENABLED) return { state: 'unavailable' };
  return { state: 'ok', snapshot: fixtureProjects(Date.now()) };
}

/** The Tauri-backed source. */
export const tauriProjectsSource: ProjectsSource = { read: readProjects };

function component(
  key: string,
  label: string | null,
  status: HealthStatus,
  latencyMs: number | null,
  value: number | null,
  checkedAt: string,
): CloudProject['components'][number] {
  return { key, label, status, latencyMs, value, detail: null, checkedAt };
}

/**
 * A plausible answer — the shape the panel expects, so the page is reviewable
 * before the contract freezes.
 *
 * `now` is a parameter so the ages it renders can be pinned in a test.
 */
export function fixtureProjects(now: number): ProjectsSnapshot {
  const iso = (agoMs: number): string => new Date(now - agoMs).toISOString();
  const projects: CloudProject[] = [
    {
      id: 'prj_checkout',
      slug: 'checkout-api',
      name: 'Checkout API',
      kind: 'service',
      visibility: 'public',
      // Derived server-side as worst-of-latest; here a degraded component wins.
      status: 'degraded',
      lastCheckedAt: iso(42_000),
      components: [
        component('http', 'HTTPS endpoint', 'up', 118, null, iso(42_000)),
        component('webhook', 'Stripe webhook delivery', 'degraded', 2_450, 37, iso(40_000)),
        // Not measured: the collector reported no latency for this one.
        component('db_replica', 'Read replica lag', 'up', null, null, iso(41_000)),
      ],
      openIncidents: [
        {
          id: 'inc_9f21',
          severity: 'warning',
          summary: 'Stripe webhook delivery is slower than its 30 s budget.',
          componentKey: 'webhook',
          openedAt: iso(28 * 60_000),
          // Not delivered: the alert was opened but no channel confirmed it.
          notifiedAt: null,
        },
      ],
    },
    {
      id: 'prj_status_page',
      slug: 'status-page',
      name: 'Public status page',
      kind: 'webapp',
      visibility: 'public',
      status: 'up',
      lastCheckedAt: iso(15_000),
      components: [
        component('http', 'HTTPS endpoint', 'up', 96, null, iso(15_000)),
        component('dns', 'DNS resolution', 'up', 12, null, iso(14_000)),
      ],
      openIncidents: [],
    },
    {
      id: 'prj_invoice_worker',
      slug: 'invoice-worker',
      name: 'Invoice worker',
      kind: 'worker',
      visibility: 'private',
      status: 'down',
      lastCheckedAt: iso(6 * 60_000),
      components: [
        component('http', 'HTTPS endpoint', 'down', null, null, iso(6 * 60_000)),
        component('queue_depth', 'Queue depth', 'degraded', null, 1_204, iso(6 * 60_000)),
      ],
      openIncidents: [
        {
          id: 'inc_1a05',
          severity: 'critical',
          summary: 'Invoice worker has not answered a probe for six minutes.',
          componentKey: 'http',
          openedAt: iso(6 * 60_000),
          notifiedAt: iso(5 * 60_000),
        },
      ],
    },
    {
      id: 'prj_docs',
      slug: 'developer-docs',
      name: 'Developer documentation and API reference site',
      kind: 'webapp',
      visibility: 'public',
      // Nothing has been measured for this project yet.
      status: 'unknown',
      lastCheckedAt: null,
      components: [component('http', 'HTTPS endpoint', 'unknown', null, null, iso(3 * 3_600_000))],
      openIncidents: [],
    },
  ];
  // Worst-first, the order a health rollup is read in: a project that goes down
  // leads the list, and the panel selects the first row. The client renders the
  // order its source gives it (held stable by `useStableOrder`), so the order is
  // the source's decision — here the fixture stands in for it.
  return {
    takenAtMs: now,
    projects: [...projects].sort((a, b) => projectRank(a) - projectRank(b)),
  };
}
