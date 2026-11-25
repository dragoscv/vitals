> Status: research snapshot, 2026-11-25. Not normative — see docs/TRACKER.md and the ADRs for decisions.

# Codwen auth + read paths for a desktop client's "my projects" view

Read-only research, 2026-11-25. Repositories: `E:\gh\codai` (branch `main`, HEAD `5080b28dfd`),
`E:\gh\agentq`. Every claim is tagged **VERIFIED** (I ran/read it, path+line) or **EXPECTED** (inferred).

Question: a client signed in with a Codwen account should see "my projects: status, health, alerts",
and the local `agentq` tool should be able to give agents project information when the owner is signed
in — in either app, synchronised, never duplicated.

---

## 1. Device-flow endpoints in codai's auth app

**App:** `apps/auth` — a panva `oidc-provider` v9 server (issuer `auth.codai.ro`, env `OIDC_ISSUER`).
Every protocol endpoint is served by `provider.callback()`; our own routes are dispatched first.

| Purpose                                           | Method + path                                                                                 | file:line                                                                                                                              | Notes                                                         |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| Device authorization (RFC 8628)                   | `POST /device/auth`                                                                           | discovery `apps/auth/src/server-surface.test.ts:106`; panva feature `apps/auth/src/provider.ts:316-348`                                | `deviceFlow.enabled: true`, charset base-20, mask `****-****` |
| User-code entry + confirm UI                      | `GET/POST /device`                                                                            | `apps/auth/src/server.ts:493-496` (rate-limit), `provider.ts:320` (`userCodeInputSource`), `provider.ts:333` (`userCodeConfirmSource`) | our HTML (`ui/views.ts`), panva supplies the xsrf form        |
| Token (device_code grant + refresh)               | `POST /token`                                                                                 | `apps/auth/src/server.ts:490` → `callback()`                                                                                           | panva handles `urn:ietf:params:oauth:grant-type:device_code`  |
| Revocation / introspection / userinfo / discovery | `/token/revocation`, `/token/introspection`, `/userinfo`, `/.well-known/openid-configuration` | `provider.ts:272-278` (`features`)                                                                                                     | panva                                                         |
| Key exchange (codai-specific)                     | `GET /connect/key`                                                                            | `apps/auth/src/server.ts:162-166`, `apps/auth/src/connect.ts:36`                                                                       | OAuth token → one-time `codai_…` API key                      |

**Refresh tokens** — VERIFIED `apps/auth/src/provider.ts:260` `issueRefreshToken` issues only when the
client allows `refresh_token` AND the scope set has `offline_access`; `provider.ts:262`
`rotateRefreshToken` rotates public clients always, confidential near expiry; TTL 30 days per token,
1-year cap (`provider.ts:385`).

**Scope registry** — VERIFIED `apps/auth/src/scope-descriptions.ts:7` `CODAI_SCOPES`:
`openid, profile, email, inference, keys:manage, usage:read, subscriptions:read, mcp, offline_access`.
Wired into the provider at `apps/auth/src/provider.ts:142` (`scopes: [...CODAI_SCOPES]`).

**Platform scopes** (the `resource:level` grammar) — VERIFIED `apps/auth/src/platform-resource.ts:17`
`PLATFORM_SCOPE_RE = /^[a-z][a-z0-9_.]*:(read|write|admin)$/`. A client may only receive the platform
scopes listed in its `oauth_clients.allowed_scopes` (carried as the `codai_platform_scopes` metadata,
never self-registered — `platform-resource.ts:63`).

**Scope enforced on a read route (examples)** — VERIFIED:

- `apps/gateway/src/mcp/oauth.ts:88-91`: `if (!scopes.includes('mcp')) throw Errors.forbidden(...)`.
- `apps/auth/src/connect.ts:53-58`: `if (!scopes.includes('keys:manage') && !scopes.includes('inference')) → 403 insufficient_scope`.

**⚠ Gap** — VERIFIED: `agents:read` appears **nowhere** in `E:\gh\codai` (`rg "agents:read"` across the
repo, no hits). Yet the vitals client requests `SCOPE = "openid agents:read"` (`crates/vitals-agents/src/cloud.rs:48`).
`agents:read` is grammar-valid as a _platform_ scope but is not in the static registry and not on any
client's `allowed_scopes`, and `/connect/key` refuses a token that lacks `keys:manage`/`inference`
(`connect.ts:54`). So on THIS checkout the vitals device-flow token cannot mint a key. Either the live
`auth.codwen.com` deployment differs from this `main`, or the flows have diverged. **EXPECTED:** this
is the first thing to reconcile before building on the device flow.

---

## 2. Read routes a desktop client could call

