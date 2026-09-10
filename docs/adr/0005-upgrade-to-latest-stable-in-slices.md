# 0005 — Upgrade every dependency to latest stable, in verifiable slices

- Status: Accepted
- Date: 2026-09-10
- Tracker: D5

## Context

The tree had drifted several majors behind on vitest, React, ESLint and
others. Upgrading everything in one commit makes a regression impossible to
attribute; never upgrading accumulates CVEs and blocks features (typed lint
needed a newer typescript-eslint).

## Decision

Target the latest **stable** of everything — no alpha, beta or RC unless the
ecosystem standard is still pre-release. Upgrade in slices, each followed by
the full gate set, so a failure names one change. `pnpm minimumReleaseAge`
stays `0`, chosen over the 24-hour quarantine.

## Consequences

- vitest 5 flipped `clearMocks` to `true`; harmless here, but found by the
  gate rather than in production.
- Typed ESLint surfaced one genuine floating promise on the first run.
- `pnpm outdated -r; cargo outdated` is a task; drift is expected to be
  corrected continuously, not in a yearly migration.
