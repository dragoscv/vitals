/**
 * Disk storage: the data shape, sorting, and what each number is allowed to
 * claim.
 *
 * # Allocated is the primary figure, logical is carried alongside
 *
 * Every total from the scanner is **allocated** size — what the files occupy
 * on the volume, cluster-rounded — because that is the space that would
 * actually be freed. Logical size, the plain sum of file lengths, is shown
 * next to it rather than instead of it: logical is what Explorer's "Size:"
 * line and `Get-ChildItem | Measure-Object -Sum Length` report, and a user
 * who sees only one of the two and compares it against Windows concludes that
 * we are the ones that are broken.
 *
 * They differ for three deliberate reasons: cluster rounding raises the
 * total, hard-link deduplication lowers it, and unreadable directories are
 * excluded from it and listed separately.
 *
 * # Unmeasured is never zero
 *
 * A cleanup candidate that exists but could not be sized carries `size:
 * null`. That is a different fact from "this is empty", and rendering the
 * first as the second is how a cleanup tool talks someone out of reclaiming
 * eight gigabytes. Every helper here keeps the two apart, and the reclaimable
 * total reports how many entries it could not include.
 */

export type ScanStrategyKey = 'mftAssisted' | 'directoryWalk';

/** How a snapshot was obtained (`ScanSnapshotDto.method`). */
export type ScanMethodKey = 'walk' | 'incremental' | 'turbo';

/** Why a walk read every folder rather than reusing the saved index. */
export type FullReasonKey =
  | 'noIndex'
  | 'requested'
  | 'journalChanged'
  | 'hardLinksChanged'
  | 'notSupported'
  | 'indexUnreadable';

/** What a running scan is doing (`ScanProgressDto.stage`). */
export type ScanStageKey = 'walking' | 'approval' | 'reading' | 'building' | 'saving';

/** The stages that get their own line; `walking` shows the folder being read. */
export const namedScanStages: readonly Exclude<ScanStageKey, 'walking'>[] = [
  'approval',
  'reading',
  'building',
  'saving',
];

export type DiskKindKey = 'hdd' | 'ssd' | 'nvme' | 'removable' | 'network' | 'optical' | 'unknown';

export type SkipReasonKey = 'accessDenied' | 'reparsePoint' | 'cancelled' | 'vanished' | 'osError';

export type SafetyKey = 'safe' | 'review' | 'risky';

export type CleanupKindKey =
  | 'userTemp'
  | 'systemTemp'
  | 'browserCache'
  | 'windowsUpdateCache'
  | 'recycleBin'
  | 'crashDump'
  | 'previousWindows'
  | 'hibernation'
  | 'packageManagerCache'
  | 'thumbnailCache'
  | 'deliveryOptimisation'
  | 'componentStore';

/** The Windows tool that frees a candidate (`CleanupCandidateDto.tool`). */
export type WindowsToolKey = 'diskCleanup' | 'componentCleanup' | 'hibernateOff';

export const windowsTools: readonly WindowsToolKey[] = [
  'diskCleanup',
  'componentCleanup',
  'hibernateOff',
];

export interface Volume {
  /** Mount point, e.g. `C:\`. Also the scan root and the React key. */
  readonly mount: string;
  readonly label: string | null;
  readonly fileSystem: string | null;
  readonly kind: DiskKindKey;
  readonly total: number;
  readonly available: number;
  readonly strategy: ScanStrategyKey;
  /** Turbo (one UAC prompt, the drive's file table read directly) is offered. */
  readonly turbo: boolean;
}

export interface DirectoryEntry {
  readonly path: string;
  /** On-disk occupancy, cluster-rounded. The figure that matters. */
  readonly allocated: number;
  /** Sum of file lengths, i.e. what Explorer shows. */
  readonly logical: number;
  readonly files: number;
  /** Set when the directory could not be fully read. */
  readonly incomplete: SkipReasonKey | null;
}

export interface SkippedPath {
  readonly path: string;
  readonly reason: SkipReasonKey;
  readonly elevationFixable: boolean;
}

