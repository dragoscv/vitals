> Status: research snapshot, 2026-11-25. Not normative — see docs/TRACKER.md and the ADRs for decisions.

# Codwen (E:\gh\codai) — project/space health, alerts and notifications: what exists today

**Question (verbatim):** what already exists in this repository for these six things, and where would new work naturally attach?

1. Project / space / tenant health and status on PRODUCTION (registry, health/status collection, uptime/alerting, per-project metadata) — the product's own idea of "my projects and whether they are healthy".
2. A SINGLE SOURCE OF TRUTH per project for status, alerts and collected values.
3. Reusable notification channels: email (via Brivio, never Resend), Telegram, and in-app notifications.
4. The "Codwen Hub" idea: where projects/spaces are listed, added, managed, organised — and its real name in this codebase.
5. Public vs private data per project, and how a differently-authenticated reader is kept out.
6. How an AGENT can read any of the above: MCP servers, gateway routes, internal APIs, CLI, SDK.

**Method:** read-only. `rg` over the tracked tree (gitignore respected, so `node_modules`/build output excluded), `git log`/`git show`, `Get-Content` and direct file reads. No file in `E:\gh\codai` was edited, staged, committed or deployed.

**Repository HEAD at the time of reading:** `5080b28dfd` (`fix(ci): correct three stale round-5 receipt claims and regenerate the receipt`), on `main`, equal to `origin/main`.

**Verdict in one line:** the _design_ for exactly this feature landed **yesterday** — a DB schema and migration for a project-health SSOT (`platform_projects` / `project_components` / `project_signals` / `project_incidents` / `notification_preferences`), commit `49861becf7`, migration `0222` — but **not one line of TypeScript reads or writes those tables yet**, and the design docs the migration cites do not exist. Everything else the question asks about exists in a _different_ shape (a global status page, a user-level notification inbox, a VM-oriented `projects` table for cloud environments).

---

## Summary table

