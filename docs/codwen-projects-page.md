# Codwen projects page — implementation brief for a separate agent

Prepared 2026-11-25 in `E:\gh\vitals`. Every claim naming a file, symbol, route
or field is **VERIFIED** by reading it, or **EXPECTED** (reasoned, not run).
British spelling. No emoji.

> **Status, 2026-11-25 later the same day.** A first cut of the page now exists:
> the `Projects` tab of the AI agents page — `apps/desktop/src/features/agents/`
> (`ProjectsPanel.tsx`, `projectsApi.ts`, `projectsModel.ts`, `useProjects.ts`,
> `DevcloudPanel.tsx`, `visibility.ts`), ADR
> `docs/adr/0050-cloud-agents-in-the-collaboration-view.md`, both locales and
> `apps/desktop/e2e/projects.spec.ts` (tracker row S26-19). Read it before
> building on it. Two things it deliberately does **not** do, both unchanged
> from this brief: the project list is a fixture **in a development build only**
> (`GET /v1/projects` and the `projects:read` scope still do not exist, so a
> packaged build says the projects could not be read), and the devcloud panel
> shows the local half only. §6.3 asked for a read-only agentq JSON verb before
> the live half could follow; that verb now exists — `agentq-cloud.ps1 -Status
-Json`, committed in agentq `36a9591` and installed — so heartbeat age, idle
> and queue depth are unblocked.

## 1. Goal, implementer, scope

**Goal.** Add a read-only **Projects** view to the Vitals desktop app showing a
signed-in user their Codwen projects (status, per-component health, open
incidents) plus a devcloud-workspace panel, reading each from its one owner and
never probing or persisting a second copy.

**Who implements it.** A **different agent, in a different chat** — `Deep Build`
or a general agent, not the author of this brief. Read `E:\gh\vitals\AGENTS.md`
first, then this file, then the source it quotes.

**What this is not.** Not the implementation, not a patch, not the platform side.
Nothing here has been committed, built or installed.

## 2. What already exists

### 2.1 The account works end to end today (**VERIFIED**)

The sign-in exists; the project data does not. Read these and you have read the
account:

| Concern                | Where (VERIFIED)                                                             | Notes                                                                         |
| ---------------------- | ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Account client         | `crates/vitals-agents/src/cloud.rs`                                          | device flow (RFC 8628), builders, parsers; no HTTP of its own                 |
| Hosts / auth           | `cloud.rs:41-42`                                                             | `https://ai.codwen.com`, `https://auth.codwen.com`                            |
| OIDC client / scope    | `cloud.rs:44,48`                                                             | `codai-cli`; `SCOPE = "openid agents:read"` (read-only)                       |
| Credential             | `cloud.rs:53` + `LEGACY_CREDENTIAL_TARGETS`                                  | Windows Credential Manager, `vitals/codwen-api-key`                           |
| Device flow            | `cloud.rs` `device_auth_request:76`, `token_request:118`, `key_request:167`  | `/device/auth` → poll `/token` → `GET /connect/key` → `codai_…` key           |
| Existing reads         | `cloud.rs` `hosts_request:230`, `sessions_request:241`, `revoke_request:206` | `/v1/hosts`, `/v1/sessions?v=2&limit=100`, `/v1/keys/self/revoke`             |
| Tauri host             | `apps/desktop/src-tauri/src/agents.rs`                                       | `client()` 3 s/10 s (`:215-232`), `http()` (`:238`), `fetch_cloud` (`:258`)   |
| Commands               | `agents.rs`                                                                  | `get_cloud_status:285`, `start/finish/cancel_cloud_sign_in`, `cloud_sign_out` |
| Key storage            | `agents.rs` `cloud_key`/`store_key`/`clear_key`                              | reads, writes and migrates the target                                         |
| 30 s cache / 2 s event | `agents.rs` `CLOUD_EVERY:136`, `REFRESH:30-31`, `LEASE:33`                   | cloud fetch ≥ 30 s; `vitals://agents` every 2 s                               |
| UI + strings           | `features/agents/CloudAccount.tsx`, `strings.ts`                             | device code, sign-in/out, `cloud.*` in both locales                           |

