/**
 * Folds the frame stream into a complete picture.
 *
 * The only file that knows a delta from a keyframe. `@vitals/client` is
 * growing a reconcile helper of its own; when it lands, this is the one place
 * to swap the import.
 */

import { applyFrame } from '@vitals/protocol';
import type { Frame, Process, SystemMetrics } from '@vitals/protocol';

/** The current state of one machine, as far as the phone knows. */
export interface Snapshot {
  readonly system: SystemMetrics;
  /** Every live process, keyed by `pid:startTime`. */
  readonly processes: ReadonlyMap<string, Process>;
  readonly seq: number;
  readonly timestampMs: number;
}

/**
 * Applies one frame to the previous snapshot.
 *
 * Exits are applied before changes: a PID recycled within a single tick
 * appears in both lists, and the other order deletes the newcomer. The rule
 * lives in `@vitals/protocol`'s `applyFrame`, which is why this is a thin
 * wrapper rather than a second implementation to keep in step.
 */
export function fold(previous: Snapshot | null, frame: Frame): Snapshot {
  const processes = applyFrame(previous?.processes ?? new Map(), frame);
  return {
    system: frame.payload.system,
    processes,
    seq: frame.seq,
    timestampMs: frame.timestampMs,
  };
}