| #   | Concern                                      | Status                                 | Exact paths (line refs)                                                                                                                                                                                                                                                                                                                                                                                                                                    | What it actually does                                                                                                                                                                                                                                                                                                                                                     |
| --- | -------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Project registry for production health       | **PARTIAL**                            | `packages/db/src/schema/projects-health.ts:47-78` (`platform_projects`); migration `packages/db/drizzle/0222_projects_health.sql:15-31`                                                                                                                                                                                                                                                                                                                    | A platform registry table exists (slug, name, kind, visibility, repo path/url, `environments` JSON, owner). **No rows, no reader, no writer anywhere in the repo.**                                                                                                                                                                                                       |
| 1   | Tenant "my projects" (cloud environments)    | **EXISTS**                             | `packages/db/src/schema/environments.ts:28-60` (`projects`), `:78-130` (`environments`, incl. `ENVIRONMENT_STATES` and `state_reason`)                                                                                                                                                                                                                                                                                                                     | A project groups VM/container environments. Health is only the per-VM lifecycle `state` (`pending`…`running`…`error`) plus `state_reason`. No uptime, no incident, no alert.                                                                                                                                                                                              |
| 1   | Health/status COLLECTION                     | **PARTIAL**                            | `packages/db/src/schema/status.ts:12-24` (`status_samples`); `apps/jobs/src/jobs/status-probe.ts:282-324`                                                                                                                                                                                                                                                                                                                                                  | One cron job probes the **gateway** (`gateway`, `readiness`) every 5 min and writes one row per component. Single-writer, global — **not per project**. Schema for per-project `project_signals` exists (`projects-health.ts:100-130`) with no collector.                                                                                                                 |
| 1   | Uptime                                       | **PARTIAL (global only)**              | `apps/web/components/status/uptime-data.ts:26-60`, `uptime-buckets.ts`, `uptime-bars.tsx`; page `apps/web/app/(marketing)/[locale]/status/page.tsx`, JSON `apps/web/app/(marketing)/status.json/route.ts`                                                                                                                                                                                                                                                  | A public status page renders 24 h and 90-day uptime bars from `status_samples`. Per-component, not per project.                                                                                                                                                                                                                                                           |
| 1   | Alerting / paging                            | **EXISTS (ops-only)**                  | `packages/db/src/alert-page.ts` (L-12, FL-172); routes `apps/gateway/src/routes/…/internal/alerts/page` and `apps/mgmt/src/routes/internal-alert-page.ts`                                                                                                                                                                                                                                                                                                  | A Cloud Monitoring incident → one unread `ops.alert` notification for every `super_admin` + FCM push to their phones. Basic-auth with its own `ALERT_PAGE_TOKEN`; dedupe 5 min in-process. Cloud-monitoring-shaped, **not per project**.                                                                                                                                  |
| 1   | Provider/lane health UI                      | **EXISTS (ops-only)**                  | `apps/hub/app/(dash)/ops/provider-health/page.tsx` + `_lib/provider-health-{data,model}.ts`                                                                                                                                                                                                                                                                                                                                                                | Live state of every model-provider lane (healthy / degraded / unservable), `super_admin` only, 404 for anyone else.                                                                                                                                                                                                                                                       |
| 2   | Single source of truth per project           | **ABSENT in code; designed in schema** | `packages/db/src/schema/projects-health.ts` (whole file); header comment at `:17-44` calls it "Project health SSOT (2026-11-25)"                                                                                                                                                                                                                                                                                                                           | The five tables are the intended SSOT. `rg -l 'platformProjects\|projectSignals\|projectIncidents\|notificationPreferences'` over the whole repo returns **only** the schema file itself (plus the migration and `schema/index.ts:98`). Today the closest _working_ single writer is `status_samples` (global), and the closest per-project fact is `environments.state`. |
| 3   | Email via Brivio                             | **EXISTS**                             | `packages/email/src/transport.ts:1-160` (the ONE outbound path; `brivio \| smtp \| off`, `POST {BRIVIO_API_URL}/v1/email/send`); senders `packages/email/src/{index,send,billing-notify,feed-digest,org-invite,onboarding,waitlist}.ts`; templates `packages/email/src/templates/*.tsx`                                                                                                                                                                    | Every surface (IdP, jobs, hub, web) sends through this one module. `EmailCategory` already includes `'alert'` (`transport.ts:45`), and `EmailMessage.idempotencyKey` is hashed into the brivio `Idempotency-Key`.                                                                                                                                                         |
| 3   | No Resend                                    | **VERIFIED**                           | `rg -i resend --glob '**/package.json' apps packages` → **no hits**                                                                                                                                                                                                                                                                                                                                                                                        | No Resend dependency in any workspace manifest. `plan/feature-platform-2026-09.md:142` records the decision and the `codai-resend-api-key` → `codai-smtp-password` swap.                                                                                                                                                                                                  |
| 3   | Telegram                                     | **ABSENT**                             | Only `packages/rules-core/src/tag.ts:30` (`'telegram_*'` policy tag) and the new enum at `projects-health.ts:196`                                                                                                                                                                                                                                                                                                                                          | No bot token, no `api.telegram.org` call, no sender. The name exists only as a policy tag and as a value in a schema enum nothing reads.                                                                                                                                                                                                                                  |
| 3   | In-app notifications                         | **EXISTS**                             | Table `packages/db/src/schema/triggers.ts:60-79` (`notifications`: user, kind, severity, title, body, url, session_id, trigger_event_id, read_at); UI `apps/hub/app/(dash)/notifications/{page.tsx,_lib/notifications-db.ts,_lib/notifications-data.ts,actions.ts}`                                                                                                                                                                                        | A user-scoped inbox. Eight writer sites today: `apps/gateway/src/triggers/service.ts:181`, `feed/notify.ts:179`, `lists/sweep.ts:35`, `schedules/service.ts:817`, `routes/environment-tasks.ts:349`, `routes/desktop-notify.ts:121`, `routes/prompts.ts:1286`, `routes/visual-qa-digest.ts:163`.                                                                          |
| 3   | Push (mobile + web)                          | **EXISTS**                             | `packages/db/src/fcm.ts:211` (`postNotificationPush`); `apps/gateway/src/push/fcm.ts:143` (`sendNotificationPush`), `push/web-push.ts:151` (`sendWebPushToUser`), `push/session-notify.ts`; subscriptions `packages/db/src/web-push.ts:28-146`                                                                                                                                                                                                             | The in-app row is inserted first, then pushed; `notifyTaskOwner` in `environment-tasks.ts:343-360` documents the pattern ("the ONE push path… Fail-open").                                                                                                                                                                                                                |
| 3   | Channel configurable per project             | **ABSENT (designed only)**             | `notification_preferences` at `projects-health.ts:202-230`                                                                                                                                                                                                                                                                                                                                                                                                 | `(user, project, channel, min_severity, enabled)`, `NULL project_id` = all projects. Schema-only; nothing writes it.                                                                                                                                                                                                                                                      |
| 4   | "Codwen Hub"                                 | **EXISTS**                             | `apps/hub` (`@codai/hub`, `apps/hub/package.json:2`), dev port 3041; named in `plan/feature-platform-2026-09.md:18` as `hub.codai.ro`                                                                                                                                                                                                                                                                                                                      | The Hub is a Next.js 16 customer dashboard (keys, usage, billing, orgs/teams, devices, settings, GDPR, apps). The term "Hub" is the product name; there is **no** "space"/"workspace" entity.                                                                                                                                                                             |
| 4   | Projects listed/added/managed in the Hub     | **EXISTS**                             | `apps/hub/app/(dash)/projects/page.tsx`, `projects/[slug]/page.tsx`, `_lib/{projects-db,projects-data,project-members-db,view-model,schemas}.ts`; components `apps/hub/components/projects/*`; nav entry `apps/hub/components/nav.ts:101` (`{ key: 'projects', href: '/projects', icon: FolderGit2 }`)                                                                                                                                                     | Lists the caller's projects with a non-destroyed-environment count, creates/deletes them, and drills into per-environment detail grouped per developer.                                                                                                                                                                                                                   |
| 4   | Real name of the tenant/organisation concept | **EXISTS**                             | `orgs` and `orgMemberships` at `packages/db/src/schema/sharing.ts:42,66`; membership helper `canManageAccount` used at `projects-db.ts:70-83`                                                                                                                                                                                                                                                                                                              | The boundary is **org** (plus the owning `user`); a `projects` row carries both `owner_user_id` and `org_id`. There is no "space" table. A Brivio "Space" is referenced only in a comment (`projects-db.ts:66`, PF-023) as an external linkage.                                                                                                                           |
| 5   | Public vs private per project                | **PARTIAL**                            | `platform_projects.visibility` enum `public \| private` at `projects-health.ts:41-45,64`; comment: public = readable by the public status page                                                                                                                                                                                                                                                                                                             | Designed. Nothing reads it.                                                                                                                                                                                                                                                                                                                                               |
| 5   | Existing public/private mechanisms           | **EXISTS**                             | Public routes mounted unauthenticated at `/v1`: `publicProvenanceRoute` (`server.ts:411`), `publicFeedRoute` (`server.ts:437`), `publicModelsRoute` (`server.ts:443`), token-addressed `/v1/public/transcripts/:token` (`server.ts:458`); share rows `sessionShares` (`sharing.ts:88`); scope allow-list `api_keys.scopes` (`api-keys.ts:133-134`) enforced by `scopedKeyAllows` (`apps/gateway/src/auth.ts:218`) and `envScopedKeyAllows` (`auth.ts:115`) | The established pattern is: never widen an authenticated query — add a **separate, explicitly filtered public route** (or a token-addressed share), and enforce per-key route scopes in gateway auth. Owner scoping is applied per query (e.g. `notifications-db.ts:29-35` binds `user_id = userId`; `projects-db.ts:30-43` binds `owner_user_id = userId`).              |
| 5   | Tenant-scope enforcement gate                | **EXISTS**                             | `scripts/ci/check-tenant-scope.mjs` (present; wired into `gate:platform` and pre-commit per `plan/feature-platform-2026-09.md:140`)                                                                                                                                                                                                                                                                                                                        | 13 unscoped `@codai/db` helpers are banned inside `apps/hub`; a deliberate violation fails the gate.                                                                                                                                                                                                                                                                      |
| 6   | MCP endpoint                                 | **EXISTS**                             | `apps/gateway/src/routes/mcp.ts:187` (`POST /mcp`); OAuth metadata `mcp.ts:134`; toolset selection `apps/gateway/src/mcp/toolsets.ts:16-160`                                                                                                                                                                                                                                                                                                               | One MCP server for the whole product; `?toolsets=a,b` or `X-MCP-Toolsets` trims `tools/list`. `tools/call` is **not** gated by the selection.                                                                                                                                                                                                                             |
| 6   | MCP tools — projects/environments            | **EXISTS**                             | `apps/gateway/src/mcp/environment-tools.ts:125-251`; group `environments` in `toolsets.ts:47-60`                                                                                                                                                                                                                                                                                                                                                           | `list_projects`, `list_environments`, `get_environment`, `start_environment`, `stop_environment`, `list_environment_ports`, `create_environment_task`, `list_environment_tasks`, `get_environment_task`, `list_my_agent_tasks`, `cancel_agent_task`, `retry_agent_task`. Each replays an in-process REST call with the caller's own `Authorization`.                      |
| 6   | MCP tools — notifications                    | **EXISTS**                             | `apps/gateway/src/mcp/notification-tools.ts:24-90`; group `notifications` in `toolsets.ts:95-102`                                                                                                                                                                                                                                                                                                                                                          | `notifications_list`, `notifications_mark_read`, `notifications_dismiss`, plus `triggers_list`, `triggers_update`, `triggers_test`, `triggers_events`. Also `orgs_list` / `org_members_list` in the `account` group (`toolsets.ts:36-37`).                                                                                                                                |
| 6   | Gateway REST routes                          | **EXISTS**                             | `apps/gateway/src/routes/environments.ts:633-1833` (projects + environments + members + ports + secrets), `routes/environment-tasks.ts`; mounted at `/v1` in `apps/gateway/src/server.ts:409-460`; documented `apps/docs/openapi/en/gateway.yaml:13246-16225`                                                                                                                                                                                              | `GET                                                                                                                                                                                                                                                                                                                                                                      | POST /v1/projects`, `GET | PATCH | DELETE /v1/projects/{id}`, `/v1/projects/{id}/teams`, `GET | POST /v1/environments`, `/v1/environments/{id}`(+`start | stop | archive | destroy`, `members`, `ports`, `secrets`, `tasks`, `exec`), `GET /v1/notifications`, `POST /v1/notifications/read`. |
| 6   | SDK                                          | **EXISTS**                             | `packages/sdk/src/resources/projects.ts` (list/create/get/update/delete); generated types `packages/sdk/src/generated/gateway.ts`; Python `packages/sdk-python`                                                                                                                                                                                                                                                                                            | The TS SDK mirrors `/v1/projects/*`. The OpenAPI file is declared ground truth for both SDKs, with parity tests.                                                                                                                                                                                                                                                          |
| 6   | CLI                                          | **PARTIAL**                            | `packages/agent-kit/package.json:15` → bin `codwen-agent-kit`; bins also in `packages/{acp,devsync,devcontainer,compression,app-platform,agent-headless}` and `apps/mobile-agent`                                                                                                                                                                                                                                                                          | There is an agent-kit CLI and several tool CLIs, but **no project/environment CLI**.                                                                                                                                                                                                                                                                                      |
| 6   | Anything for project signals/incidents       | **ABSENT**                             | —                                                                                                                                                                                                                                                                                                                                                                                                                                                          | No route, no MCP tool, no SDK method, no UI reads `project_signals` or `project_incidents`.                                                                                                                                                                                                                                                                               |

