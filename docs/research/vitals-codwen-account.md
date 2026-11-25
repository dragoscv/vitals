> Status: research snapshot, 2026-11-25. Not normative — see docs/TRACKER.md and the ADRs for decisions.

# Vitals ↔ Codwen account: what exists today, and the smallest honest path to a "my projects" page

Read-only research, 2026-11-25. Scope: `E:\gh\vitals` only (a Codwen-side check from
this workspace is impossible — `E:\gh\codai` is outside the workspace, so its claims
are marked EXPECTED / NOT VERIFIED).

## The question

What exists today that lets this app talk to Codwen (hub.codai.ro / ai.codwen.com /
the Codwen account), and what is the smallest honest path for a signed-in "my
projects: status, health, alerts" page?

## Verdict (one paragraph)

**Far more exists than the brief implies: the whole Codwen account already works.**
Vitals is a signed-in Codwen client today — OIDC device-flow sign-in, a key in Windows
Credential Manager, three gateway calls (`/v1/hosts`, `/v1/sessions`, `/v1/keys/self/revoke`),
an account UI on the AI/Agents page, and a 30 s refresh of cloud sessions merged into
that page. What does **not** exist is the _project-health_ data itself: there is no
`GET /v1/projects`, no `projects:read` scope, and no project/health/incident field
crossing the boundary anywhere in this repo. The "my projects" page is therefore a
**read-client for a server contract that has been written down but not built**. The
smallest honest path is: build the view against the brief's agreed JSON with a fixture
(TypeScript only), then add one scope string + one request builder (Rust, in the
existing cloud module) + one Tauri command + one hook. Nothing about auth, storage,
transport or refresh has to be invented — all of it already exists.

## What exists — hits table

| Path                                                                           | What it is                                                                                                                                                                                                                                                                                                         | Real or incidental             |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------ |
| `crates/vitals-agents/src/cloud.rs`                                            | The Codwen account client: OIDC device flow (RFC 8628), request builders, parsers. `DEFAULT_GATEWAY`/`DEFAULT_AUTH` (`cloud.rs:41-42`), `SCOPE = "openid agents:read"` (`:48`), `CREDENTIAL_TARGET = "vitals/codwen-api-key"` (`:53`), `hosts_request`/`sessions_request`/`revoke_request` (`:230`/`:241`/`:206`). | **Real** — this is the account |
| `apps/desktop/src-tauri/src/agents.rs`                                         | The Tauri host side: reqwest `client()` (3 s connect / 10 s overall, `:215-232`), `fetch_cloud` (`:258`), `CloudCache` (30 s, `:136`), and the commands `get_cloud_status`/`start_cloud_sign_in`/`finish_cloud_sign_in`/`cancel_cloud_sign_in`/`cloud_sign_out` (`:285`+).                                         | **Real**                       |
| `apps/desktop/src/features/agents/CloudAccount.tsx` (+`CloudAccount.test.tsx`) | The sign-in UI: device code on screen, sign-in/cancel/sign-out, connected hosts.                                                                                                                                                                                                                                   | **Real**                       |
| `apps/desktop/src/features/agents/strings.ts` (`cloud.*`)                      | All the cloud strings, both locales.                                                                                                                                                                                                                                                                               | **Real**                       |
| `apps/desktop/src-tauri/capabilities/default.json:27-29`                       | `opener:allow-open-url` for `https://auth.codwen.com/device*` (+ legacy `auth.kodwen.com`, `auth.codai.ro`) — the only browser-open the sign-in needs.                                                                                                                                                             | **Real**                       |
| `apps/desktop/src-tauri/windows/hooks.nsh:235-251`                             | Uninstaller deletes the key targets `vitals/codwen-api-key` (+ legacy).                                                                                                                                                                                                                                            | **Real**                       |
| `docs/integrations/codwen-project-health.md`                                   | The inbound Codwen brief for exactly this page: `GET /v1/projects` + `projects:read` + response shape + the five rules. A **spec**, not code.                                                                                                                                                                      | **Real (spec)**                |
| `docs/adr/0039-ai-agents-page-and-agentq.md`                                   | The ADR for the AI/agents page the cloud view lives on.                                                                                                                                                                                                                                                            | **Real**                       |
| `crates/vitals-agents/src/agentq.rs:770` (`read_cloud_down`)                   | `cloud-down/*.json` records from `%USERPROFILE%\.codai\coord` — an **agentq** "cloud destination is down" flag, **local**. Name collides with Codwen cloud; unrelated.                                                                                                                                             | **Incidental (name)**          |
| `apps/desktop/src-tauri/src/health.rs` + `apps/desktop/src/features/health/*`  | A **different** account: the owner's personal health from **vmui**, paired with a vmui token (`vitals/vmui-health-token`). Not Codwen.                                                                                                                                                                             | **Real, but vmui not Codwen**  |
| `apps/desktop/src-tauri/src/mqtt.rs`                                           | MQTT broker password in the OS secret store (`vitals/mqtt-password`). User-provided config, not Codwen.                                                                                                                                                                                                            | **Incidental**                 |
| `apps/desktop/src/features/storage/devclean/*` (`projects`)                    | Local folder scan for dev-junk cleanup.                                                                                                                                                                                                                                                                            | **Incidental (name)**          |
| `crates/vitals-server/src/prometheus.rs:451` (`render_agents`)                 | LAN/Metrics agent series.                                                                                                                                                                                                                                                                                          | **Real (LAN, local)**          |

