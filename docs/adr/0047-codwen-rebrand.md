# 0047 — The AI service is Codwen now, and once more only its public name moved

- **Status:** accepted
- **Date:** 2026-11-25
- **Tracker:** S26-09, S26-10
- **Supersedes:** nothing; extends [0042](0042-kodwen-rebrand.md)

## Context

The author's AI gateway has been renamed a third time. It was _codai_, then
_Kodwen_ in November 2026 ([ADR 0042](0042-kodwen-rebrand.md)), and it is
**Codwen** as of 2026-11-25: the public domain is `codwen.com`, so the AI agents
page (ADR 0039) now signs in at `auth.codwen.com` and reads sessions from
`ai.codwen.com`.

Vitals shows that name in more places than the AI page: the client label
(“Kodwen” beside a session row and on the usage panel, ADR 0043), the sign-in
card, the phone, watch and TV builds, PRIVACY in both locales, the OpenAPI
document and API README, the site, both store listings and the legal handover.
Every one of them was a rename behind, so a user read a product name the service
no longer answers to while signing in to it.

The same three classes of string as ADR 0042 apply, and this rename is only
safe because they are still treated separately:

1. **The public name and the hosts** — the product name, `ai.codwen.com` /
   `auth.codwen.com`, the credential's display name, the sign-in copy. These
   follow the rename.
2. **The wire identifiers the service itself issues** — the device-flow OIDC
   client id `codai-cli`, the `codai_` / `kodwen_` key prefixes, and the
   `x-codai-client` / `X-Codai-Device-Name` headers. These are chosen by the
   service, not here: renaming one breaks sign-in for every build that does not
   match, and none of them is visible to a user.
3. **Machine-local paths and repository-derived names** — `%USERPROFILE%\.codai`,
   `E:\gh\vitals`, the agentq resources `build:vitals`, `commit:vitals`. Still
   untouchable; a rename there breaks coordination rather than branding.

## Decision

Rename the **public** name and hosts to Codwen again, keep the **wire
identifiers** the service issues, and make every decoder of a client name accept
all three spellings.

Concretely:

- `DEFAULT_GATEWAY` → `https://ai.codwen.com`, `DEFAULT_AUTH` →
  `https://auth.codwen.com`. The old hosts keep working, so an un-updated build
  is not bricked, and the desktop's `opener` capability lists every device
  authorisation URL ever issued (`auth.codwen.com`, `auth.kodwen.com`,
  `auth.codai.ro`) for one release.
- `AgentClient::Kodwen` becomes `AgentClient::Codwen`, wire value `codwen`, and
  `AgentUsageKind` with it. Both carry `#[serde(alias = "kodwen", alias = "codai")]`,
  verified by a test that decodes both older spellings into `Codwen` and asserts
  that serialisation is `codwen`. Serde accepts `alias` on an enum variant;
  without it, every frame a store or a phone already holds would fail to parse
  rather than display the name this release uses.
- The local-session matcher and the model display accept a client name
  containing **any** of `codwen`, `kodwen` or `codai`, and `normalise_model`
  shows all three as `Codwen`, so a machine still running an older CLI does not
  name a product that no longer exists.
- The Credential Manager target is `vitals/codwen-api-key`, and a key saved
  under **either** earlier target is read once, migrated and left in place; the
  uninstaller removes all three so no working key is left behind.
- `CLIENT_ID` (`codai-cli`), the key-prefix filter (now accepting `codai_`,
  `kodwen_` and `codwen_`) and the request headers keep their service-side
  spellings.
- The client enums of the phone (Kotlin `TolerantEnumSerializer`) and the iPhone
  (Swift `TolerantEnum`) are **not** given the older spellings. An un-updated
  phone shows `unknown` for a session named by a newer build. That is accepted
  deliberately: a client is a decoder, and the tolerance is already there — the
  alternative is shipping a list of every name the service has ever had.

## Consequences

- The AI page, PRIVACY (both locales), the OpenAPI document, the API README,
  the site, both MSIX listings, the Play changelogs and the App Store copy name
  the service Codwen; the desktop, phone, watch and TV builds show “Codwen” as
  the client label.
- A copy that signed in before the update keeps working without signing in
  again: the key is migrated on first read, from either older target.
- A frame, a stored session or a phone from an earlier release still reads as
  Codwen rather than as an unknown tool, because the aliases are on the Rust
  enums.
- The generated TypeScript binding changes with it (`"codwen"`), so an older
  front end with a newer backend reads an unknown member rather than the wrong
  one — the same tolerance, one layer up.
- The changelog's older entries, the accepted ADRs and the historical tracker
  rows keep their old spelling on purpose: they record what was true when they
  were written, and this repository's ADR README forbids editing an accepted
  record.
- The old hosts remain allowed in the `opener` capability for one release. It is
  a list to shorten deliberately once `kodwen.com` and `codai.ro` are retired,
  not something to leave growing: this record is the second time that list has
  been extended.
