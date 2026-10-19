/**
 * Per-drive clean-up: the wire shapes and the selection rules.
 *
 * # The backend decides what exists, the user decides what goes
 *
 * `analyse_drive_cleanup` returns a plan of measured items for one drive,
 * each with its own id. The webview only ever sends those ids back with
 * `confirmed: true`; it never names a path to delete.
 *
 * # Select-all never reaches a dangerous item
 *
 * A group's "select all" ticks the safe and review items only. An item marked
 * `danger` (a named Docker volume, say) has to be ticked on its own *and*
 * acknowledged on its own, so one sweep of the mouse cannot take a database
 * with it.
 *
 * # Unmeasured is not zero
 *
 * `size: null` means the backend could not measure it. Totals count those
 * items separately rather than adding a zero that would make the gain look
 * smaller than it is.
 */

export type DriveRole = 'system' | 'dev' | 'data' | 'external' | 'network' | 'optical';

export const driveRoles: readonly DriveRole[] = [
  'system',
  'dev',
  'data',
  'external',
  'network',
  'optical',
];

/**
 * The reasons `drive_role.rs` can give, as `role.reason.<key>`. A literal
 * list so a reason the backend adds without a string fails the strings test
 * rather than rendering as nothing.
 */
export const roleReasonKeys = [
  'network',
  'optical',
  'holdsWindows',
  'removable',
  'data',
  'devDrive',
  'holdsProjects',
  'holdsPnpmStore',
  'holdsWorktrees',
  'holdsWslDisks',
  'holdsDockerDisk',
] as const;

export type PlanGroupKey =
  | 'windows'
  | 'temp'
  | 'caches'
  | 'devArtefacts'
  | 'docker'
  | 'wsl'
  | 'compaction'
  | 'sparse'
  | 'recycleBin';

export const planGroupKeys: readonly PlanGroupKey[] = [
  'windows',
  'temp',
  'caches',
  'devArtefacts',
  'docker',
  'wsl',
  'compaction',
  'sparse',
  'recycleBin',
];

export type PlanSafety = 'safe' | 'review' | 'danger';

/**
 * The stages `analyse` reports, in order (`drive_cleanup::STAGES`). Not the
 * group keys: the walk measures build output under `artefacts` and finds
 * what would stop under `blockers`.
 */
export const analyseStages = [
  'role',
  'windows',
  'temp',
  'recycleBin',
  'caches',
  'artefacts',
  'docker',
  'wsl',
  'blockers',
] as const;

/** Every item kind the backend can send; each has a name and a detail line. */
export const planItemKinds = [
  'tempOld',
  'windowsUpdate',
  'componentStore',
  'deliveryOptimization',
  'thumbnailCache',
  'minidumps',
  'memoryDump',
  'hibernateOff',
  'hiberfilOrphan',
  'recycleBin',
  'npmCache',
  'pnpmCache',
  'pnpmStore',
  'cargoRegistry',
  'gradleCaches',
  'nugetCache',
  'rustTarget',
  'nodeModules',
  'buildDir',
  'dockerBuildCache',
  'dockerOldImages',
  'dockerStoppedContainer',
  'dockerAnonVolumes',
  'dockerNamedVolume',
  'wslAptCache',
  'wslJournal',
  'wslTmp',
  'wslUserCache',
  'wslRustTarget',
  'wslBuildDir',
  'wslDockerPrune',
  'compactVhd',
  'sparseVhd',
] as const;

export type PlanItemKind = (typeof planItemKinds)[number];

export interface PlanItem {
  readonly id: string;
  /** One of `planItemKinds`; typed loosely so a newer backend cannot crash the list. */
  readonly kind: string;
  /** A distro, image or folder name when the kind alone does not say which. */
  readonly label: string | null;
  /** Bytes it would give back; `null` when it could not be measured. */
  readonly size: number | null;
  readonly safety: PlanSafety;
  readonly defaultSelected: boolean;
  readonly irreversible: boolean;
  readonly needsElevation: boolean;
  readonly stopsWsl: boolean;
  readonly stopsDocker: boolean;
  /** A path or a short technical line, shown small. */
  readonly detail: string | null;
  /**
   * False when it is shown for its size but cannot be done from here; the
   * reason is in `detail`, and the backend refuses its id. The backend always
   * sends it; absent (an older backend) means available.
   */
  readonly available?: boolean;
  /**
   * One of `unavailableReasons` when `available` is false and the backend
   * knows why; typed loosely so a newer backend's reason cannot crash the list.
   */
  readonly unavailableReason?: string | null;
}