The webview makes **no** HTTP call to Codwen; its only network action is `openUrl`
(the `opener` plugin) to open the browser for approval (**VERIFIED**,
`CloudAccount.tsx`). Webview renders, Rust computes.

### 2.2 The collaboration page (**VERIFIED**)

`features/agents/CollabScreen.tsx` + `useCollab.ts` render agentq's coordination
directory read-only via `get_collab`, on route `collab`. ADR
`docs/adr/0049-agent-collaboration-reads-the-coordination-directory.md`: read-only,
never write; unknown is `None` rendered as an em dash; poll while visible. It is
**local only** — that ADR explicitly defers the cloud half (devcloud runners
cannot see the directory) to "a shared store both sides can reach".

### 2.3 Navigation and route chain (**VERIFIED**)

A page exists when four things agree: `routeIds` (closed union,
`apps/desktop/src/shell/navigation.ts:41-57`), `navItems` (sidebar + Ctrl+1…9
order, `:92-108`), `RouteContent` (exhaustive switch,
`apps/desktop/src/routes.tsx:447-500`; the `never` default makes a missing case a
compile error), and a `nav.*` label in `packages/i18n/src/locales/{en,ro}.json`.
Visibility truth is `isRouteVisible` (`navigation.ts:~186-200`): `health` only
when a vmui connection is stored; `agents`/`collab` unless agentq is known absent
(`agentq === false`). Sidebar, palette, shortcuts, restore and preload all read it.

### 2.4 The one untracked brief (**VERIFIED** — read, never edit)

`docs/integrations/codwen-project-health.md`: the inbound Codwen contract —
`GET /v1/projects`, the new `projects:read` scope, a suggested response shape
(written **snake_case**) and five rules. Another agent's untracked file.

### 2.5 The honest bottom line

**The account works today; the project data exists nowhere yet.** `rg` over this
repo finds `/v1/projects` / `projects:read` **only** inside the brief
(**VERIFIED**). On the Codwen side, migration `0222` created the tables but **no
TypeScript reads or writes them** (**VERIFIED**,
`E:\gh\codai\packages\db\src\schema\projects-health.ts`; `rg -l
'platformProjects|projectSignals|projectIncidents'` returns only that file). The
page is a **read-client for a server contract written down but not built**.

## 3. The data contract

### 3.1 The JSON the page expects (camelCase)

This repo generates every wire type from one Rust definition and requires
`#[serde(rename_all = "camelCase")]` on any `ts_rs::TS` type (`check-drift.ps1`
check 1). The brief writes snake_case; reconcile (§10 Q3). The page expects:

```ts
type HealthStatus = 'up' | 'degraded' | 'down' | 'unknown';

/** One platform project + its derived health. Server-owned unless noted. */
interface CloudProject {
  id: string;
  slug: string;
  name: string; // platform_projects
  kind: ProjectKind; // final enum to ratify
  visibility: 'public' | 'private';
  status: HealthStatus; // DERIVED server-side
  lastCheckedAt: string | null; // DERIVED; ISO-8601
  components: ProjectComponent[]; // latest signal each
  openIncidents: Incident[]; // open only
}
interface ProjectComponent {
  key: string;
  label: string | null;
  status: HealthStatus;
  latencyMs: number | null; // null = not measured, NEVER 0
  value: number | null;
  detail: Record<string, unknown> | null;
  checkedAt: string; // ISO-8601 (timestamptz)
}
interface Incident {
  id: string;
  severity: 'info' | 'warning' | 'critical';
  summary: string;
  componentKey: string;
  openedAt: string;
  notifiedAt: string | null; // null = undelivered
}
```

**Server-owned** (never synthesise): `status`, `lastCheckedAt`, every
`component.status`/`latencyMs`/`checkedAt`, every `incident`. "Derived
server-side" means the API computes it (worst-of-latest); it is not a column.

### 3.2 Platform side: what exists, what does not (**VERIFIED** on `E:\gh\codai`)