Nothing else: no `codwen`/`codai` string appears in the webview's own config or in
`packages/` source beyond the above (**VERIFIED** by `rg`).

## 2. The account/token, in detail

**Obtained (device flow, `cloud.rs:76-186`; hosted in `agents.rs`):**

1. `POST {auth}/device/auth` with `client_id=codai-cli`, `scope="openid agents:read"`
   → `DeviceAuth { device_code, user_code, verification_uri, … }` (`parse_device_auth`, `cloud.rs:112`).
2. UI shows `user_code` and opens `verification_uri_complete` in the browser
   (`CloudAccount.tsx` `openUrl`).
3. `start_cloud_sign_in` stores the flow in a process-global `SignIns` and returns an
   **opaque handle**; the secret `device_code` never reaches the webview
   (`cloud.rs:402-414`; test `the_device_code_never_reaches_the_webview_only_an_opaque_handle_does`).
4. `finish_cloud_sign_in` polls `POST {auth}/token` (device_code grant) until
   `Granted`, then `GET {auth}/connect/key` with header `X-Codai-Device-Name: Vitals desktop`
   → a long-lived key `codai_…`/`kodwen_…`/`codwen_…` (`parse_key`, `cloud.rs:186`).

**Stored:** Windows Credential Manager, generic credential, target `vitals/codwen-api-key`
(`store_key`, `agents.rs:437`). macOS uses the Keychain; other OSes refuse
("cloud sign-in needs Windows Credential Manager"). **Never** in `settings.json`, never in
the webview — it only ever sees the key's presence via `signedIn`.

**Used on a request:** `b.bearer_auth(t)` in the shared `http()` (`agents.rs:245-246`),
plus header `x-codai-client: vitals/<version>` (`cloud.rs:250`). Read-only scope: the
gateway refuses the key for anything but reads and its own revocation (`cloud.rs:19-25`;
test `sign_in_asks_for_read_only_access_and_nothing_that_runs_spends_or_manages_keys`).

**When absent:** `cloud_key()` returns `None` → `CloudCache` sets `fetched = None`
(`agents.rs:160-168`) and `get_cloud_status` returns
`CloudStatus { signed_in: false, hosts: [], error: None }` (`agents.rs:293-300`). The UI
renders the sign-in empty state, not an empty project/host list. A 401/403 from the
gateway decodes as `Err("signed-out")` (`cloud.rs` `check`, `parse_hosts`).

## 3. How the app reads remote data

**Everything remote goes through Rust. The webview never makes an HTTP request** to
Codwen. The only webview network action is `openUrl` (the `opener` plugin) to open the
browser for device approval (`CloudAccount.tsx`). The webview talks to Rust over
`invoke('command')` and a Rust-emitted event; Rust talks HTTP via `reqwest`
(`apps/desktop/src-tauri/src/agents.rs` is the file that does Codwen HTTP;
`apps/desktop/src-tauri/src/health.rs` is the file that does vmui HTTP).

**Complete worked example — the vmui Health feature** (cleaner error taxonomy than the
cloud one; the cloud version is identical minus the typed error kinds):

- **Command** `health_fetch` (`health.rs:643`) → `GET {base}/api/v1/me/health`
  (`health.rs:654`) with a bearer token.
- **Timeout** one shared client, 3 s connect / 10 s overall (`health.rs` `client()`,
  same shape as `agents.rs:224-226`).
- **Error kinds** `FetchErrorKind { SignedOut, NotOwner, Unreachable, NotConfigured }`
  (`health.rs:450-457`); `kind_for_status` maps 401→`SignedOut`, 403→`NotOwner`,
  else→`Unreachable` (`health.rs:471-479`). Serialised as `{"kind":"signedOut"}`
  (`health.rs:788-789`).
