# 0042 — The AI service is Kodwen, and only its name moved

- **Status:** accepted
- **Date:** 2026-11-17
- **Tracker:** S25-01, S25-02

## Context

The author's AI gateway was renamed from _codai_ to _Kodwen_ in November 2026:
the public domain moved to `kodwen.com` (`ai.`, `auth.`, `hub.`, `app.`,
`chat.`, `docs.`, `pay.`, `stats.`) and the product name changed everywhere it
is shown. Vitals has an AI agents page (ADR 0039) that signs in to that service
and reads the user's cloud sessions, so the old name is visible on that page,
in PRIVACY, in the API docs and in the desktop, phone, watch and TV builds.

Two classes of string exist, and they must not be treated the same:

1. **The public name and the hosts** — the product name, `ai.codai.ro` /
   `auth.codai.ro`, the credential’s display name, the OIDC scope copy. These
   follow the rebrand.
2. **The wire-level identifiers the service itself issues** — the device-flow
   OIDC client id `codai-cli`, the `codai_…` API-key prefix, and the
   `x-codai-client` / `X-Codai-Device-Name` headers. Verified against the
   service’s own source on 2026-11-17: these did **not** change, so a client
   that renamed them would stop being able to sign in.

A third class is machine-local and must never be touched: the agentq
coordination directory `%USERPROFILE%\.codai\coord`, the repository directory
`E:\gh\codai`, the worktree root `.wt\codai`, and the agentq resource names
derived from the repository name (`build:codai`, `commit:codai`). A rename
there would break coordination, not branding.

## Decision

Rename the **public** name and hosts to Kodwen, keep the **wire identifiers**
the service issues, and leave **machine-local paths and repository-derived
names** untouched.

Concretely:

- `DEFAULT_GATEWAY` → `https://ai.kodwen.com`, `DEFAULT_AUTH` →
  `https://auth.kodwen.com`. The old hosts keep working, so an un-updated
  build is not bricked, and the desktop’s `opener` capability allows both
  device-authorisation URLs during the transition.
- The `AgentClient::Codai` variant becomes `Kodwen` (wire value `kodwen`)
  across the core model, the generated TypeScript, the Android
  `TolerantEnum` and the Swift `TolerantEnum`. Older clients decode the
  unknown value as `other` rather than failing; both clients ship in the same
  release.
- The local-session matcher accepts a client name containing **either**
  `kodwen` or `codai`, because a machine may still be running a CLI that
  identifies as codai.
- A model the service reports as `codai` is displayed as `Kodwen`, so a
  session does not name a product that no longer exists.
- The Credential Manager target is renamed `vitals/codai-api-key` →
  `vitals/kodwen-api-key`. An existing key is read once from the old target
  and migrated; the uninstaller removes **both** targets so no working key is
  left behind.
- `CLIENT_ID` (`codai-cli`), the key-prefix filter (now accepting `codai_`
  and `kodwen_`) and the request headers keep their service-side spellings.

## Consequences

- The AI page, PRIVACY (both locales), the OpenAPI document and the API
  README name the service Kodwen; the desktop, phone, watch and TV builds show
  “Kodwen” as the client label.
- A copy that signed in before the update keeps working without signing in
  again: the key is migrated on first read.
- The changelog’s older entries and the historical ADRs and tracker log keep
  the old spelling on purpose — they record what was true when they were
  written, and [the ADR README](README.md) forbids editing an accepted record.
  This record supersedes nothing and is the place a reader is sent for the
  rename.
- A second domain is allowed by the `opener` capability for one release so a
  build that has not updated yet still opens the right page; it can be removed
  once the old hosts are retired.
