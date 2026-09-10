# 0024 — Two TypeScript compilers: 7 for `tsc`, 6 for ESLint

- Status: Accepted
- Date: 2026-09-10
- Tracker: S10-02

## Context

TypeScript 7 (the Go port) type-checks the repo several times faster than 6,
but does not yet expose a stable programmatic API. `typescript-eslint`
depends on that API and declares a peer range of `<6.1.0`; its upstream
issue for TypeScript 7 support is closed "not planned" pending TS 7.1.

## Decision

Install both, side by side, as Microsoft's
[documented arrangement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0)
describes: `@typescript/native` (7.0.2) provides `tsc` and runs `pnpm
typecheck`; the `typescript` package name resolves to
`@typescript/typescript6` (6.0.2) so `typescript-eslint` finds the API it
needs.

## Consequences

- Type-checking and linting can disagree on an edge case; `tsc` is the
  authority.
- Collapse to one compiler when typescript-eslint supports 7.1 — a one-line
  change in the root `package.json`.
- `pnpm typecheck` is fast enough to run on every save, which is what made
  `exactOptionalPropertyTypes` and `noUncheckedIndexedAccess` tolerable.