| Need                                                   | Route / scope / producer                                               | State                                                                                                         |
| ------------------------------------------------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Registry `platform_projects`                           | table + migration `0222`                                               | **EXISTS, empty** — no rows, no reader, no writer                                                             |
| `GET /v1/projects` (+ `/:id/status`, `/:id/incidents`) | scope `projects:read`                                                  | **ABSENT** — nothing serves it                                                                                |
| `projects:read` scope                                  | `apps/auth/src/scope-descriptions.ts:7`                                | **ABSENT** — not in `CODAI_SCOPES`                                                                            |
| Signal collector (writes `project_signals`)            | job                                                                    | **ABSENT**                                                                                                    |
| Incident opener (`project_incidents`)                  | job                                                                    | **ABSENT**                                                                                                    |
| Per-project uptime                                     | —                                                                      | **ABSENT** — global only (`status_samples`)                                                                   |
| MCP project-health toolset                             | `apps/gateway/src/mcp/toolsets.ts`                                     | **ABSENT**                                                                                                    |
| Agent read path (tenant projects)                      | `GET /v1/projects` (`routes/environments.ts:633`), MCP `list_projects` | **EXISTS** — but the _tenant_ `projects` table (`schema/environments.ts:28`), a different thing; do not merge |

### 3.3 The five rules from the brief (**VERIFIED**)

1. **Read-only** — never write project state; never probe a project's URL (the
   SSOT collector is the one prober).
2. **No second copy** — never persist the cloud list as local truth; a short-TTL
   display cache only.
3. **Two distinct things, never merged** — cloud projects vs this machine's agentq
   state; link, never merge.
4. **Signed-out is honest** — no account → the local view + "sign in to see your
   Codwen projects"; never an empty list that reads as "no projects".
5. **No secrets, no telemetry** — opt-in, read-only (ADR-0039 posture).

## 4. The page

### 4.1 A tab, not a new page (recommended)

Add a **`projects`** tab to the existing AI agents page, beside `cloud`. Reasons:

1. The account card and its signed-out state already render on the `cloud` tab
   (`AgentsScreen.tsx`: `<CloudPanel snapshot={s} account={<CloudAccount />} />`);
   rule 4 is already written and translated.
2. One poll loop: fold the projects read into the existing `CloudCache` 30 s tick
   and ride the 2 s event (§4.4) instead of a second subscription + lease.
3. Zero route-parity cost: the e2e suite asserts `e2e/support.ts` `routes` equals
   `routeIds` (`navigation.spec.ts:12-27`); a tab adds no `RouteId`.

**Required change if you take the tab:** widen `isRouteVisible('agents')`. Today
it is `state.agentq !== false`, so a Windows user with no agentq is hidden and —
signed in — could never reach projects. Add a `codwen` fact to `useNavVisibility`,
fed from the same `get_cloud_status` the account card calls (mirror
`features/health/visibility.ts`), and show the page when signed in.

**If a separate page is preferred (§10 Q1):** append `'projects'` at the **end of
the `more` group** (so Ctrl+1…9 do not shift), add `nav.projects` to both
locales, a lazy `loadProjects` in `routes.tsx`, a `RouteContent` case, and an
`e2e/support.ts` row whose `landmark` is the screen `<h2>`. Gate it like `health`.

### 4.2 Files

Taking the tab, **no new feature folder** — the code joins the account and the
poll loop it shares (`apps/desktop/src/features/agents/`):

| File                                               | New?  | Role                                                                           |
| -------------------------------------------------- | ----- | ------------------------------------------------------------------------------ |
| `ProjectsPanel.tsx`                                | new   | the tab body: list, states, refresh                                            |
| `DevcloudPanel.tsx`                                | new   | the devcloud workspace panel (§6)                                              |
| `projectsModel.ts` (+ `.test.ts`)                  | new   | pure shaping (worst status, sort)                                              |
| `strings.ts`                                       | edit  | `project.*`, `devcloud.*` — **both locales**                                   |
| `model.ts`                                         | edit  | add `'projects'` to `AgentsTab` / `agentsTabs`                                 |
| `AgentsScreen.tsx`                                 | edit  | add the `TabsTrigger` / `TabsContent`                                          |
| `src/shell/navigation.ts`                          | edit  | widen agents visibility (§4.1)                                                 |
| `src-tauri/src/agents.rs`                          | edit  | `get_cloud_projects` (+ `get_devcloud_status` if built)                        |
| `src-tauri/src/lib.rs`                             | edit  | register in `generate_handler!`                                                |
| `crates/vitals-agents/src/cloud.rs`                | edit  | `projects_request` + `parse_projects`, mirroring `hosts_request`/`parse_hosts` |
| `crates/vitals-core/src/agents.rs`                 | edit  | wire types (ts-rs) if they cross the LAN/phone boundary                        |
| `packages/protocol/src/generated/**`               | regen | `pnpm protocol:generate`                                                       |
| `ProjectsPanel.test.tsx`, `DevcloudPanel.test.tsx` | new   | tests (§9)                                                                     |

