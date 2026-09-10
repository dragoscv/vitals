# 0022 — Accept all eleven extras

- Status: Accepted
- Date: 2026-09-10
- Tracker: D22

## Context

Beyond the nine features (ADR 0008), the audit proposed eleven smaller
items: mDNS advertisement, supply-chain audits, typed ESLint, `DiskCounters`,
ADRs, coverage, keyboard shortcuts, table search, the slow-PC report,
Dependabot and an ARM64 release leg.

## Decision

All eleven accepted and scheduled into the existing slices.

## Consequences

- mDNS (`_vitals._tcp`) starts and stops with the server (S4).
- `cargo audit` via `taiki-e/install-action` — never `rustsec/audit-check`,
  which compiles cargo-audit from source on every run — and
  `pnpm audit --audit-level high` run in CI; Dependabot covers cargo, npm
  and github-actions (S10).
- Typed ESLint (`recommendedTypeChecked`) is on (S1).
- `DiskCounters` is the disk source (ADR 0012).
- This directory exists (S10).
- The ARM64 leg is in the release matrix but is marked in the tracker as
  unproven until a run on `windows-11-arm` has been observed to pass.
