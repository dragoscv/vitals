# Codwen project health — Vitals read-client brief

Prepared by the Codwen (codai) side on 2026-11-25 for the agent working in **Vitals**. This is the
inbound contract: what Vitals reads, how it authenticates, and what it must **not** do. The server side
lives in codai — see `E:\gh\codai\docs\project-health.md` and
`E:\gh\codai\docs\decisions\2026-11-25-projects-health-ssot.md`.

## What this is

Codwen keeps **one source of truth** for every project's status, health checks, alerts and signals. Vitals
is a **read client**: signed in with a Codwen account, it shows the same numbers the hub and the agents
see — without owning, caching into a second truth, or re-probing on its own.

## What Vitals already has (do not rebuild)

- `crates/vitals-agents/src/cloud.rs` — the Codwen account: **OIDC device flow** (RFC 8628, public client
  `codai-cli`), key in Windows Credential Manager, target `vitals/codwen-api-key`. Defaults
  `https://ai.codwen.com` + `https://auth.codwen.com`. It already calls `GET /v1/hosts`,
  `GET /v1/sessions?v=2`, `POST /v1/keys/self/revoke`. Scope today: `openid agents:read`.
- `crates/vitals-agents/src/agentq.rs` — **local** agentq state, read-only, from `%USERPROFILE%\.codai\coord`.
- `docs/adr/0039-ai-agents-page-and-agentq.md` — the AI/agents page this view belongs on.
- British spelling, Tauri 2 desktop + React 19 frontend, thin Rust host.

## What Vitals will read (to be built in codai)

A new, owner-scoped, read-only endpoint on the gateway:

```
GET /v1/projects            → the caller's projects + a status summary
GET /v1/projects/:id/status → components, latest signal each, open incidents
GET /v1/projects/:id/incidents
```

Authorised by the existing Codwen API key with a new scope **`projects:read`** (to be added to
`CODAI_SCOPES` in `apps/auth`). The key the device flow already produces is reused — Vitals' `SCOPE`
constant gains `projects:read`. No new login, no new credential, no platform-app install.

Suggested response shape (final schema to be ratified server-side):

```jsonc
{
  "projects": [
    {
      "id": "…",
      "slug": "codwen",
      "name": "Codwen",
      "kind": "product",
      "visibility": "private",
      "status": "up | degraded | down | unknown",
      "components": [{ "key": "gateway", "status": "up", "latency_ms": 42, "checked_at": "…" }],
      "open_incidents": [{ "id": "…", "severity": "warning", "summary": "…", "opened_at": "…" }],
    },
  ],
}
```

## The rules Vitals must honour

1. **Read-only.** Vitals never writes project state; it never probes a project's URL itself to build this
   view (the SSOT collector does that). One source of truth means one prober.
2. **No second copy.** Do not persist the cloud project list as a local truth; cache it only as a display
   cache with a short TTL and re-read.
3. **Two distinct things, never merged.** "Cloud projects" (this Codwen-account view) and "this machine's
   agentq state" (local) are different data with different owners. Show them as separate sections; a
   project that is also a local workspace may be _linked_, never _merged_.
4. **Signed-out is honest.** With no Codwen account, show the local agentq view and a clear "sign in to see
   your Codwen projects" state — never an empty list that looks like "no projects".
5. **No secrets, no telemetry.** Same posture as ADR-0039: opt-in, read-only, nothing sent that the user
   did not ask for.

## Where the view lives (suggested)

The desktop **AI / Codwen agents page** (`crates/vitals-agents` owns the cloud client and the model; the
host is `apps/desktop/src-tauri/src/agents.rs`; UI in the desktop React app). A "Projects" tab beside the
cloud agents tab. The Astro `site/` is docs only; the LAN `vitals-server` has no account and should show
only local state.

## Suggested Vitals tracker row

> **VIT-PH1** — Codwen project-health read client: reuse the device-flow key + scope `projects:read`,
> call `GET /v1/projects`, render a read-only Projects view on the AI page; local agentq state stays a
> separate section. Done when signed-in shows the Codwen projects, signed-out shows an honest empty state,
> and no project state is written or locally persisted as truth.

## Sequence / dependency

The codai endpoint (`GET /v1/projects` + the `projects:read` scope) must land **before** Vitals can call
it. Until then, Vitals can build the UI against the agreed shape with a fixture. Coordinate the scope name
and the response schema with the codai side before freezing the client.