/** Every reason the backend gives for an item it shows but cannot do. */
export const unavailableReasons = [
  'hibernationRefused',
  'sparseDisabledByWsl',
  'sparseNeedsNewerWsl',
  'wslVersionUnknown',
] as const;

export type UnavailableReason = (typeof unavailableReasons)[number];

/** The item's known reason for being unavailable, or null. */
export function unavailableReason(item: PlanItem): UnavailableReason | null {
  const reason = item.unavailableReason;
  if (isAvailable(item) || reason === undefined || reason === null) return null;
  return (unavailableReasons as readonly string[]).includes(reason)
    ? (reason as UnavailableReason)
    : null;
}

/** Whether an item can be ticked at all. */
export function isAvailable(item: PlanItem): boolean {
  return item.available !== false;
}

export interface PlanGroup {
  readonly key: PlanGroupKey;
  readonly items: readonly PlanItem[];
}

export type BlockerKind =
  | 'wslDistro'
  | 'dockerDesktop'
  | 'container'
  | 'scheduledTask'
  | 'githubRunner'
  | 'hyperVVm'
  | 'vscodeServer';

export const blockerKinds: readonly BlockerKind[] = [
  'wslDistro',
  'dockerDesktop',
  'container',
  'scheduledTask',
  'githubRunner',
  'hyperVVm',
  'vscodeServer',
];

export interface Blocker {
  readonly kind: BlockerKind;
  readonly name: string;
  /** Whether the clean-up starts it again afterwards. */
  readonly restartAfter: boolean;
}

/**
 * The name the backend gives the one Hyper-V blocker it sends when it could
 * not list the machines (not running as administrator): "some virtual
 * machines, unknown which", not a machine called "unknown".
 */
export const UNKNOWN_HYPERV_NAME = 'unknown';

export function isUnknownHyperV(blocker: Blocker): boolean {
  return blocker.kind === 'hyperVVm' && blocker.name === UNKNOWN_HYPERV_NAME;
}

export interface DriveCleanupPlan {
  readonly scanId: number;
  readonly mount: string;
  readonly role: DriveRole;
  /** i18n keys such as `role.reason.holdsWindows`. */
  readonly roleReasons: readonly string[];
  readonly freeBytes: number;
  readonly totalBytes: number;
  readonly groups: readonly PlanGroup[];
  readonly blockers: readonly Blocker[];
}

export type DriveItemStatus = 'done' | 'skipped' | 'failed';

export const driveItemStatuses: readonly DriveItemStatus[] = ['done', 'skipped', 'failed'];

export interface DriveReportItem {
  readonly id: string;
  readonly status: DriveItemStatus;
  readonly freed: number | null;
  readonly message: string | null;
}

export interface DriveCleanupReport {
  readonly items: readonly DriveReportItem[];
  readonly driveFreedBytes: number | null;
  readonly restarted: readonly Blocker[];
  readonly restartFailed: readonly Blocker[];
}

/** `vitals://storage/drive-analyse-progress`. */
export interface DriveAnalyseProgress {
  readonly stage: string;
  readonly done: number;
  readonly total: number;
}

export type DriveCleanStage =
  'cleaning' | 'stopping' | 'trimming' | 'compacting' | 'restarting' | 'remeasuring';

export const driveCleanStages: readonly DriveCleanStage[] = [
  'cleaning',
  'stopping',
  'trimming',
  'compacting',
  'restarting',
  'remeasuring',
];

/** `vitals://storage/drive-clean-progress`. */
export interface DriveCleanProgress {
  /** One of `driveCleanStages`, or an item stage the backend adds later. */
  readonly stage: string;
  /** The item being worked on, when there is one. */
  readonly current: string | null;
  readonly done: number;
  readonly total: number;
  /** 1-based: the first disk is 1 of `diskCount`. */
  readonly diskIndex: number | null;
  readonly diskCount: number | null;
}