Use the **backend seam** of `features/health/api.ts:100-125` (interface +
`tauri…Api` + `noHost…Api` + `default…Api`), `invoke` as a **dynamic import**.

### 4.3 States and rendering

| State      | Trigger                     | Render                                                                                                   |
| ---------- | --------------------------- | -------------------------------------------------------------------------------------------------------- |
| Signed out | `signedIn === false`        | the existing `cloud.signIn` empty state + a sign-in button (`CloudAccount.tsx`); **never an empty list** |
| Loading    | first read pending          | skeletons, `aria-busy="true"` + `aria-label`                                                             |
| Empty      | signed in, no projects      | `EmptyState` with distinct "no projects yet" copy                                                        |
| Error      | fetch failed, no body       | `role="alert"` line; **typed** error kind so signed-out and gateway-down differ                          |
| Stale list | failed fetch, previous body | last list + an "out of date" badge (the `useHealth` rule)                                                |
| No host    | browser preview             | the shared `NO_HOST` empty state                                                                         |

### 4.4 Refresh strategy to reuse

No new timer. Fold projects into `CloudCache` (**30 s**, `CLOUD_EVERY`), sharing
one clock and one `reqwest::Client`, and ride the 2 s `vitals://agents` snapshot
(`useAgents` re-subscribes every `RENEW_MS` = 15 s). When no key is stored,
`CloudCache` sets `fetched = None` and the panel shows the sign-in state.

### 4.5 Accessibility the repo enforces

- **Landmarks / headings.** Screen root `<div className="screen">` with a
  `<header className="screen-header">` carrying the page **`<h2>`**
  (`AgentsScreen.tsx:118-152`). On the tab, keep the one page `h2`; the panel is a
  `<section aria-label={t('tab.projects')}>` with `<h3>` group headings. No second
  `h2`.
- **Live region.** Neutral refresh/"updated" announcements use `role="status"`
  (as `HealthScreen` and `CloudAccount`); errors use `role="alert"`.
- **Focus.** Reuse the existing `Tabs`/`TabsTrigger` roving focus; keep
  `focus-visible:ring` on the sign-in button.
- **Em dash.** An unmeasured number is `None`, rendered `—`, never `0`
  (`AGENTS.md`; `CloudPanel` already does this).
- **Locale parity.** Both `en` and `ro`, side by side; enforced by
  `strings.test.ts` and `check-drift.ps1` check 4 (CLDR plurals exempt).
- **Contrast.** Tokens only (`var(--color-accent-text)` on accent surfaces, not
  `var(--color-accent)` = 4.12:1). The axe gate fails serious/critical WCAG 2.2 AA.

### 4.6 Files that are the pattern to copy (**VERIFIED**)

- `features/health/useHealth.ts:25-40` — a discriminated union (`loading`/`ok`/
  `error`) keeping the last good body across `unreachable`, with a typed error kind.
- `features/health/api.ts:100-125` — interface + tauri/no-host/default API, dynamic `invoke`.
- `features/agents/CloudAccount.tsx` — signed-out empty state, `role="status"`
  code, `role="alert"` errors.
- `features/agents/ActivityPanels.tsx` `CloudPanel` — the em-dash time helper,
  `cloudDown` mapping, KPI row; it already folds account + local state.

## 5. The animation

### 5.1 Repo motion rules (**VERIFIED**)

- Components: `import * as m from 'motion/react-m'`, render `m.div`;
  `AnimatePresence` from `motion/react` (`SessionsPanel.tsx:10-11`).
