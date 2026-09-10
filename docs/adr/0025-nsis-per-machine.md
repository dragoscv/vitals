# 0025 — NSIS installer, `perMachine`

- Status: Accepted
- Date: 2026-09-10
- Tracker: S10-02

## Context

Tauri can bundle MSI (WiX) or NSIS. The product goal is an install with no
wizard and no questions that takes a few seconds; the updater must be able
to replace a running binary; and one install should serve every user of the
machine.

## Decision

`bundle.targets = ["nsis"]` with `installMode: "perMachine"`. Custom NSIS
hooks terminate a running instance before install and uninstall, because
NSIS otherwise overwrites an open binary and the update silently no-ops.
Uninstalling keeps user data.

## Consequences

- One UAC prompt at install (per-machine writes `Program Files`), none at
  run time.
- The installer is 2.8 MB and installs in ~5 s with no windows; both are
  gated by `check-size.ps1`.
- No MSI means no Group Policy deployment via `msiexec`; `/S` gives silent
  install for scripts instead.
- SmartScreen warns on first run because the build is not signed with a
  commercial certificate; provenance attestation is the substitute
  (`docs/distribution.md`).
