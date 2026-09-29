/**
 * Sorting and safety rules for the Programs tab.
 *
 * `ProcessFlags` arrives as a bare number because ts-rs erases the bitflags
 * struct; the two bits needed here are mirrored from
 * `crates/vitals-core/src/process.rs`, as the desktop and Android do.
 */

import type { Process } from '@vitals/protocol';

export const FLAG_EFFICIENCY_MODE = 1 << 3;
export const FLAG_CRITICAL = 1 << 5;

/** Enough rows to find anything that matters; a thousand focus stops would make the list unusable with a remote. */
export const SHOWN = 150;

export type SortKey = 'cpu' | 'memory' | 'name';

/**
 * Ties broken by PID so an idle machine, where most programs sit at 0 %,
 * does not reshuffle the list every second under the remote's focus.
 */
export function sortProcesses(all: Iterable<Process>, key: SortKey): Process[] {
  const list = [...all];
  const byPid = (a: Process, b: Process) => a.key.pid - b.key.pid;
  switch (key) {
    case 'cpu':
      return list.sort((a, b) => b.cpu - a.cpu || b.memoryPrivate - a.memoryPrivate || byPid(a, b));
    case 'memory':
      return list.sort((a, b) => b.memoryPrivate - a.memoryPrivate || byPid(a, b));
    case 'name':
      return list.sort(
        (a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }) || byPid(a, b),
      );
  }
}

/** Mirrors `Process::is_safely_terminable` in Rust: never offer to end what would crash Windows. */
export function isSafelyTerminable(p: Process): boolean {
  return (p.flags & FLAG_CRITICAL) === 0 && p.protection === 'none' && p.kind !== 'system';
}

export function inEfficiencyMode(p: Process): boolean {
  return (p.flags & FLAG_EFFICIENCY_MODE) !== 0;
}