/** What a running scan has seen so far (`ScanProgressDto`). */
export interface ScanProgress {
  readonly root: string;
  readonly stage: ScanStageKey;
  /** For Turbo, file records read so far. */
  readonly filesSeen: number;
  readonly directoriesSeen: number;
  readonly bytesSeen: number;
  readonly elapsedMs: number;
  /** The folder most recently read; empty when the stage reads no folders. */
  readonly currentPath: string;
}

export interface ScanSnapshot {
  readonly root: string;
  readonly largest: readonly DirectoryEntry[];
  readonly allocated: number;
  readonly logical: number;
  /** `null` disables cluster rounding, which under-reports. Say so. */
  readonly clusterBytes: number | null;
  readonly filesScanned: number;
  readonly directoriesScanned: number;
  readonly hardLinkDuplicates: number;
  readonly hardLinkBytesSaved: number;
  readonly cancelled: boolean;
  readonly complete: boolean;
  readonly elapsedMs: number;
  /** A bounded sample; `skippedTotal` holds the real count. */
  readonly skipped: readonly SkippedPath[];
  readonly skippedTotal: number;
  /** Links recorded rather than followed; not gaps in the totals. */
  readonly linksNotFollowed: number;
  /** Identifies the tree kept in memory for navigation. */
  readonly scanId: number;
  /** Where navigation starts. */
  readonly rootNode: number;
  /** The largest individual files anywhere under the root, largest first. */
  readonly largestFiles: readonly LargeFile[];
  readonly method: ScanMethodKey;
  /** Folders taken unchanged from the saved index; 0 unless incremental. */
  readonly reusedFolders: number;
  /** Folders actually read from disk this time. */
  readonly relistedFolders: number;
  /** Why a walk read everything; `null` for incremental and Turbo. */
  readonly fullReason: FullReasonKey | null;
  /** An index for fast rescans was written after this scan. */
  readonly indexSaved: boolean;
}

/** The reasons worth a sentence: the ones a user would not have guessed. */
export type InformativeFullReason = Exclude<FullReasonKey, 'noIndex' | 'requested'>;

/**
 * Why a walk read every folder, when saying so tells the user something.
 *
 * `noIndex` (the first scan) and `requested` (they pressed Full rescan) are
 * what the user already knows, so they stay silent. The others explain why a
 * rescan that was expected to take seconds took minutes, which otherwise
 * reads as the fast path being broken.
 */
export function informativeFullReason(snapshot: ScanSnapshot): InformativeFullReason | null {
  if (snapshot.method !== 'walk') return null;
  const reason = snapshot.fullReason;
  if (reason === null || reason === 'noIndex' || reason === 'requested') return null;
  return reason;
}

export interface LargeFile {
  readonly path: string;
  readonly allocated: number;
  readonly logical: number;
  /** The folder it is in, for "show in map". */
  readonly dirNode: number;
}

/** One folder of a kept scan (`StorageNodeDto`). */
export interface StorageNode {
  readonly node: number;
  readonly name: string;
  readonly path: string;
  readonly allocated: number;
  readonly logical: number;
  readonly files: number;
  /** Files directly inside this folder, not below it. */
  readonly ownAllocated: number;
  readonly ownFiles: number;
  readonly hasChildren: boolean;
  readonly incomplete: SkipReasonKey | null;
}

export interface StorageListing {
  /** Root first, ending with the folder itself. */
  readonly ancestry: readonly StorageNode[];
  /** Largest first. */
  readonly children: readonly StorageNode[];
}

export type MapShape = 'icicle' | 'treemap';
export const mapShapes: readonly MapShape[] = ['icicle', 'treemap'];

export type MapCellKind = 'directory' | 'files' | 'smaller';