Route files live in `apps/gateway/src/routes/`; the tenant `projects`/`environments` data is in
`packages/db/src/schema/environments.ts` (migration 0112).

| Purpose                       | Method + path                    | file:line                               | Guard                                       | Device-flow token?                  |
| ----------------------------- | -------------------------------- | --------------------------------------- | ------------------------------------------- | ----------------------------------- |
| My projects                   | `GET /v1/projects`               | `routes/environments.ts:633`            | `requirePrincipal`                          | No — needs a `codai_…` key          |
| Project detail                | `GET /v1/projects/:id`           | `routes/environments.ts:681`            | `requirePrincipal` (+ env-scoped narrowing) | No                                  |
| My environments               | `GET /v1/environments`           | `routes/environments.ts:774`            | `requirePrincipal`                          | No                                  |
| Environment detail            | `GET /v1/environments/:id`       | `routes/environments.ts:1158`           | `requirePrincipal`                          | No                                  |
| Environment tasks             | `GET /v1/environments/:id/tasks` | `routes/environment-tasks.ts:551`       | `requirePrincipal`                          | No                                  |
| Team teams                    | `GET /v1/projects/:id/teams`     | `routes/agent-teams.ts:229`             | project member                              | No                                  |
| Notifications inbox           | `GET /v1/notifications`          | `routes/triggers.ts:407`                | `requirePrincipal`                          | No                                  |
| Notifications read            | `POST /v1/notifications/read`    | `routes/triggers.ts` (see header `:22`) | `requirePrincipal`                          | No                                  |
| Notification delete           | `DELETE /v1/notifications/:id`   | `routes/triggers.ts:23`                 | `requirePrincipal`                          | No                                  |
| Connected hosts               | `GET /v1/hosts`                  | `routes/hosts.ts:373`                   | `requirePrincipal`                          | No (vitals uses this, cloud.rs:230) |
| Gateway liveness              | `GET /health`                    | `routes/health.ts:14`                   | **none** (unauthenticated, mounted at `/`)  | n/a                                 |
| Gateway readiness (DB+schema) | `GET /health/ready`              | `routes/health.ts:31`                   | **none**                                    | n/a                                 |
| MCP transport                 | `POST /mcp`                      | `routes/mcp.ts:187`                     | API key OR OAuth JWT                        | **Yes** (scope `mcp`)               |

**Guard** — VERIFIED `apps/gateway/src/routes/devices.ts:60-64`: `requirePrincipal` →
`extractApiKey` → `resolveApiKey`, i.e. **`Authorization: Bearer codai_…` API keys only**. It does NOT
accept OAuth JWTs. The MCP route is the only one that branches on JWT, explicitly
(`routes/mcp.ts:170-178`: `if (looksLikeJwt(raw)) return resolveOAuthPrincipal(raw)`).

**How the org is chosen** — VERIFIED `routes/environments.ts:635-638`: `GET /v1/projects` computes
`orgIdsOf(db, principal.userId)` and returns projects where `ownerUserId = caller` OR
`orgId ∈ caller's orgs`. The org is derived from the **caller's own membership rows**, never from a
token claim, header or query. (`POST /projects` takes an optional `org_id` in the body only.)

**Can a device-flow token read them?** Not directly. The device-flow access token must first be
exchanged at `GET /connect/key` for a long-lived `codai_…` key (exactly vitals' flow:
`cloud.rs:167` `key_request` → `/connect/key`; the key is then stored in the Windows Credential Manager
under `vitals/codwen-api-key`, `cloud.rs:53`), and every `/v1/*` route then authenticates with that key.
VERIFIED.

**Project _health_** — there is NO read route today. The `GET /health*` routes are gateway liveness,
not project health. See §6.

---

## 3. Streaming (SSE / WebSocket / long-poll)

Streaming exists — VERIFIED:

| Kind      | Method + path                                  | file:line                           | Guard                       |
| --------- | ---------------------------------------------- | ----------------------------------- | --------------------------- |
| SSE       | `GET /v1/sessions/stream`                      | `routes/sessions-v2.ts:1271`        | `requirePrincipal`          |
| SSE       | `GET /v1/sessions/:id/stream`                  | `routes/sessions-v2.ts:1925`        | `requirePrincipal` (viewer) |
| SSE       | `GET /v1/hosts/stream`                         | `routes/hosts.ts:382`               | `requirePrincipal`          |
| SSE       | `POST /mcp` (chat tool)                        | `routes/mcp.ts:1-40` header         | API key / JWT               |
| WebSocket | `GET /v1/realtime` (voice)                     | `routes/realtime.ts` (header `:27`) | `?key=` / bearer            |
| WebSocket | `/v1/sessions/:id/ws`                          | `server.ts:600` dispatcher          | `requirePrincipal`          |
| WebSocket | `/v1/studio/jam`                               | `server.ts:604-608`                 | key                         |
| WebSocket | `wss://relay.codai.ro/v1/environments/<id>/ws` | `relay-ticket.ts:104-111`           | ticket                      |
| Long-poll | agentq waiters inside the _local_ daemon       | `E:\gh\agentq\src\daemon.ts`        | loopback bearer             |

