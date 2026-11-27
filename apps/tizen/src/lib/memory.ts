/**
 * The memory line's numbers, as a testable decision.
 *
 * A total of zero means "not reported", never "no memory": a PC that told the
 * sampler nothing about its RAM is not a PC using none of it. Returning
 * `null` here lets the caller print the em dash, so the two facts never
 * collapse into a confident `0 B of 0 B`.
 */

import type { MemoryMetrics } from '@vitals/protocol';

export function memoryUsage(
  memory: MemoryMetrics,
): { readonly used: number; readonly total: number } | null {
  return memory.total > 0 ? { used: memory.used, total: memory.total } : null;
}
