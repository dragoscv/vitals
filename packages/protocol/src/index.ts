/**
 * The Rust ↔ TypeScript contract.
 *
 * Everything under `generated/` is produced from the Rust domain model by
 * `pnpm protocol:generate` and is not editable. CI runs `pnpm protocol:check`,
 * which regenerates and fails on any diff — so the two sides of the boundary
 * cannot drift without the build going red.
 *
 * Hand-written helpers that operate on those types live in this file and its
 * siblings, never inside `generated/`.
 */

export * from './commands';
export * from './generated';
export * from './guards';
export * from './process';
export * from './select';