The gateway's WS upgrades ride the same HTTP server (`server.ts:587-612`). **There is no stream for
project health or notifications** — those are poll-only today. EXPECTED (no SSE route for them exists
in `routes/`).

---

## 4. The MCP server in codai

- **Transport:** Streamable HTTP (spec 2025-11-25) at `POST /mcp` (`routes/mcp.ts:187`) and `GET /mcp`
  (`routes/mcp.ts:592`), on the gateway host `ai.codai.ro` (`MCP_RESOURCE`). Lazy-loaded chunk
  (`server.ts:154`, `routes/mcp-bundle.ts`). VERIFIED.
- **Auth:** two credential types (`routes/mcp.ts:33-37`, `authenticate` `:170`): (1)
  `Authorization: Bearer codai_…` static API key; (2) OAuth 2.1 **JWT** minted by `auth.codai.ro` for
  resource `https://ai.codai.ro/mcp`, verified locally against JWKS, **requiring scope `mcp`**
  (`mcp/oauth.ts:88-91`). Unauthenticated → 401 with the RFC 9728 `WWW-Authenticate` challenge.
- **Toolsets** (`mcp/toolsets.ts`): `core` (always), `account`, `environments` (includes
  `list_projects`, `list_environments`, `get_environment`, `start/stop_environment`, task tools),
  `resolve`, `transcripts`, `schedules`, `sessions`, `notifications`, `cards`, `vqa`, studio groups.
  Selected via `?toolsets=a,b` or `X-MCP-Toolsets`; `all` = full catalog. `tools/call` is not gated by
  the selection. VERIFIED.
  - `list_projects` replays `GET /v1/projects` as the caller (`mcp/environment-tools.ts:291`).
- **Reachable from a local process?** **YES.** `/mcp` is a public HTTPS endpoint designed for external
  clients (Claude Desktop/Code, VS Code, ChatGPT connectors — `mcp/oauth.ts:1-20`). Any local process
  with network access and a key or JWT can call it — including the local `agentq` tool. VERIFIED.

---

## 5. Identity for the local `agentq` tool

**What exists today** — VERIFIED by `rg -i "codai|codwen|keyring|credential|oauth|bearer|token" src ps`:

- **No Codwen account identity.** No OAuth, no refresh token, no keyring, no Windows Credential
  Manager use for a Codwen user. The only auth in `src/` is the **local ACP-Lock daemon**: a JSON-RPC 2.0
  HTTP server on `127.0.0.1` with a random 32-byte bearer token in `daemon.json` (mode 0600)
  (`src/daemon.ts:10-14`, `src/client.ts:30-44`). Loopback only; unrelated to Codwen.
- **One real Codwen credential, for the cloud mirror:** `ps/bin/agentq-sync.ps1` POSTs to
  `https://ai.codai.ro/v1/agentq/sync` (`:433`) with a **super_admin `codai_…` key stored
  DPAPI-encrypted (CurrentUser) in `~/.codai/agentq-sync.key`** (`:14-15`, `:43`, `:209`). Env
  `AGENTQ_SYNC_URL` (default `https://ai.codai.ro`, `:45`). The gateway side is
  `routes/agentq.ts:1` (`POST /v1/agentq/sync` requires a super_admin key, `:216`; `/queue` and `/claim`
  are per-project and any normal key that can see the project may use them, `:14-19`).
- **Absent:** any notion of a Codwen _account_, a per-user key, a stored access/refresh token, or a
  Credential Manager target.

**Options for holding a Codwen identity:**

1. **Windows Credential Manager, target `agentq/codwen-api-key`** — device-flow login → `/connect/key`
   → store the `codai_…` key. _Pros:_ matches the vitals pattern exactly; OS-protected; no secret file,
   no env leak. _Cons:_ agentq is TS + PS — reading it needs `cmdkey`/`CredRead` (PowerShell P/Invoke)
   or a Node keyring module; keep the read off the hot path.
2. **DPAPI-protected token file** `~/.codai/agentq-codwen.key` — same technique the repo already uses
   for `agentq-sync.key` (`ProtectedData.Protect`, CurrentUser). _Pros:_ zero new dependency, an existing
   in-repo pattern. _Cons:_ less standard than the Credential Manager; per-user profile only.
