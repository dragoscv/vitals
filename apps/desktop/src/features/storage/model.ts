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
  | 'deliveryOptimisation';

export interface Volume {
  /** Mount point, e.g. `C:\`. Also the scan root and the React key. */
  readonly mount: string;
  readonly label: string | null;
  readonly fileSystem: string | null;
  readonly kind: DiskKindKey;
  readonly total: number;
  readonly available: number;
  readonly strategy: ScanStrategyKey;
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
  readonly filesSeen: number;
  readonly directoriesSeen: number;
  readonly bytesSeen: number;
  readonly elapsedMs: number;
  /** The folder most recently read. */
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
}

export interface CleanupCandidate {
  readonly path: string;
  readonly kind: CleanupKindKey;
  /** `null` when the location exists but could not be measured. */
  readonly size: number | null;
  readonly safety: SafetyKey;
  readonly allowsOneClick: boolean;
  readonly needsElevation: boolean;
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
