# 0009 — Tracker as a CSV for state and a Markdown file for reasoning

- Status: Accepted
- Date: 2026-09-10
- Tracker: D9

## Context

Several agents work in the same clone. A single prose tracker is edited
concurrently and merges badly; a spreadsheet alone loses the reasoning that
explains why a row is the way it is.

## Decision

`docs/tracker.csv` holds one row per work item (`id, slice, category, title,
detail, decision, status, verification`) and is the source of truth for
state. `docs/TRACKER.md` holds the decisions table, the audit findings and a
verification log with real command output. Every ID in the Markdown exists
in the CSV.

## Consequences

- Line-per-item CSV edits merge cleanly across agents.
- "Done" without a `verification` cell is visibly incomplete.
- The two files can drift; keeping them in step is a stated rule in
  `AGENTS.md`'s "Before you finish".