- **`motion.*` is forbidden** — the root `LazyMotion(domAnimation, strict)`
  **throws** when one renders, pulling the full bundle into that chunk
  (`theme/MotionRuntime.tsx:8-14,29`).
- **No new `LazyMotion`** — providers live only in `theme/MotionRuntime.tsx`.
- **Reduced motion** via `useReducedMotion()` (shared `data-reduce-motion` override).
- **Prefer CSS/WAAPI for a one-property fade** — `routes.tsx` documents the
  measured reason a Motion route fade was rejected (7.7 KB `m` + 15 KB vs 4.3 KB
  headroom).
- ADR `0017`: `domAnimation` only — no layout animation; expand with height+opacity.

### 5.2 Concrete plan

- **Entering:** project/incident rows use the `SessionCard` vocabulary —
  `initial={{ opacity: 0, y: 8 }}`, `animate={{ opacity: 1, y: 0 }}`,
  `transition={{ duration: 0.25 }}`, inside `<AnimatePresence initial={false}>`.
- **On a status change:** only the status **chip** cross-fades, and only when the
  value actually changes (the "live" dot pulse is precedent); re-key it on the
  status string.
- **Must not animate:** the numbers, the em dashes, and the list **order** — sort
  stable against the 2 s tick with `useStableOrder` + `useHoverHold`
  (`@vitals/ui`), as `SessionsPanel`/`JobsPanel`/`InsightsPanel` do.
- **Reduced motion:** `useReducedMotion()` collapses entry/exit; text still updates.

## 6. The devcloud panel

Wanted: workspace, heartbeat age, idle/busy, queue depth, estimated cost.

### 6.1 Where the data lives today (**VERIFIED**)

| Fact              | Source                                                                                                        | Format                                                                                                 |
| ----------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Workspaces        | `agentq-config.json` `cloudRegistry` + built-ins `codai`, `vitals-win` (`ps/bin/agentq-cloud.ps1:63-64`)      | `{ "<ws>": { project, zone } }`                                                                        |
| Heartbeat         | GCS `gs://<ctlProject>-devcloud-jobs/ws/<ws>/heartbeat`                                                       | `{at, host, state: idle\|busy\|suspending, idleSec}`, every 30 s (`E:\gh\agentq\spec\job.md:18`)       |
| Queue depth       | GCS `ws/<ws>/queue/<id>.json`                                                                                 | one spec per job; depth = object count (`spec/job.md:14`)                                              |
| Known shas        | GCS `ws/<ws>/repos/<repo>.json`                                                                               | published shas                                                                                         |
| Control plane     | `dragos-devcloud` (default)                                                                                   | bucket `<ctlProject>-devcloud-jobs`                                                                    |
| Local run history | `~/.codai/coord/journal.jsonl` `cloud-queued`/`cloud-done`/`cloud-fail`                                       | `resource: "cloud:<ws>"`, `host`, `lane`, `repo`, `holdSec`, `waitSec`, `exit`, `phases` — **no cost** |
| Unavailable       | `~/.codai/coord/cloud-down/<ws>.json`                                                                         | `{ws, reason, until}`                                                                                  |
| Cost              | the **target repo's** run journal (`RUNS.md` first table, "expected cost / actual") and the job `result.json` | a markdown cell (`agentq-cloud.ps1:102,127,209`)                                                       |

### 6.2 Reachable from Vitals today

Vitals reads **only** `%USERPROFILE%\.codai\coord` (`agentq.rs` `default_root`);
no gcloud, no GCS client, and the LAN server holds no account.

| Field                                 | Reachable? | Via                                                      |
| ------------------------------------- | ---------- | -------------------------------------------------------- |
| Workspace name, run counts, hold/wait | **YES**    | journal events `cloud:<ws>` (folded at `journal.rs:237`) |
| Unavailable + reason + until          | **YES**    | `agentq.rs:770` `read_cloud_down`                        |
| Workspace list                        | **NO**     | `cloudRegistry` / MCP `cloud_workspaces`                 |
| Heartbeat age, idle/busy, queue depth | **NO**     | GCS heartbeat + queue — needs `cloud_status`             |
| Estimated cost                        | **NO**     | the repo run journal / `result.json`                     |