3. **Env var** (`CODAI_API_KEY`). _Pros:_ trivial for CI. _Cons:_ leaks into logs and child processes;
   unsuitable for an interactive sign-in.
4. **Shared identity with vitals** — read vitals' existing target `vitals/codwen-api-key`
   (`cloud.rs:53`) instead of minting a second key. _Pros:_ truly one sign-in, no duplication.
   _Cons:_ couples the two apps' keyring targets; a vitals sign-out breaks agentq.

**Chosen: option 1** — store a minted `codai_…` key in the Windows Credential Manager under
`agentq/codwen-api-key`, read once at startup and held in memory. It is the same sign-in story as the
desktop client (consistent UX, one device-flow implementation to copy from `cloud.rs`), it keeps the
secret out of files and the environment, and agentq is already PowerShell-heavy so `cmdkey`/`CredRead`
is reachable with no new runtime dependency. If the "never duplicated" requirement is read strictly as
_one credential per user_, promote it to **option 4** (a single shared target both apps read) rather
than minting two keys.

---

## 6. The "one source of truth per project" tables

**Confirmed** (previous pass was right) — VERIFIED `packages/db/src/schema/projects-health.ts`:

- `platform_projects` (`:57`) — the registry (identity, kind, visibility, environments, owner).
- `project_components` (`:93`) — checks a project declares in `codwen.project.json`.
- `project_signals` (`:128`) — one measurement of one component at one time.
- `project_incidents` (`:169`) — an episode opened by a rule; partial unique index
  `project_incidents_open_dedupe_unique` enforces one OPEN row per `dedupe_key` (`:196`).
- plus `notification_preferences` (`:203`).

**Migration:** VERIFIED `packages/db/drizzle/0222_projects_health.sql`, commit
`49861becf7 feat(db): projects-health registry, signals, incidents (0222)`.

**Correction:** the file header and the migration comment reference `docs/project-health.md` and
`docs/decisions/2026-11-25-projects-health-ssot.md` — **neither file exists** (`Test-Path` → False).
They are cited but unwritten.

**Reads/writes today:** NONE — VERIFIED `rg -l "platformProjects|projectSignals|projectIncidents|projectComponents|notificationPreferences"`
returns only `packages/db/src/schema/projects-health.ts` itself. No route, no collector, no consumer.
The registry is a schema with no producer and no reader. The _tenant_ `projects` table
(`environments.ts`, migration 0112) is a separate, actively-used table.

---

## What is missing for the requirement (shortest path first)

1. **A read route for project health.** The SSOT tables have no API. Add e.g.
   `GET /v1/projects/:slug/health` (platform_projects + latest project_signals + open project_incidents),
   guarded by `requirePrincipal`, and mount it in `apps/gateway/src/server.ts`. Introduce the module under
   the namespace `@codai/app-platform` cap, not tenant code, so the permission grammar matches.
   → **file:** new `apps/gateway/src/routes/project-health.ts`, `apps/gateway/src/server.ts`, a query
   helper in `packages/db`.
2. **A producer for the SSOT.** Nothing writes `project_signals`/`project_incidents`. Add a collector job
   that reads `project_components` and writes signals, then opens/resolves incidents.
   → **file:** a new job in `apps/jobs`; possibly `apps/gateway` for the HTTP checks.
3. **Define the scope.** `agents:read` is undefined in codai. Add it to `CODAI_SCOPES`
   (`apps/auth/src/scope-descriptions.ts:7`) and the relevant client's `allowed_scopes`, AND widen
   `/connect/key` (`apps/auth/src/connect.ts:54`) to accept it — or reuse the existing `mcp` scope and
   read health over `/mcp` instead of adding one.
   → **files:** `scope-descriptions.ts`, `connect.ts`, the `oauth_clients` rows.
4. **An agentq identity + read tool.** Add `agentq login` (device flow → `/connect/key` → Credential
   Manager) and a `project_health` tool on agentq's own MCP server.
   → **files:** new `ps/bin/agentq-login.ps1`; `src/mcp.ts` (agentq's MCP server); a small TS client.
5. **Reconcile the client scope first.** Confirm which deployment `auth.codwen.com` runs and whether it
   already defines `agents:read`; the whole design hangs on this (§1).
   → **file:** `crates/vitals-agents/src/cloud.rs` (reference) vs `apps/auth`.

---

## Fix plan / Next step

Shortest usable slice: (a) confirm `agents:read` on the live auth host; (b) add a
`GET /v1/projects/:slug/health` route over the existing SSOT tables with a minimal collector job;
(c) give agentq a device-flow login storing the key in the Credential Manager; (d) surface it through
agentq's MCP server. Do (a) before writing any code — it decides whether a new scope is needed at all.