---

## The natural extension point, per concern

### 1. Project health and status on production

- **Registry:** `packages/db/src/schema/projects-health.ts:47-78` is already the intended registry — a new feature **adds rows to it**, it does not invent a table. The tenant table to keep separate is `projects` in `packages/db/src/schema/environments.ts:28-60`; the schema comment at `projects-health.ts:24-28` states the split explicitly (ADR D1) precisely so nobody merges them.
- **Collection:** the collector belongs beside the existing cron job — `apps/jobs/src/jobs/status-probe.ts` (registered in `apps/jobs/src/jobs/index.ts:39`) is the template: one job, one writer, one row per component per probe. For per-project checks it should write `project_signals` instead of `status_samples`.
- **Alerting:** `packages/db/src/alert-page.ts` is the paging core for the _ops_ case; the per-project equivalent wants the same shape but sourced from `project_incidents` (already carrying `dedupe_key`, `notified_at`, partial unique index `project_incidents_open_dedupe_unique`, migration `0222_projects_health.sql`).
- **Do not** fold per-project health into `status_samples`: it has no project column and its only consumer is the public `/status` page.

### 2. Single source of truth per project

- **The intended SSOT already exists as a schema, and nowhere else.** `platform_projects` (identity + visibility + environments) → `project_components` (declared checks) → `project_signals` (the measurements) → `project_incidents` (episodes) → `notification_preferences` (routing). One collector writes signals; everything else reads.
- **The closest working thing today:** `status_samples` for global uptime, plus `environments.state` for per-VM status. Neither is per project, and neither is an SSOT for alerts.
- **Honest statement:** there is **no** single source of truth in code today. There is a committed, migrated, empty one.
- **Attach: nowhere else.** Adding a second "project status" table would contradict the file's own header comment and the cited ADR.

