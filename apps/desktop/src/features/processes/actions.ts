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

/** The narrow surface the UI needs; injectable so tests need no Tauri host. */
export interface ProcessActionsApi {
  planTerminate(process: Process): Promise<ActionPlan>;
  planSuspend(process: Process): Promise<ActionPlan>;
  terminate(process: Process): Promise<void>;
  suspend(process: Process): Promise<void>;
  resume(process: Process): Promise<void>;
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
};

/**
 * Actions the UI shows but the backend cannot yet perform.
 *
 * Listed explicitly rather than quietly omitted: a task manager missing
 * "Open file location" reads as unfinished, whereas one that shows it
 * disabled with a reason reads as honest. Nothing here is faked — the menu
 * items are inert and say why.
 */
export const UNIMPLEMENTED_ACTIONS = [
  'priority',
  'affinity',
  'openLocation',
  'properties',
] as const;

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
