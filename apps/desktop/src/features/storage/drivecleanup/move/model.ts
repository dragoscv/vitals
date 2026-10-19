/**
 * Guided disk move: the wire shapes from `drivemove.rs` and the rules the
 * dialog applies to them (ADR-0040).
 *
 * # The backend decides where a disk may go
 *
 * `plan_drive_move` lists the disks on one drive and, for every mounted
 * drive, either `refusal: null` (it can take the disk) or the reason it
 * cannot. The dialog shows the reasons and only ever sends back the plan id,
 * a disk id and a drive the plan marked eligible; the backend checks all of
 * it again before anything stops.
 *
 * # Unmeasured is not zero
 *
 * `size: null` means the disk could not be measured. Such a disk is never
 * offered a target (`sizeUnknown`), because whether it fits is unknown.
 */

import type { Blocker } from '../model';

/** Why a drive cannot take the disk (`TargetRefusal::key`). */
export const moveRefusals = [
  'sameDrive',
  'notFixed',
  'notNtfs',
  'tooLittleSpace',
  'sizeUnknown',
  'compressedOrEncrypted',
  'occupied',
] as const;

/** Why a disk is shown but cannot be moved from here (`MoveUnavailable::key`). */
export const moveUnavailables = ['wslTooOld', 'wslVersionUnknown', 'wsl1', 'unsafeName'] as const;

/** The stages a move reports, in order. `moving` cannot be interrupted. */
export const moveStages = ['checking', 'stopping', 'moving', 'verifying', 'restarting'] as const;

export const moveStatuses = ['moved', 'movedWithProblems', 'notMoved'] as const;

export const diskOwners = ['distro', 'dockerDesktop'] as const;

export type MoveRefusal = (typeof moveRefusals)[number];
export type MoveUnavailable = (typeof moveUnavailables)[number];
export type MoveStage = (typeof moveStages)[number];
export type MoveStatus = (typeof moveStatuses)[number];
export type DiskOwner = (typeof diskOwners)[number];

export interface MoveTarget {
  readonly mount: string;
  readonly label: string | null;
  readonly fileSystem: string | null;
  readonly freeBytes: number;
  readonly totalBytes: number;
  /** One of `moveRefusals`, or `null` when the drive can take the disk. */
  readonly refusal: string | null;
}

export interface MovableDisk {
  readonly id: string;
  /** One of `diskOwners`; typed loosely so a newer backend cannot crash the dialog. */
  readonly owner: string;
  readonly name: string;
  readonly path: string;
  readonly size: number | null;
  /** The free space a target needs: the disk, 10 % and a margin. */
  readonly requiredBytes: number | null;
  readonly running: boolean;
  readonly unavailable: string | null;
  readonly targets: readonly MoveTarget[];
  readonly blockers: readonly Blocker[];
}

export interface SuggestedFolder {
  readonly mount: string;
  readonly folder: string;
}

export interface DockerDisk {
  readonly installed: boolean;
  readonly path: string | null;
  readonly size: number | null;
  /** Docker Desktop's "Disk image location" setting, when set. */
  readonly location: string | null;
  readonly version: string | null;
  /** `false` when this Docker Desktop fails moves to another drive. */
  readonly crossDriveOk: boolean | null;
  readonly suggested: readonly SuggestedFolder[];
}

export interface MovePlan {
  readonly planId: number;
  readonly mount: string;
  readonly wslVersion: string | null;
  readonly disks: readonly MovableDisk[];
  readonly docker: DockerDisk;
}

export interface MoveProgress {
  readonly stage: string;
  /** Bytes written at the destination so far, while moving. */
  readonly done: number | null;
  readonly total: number | null;
}

/** `null` = not checked (the move did not happen), never "passed". */
export interface MoveVerification {
  readonly registeredAtTarget: boolean | null;
  readonly fileAtTarget: boolean | null;
  readonly oldFileGone: boolean | null;
  readonly starts: boolean | null;
  readonly freedBytes: number | null;
}

export interface MoveReport {
  readonly status: string;
  readonly message: string | null;
  readonly refusal: string | null;
  readonly command: string | null;
  readonly from: string | null;
  readonly to: string | null;
  readonly verification: MoveVerification;
  readonly restarted: readonly Blocker[];
  readonly restartFailed: readonly Blocker[];
}

export function isKnown<T extends string>(list: readonly T[], value: string | null): value is T {
  return value !== null && (list as readonly string[]).includes(value);
}

/** The drives that can take `disk`, in drive-letter order. */
export function eligibleTargets(disk: MovableDisk): readonly MoveTarget[] {
  return disk.targets.filter((target) => target.refusal === null);
}

/** Whether `disk` can be moved by Vitals itself (a WSL distro, offered). */
export function canMoveHere(disk: MovableDisk): boolean {
  return disk.owner === 'distro' && disk.unavailable === null && eligibleTargets(disk).length > 0;
}

/**
 * Below either line a drive is worth offering a move for: under 15 % free,
 * or under 25 GB free. The ratio catches a big drive filling up, the floor a
 * small one (a 128 GB drive at 20 % still has only 25 GB to give).
 */
export const LOW_FREE_RATIO = 0.15;
export const LOW_FREE_BYTES = 25 * 1024 ** 3;

export function isLowOnSpace(drive: {
  readonly freeBytes: number;
  readonly totalBytes: number;
}): boolean {
  if (drive.totalBytes <= 0) return false;
  return drive.freeBytes / drive.totalBytes < LOW_FREE_RATIO || drive.freeBytes < LOW_FREE_BYTES;
}

/** The disk the dialog opens on: the one named, else the first that can move. */
export function initialDisk(plan: MovePlan, focus: string | null): MovableDisk | null {
  return (
    plan.disks.find((disk) => disk.name === focus) ??
    plan.disks.find(canMoveHere) ??
    plan.disks[0] ??
    null
  );
}

/** Whether Docker's disk is now somewhere other than the drive it was on. */
export function dockerMovedOff(docker: DockerDisk, mount: string): boolean | null {
  if (docker.path === null) return null;
  return !docker.path.toUpperCase().startsWith(mount.toUpperCase());
}