### 3. Notification channels

- **Email:** call `createMailer(...).send(msg)` from `packages/email/src/transport.ts`; add a sender module beside `packages/email/src/billing-notify.ts` and a template in `packages/email/src/templates/`. `category: 'alert'` already exists. Never add a provider SDK — the transport is the only seam.
- **Telegram:** nothing to attach to. A new channel is a new module (there is no `packages/notify` yet) plus a token in the managed secrets; the schema slot for it is `NOTIFICATION_CHANNELS` at `projects-health.ts:196`.
- **In-app:** the established path is _insert a `notifications` row, then push_ — see `notifyTaskOwner` in `apps/gateway/src/routes/environment-tasks.ts:343-360` and `packages/db/src/fcm.ts:211` / `apps/gateway/src/push/{fcm,web-push}.ts`. A per-project alert should reuse `notifications` (kind, severity, url) so the Hub inbox and the MCP tools pick it up for free.
- **Per-project channel config:** `notification_preferences` (`projects-health.ts:202-230`) is the slot. Note the in-file caveat at `:224-227`: the unique index does **not** dedupe the `project_id IS NULL` ("all projects") row — callers must upsert on `project_id IS NULL` explicitly.

### 4. The Codwen Hub

- **The Hub is `apps/hub`** (`@codai/hub`, `hub.codai.ro`). Projects are already listed, created and opened there: `apps/hub/app/(dash)/projects/page.tsx` (list), `projects/[slug]/page.tsx` (detail), server layers in `projects/_lib/*`, components in `apps/hub/components/projects/*`, nav at `apps/hub/components/nav.ts:101`.
- **The tenant unit is `org`**, not "space"/"workspace" (`packages/db/src/schema/sharing.ts:42,66`; membership via `canManageAccount`). The per-project unit is `projects` (`environments.ts:28`).
- **Natural attach point for a health view:** a new section/route under `apps/hub/app/(dash)/projects/[slug]/` (detail page already renders the project + its environments), reading the `projects-health` tables through a new `_lib/` data module following `notifications-db.ts`'s owner-scoped pattern.