- **UI** `features/health/api.ts` narrows the rejection with `errorKind()` (`:120-131`)
  and `HealthScreen.tsx` picks words/icons per kind (`:284-287`, `:308-311`) — the
  strings live in `features/health/strings.ts`.

The **cloud** equivalent is `get_cloud_status` (`agents.rs:285`) whose error is a plain
string (`"signed-out"` or `"Codwen answered N"`), and `CloudAccount.tsx` renders that
string directly. A projects command should follow the _health_ pattern (a typed kind),
not the plain-string one, so "signed out" and "gateway down" read differently.

## 4. Background / polling refresh a page could reuse

- **Event push, Rust-driven:** the agents reader thread emits `vitals://agents`
  every `REFRESH = 2 s` while subscribed (`agents.rs:30-31`, `run() :110`).
- **Cloud data:** `CLOUD_EVERY = 30 s` inside the same thread (`agents.rs:136`), only
  while the page is open and signed in.
- **Frontend** `features/agents/useAgents.ts`: `listen(AGENTS_EVENT)` for pushes,
  re-`subscribe` every `RENEW_MS = 15 000` (`:29`, `:148`), a 1 s clock tick (`:149`),
  and drops the subscription when the document is hidden (`useVisible`). `useCollab.ts`
  polls similarly (`REFRESH_MS`, `:98`).
- The lease rule: a subscription not renewed within 30 s is dropped (`LEASE`, `agents.rs:33`).

A new Projects tab would ride the **same** mechanism: fold a project fetch into
`CloudCache` (add a second cached body on the same 30 s tick) rather than a new timer.

## 5. The LAN server

A paired phone/device can reach (token required for all but `/health` and `/pair`):
`/api/v1/{snapshot,summary,history,sensors,connectivity,stream,ws,host,alerts,agents}`,
`POST /api/v1/agents/actions`, `POST /mcp`, `GET /metrics` (`crates/vitals-server/src/router.rs:148-165`).
**It exposes nothing beyond local metrics and this machine's local agentq/agent state.**
`GET /api/v1/agents` is the _local_ AI page, redacted, and is **not** part of the
tokenless loopback read (403 without a token) — precisely because it holds one user's
session titles (`docs/api/README.md:53-58`).

**It cannot serve account-scoped data**: the LAN server has no Codwen account, no key,
and no HTTP client of its own — it is handed an `AgentsProvider` by the host
(`crates/vitals-server/src/agents.rs:27-31`), and the desktop's `LanAgents`
(`agents.rs:631-676`) only returns the local snapshot and runs local agentq actions.
The brief agrees: "the LAN `vitals-server` has no account and should show only local
state" (`docs/integrations/codwen-project-health.md`). So the Codwen Projects view is a
**desktop-only** screen, not a LAN route.

## 6. Sign-in ergonomics and the settings store

- **Browser-opening flow:** yes — OIDC **device code** (RFC 8628), not a loopback
  redirect and not a client secret. The webview opens the browser via the `opener`
  plugin to `auth.codwen.com/device*`; all polling and key exchange stay in Rust. This is
  the flow to reuse.
- **A settings screen for a server URL or key?** For **Codwen, none** — the two hosts are
  compile-time constants (`cloud.rs:41-42`) and the key is device-flow-minted, never typed.
  (There _is_ a vmui address + key screen in Settings for the Health page, and an MQTT
  address/password screen — different integrations.)
- **User-provided config store:** `@tauri-apps/plugin-store`, single file
  **`settings.json`** in the app config dir, with two keys: **`app`** (the whole
  `AppSettings` blob — `apps/desktop/src/settings/persistence.ts:14`) and **`layout`**
  (dashboard layout — `apps/desktop/src/features/dashboard/useLayout.ts:32`), both via
  `apps/desktop/src/lib/storeBackend.ts`. Secrets are deliberately excluded from that
  JSON and live in the OS secret store (`settings/schema.ts` `MqttSettings` comment;
  `mqtt.rs` header).

## Smallest honest path (3-6 changes, priority order)

