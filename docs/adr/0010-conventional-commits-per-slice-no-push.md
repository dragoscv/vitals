# 0010 — Conventional Commits per slice, explicit paths, no push

- Status: Accepted
- Date: 2026-09-10
- Tracker: D10

## Context

Multiple agents share one working tree. `git add -A` sweeps another agent's
half-finished edits into a commit; a push from an agent publishes work the
owner has not reviewed.

## Decision

- One commit per slice (or coherent sub-slice), in
  [Conventional Commits](https://www.conventionalcommits.org) form.
- Stage explicit paths only. Never `-A`, `.` or `commit -a`.
- Gates run in a separate shell command from the commit — a `;`-chained
  commit runs regardless of the gate's exit code and shipped two type errors
  once (`217e5bc`).
- Agents never push.

## Consequences

- `git log --oneline` reads as a changelog; the Unreleased section of
  `CHANGELOG.md` is derived from it.
- A commit may legitimately contain another agent's already-staged files;
  the report names them rather than unstaging (which races their `git add`).
- The pre-commit hook (`scripts/hooks/pre-commit.ps1`) enforces formatting,
  drift, secrets and `.only`.