### 5. Public vs private

- **The column to use is `platform_projects.visibility`** (`projects-health.ts:64`), which already declares its intent: `public` → readable by the public status page, `private` → hub/owner only.
- **The mechanism to copy** is the existing convention: an authenticated owner/org-scoped route (bound by `user_id` / `owner_user_id`, as in `notifications-db.ts:29-35` and `projects-db.ts:30-43`) **plus** a separate, separately-mounted unauthenticated route under `/v1/public/*` or the static Next route (as `apps/web/app/(marketing)/status.json/route.ts` does today). Never relax an existing query's `WHERE`.
- **Agent keys:** a read-only key is expressed as `api_keys.scopes` (`api-keys.ts:133-134`) and enforced in `apps/gateway/src/auth.ts:218`; environment keys additionally via `envScopedKeyAllows` (`auth.ts:115`).

### 6. Agent access

- **Add an MCP toolset group** in `apps/gateway/src/mcp/toolsets.ts` beside `environments` / `notifications`, with the tool module in `apps/gateway/src/mcp/<name>-tools.ts` following `environment-tools.ts` (each tool replays an in-process REST call with the caller's own `Authorization`, so per-user scoping and the exact API validation apply).
- **Add REST routes** in a new `apps/gateway/src/routes/project-health.ts` and mount with `app.route('/v1', …)` in `apps/gateway/src/server.ts` (lines 409-460 are the mount block).
- **Mirror them** into `apps/docs/openapi/en/gateway.yaml` + `ro/gateway.yaml` (the declared ground truth) and regenerate `packages/sdk/src/generated/gateway.ts`; add the TS SDK resource beside `packages/sdk/src/resources/projects.ts` and the Python counterpart.

---

## Files a new feature would have to touch (5-8)

1. `packages/db/src/schema/projects-health.ts` — the five tables already exist; any new column/value lands here.
2. `packages/db/drizzle/0223_*.sql` **and** `packages/db/drizzle/meta/_journal.json` — additive, guarded, re-runnable in the style of `0222_projects_health.sql` (the journal already carries `"tag": "0222_projects_health"` at line 1549).
3. `apps/jobs/src/jobs/<collector>.ts` + registration in `apps/jobs/src/jobs/index.ts` — the single writer of `project_signals` (template: `status-probe.ts`).
4. `apps/gateway/src/routes/<project-health>.ts` + the mount line in `apps/gateway/src/server.ts` — the read API.
5. `apps/gateway/src/mcp/<project-health>-tools.ts` + a group in `apps/gateway/src/mcp/toolsets.ts` — agent access.
6. `apps/hub/app/(dash)/projects/[slug]/…` + a `_lib/*-data.ts` — the human view, owner/org-scoped.
7. `apps/hub/messages/en.json` **and** `apps/hub/messages/ro.json` — both locales (parity test `apps/hub/i18n/messages-parity.test.ts`).
8. `apps/docs/openapi/{en,ro}/gateway.yaml` + `packages/sdk/src/generated/gateway.ts` + `packages/sdk/src/resources/*` — contract parity.

Plus, for notifications: `packages/email/src/<new-sender>.ts` and `packages/email/src/templates/<new>.tsx`.

### Invariants a change must respect

- **Two project notions, deliberately separated.** `platform_projects` is the platform registry; `projects` (`environments.ts`) is org-scoped tenant data. The schema states this (ADR D1) so a change must not merge them.
- **One open incident per `dedupe_key`**, enforced by the partial unique index `project_incidents_open_dedupe_unique` (`projects-health.ts:161-167`) — "so two channels can never disagree". Do not add a second incident row for the same episode.
- **One `project_components.key` per project**, DB-enforced (`project_components_project_key_unique`).
- **`notification_preferences` does not dedupe the all-projects row** (NULLs are distinct in Postgres) — upsert on `project_id IS NULL` explicitly.
- **Tenant scoping is gated.** `scripts/ci/check-tenant-scope.mjs` bans 13 unscoped `@codai/db` helpers inside `apps/hub`; every new hub query must bind `user_id` / `owner_user_id` / `org_id`.
- **Never widen an existing query for public data** — add a separate, explicitly filtered public route or a token-addressed share.
- **Migrations are additive and guarded** (the 0221/0222 discipline: every statement re-runnable, nothing drops or mutates data).
- **Locale parity is tested** for the hub (`apps/hub/i18n/messages-parity.test.ts`) — EN and RO ship together.
- **Generated contract parity:** `apps/docs/openapi/en/gateway.yaml` is the ground truth; the RO file and both SDKs have parity tests that fail when an operation id is missing.
- **Deploys:** build from a clean detached worktree, order commit → push → deploy (repo-wide rule recorded in `plan/feature-platform-2026-09.md:141`).

---

## What the agent in the Vitals repository needs from Codwen

The shape below is **derived from tables that exist**, with the derived fields named explicitly. A Vitals page showing "my projects: status, health, alerts" needs exactly this and nothing more:

```ts
/** One row per platform project, plus its derived health. */
export interface CodwenProject {
  slug: string; // platform_projects.slug — unique, stable key
  name: string; // platform_projects.name
  kind: 'product' | 'service' | 'app' | 'site' | 'library' | 'repo' | 'research' | 'tool'; // PLATFORM_PROJECT_KINDS
  visibility: 'public' | 'private'; // platform_projects.visibility
  description: string | null;
  repoUrl: string | null;
  /** Where the project runs: platform_projects.environments (jsonb). */
  environments: { key: string; url?: string }[];

  /** DERIVED — worst status across the project's latest component signals. */
  health: 'up' | 'degraded' | 'down' | 'unknown';
  /** ISO-8601 of the newest signal for the project. DERIVED. */
  lastCheckedAt: string | null;

  components: {
    key: string; // project_components.key
    label: string | null;
    kind: 'http' | 'liveness' | 'readiness' | 'job' | 'queue' | 'cost' | 'quota' | 'custom';
    status: 'up' | 'degraded' | 'down' | 'unknown'; // project_signals.status
    latencyMs: number | null;
    value: number | null; // numeric(20,6)
    detail: Record<string, unknown> | null;
    checkedAt: string; // timestamptz
  }[];

  openIncidents: {
    severity: 'info' | 'warning' | 'critical';
    summary: string;
    componentKey: string;
    openedAt: string;
    notifiedAt: string | null; // null = no channel has delivered it yet
  }[];
}
```

**Fields that DO NOT exist yet** (they must be produced by whatever serves this; do not assume they are stored):

| Field                                              | Why it does not exist                                                                                                                                        |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `health` (project-level aggregate)                 | No such column anywhere. `platform_projects` has no status column; `project_signals.status` is per component. Must be computed (worst-of-latest) by the API. |
| `lastCheckedAt`                                    | Only per signal (`project_signals.checked_at`).                                                                                                              |
| `uptimePct24h` / `uptimePct90d`                    | Uptime is computed today **only globally**, from `status_samples` (`apps/web/components/status/uptime-data.ts`). No per-project uptime exists.               |
| Any HTTP endpoint or MCP tool returning this shape | No route, tool, SDK method or UI reads `platform_projects` / `project_signals` / `project_incidents`.                                                        |
| A public variant for `visibility = 'public'`       | The column exists; no route serves it.                                                                                                                       |
| Telegram delivery status                           | No Telegram integration exists at all.                                                                                                                       |
| `incidents[]` history (resolved)                   | The table holds resolved rows, but nothing exposes them; `project_incidents.status` is `open \| resolved`.                                                   |

**Minimum viable Vitals page:** for each project — `name`, `health`, `lastCheckedAt`, the failing component(s) and the open incidents' `severity` + `summary`. Everything in that sentence is expressible in the schema above and exposed by **nothing** today: the Codwen side of this feature is a backend (collector + read API + one MCP toolset group) that has not been written.

---

## VERIFIED vs EXPECTED

**VERIFIED (read the file / ran the command and saw the output):**

- `packages/db/src/schema/projects-health.ts` in full — five tables, enums, the two DB-enforced invariants, the SSOT header comment, the `notification_preferences` NULL caveat.
- `packages/db/drizzle/0222_projects_health.sql` header + `CREATE TABLE platform_projects`; `packages/db/drizzle/meta/_journal.json:1549`.
- `rg -l 'platformProjects|projectComponents|projectSignals|projectIncidents|notificationPreferences' .` → only `packages/db/src/schema/projects-health.ts`.
- `rg -l -i 'project health|projects-health|platform_projects' .` → only the migration, the schema and `schema/index.ts`.
- `git show --name-only 49861becf7` → db schema, migration, journal, index only. `git log -1` HEAD `5080b28dfd`.
- `Test-Path docs/project-health.md` → **False**; `Test-Path docs/decisions/2026-11-25-projects-health-ssot.md` → **False**.
- `packages/db/src/schema/environments.ts:1-140`; `.../status.ts:1-24`; `.../triggers.ts:1-80`; `.../hub.ts:1-120`; `.../sharing.ts` table list.
- `packages/db/src/alert-page.ts:1-50`.
- `packages/email/src/transport.ts:1-160`; `rg -i resend --glob '**/package.json' apps packages` → no hits.
- `apps/gateway/src/mcp/notification-tools.ts:1-120`; `mcp/toolsets.ts:12-160`; `mcp/environment-tools.ts:125-251`; `routes/mcp.ts:115-592`.
- `apps/gateway/src/routes/environments.ts:633-1833`; `routes/environment-tasks.ts:338-360`.
- `packages/sdk/src/resources/projects.ts:1-56`; `packages/agent-kit/package.json:15`.
- `apps/hub/app/(dash)/projects/_lib/projects-db.ts:1-150`; `.../projects/_lib/view-model.ts:1-60`; `.../(dash)/notifications/_lib/notifications-db.ts:1-60`; `apps/hub/components/nav.ts:101`.
- `apps/hub/app/(dash)/ops/provider-health/page.tsx:1-25`; `apps/hub/package.json`.
- `apps/jobs/src/jobs/status-probe.ts` (lines 282, 312, 323) and `apps/jobs/src/jobs/index.ts:39`.
- `apps/web/components/status/*`, `apps/web/app/(marketing)/[locale]/status/page.tsx`, `apps/web/app/(marketing)/status.json/route.ts`.
- `apps/docs/openapi/en/gateway.yaml` path entries at lines 13246-16225.
- `Test-Path scripts/ci/check-tenant-scope.mjs` → **True**; `apps/hub/i18n/messages-parity.test.ts` exists.
- `apps/gateway/src/auth.ts` exports (`scopedKeyAllows:218`, `envScopedKeyAllows:115`).
- `packages/db/src/schema/api-keys.ts:133-134` (scopes column).

**EXPECTED / inferred (documentation-based or reasoned, not executed):**

- The `gate:platform` / pre-commit wiring of `check-tenant-scope.mjs` — taken from the W10-03 entry in `plan/feature-platform-2026-09.md:140`, not from reading the CI config.
- That the client-owned MCP bridges under `apps/desktop/mcp/*.mjs` are separate from the gateway `/mcp` server (inferred from their location and names).
- That `apps/hub` (`hub.codai.ro`) is the surface the question means by "Codwen Hub" — the name is confirmed in `plan/feature-platform-2026-09.md:18`, but the deployment/DNS binding was not exercised.
- Any claim about PRODUCTION state (whether migration `0222` has been applied to the production database) — not verified; the migration exists in the repository only.

---

## Commands used (read-only)

```powershell
Set-Location E:\gh\codai
rg -l 'platformProjects|projectComponents|projectSignals|projectIncidents|notificationPreferences' .
rg --files packages/db/src/schema
git show --name-only 49861becf7
git log --oneline -20
rg -n -i 'uptime|statuspage|public status' apps packages
rg -l 'modelcontextprotocol' apps packages --glob '!*.test.ts'
rg -n 'insert\(schema\.notifications|insert\(notifications' apps packages --glob '!*.test.ts'
rg -n -i 'api\.telegram\.org|telegram_bot|TELEGRAM_' apps packages --glob '!*.test.ts'
rg -n -i resend --glob '**/package.json' apps packages
```

No file in `E:\gh\codai` was modified; nothing was staged, committed or deployed.