/** One rectangle; coordinates are fractions of the drawing area. */
export interface MapCell {
  readonly kind: MapCellKind;
  readonly node: number;
  readonly depth: number;
  readonly x0: number;
  readonly y0: number;
  readonly x1: number;
  readonly y1: number;
  readonly allocated: number;
  /** Files for a folder or files block; folders for a "smaller" block. */
  readonly count: number;
  readonly openable: boolean;
  readonly name: string | null;
  readonly incomplete: SkipReasonKey | null;
}

export interface CleanupCandidate {
  readonly path: string;
  readonly kind: CleanupKindKey;
  /** `null` when the location exists but could not be measured. */
  readonly size: number | null;
  readonly safety: SafetyKey;
  readonly allowsOneClick: boolean;
  readonly needsElevation: boolean;
  /** `null` when the space is not Windows-managed (the basket's job). */
  readonly tool: WindowsToolKey | null;
  /** The tool's effect cannot be undone; it needs its own confirmation. */
  readonly irreversible: boolean;
}

export type CleanupStageKey = 'measuring' | 'approval' | 'running' | 'remeasuring';

export const cleanupStages: readonly CleanupStageKey[] = [
  'measuring',
  'approval',
  'running',
  'remeasuring',
];

/** `WindowsCleanupProgressDto`, about twice a second while a tool runs. */
export interface WindowsCleanupProgress {
  readonly path: string;
  readonly stage: CleanupStageKey;
  readonly elapsedMs: number;
  /** Free space the drive has gained so far; `null` when unreadable. */
  readonly driveFreed: number | null;
}

export type WindowsCleanupOutcomeKey = 'done' | 'needsRestart' | 'toolFailed';

/** `WindowsCleanupReportDto`: the figures read before and after, not estimates. */
export interface WindowsCleanupReport {
  readonly path: string;
  readonly outcome: WindowsCleanupOutcomeKey;
  readonly code: number | null;
  readonly locationBefore: number | null;
  readonly locationAfter: number | null;
  readonly locationFreed: number | null;
  readonly driveFreed: number | null;
  readonly elapsedMs: number;
}

/**
 * The one figure to headline, and which measurement it is: what the location
 * lost when it shrank, else what the drive gained. Some Windows handlers act
 * beyond the folder shown (Update Cleanup trims the component store), so a
 * location that did not move is not proof nothing was freed. Never below
 * zero, and `null` when neither was measured — "we could not read it" is not
 * "nothing was freed".
 */
export function measuredFreed(
  report: WindowsCleanupReport,
): { readonly bytes: number; readonly from: 'location' | 'drive' } | null {
  if (report.locationFreed !== null && report.locationFreed > 0) {
    return { bytes: report.locationFreed, from: 'location' };
  }
  if (report.driveFreed !== null) return { bytes: Math.max(0, report.driveFreed), from: 'drive' };
  if (report.locationFreed !== null) return { bytes: 0, from: 'location' };
  return null;
}

/** Something the user chose to review for the Recycle Bin. */
export interface BasketItem {
  readonly path: string;
  readonly name: string;
  readonly kind: 'folder' | 'file';
  /** On-disk bytes as the scan measured them. */
  readonly allocated: number;
}

export type HolderKindKey = 'window' | 'service' | 'explorer' | 'console' | 'critical' | 'other';

/** A program holding an item open (`HolderDto`). */
export interface Holder {
  readonly pid: number;
  readonly name: string;
  readonly service: string | null;
  readonly kind: HolderKindKey;
}

export type RecycleOutcomeKey =
  'recycled' | 'refused' | 'wouldBePermanent' | 'missing' | 'locked' | 'accessDenied' | 'failed';

export const recycleOutcomes: readonly RecycleOutcomeKey[] = [
  'recycled',
  'refused',
  'wouldBePermanent',
  'missing',
  'locked',
  'accessDenied',
  'failed',
];

export type ProtectionKey =
  | 'invalid'
  | 'driveRoot'
  | 'systemFolder'
  | 'userFolder'
  | 'systemFile'
  | 'noRecycleBin'
  | 'vitals';

