/**
 * Narrowing helpers over the generated types.
 *
 * These exist because the generated unions are structural, and reaching into
 * a discriminated union inline at every call site is both noisy and easy to
 * get subtly wrong when a variant is added later.
 */

import type { Frame } from './generated/core/Frame';
import type { FramePayload } from './generated/core/FramePayload';
import type { Process } from './generated/core/Process';
import type { ProcessKey } from './generated/core/ProcessKey';

/** A keyframe carries a full snapshot; a delta carries only changes. */
export function isKeyframe(
  payload: FramePayload,
): payload is Extract<FramePayload, { kind: 'keyframe' }> {
  return payload.kind === 'keyframe';
}

export function isDelta(
  payload: FramePayload,
): payload is Extract<FramePayload, { kind: 'delta' }> {
  return payload.kind === 'delta';
}

/**
 * Stable string form of a process identity, for use as a React key or a Map
 * key.
 *
 * The PID alone is unsuitable: the OS recycles PIDs within seconds, so a bare
 * PID key would cause React to reuse a row's DOM — and its animation and
 * selection state — for a completely unrelated process.
 */
export function processKeyId(key: ProcessKey): string {
  return `${key.pid}:${key.startTime}`;
}

export function sameProcess(a: ProcessKey, b: ProcessKey): boolean {
  return a.pid === b.pid && a.startTime === b.startTime;
}

/**
 * Applies a frame to an existing process map, returning a new map.
 *
 * Centralised because delta reconciliation is easy to get wrong in ways that
 * are invisible until a process lingers in the table forever after exiting.
 */
export function applyFrame(
  current: ReadonlyMap<string, Process>,
  frame: Frame,
): Map<string, Process> {
  const payload = frame.payload;

  if (payload.kind === 'keyframe') {
    // A keyframe replaces everything: anything absent from it has exited.
    return new Map(payload.processes.map((p) => [processKeyId(p.key), p] as const));
  }

  const next = new Map(current);

  // Removals are applied BEFORE additions, and are matched by full identity
  // rather than by PID alone.
  //
  // Order matters: the OS can hand a just-freed PID to a new process within
  // the same sample interval, so a frame legitimately contains the same PID
  // in both `exited` and `changed`. Deleting by bare PID after merging would
  // remove the newcomer and silently drop a running process from the table.
  if (payload.exited.length > 0) {
    const exited = new Set(payload.exited);
    for (const [id, process] of next) {
      if (exited.has(process.key.pid)) next.delete(id);
    }
  }

  for (const process of payload.changed) {
    next.set(processKeyId(process.key), process);
  }

  return next;
}
