# 0019 — Add `origin` → `github.com/dragoscv/vitals`; agents never push

- Status: Accepted
- Date: 2026-09-10
- Tracker: D19

## Context

The clone had no remote. Documentation, issue templates and the updater
manifest URL all need one canonical repository name; `ISSUE_TEMPLATE/
config.yml` pointed at a different organisation (F17).

## Decision

The canonical repository is `dragoscv/vitals`. `origin` is configured to it.
Every URL in the tree uses it. Agents commit locally and never push; the
owner reviews and pushes.

## Consequences

- `gh attestation verify … --repo dragoscv/vitals` in the README is the
  right invocation.
- The updater endpoint
  `https://github.com/dragoscv/vitals/releases/latest/download/latest.json`
  resolves once a tagged release exists.
