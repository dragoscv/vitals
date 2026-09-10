/**
 * The bridge to the Rust process actions.
 *
 * Every mutating call is preceded by a *plan* call that reports what will
 * actually happen. The two are separate on purpose: a dialog that says
 * "Ending it will crash Windows immediately" and one that says "Unsaved work
 * will be lost" must not be the same dialog, because a warning that looks the
 * same every time is a warning nobody reads.
 */

import { invoke } from '@tauri-apps/api/core';

import type { Process } from '@vitals/protocol';

import { ProcessFlag, hasFlag } from './constants';

export type ActionRisk = 'safe' | 'disruptive' | 'critical' | 'forbidden';

export interface ActionPlan {
  readonly risk: ActionRisk;
  /** A translation key, not English. Resolved by the dialog. */
  readonly consequence: string;
  readonly needsConfirmation: boolean;
  /**
   * Whether "retry as administrator" is worth offering.
   *
   * False for protected processes. Task Manager offers elevation there and it
   * never works — the user elevates, fails again, and learns that the app
   * lies. We would rather say plainly that it cannot be done.
   */
  readonly elevationMightHelp: boolean;
}

/**
 * Scheduling priorities, slowest to fastest.
 *
 * Ordered because the menu presents them as a scale, and an alphabetical or
 * arbitrary order makes "above normal" and "below normal" easy to misread at
 * a glance.
 */
export const priorities = [
  'idle',
  'below-normal',
  'normal',
  'above-normal',
  'high',
  'realtime',
] as const;

export type ProcessPriority = (typeof priorities)[number];

/** The narrow surface the UI needs; injectable so tests need no Tauri host. */
export interface ProcessActionsApi {
  planTerminate(process: Process): Promise<ActionPlan>;
  planSuspend(process: Process): Promise<ActionPlan>;
  terminate(process: Process): Promise<void>;
  suspend(process: Process): Promise<void>;
  resume(process: Process): Promise<void>;
  setPriority(process: Process, priority: ProcessPriority): Promise<void>;
  /**
   * Pins a process to a set of logical processors.
   *
   * The backend command is wired; what is missing is a core picker to choose
   * the mask. Until that lands the menu item is listed under
   * [`UNIMPLEMENTED_ACTIONS`] as "needs a picker", which is the honest state:
   * not "the backend cannot", but "the UI cannot yet ask you which cores".
   */
  setAffinity(process: Process, mask: bigint): Promise<void>;
}

/**
 * Whether the OS actively protects this process.
 *
 * Passed to the planner rather than recomputed there, because the planner
 * takes a bare PID and the protection level only exists on the sampled row.
 */
function isProtected(process: Process): boolean {
  return process.protection !== 'none' || hasFlag(process.flags, ProcessFlag.Critical);
}

export const tauriProcessActions: ProcessActionsApi = {
  planTerminate: (process) =>
    invoke<ActionPlan>('plan_terminate_process', {
      pid: process.key.pid,
      name: process.name,
      protected: isProtected(process),
    }),

  planSuspend: (process) =>
    invoke<ActionPlan>('plan_suspend_process', {
      pid: process.key.pid,
      name: process.name,
      protected: isProtected(process),
    }),

  // The start time travels with every mutation. Between the frame that listed
  // this process and this call it can exit and Windows can hand its PID to
  // something else; acting on the PID alone would kill the newcomer.
  terminate: (process) =>
    invoke<void>('terminate_process', {
      pid: process.key.pid,
      startTime: process.key.startTime,
    }),

  suspend: (process) =>
    invoke<void>('suspend_process', {
      pid: process.key.pid,
      startTime: process.key.startTime,
    }),

  resume: (process) =>
    invoke<void>('resume_process', {
      pid: process.key.pid,
      startTime: process.key.startTime,
    }),

  setPriority: (process, priority) =>
    invoke<void>('set_process_priority', {
      pid: process.key.pid,
      startTime: process.key.startTime,
      priority,
    }),

  setAffinity: (process, mask) =>
    invoke<void>('set_process_affinity', {
      pid: process.key.pid,
      startTime: process.key.startTime,
      // Sent as a JSON number, not a string: serde refuses a string for a
      // `u64` field. Verified — `{"mask":"255"}` fails with `invalid type:
      // string`, while a bare `18446744073709551615` parses fine, because
      // serde_json reads the literal digits rather than going through an f64.
      //
      // `JSON.stringify` cannot serialise a bigint, so the conversion has to
      // happen here. Above 2^53 this loses precision — a machine with more
      // than 53 logical processors cannot express a mask touching its top
      // cores. That is a real limit, and the reason the caller is not wired
      // up yet: the core picker that produces this mask has to decide what
      // to do about it first.
      mask: Number(mask),
    }),
};

/**
 * Actions the UI shows but the backend cannot yet perform.
 *
 * Listed explicitly rather than quietly omitted: a task manager missing
 * "Open file location" reads as unfinished, whereas one that shows it
 * disabled with a reason reads as honest. Nothing here is faked — the menu
 * items are inert and say why.
 */
export const UNIMPLEMENTED_ACTIONS = ['affinity', 'openLocation', 'properties'] as const;

export type UnimplementedAction = (typeof UNIMPLEMENTED_ACTIONS)[number];

/** The error shape `CommandError` serialises to. */
export interface CommandErrorShape {
  readonly kind: 'access-denied' | 'not-found' | 'unsupported' | 'internal';
  readonly message: string;
}

export function isCommandError(value: unknown): value is CommandErrorShape {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as { kind?: unknown; message?: unknown };
  return typeof candidate.kind === 'string' && typeof candidate.message === 'string';
}

export function errorMessage(error: unknown): string {
  if (isCommandError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}
