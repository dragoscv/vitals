# 0012 — `apps/helper` stays stubbed; disk I/O comes from `DiskCounters`

- Status: Accepted
- Date: 2026-09-10
- Tracker: D12

## Context

`apps/helper` is the planned SYSTEM-level service for ETW tracing and
privileged operations. It `bail!`s on start. Meanwhile `vitals-win` claimed
`Capability::HandleEnumeration` and `SetEfficiencyMode` unconditionally with
nothing behind them (F3), and per-process disk I/O was assumed to need ETW.

## Decision

- The helper remains a stub. Shipping a SYSTEM service is a security
  commitment (see SECURITY.md) the project is not ready to make.
- The false capability claims are removed; capabilities are declared only
  when implemented.
- Per-process disk I/O uses `SYSTEM_PROCESS_INFORMATION_EXTENSION.DiskCounters`
  — the same source as Task Manager's Disk column, free inside a call the
  sampler already makes — instead of ETW.

## Consequences

- No elevated surface ships today; the SECURITY.md helper section describes
  intent, and the LAN section describes what is real.
- `DiskCounters` needs the full information class (148), which is refused
  without `SeDebugPrivilege`. The sampler falls back to class 5 permanently
  for the run and reports `storage_read_bytes: None`, never zero.