### 6.3 Recommended crossing shape

Keep the account-client method and "writes go through agentq, never around it"
(ADR-0039): a workspace's live status must come from **agentq**, not from Vitals
reading GCS itself.

1. **New agentq surface (missing — name it).** A read-only JSON verb, e.g.
   `agentq-cloud.ps1 -Status -Json` (today `-Status` prints human text,
   `agentq-cloud.ps1:274-288`), or a bulk `agentq workspace-status --json`. The
   read-only MCP tools `cloud_workspaces` / `cloud_status` already exist
   (`src/mcp.ts:696-712`) at the agent layer; a CLI verb is the process boundary
   Vitals can call.
2. **New Tauri command `get_devcloud_status`** (`agents.rs`), shelling to that verb
   through `vitals_agents::actions` (its guard/ownership rules), folded into the
   same 30 s tick.
3. **Wire shape** (camelCase):

```ts
interface DevcloudWorkspace {
  name: string;
  project: string | null;
  zone: string | null;
  vmState: string | null;
  heartbeat: { at: string; state: 'idle' | 'busy' | 'suspending'; idleSec: number | null } | null;
  queueDepth: number | null; // null = not measured, NEVER 0
  estimatedCost: string | null; // needs §6.3.1
  unavailable: { reason: string | null; until: string | null } | null;
}
```

Ship the **local** half now (run counts, hold/wait, unavailable); the GCS half
needs §6.3.1 first. No heartbeat → an em dash, not "0 s".

## 7. The one source of truth

### 7.1 The per-project record (**VERIFIED** on `E:\gh\codai`)

