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

import type {
  Capabilities,
  DiskCounterSource,
  HandleInfo,
  ModuleInfo,
  Process,
} from '@vitals/protocol';

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
  /**
   * `confirmed`: the user clicked through the risk dialog for this process.
   * The backend refuses a process it finds critical without it.
   */
  terminate(process: Process, confirmed?: boolean): Promise<void>;
  suspend(process: Process, confirmed?: boolean): Promise<void>;
  resume(process: Process): Promise<void>;
  setPriority(process: Process, priority: ProcessPriority): Promise<void>;
  /**
   * Pins a process to a set of logical processors.
   *
   * The detail panel offers presets computed from the machine's core
   * topology rather than a free-form core picker: "performance cores only"
   * is a thing people want, and a 24-checkbox grid is a thing they get wrong.
   */
  setAffinity(process: Process, mask: bigint): Promise<void>;
  /**
   * Whether the OS is throttling this process.
   *
   * `null` means the state could not be read — a protected process denies
   * the handle — and is deliberately distinct from `false`. The UI must not
   * offer to switch off a throttle it cannot see.
   */
  getEfficiencyMode(process: Process): Promise<boolean | null>;
  setEfficiencyMode(process: Process, enabled: boolean): Promise<void>;
  /**
   * The kernel handles this process holds.
   *
   * On demand only. The backend walks the whole system handle table, which
   * is megabytes on a busy machine, so this is called when the user expands
   * the section and never on a timer.
   */
  getHandles(process: Process): Promise<readonly HandleInfo[]>;
  /** The modules mapped into this process. On demand, same reason. */
  getModules(process: Process): Promise<readonly ModuleInfo[]>;
  /**
   * The full executable path, or `null` when the process denies us a handle.
   * The sampler carries only the file name, so this is read once per
   * selected row and gates the two shell actions below.
   */
  getExecutablePath(process: Process): Promise<string | null>;
  openFileLocation(path: string): Promise<void>;
  showFileProperties(path: string): Promise<void>;
  /**
   * Performs one action as administrator, behind a UAC prompt.
   *
   * For a process the unelevated app was refused — SYSTEM's or another
   * account's. Rejects with `refused` when the prompt is dismissed, which
   * the screen treats as the user's answer rather than a failure.
   */
  runAsAdmin(action: ElevatedAction, process: Process, confirmed?: boolean): Promise<void>;
}

/** What can be retried as administrator. Mirrors `ElevatedActionDto`. */
export type ElevatedAction = 'terminate' | 'suspend' | 'resume';

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
  terminate: (process, confirmed = false) =>
    invoke<void>('terminate_process', {
      pid: process.key.pid,
      startTime: process.key.startTime,
      confirmed,
    }),

  suspend: (process, confirmed = false) =>
    invoke<void>('suspend_process', {
      pid: process.key.pid,
      startTime: process.key.startTime,
      confirmed,
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

  getEfficiencyMode: (process) =>
    invoke<boolean | null>('get_efficiency_mode', {
      pid: process.key.pid,
      startTime: process.key.startTime,
    }),

  setEfficiencyMode: (process, enabled) =>
    invoke<void>('set_efficiency_mode', {
      pid: process.key.pid,
      startTime: process.key.startTime,
      enabled,
    }),

  getHandles: (process) =>
    invoke<HandleInfo[]>('get_process_handles', {
      pid: process.key.pid,
      startTime: process.key.startTime,
    }),

  getModules: (process) =>
    invoke<ModuleInfo[]>('get_process_modules', {
      pid: process.key.pid,
      startTime: process.key.startTime,
    }),

  getExecutablePath: (process) =>
    invoke<string | null>('get_executable_path', {
      pid: process.key.pid,
      startTime: process.key.startTime,
    }),

  openFileLocation: (path) => invoke<void>('open_file_location', { path }),

  showFileProperties: (path) => invoke<void>('show_file_properties', { path }),

  runAsAdmin: (action, process, confirmed = false) =>
    invoke<void>('process_action_as_admin', {
      action,
      pid: process.key.pid,
      startTime: process.key.startTime,
      confirmed,
    }),
};

/**
 * What `get_capabilities` answers.
 *
 * `diskCounterSource` is not a capability — the Disk column works either way
 * — but it decides how the column must be *labelled*, and it is only knowable
 * after the first sample. `null` before then, and rendered as such.
 */
export interface CapabilityReport extends Capabilities {
  readonly diskCounterSource: DiskCounterSource | null;
}

// Shared with every screen, so a command error reads the same everywhere.
export { errorMessage, isCommandError, type CommandErrorShape } from '../../lib/commandError';