export function allItems(plan: DriveCleanupPlan): readonly PlanItem[] {
  return plan.groups.flatMap((group) => group.items);
}

/** True for a drive the wizard is never offered on. */
export function isCleanableKind(kind: string): boolean {
  return kind !== 'network' && kind !== 'optical';
}

/**
 * The same drive however it is written: `C:`, `c:\` and `C:\` are one key.
 * Alerts name a drive `C:` and the storage list `C:\`.
 */
export function mountKey(mount: string): string {
  return mount.replace(/[\\/]+$/, '').toUpperCase();
}

/**
 * What is ticked when a plan arrives.
 *
 * With a remembered profile, the remembered kinds; otherwise the backend's
 * defaults. Danger items are never preselected either way: each one needs a
 * fresh decision.
 */
export function initialSelection(
  plan: DriveCleanupPlan,
  profile: readonly string[] | undefined,
): ReadonlySet<string> {
  const kinds = profile === undefined ? null : new Set(profile);
  return new Set(
    allItems(plan)
      .filter((item) => item.safety !== 'danger' && isAvailable(item))
      .filter((item) => (kinds === null ? item.defaultSelected : kinds.has(item.kind)))
      .map((item) => item.id),
  );
}

/** The ids a group's "select all" may tick: never a danger item. */
export function groupSelectable(group: PlanGroup): readonly string[] {
  return group.items
    .filter((item) => item.safety !== 'danger' && isAvailable(item))
    .map((item) => item.id);
}

export type CheckState = boolean | 'indeterminate';

/** The group header's box, over the items select-all can reach. */
export function groupCheckState(group: PlanGroup, selected: ReadonlySet<string>): CheckState {
  const ids = groupSelectable(group);
  if (ids.length === 0) return false;
  const on = ids.filter((id) => selected.has(id)).length;
  if (on === 0) return false;
  return on === ids.length ? true : 'indeterminate';
}

export interface SizeTotal {
  readonly count: number;
  readonly bytes: number;
  /** Items in the total whose size is unknown. */
  readonly unmeasured: number;
}

export function sizeTotal(items: readonly PlanItem[], selected?: ReadonlySet<string>): SizeTotal {
  let count = 0;
  let bytes = 0;
  let unmeasured = 0;
  for (const item of items) {
    if (selected !== undefined && !selected.has(item.id)) continue;
    count += 1;
    if (item.size === null) unmeasured += 1;
    else bytes += item.size;
  }
  return { count, bytes, unmeasured };
}

export function selectedItems(
  plan: DriveCleanupPlan,
  selected: ReadonlySet<string>,
): readonly PlanItem[] {
  return allItems(plan).filter((item) => selected.has(item.id));
}

/**
 * The programs a selection will stop.
 *
 * Stopping WSL takes everything with it (Docker Desktop runs inside it); a
 * Docker-only step stops Docker Desktop and its containers. Empty when no
 * selected item stops either.
 */
export function blockersFor(
  plan: DriveCleanupPlan,
  items: readonly PlanItem[],
): readonly Blocker[] {
  if (items.some((item) => item.stopsWsl)) return plan.blockers;
  if (items.some((item) => item.stopsDocker)) {
    return plan.blockers.filter(
      (blocker) => blocker.kind === 'dockerDesktop' || blocker.kind === 'container',
    );
  }
  return [];
}

/** Danger items that are ticked but not yet acknowledged one by one. */
export function unacknowledged(
  items: readonly PlanItem[],
  acknowledged: ReadonlySet<string>,
): readonly PlanItem[] {
  return items.filter((item) => item.safety === 'danger' && !acknowledged.has(item.id));
}

/** The kinds a profile remembers: never a danger kind, for the reason above. */
export function profileKinds(items: readonly PlanItem[]): readonly string[] {
  return [...new Set(items.filter((item) => item.safety !== 'danger').map((item) => item.kind))];
}

/** The ids a remembered profile selects in this plan. */
export function profileSelection(
  plan: DriveCleanupPlan,
  profile: readonly string[],
): readonly string[] {
  return [...initialSelection(plan, profile)];
}

export function isKnownKind(kind: string): kind is PlanItemKind {
  return (planItemKinds as readonly string[]).includes(kind);
}
