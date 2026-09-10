# 0016 — Build the command palette on Radix Dialog; remove `cmdk`

- Status: Accepted
- Date: 2026-09-10
- Tracker: D16

## Context

`cmdk` was a declared dependency. At decision time it was eighteen months
stale with an open defect in its core combobox ARIA contract. `@vitals/ui`
already carries Radix Dialog.

## Decision

The palette (`Ctrl+K`) is a Radix `Dialog` with a hand-rolled listbox:
filtering, keyboard navigation and `aria-activedescendant` are ours. `cmdk`
is removed.

## Consequences

- One fewer dependency, no new primitive in the bundle.
- The ARIA behaviour is testable and owned; a fix does not wait on an
  upstream release.
- Fuzzy matching is deliberately simple (substring over command titles);
  that is the trade for owning the code.