export const protections: readonly ProtectionKey[] = [
  'invalid',
  'driveRoot',
  'systemFolder',
  'userFolder',
  'systemFile',
  'noRecycleBin',
  'vitals',
];

export interface RecycleItem {
  readonly path: string;
  readonly outcome: RecycleOutcomeKey;
  readonly protection: ProtectionKey | null;
  /** Only for `locked`; `null` when Windows could not say who. */
  readonly holders: readonly Holder[] | null;
  readonly code: number | null;
  /** Bytes taken out of the kept scan, `null` when it did not contain it. */
  readonly freed: number | null;
}

export interface RecycleReport {
  readonly items: readonly RecycleItem[];
  /** The kept scan's figures after the change; `null` when nothing in it changed. */
  readonly scan: ScanTotals | null;
}

/** The parts of a snapshot a recycle changes (`ScanTotalsDto`). */
export interface ScanTotals {
  readonly allocated: number;
  readonly logical: number;
  readonly filesScanned: number;
  readonly largest: readonly DirectoryEntry[];
  readonly largestFiles: readonly LargeFile[];
}

/**
 * Adds an item, or leaves the basket alone if it is already covered.
 *
 * Adding a folder drops anything already in the basket below it: the folder
 * takes them with it, and counting them twice would inflate the total. Adding
 * something inside a folder already in the basket is a no-op for the same
 * reason.
 */
export function addToBasket(
  basket: readonly BasketItem[],
  item: BasketItem,
): readonly BasketItem[] {
  const key = item.path.toLowerCase();
  const under = (child: string, parent: string) =>
    child === parent || child.startsWith(parent.endsWith('\\') ? parent : `${parent}\\`);
  if (basket.some((b) => under(key, b.path.toLowerCase()))) return basket;
  return [...basket.filter((b) => !under(b.path.toLowerCase(), key)), item];
}

export function removeFromBasket(
  basket: readonly BasketItem[],
  path: string,
): readonly BasketItem[] {
  const key = path.toLowerCase();
  return basket.filter((b) => b.path.toLowerCase() !== key);
}

export function inBasket(basket: readonly BasketItem[], path: string): boolean {
  const key = path.toLowerCase();
  return basket.some((b) => b.path.toLowerCase() === key);
}

export function basketTotal(basket: readonly BasketItem[]): number {
  return basket.reduce((sum, item) => sum + item.allocated, 0);
}

/** The last path component, for a row label. */
export function leafName(path: string): string {
  const trimmed = path.endsWith('\\') ? path.slice(0, -1) : path;
  return trimmed.slice(trimmed.lastIndexOf('\\') + 1) || path;
}

export const directorySorts = ['allocated', 'logical', 'files', 'path'] as const;
export type DirectorySort = (typeof directorySorts)[number];

/** Safety tiers in the order they are presented: safest first. */
export const safetyOrder: readonly SafetyKey[] = ['safe', 'review', 'risky'];

/** Fraction of the volume in use, 0..100, or `null` if capacity is unknown. */
export function usedPercent(volume: Volume): number | null {
  // A zero-capacity volume is a card reader with no card, or a mount that
  // refused GetDiskFreeSpaceEx. Dividing by it yields NaN or Infinity, and a
  // meter rendered from either reads as a confident 0% or a full bar.
  if (volume.total <= 0) return null;
  const used = Math.max(volume.total - volume.available, 0);
  return Math.min((used / volume.total) * 100, 100);
}

export function usedBytes(volume: Volume): number {
  return Math.max(volume.total - volume.available, 0);
}

/**
 * Sorts volumes by mount point.
 *
 * Plain lexicographic on the drive letter, which is what a user scanning a
 * picker expects: C: before D: regardless of size. Sorting by capacity would
 * move the system drive around as external media come and go.
 */
export function sortVolumes(volumes: readonly Volume[]): readonly Volume[] {
  return [...volumes].sort((a, b) => a.mount.localeCompare(b.mount, 'en'));
}