`packages/db/src/schema/projects-health.ts` (header "Project health SSOT
(2026-11-25)", migration `0222`, commit `49861becf7`):
`platform_projects` → `project_components` → `project_signals` →
`project_incidents` (partial unique index `project_incidents_open_dedupe_unique` =
**one open incident per `dedupe_key`**) → `notification_preferences`. No producer,
no reader; the two design docs it cites do not exist (`Test-Path` false).

### 7.2 Channels read that one record

| Channel  | Where                                                                                              | State                                                         |
| -------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| Email    | `packages/email/src/transport.ts` → **Brivio** (`POST /v1/email/send`); `category: 'alert'` exists | **EXISTS**; never Resend (**VERIFIED**: none in any manifest) |
| In-app   | `notifications` (`schema/triggers.ts:60`) + FCM/web-push (`apps/gateway/src/push/*`)               | **EXISTS**; insert-then-push (`environment-tasks.ts:343-360`) |
| Push     | `packages/db/src/fcm.ts:211`, `push/web-push.ts`                                                   | **EXISTS**                                                    |
| Telegram | —                                                                                                  | **ABSENT** — only a policy tag + a schema enum                |
| Vitals   | this page                                                                                          | **to build** — a reader                                       |

All read the one record and one open incident per `dedupe_key`; none re-probes or
stores its own copy.

### 7.3 Reusable vs per-project

- **Reusable:** the collector job (template `apps/jobs/src/jobs/status-probe.ts`),
  the notification transport, the inbox + push path, the MCP toolset pattern.
- **Per-project:** declared components, `visibility` (`public` = status page,
  `private` = hub/owner), and `notification_preferences` (`user, project, channel,
min_severity, enabled`; `NULL project_id` = all — NULLs are not deduped, upsert
  explicitly).

### 7.4 What it means for this client

A **reader, never a second source**: no write, no self-probe (rule 1), no local
persistence as truth (rule 2), a short-TTL display cache only; a project that is
also a local workspace may be **linked, never merged** (rule 3).

### 7.5 Codwen-side pieces (existing / missing)

SSOT schema + `0222` **EXISTS** (empty); signal collector **MISSING**; incident
opener **MISSING**; `GET /v1/projects` (+ status/incidents) **MISSING**;
`projects:read` **MISSING**; MCP project-health group **MISSING**; email / in-app /
push **EXIST**; Telegram **MISSING**.

## 8. Do not invent

Each names the missing endpoint or field:

- **The project list** — `GET /v1/projects` does not exist; only a fixture or an
  honest "not available yet" state until it does.
- **Per-project `status` / `lastCheckedAt`** — no column; the API must compute
  worst-of-latest. In no route.
- **Per-component `latencyMs` / `checkedAt`** — server-owned; the client must not
  probe a URL itself (rule 1).
- **`openIncidents[]`** (`severity`/`summary`/`openedAt`) — the table exists,
  nothing exposes it.
- **The `projects:read` scope** — not in `CODAI_SCOPES`; this app cannot mint it,
  and sending it early would fail the device flow.
- **Codwen _alerts_** — do not conflate with the app's local alert rules or the LAN
  `GET /api/v1/alerts` (this machine, not the account).
- **"Projects" from other features** — `features/storage/devclean/*` and agentq's
  `cloud-down` records are unrelated; never under a "my Codwen projects" heading.
- **A locally persisted project list as truth** — forbidden (rule 2).
- **Devcloud heartbeat / idle-busy / queue depth / cost** — only after §6.3.1.
- **A LAN route for any of this** — the LAN server holds no account; desktop-only.

## 9. The gate checklist

`pwsh -NoProfile -File scripts/verify.ps1` runs everything CI runs; the page also
touches these:

| Gate           | Command                                                                                  | Must print                                                             |
| -------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Everything     | `pwsh -NoProfile -File scripts/verify.ps1`                                               | exit 0                                                                 |
| Contract drift | `pwsh -NoProfile -File scripts/check-drift.ps1`                                          | exit 0 (serde casing, `invoke`↔command, locale parity, sync allowlist) |
| Bindings drift | `pnpm protocol:check`                                                                    | empty `git diff` on `packages/protocol/src/generated`                  |
| Typecheck      | `pnpm --filter @vitals/desktop typecheck`                                                | exit 0                                                                 |
| Lint           | `pnpm --filter @vitals/desktop lint`                                                     | exit 0 (type-aware; no `any`, no floating promises)                    |
| Unit tests     | `pnpm --filter @vitals/desktop test`                                                     | pass; coverage ≥ 70 %                                                  |
| Rust           | `cargo clippy --workspace --all-targets -- -D warnings`                                  | clean (if Rust changed)                                                |
| Rust tests     | `cargo test --workspace`                                                                 | pass                                                                   |
| e2e            | `pnpm --filter @vitals/desktop e2e`                                                      | route parity holds; new surface renders; no console errors             |
| a11y           | (inside `pnpm e2e`) `a11y.spec.ts`                                                       | zero serious/critical WCAG 2.2 AA                                      |
| Size budget    | `pnpm --filter @vitals/desktop build:vite; pwsh -NoProfile -File scripts/check-size.ps1` | within `size-budget.json` (stay lazy, ADR-0018)                        |

Add `docs/tracker.csv` + `docs/TRACKER.md` rows (state + reasoning + a
verification-log entry with real output) — the implementing agent owns those;
this brief does not touch them.

## 10. Open questions for the owner

1. **Tab or its own page?** _Recommended: a tab_ on the AI agents page (§4.1) —
   the account card, signed-out state, poll loop and zero route-parity cost.
   Cost: the agents route's visibility must widen so a signed-in user without
   agentq reaches it.
2. **Before the endpoint exists, what does the tab show?** _Recommended: build the
   view against a fixture behind a dev-only flag_, with a visible "awaiting the
   Codwen endpoint" note — reviewable before the contract freezes. Alt: defer the
   tab.
3. **camelCase or snake_case on the wire?** _Recommended: camelCase_ (this repo
   generates types from Rust with `rename_all = "camelCase"`; `check-drift`
   enforces it). The brief is snake_case — the gateway emits camelCase or the Rust
   parser maps it; agree with Codwen before freezing the client.
4. **Devcloud panel now or after the agentq surface?** _Recommended: the local
   half now_ (run counts, hold/wait, unavailable from `coord`), the GCS half once
   `-Status -Json` exists — showing stale-by-design numbers reads as a second
   source.
5. **Anything else to add, correct, or that the brief has misunderstood?**
