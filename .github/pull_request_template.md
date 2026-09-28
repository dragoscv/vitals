## What and why

<!-- What does this change, and what problem does it solve? Link the issue or discussion. -->

Closes #

## How it was verified

<!-- The command you ran and what it printed. "It compiles" is not verification. -->

## Checklist

- [ ] `pwsh -NoProfile -File scripts/verify.ps1` passes locally.
- [ ] Every new or changed user-facing string exists in **both** `en` and `ro`.
- [ ] A metric that cannot be measured is `None`/`null`, never `0`.
- [ ] Tests state a guarantee in their name, and I have seen them fail without the change.
- [ ] If `crates/vitals-core` changed: `pnpm protocol:generate` was run and the output committed.
- [ ] `docs/tracker.csv` and `docs/TRACKER.md` are updated (with real command output in the verification log).
- [ ] The ripple is closed: callers, CLI, SDK and docs, or I have said below what was not updated and why.
- [ ] Every commit is signed off (`git commit -s`) under the [Developer Certificate of Origin](https://developercertificate.org).
- [ ] For UI changes: screenshots before and after are attached below.

## Screenshots

<!-- For UI changes. Delete this section otherwise. -->

## Not updated, and why

<!-- Anything adjacent you deliberately left alone. -->