/**
 * Sorts directory rows, biggest first for the size columns.
 *
 * There is no missing-value case here: a directory that could not be read
 * still carries a real partial figure plus an `incomplete` marker, so unlike
 * the installed-apps list there is nothing to push to the bottom. The marker
 * is what tells the user the number is a floor.
 */
export function sortDirectories(
  entries: readonly DirectoryEntry[],
  sort: DirectorySort,
  locale: string,
): readonly DirectoryEntry[] {
  const byPath = (a: DirectoryEntry, b: DirectoryEntry) => a.path.localeCompare(b.path, locale);

  return [...entries].sort((a, b) => {
    switch (sort) {
      case 'allocated':
        return b.allocated !== a.allocated ? b.allocated - a.allocated : byPath(a, b);
      case 'logical':
        return b.logical !== a.logical ? b.logical - a.logical : byPath(a, b);
      case 'files':
        return b.files !== a.files ? b.files - a.files : byPath(a, b);
      case 'path':
        return byPath(a, b);
      default: {
        const exhaustive: never = sort;
        return exhaustive;
      }
    }
  });
}

export function filterDirectories(
  entries: readonly DirectoryEntry[],
  query: string,
): readonly DirectoryEntry[] {
  const needle = query.trim().toLowerCase();
  if (needle === '') return entries;
  return entries.filter((entry) => entry.path.toLowerCase().includes(needle));
}

export interface ReclaimableTotal {
  /** Sum of the candidates that could be measured. */
  readonly bytes: number;
  readonly measured: number;
  /**
   * Candidates that exist but could not be sized.
   *
   * Reported separately and never folded into `bytes` as zero, so the UI can
   * say the total is a floor instead of presenting it as complete.
   */
  readonly unmeasured: number;
}

/**
 * Sums what could be reclaimed, up to and including a safety tier.
 *
 * `upTo` is inclusive along `safetyOrder`, so `'review'` covers safe and
 * review but not risky. The risky tier — the recycle bin, `Windows.old`, the
 * hibernation image — is still *shown*, because a 34 GB hibernation file is
 * exactly what someone hunting for space wants to know about, but it does not
 * belong inside a headline "you could free this much" figure.
 */
export function reclaimableTotal(
  candidates: readonly CleanupCandidate[],
  upTo: SafetyKey = 'review',
): ReclaimableTotal {
  const limit = safetyOrder.indexOf(upTo);
  let bytes = 0;
  let measured = 0;
  let unmeasured = 0;

  for (const candidate of candidates) {
    if (safetyOrder.indexOf(candidate.safety) > limit) continue;
    if (candidate.size === null) unmeasured += 1;
    else {
      bytes += candidate.size;
      measured += 1;
    }
  }

  return { bytes, measured, unmeasured };
}

/** Groups candidates into the three tiers, preserving incoming order. */
export function groupBySafety(
  candidates: readonly CleanupCandidate[],
): readonly { readonly safety: SafetyKey; readonly items: readonly CleanupCandidate[] }[] {
  return safetyOrder
    .map((safety) => ({
      safety,
      items: candidates.filter((candidate) => candidate.safety === safety),
    }))
    .filter((group) => group.items.length > 0);
}

/**
 * Why a candidate has no size, as a translation key.
 *
 * Two distinct causes, and conflating them would leave the user with no idea
 * whether elevating would help. `null` means the size is present and there is
 * nothing to explain.
 */
export function unmeasuredReason(
  candidate: CleanupCandidate,
): 'needsElevation' | 'unreadable' | null {
  if (candidate.size !== null) return null;
  return candidate.needsElevation ? 'needsElevation' : 'unreadable';
}

/**
 * Whether the scan's figures should be qualified in the UI.
 *
 * True when the walk was cancelled, when any directory was skipped, or when
 * the cluster size could not be read — the last one disables rounding
 * entirely, so the totals are a slight under-count rather than exact.
 */
export function needsQualifier(snapshot: ScanSnapshot): boolean {
  return !snapshot.complete || snapshot.clusterBytes === null;
}