**Prerequisites state (important):** the server endpoint does not exist yet —
`rg` over `E:\gh\vitals` finds `/v1/projects` / `projects:read` **only** inside the brief
document, never in code (**VERIFIED**). Whether codai has since built it could not be
verified from this workspace (**EXPECTED: not built**, per the brief's "must land before
Vitals can call it").

1. **Build the view against a fixture — TypeScript only.**
   New `apps/desktop/src/features/agents/ProjectsPanel.tsx` + a fixture module, a
   "Projects" tab in `apps/desktop/src/features/agents/AgentsScreen.tsx`, and `cloud.projects*`
   strings in `apps/desktop/src/features/agents/strings.ts` (both `en` and `ro`).
   _Boundary:_ renders the brief's shape directly; no Rust. _Why first:_ it is honest —
   the UI can be reviewed before the server contract is frozen, exactly as the brief suggests.
2. **Add the scope + request builder and parser — Rust, one file.**
   `crates/vitals-agents/src/cloud.rs`: extend `SCOPE` (`:48`) to include
   `projects:read`, and add `projects_request(gateway, key)` + `parse_projects(res)`
   mirroring `hosts_request`/`parse_hosts` (`:230`/`:283`). _Boundary data:_ the brief's
   `{ projects: [{ id, slug, name, kind, visibility, status, components[], open_incidents[] }] }`;
   every struct needs `#[serde(rename_all = "camelCase")]` (repo drift gate).
   _Depends on the codai endpoint existing._
3. **Add one Tauri command — Rust, host side.**
   `apps/desktop/src-tauri/src/agents.rs`: `get_cloud_projects()` following the
   `get_cloud_status` pattern (`:285`), reusing the shared `client()` (3 s/10 s) and the
   `CloudCache` 30 s tick; register it in `apps/desktop/src-tauri/src/lib.rs`
   `generate_handler!` (near `:472`). Return a **typed error kind** (signed-out vs
   unreachable) like `health.rs`, not a bare string.
4. **Wire the tab to the command — TypeScript only.**
   `ProjectsPanel.tsx`: `call<CloudProjects>('get_cloud_projects')` (mirror
   `CloudAccount.tsx`), signed-out → the existing `cloud.signIn` empty state, failed
   fetch → keep the last list + a `cloud_error` line (the `CloudCache` rule).
   _Boundary:_ `{ signedIn, projects[], error }`, camelCase.
5. **Do not add a LAN route** — stated, not built (see §5). If a phone view is ever
   wanted, it needs a different design (the LAN server holds no account).
6. **Docs ripple:** tick the `VIT-PH1` tracker row in `docs/tracker.csv` /
   `docs/TRACKER.md` and add a verification-log entry when 1-4 land.

## What must NOT be invented

- **`GET /v1/projects` (and `/v1/projects/:id/status`, `/incidents`)** — no server-side
  endpoint is referenced by any client code; it is "to be built in codai"
  (`docs/integrations/codwen-project-health.md:29-31, 86-89`). Until it exists, the page
  can only show a fixture or an honest "not available yet" state.
- **The `projects:read` scope** — must be added to `CODAI_SCOPES` in codai's `apps/auth`
  (`brief:34-36`); this app cannot mint it. Sending `projects:read` before the auth
  service accepts it would fail the device flow.
- **Per-project `status`, per-component `latency_ms`/`checked_at`, `open_incidents[]`
  (`severity`/`summary`/`opened_at`)** — server-owned. The client **must not** probe a
  project's URL itself to populate them (rule 1: one prober, the SSOT collector).
- **Codwen account _alerts_** — do not conflate with what exists: the app already has
  _local_ user-written alert rules and the LAN `GET /api/v1/alerts`
  (`crates/vitals-server/src/router.rs:157`) — both are **this machine**, not the Codwen
  account. A "Codwen alerts" section needs an incident/alert field from the same
  non-existent endpoint.
- **"Projects" from any other feature** — `features/storage/devclean/*` (local folder
  scan) and agentq's `cloud-down` records are unrelated; they must not be shown under a
  "my Codwen projects" heading.
- **A locally persisted project list** as truth — forbidden by rule 2; a display cache
  with a short TTL only.

## Commands run

- `rg -n "codwen|codai|kodwen"` (whole repo) — 501 raw hits, 5 files with real content.
- `rg -n "sign[-_ ]?in|login|account|auth|bearer|api[-_ ]?key"` in `apps/desktop/src-tauri/**`.
- `rg -n "cloud|sign_in|CREDENTIAL_TARGET"` in `crates/vitals-agents/src/**`.
- `rg -n "projects:read|/v1/projects"` — hits only in `docs/integrations/codwen-project-health.md`.
- Reads: `agents.rs`, `cloud.rs`, `health.rs`, `server.rs`, `router.rs`, `CloudAccount.tsx`,
  `api.ts`, `useAgents.ts`, `persistence.ts`, `schema.ts`, `CHANGELOG.md`.

## Next step

The blocker is server-side. Decide with the codai owner when `GET /v1/projects` +
`projects:read` land; until then, execute step 1 (fixture-backed UI) so the view is ready
the day the endpoint is, and hold steps 2-4 behind it.
