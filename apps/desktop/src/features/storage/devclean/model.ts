/**
 * Developer cleanup: the wire contract with `scan_dev_cleanup` /
 * `run_dev_cleanup` and the pure helpers the screen needs.
 *
 * # The UI names items, never paths
 *
 * Every actionable row carries an `id` minted by the backend for the scan it
 * came from. `run_dev_cleanup` takes ids only and re-checks each against the
 * kept scan, so the webview cannot ask for an arbitrary folder to be removed.
 *
 * # Unmeasured is `null`, never `0`
 *
 * A size that could not be read is `null` and renders as "Not measured". A
 * freed figure is what was measured after the action; when it could not be
 * measured it is `null`, not `0`.
 */

export type ArtefactKindKey =
  | 'nodeModules'
  | 'cargoTarget'
  | 'next'
  | 'turbo'
  | 'dist'
  | 'build'
  | 'gradle'
  | 'pycache'
  | 'venv'
  | 'pytestCache'
  | 'parcelCache'
  | 'svelteKit'
  | 'nuxt'
  | 'coverage'
  | 'expo';

export const artefactKinds: readonly ArtefactKindKey[] = [
  'nodeModules',
  'cargoTarget',
  'next',
  'turbo',
  'dist',
  'build',
  'gradle',
  'pycache',
  'venv',
  'pytestCache',
  'parcelCache',
  'svelteKit',
  'nuxt',
  'coverage',
  'expo',
];

/** A regenerable build output folder inside a project. */
export interface Artefact {
  readonly id: string;
  readonly path: string;
  readonly kind: ArtefactKindKey;
  /** Allocated bytes; `null` when the folder could not be read. */
  readonly size: number | null;
  readonly files: number | null;
  /**
   * pnpm layout: most files are hard links into the pnpm store, so removing
   * the folder frees little until the store is pruned.
   */
  readonly sharedWithPnpmStore: boolean;
  /** The command that brings it back, shown verbatim (`pnpm install`). */
  readonly restore: string;
  /** Ticked after the scan: the project has been idle 30+ days. */
  readonly preselected: boolean;
}

export interface Project {
  readonly path: string;
  readonly name: string;
  readonly isGit: boolean;
  /**
   * Days since anything happened here: the later of the last commit, the
   * last reflog entry and the newest top-level file. `null` when unknown.
   */
  readonly idleDays: number | null;
  readonly artefacts: readonly Artefact[];
}

export type WorktreeStateKey =
  'removable' | 'prunable' | 'dirty' | 'unpushed' | 'active' | 'locked';

export const worktreeStates: readonly WorktreeStateKey[] = [
  'removable',
  'prunable',
  'dirty',
  'unpushed',
  'active',
  'locked',
];

/**
 * A linked git worktree. Only `removable` (clean, pushed, idle) and
 * `prunable` (its folder is already gone) carry an `id`; the rest are shown
 * with the reason they are kept.
 */
export interface Worktree {
  readonly id: string | null;
  readonly path: string;
  readonly repo: string;
  readonly branch: string | null;
  readonly size: number | null;
  readonly state: WorktreeStateKey;
  /** Tracked + untracked changes, for `dirty`. */
  readonly changes: number | null;
  readonly idleHours: number | null;
}

export type CacheKindKey =
  | 'pnpm'
  | 'npm'
  | 'yarn'
  | 'cargo'
  | 'gradle'
  | 'nuget'
  | 'pip'
  | 'uv'
  | 'go'
  | 'playwright'
  | 'electron';

export const cacheKinds: readonly CacheKindKey[] = [
  'pnpm',
  'npm',
  'yarn',
  'cargo',
  'gradle',
  'nuget',
  'pip',
  'uv',
  'go',
  'playwright',
  'electron',
];

/** A package manager cache, freed by its own tool where one exists. */
export interface PackageCache {
  /** `null` when the tool is not installed, so nothing can be run. */
  readonly id: string | null;
  readonly kind: CacheKindKey;
  readonly path: string;
  readonly size: number | null;
  /** `command`: the tool's own clean command; `delete`: no tool, regenerable. */
  readonly method: 'command' | 'delete';
  /** Shown verbatim, e.g. `pnpm store prune`. */
  readonly command: string;
  readonly restore: string;
}

export type DockerItemKindKey =
  'danglingImages' | 'unusedImages' | 'buildCache' | 'stoppedContainers' | 'volumes';

export const dockerItemKinds: readonly DockerItemKindKey[] = [
  'danglingImages',
  'unusedImages',
  'buildCache',
  'stoppedContainers',
  'volumes',
];

export interface DockerItem {
  /** `null` for volumes: they hold data and are never offered. */
  readonly id: string | null;
  readonly kind: DockerItemKindKey;
  readonly count: number | null;
  /** What Docker says could be reclaimed. */
  readonly reclaimable: number | null;
  readonly command: string;
}

export type DockerStateKey = 'ok' | 'notInstalled' | 'notRunning';

export interface Docker {
  readonly state: DockerStateKey;
  readonly items: readonly DockerItem[];
}

/** A WSL or Docker virtual disk that only grows until it is compacted. */
export interface VirtualDisk {
  readonly id: string;
  readonly path: string;
  readonly kind: 'docker' | 'wsl';
  readonly distro: string | null;
  /** File size on the host. */
  readonly size: number;
}

export interface DevScan {
  readonly scanId: number;
  readonly roots: readonly string[];
  readonly projects: readonly Project[];
  readonly worktrees: readonly Worktree[];
  readonly caches: readonly PackageCache[];
  readonly docker: Docker;
  readonly vdisks: readonly VirtualDisk[];
  readonly elapsedMs: number;
}

export type DevScanPhaseKey = 'projects' | 'sizing' | 'worktrees' | 'caches' | 'docker' | 'vdisks';

export const devScanPhases: readonly DevScanPhaseKey[] = [
  'projects',
  'sizing',
  'worktrees',
  'caches',
  'docker',
  'vdisks',
];

/** `vitals://storage/dev-scan-progress`. */
export interface DevScanProgress {
  readonly phase: DevScanPhaseKey;
  readonly found: number;
  readonly currentPath: string | null;
}

/** `vitals://storage/dev-clean-progress`. */
export interface DevCleanProgress {
  readonly id: string;
  readonly index: number;
  readonly total: number;
}

export type DevOutcomeKey = 'done' | 'partial' | 'failed' | 'refused' | 'unavailable';

export const devOutcomes: readonly DevOutcomeKey[] = [
  'done',
  'partial',
  'failed',
  'refused',
  'unavailable',
];

export interface DevCleanItem {
  readonly id: string;
  readonly path: string;
  readonly outcome: DevOutcomeKey;
  /** Measured: size before minus size after. `null` when unmeasurable. */
  readonly freed: number | null;
  /** What the tool itself reported (Docker's "Total reclaimed space"). */
  readonly toolFreed: number | null;
  /** The tool's or Windows' own words, for `failed` / `partial`. */
  readonly message: string | null;
  /** For `partial` / `failed` on a folder: who holds what was left. */
  readonly holders: readonly { readonly pid: number; readonly name: string }[] | null;
}

export interface DevCleanReport {
  readonly items: readonly DevCleanItem[];
  /** Free space each touched drive gained, measured before and after. */
  readonly drives: readonly { readonly drive: string; readonly freed: number }[];
}

/** Everything a selection can hold, flattened, for totals and the confirm list. */
export interface Selectable {
  readonly id: string;
  readonly path: string;
  readonly label: string;
  readonly size: number | null;
  /** The permanent-removal kinds need the "cannot be undone" tick. */
  readonly permanent: boolean;
  readonly restore: string | null;
}

/** Ids pre-ticked after a scan: artefacts of projects idle 30+ days. */
export function preselectedIds(scan: DevScan): ReadonlySet<string> {
  const ids = new Set<string>();
  for (const project of scan.projects) {
    for (const artefact of project.artefacts) if (artefact.preselected) ids.add(artefact.id);
  }
  return ids;
}

/** Sum of the selected sizes that were measured, and how many were not. */
export function selectionTotal(
  items: readonly Selectable[],
  selected: ReadonlySet<string>,
): { readonly bytes: number; readonly unmeasured: number; readonly count: number } {
  let bytes = 0;
  let unmeasured = 0;
  let count = 0;
  for (const item of items) {
    if (!selected.has(item.id)) continue;
    count += 1;
    if (item.size === null) unmeasured += 1;
    else bytes += item.size;
  }
  return { bytes, unmeasured, count };
}

/** A project's measured artefact bytes. */
export function projectTotal(project: Project): number {
  return project.artefacts.reduce((sum, a) => sum + (a.size ?? 0), 0);
}

/** Projects largest first, then by path; stable for equal sizes. */
export function sortProjects(projects: readonly Project[]): readonly Project[] {
  return [...projects].sort(
    (a, b) => projectTotal(b) - projectTotal(a) || a.path.localeCompare(b.path),
  );
}

/** Every item that can be ticked, in display order. */
export function selectables(
  scan: DevScan,
  label: (kind: string, path: string) => string,
): readonly Selectable[] {
  const out: Selectable[] = [];
  for (const project of scan.projects) {
    for (const a of project.artefacts) {
      out.push({
        id: a.id,
        path: a.path,
        label: label(a.kind, a.path),
        size: a.size,
        permanent: true,
        restore: a.restore,
      });
    }
  }
  for (const w of scan.worktrees) {
    if (w.id === null) continue;
    out.push({
      id: w.id,
      path: w.path,
      label: label(`worktree:${w.state}`, w.path),
      size: w.state === 'prunable' ? 0 : w.size,
      permanent: w.state === 'removable',
      restore: null,
    });
  }
  for (const c of scan.caches) {
    if (c.id === null) continue;
    out.push({
      id: c.id,
      path: c.path,
      label: label(`cache:${c.kind}`, c.path),
      size: c.size,
      permanent: c.method === 'delete',
      restore: c.restore,
    });
  }
  for (const d of scan.docker.items) {
    if (d.id === null) continue;
    out.push({
      id: d.id,
      path: d.command,
      label: label(`docker:${d.kind}`, d.command),
      size: d.reclaimable,
      permanent: true,
      restore: null,
    });
  }
  for (const v of scan.vdisks) {
    out.push({
      id: v.id,
      path: v.path,
      label: label(`vdisk:${v.kind}`, v.path),
      size: null,
      permanent: false,
      restore: null,
    });
  }
  return out;
}

/** Items the selection holds that stop WSL and Docker while they run. */
export function stopsWsl(scan: DevScan, selected: ReadonlySet<string>): boolean {
  return scan.vdisks.some((v) => selected.has(v.id));
}

/** The idle threshold the backend pre-ticks at, and the screen filters by. */
export const STALE_DAYS = 30;

/**
 * Below this a project reads as "Active" rather than "Idle 3 days": a
 * project touched this week is being worked on, and saying how many hours
 * ago is noise.
 */
export const ACTIVE_DAYS = 7;

export type IdleKey = 'active' | 'idle' | 'unknown';

export function idleKey(project: Project): IdleKey {
  if (project.idleDays === null) return 'unknown';
  return project.idleDays < ACTIVE_DAYS ? 'active' : 'idle';
}

/** Idle 30+ days. Unknown is not stale: nothing proves it was left alone. */
export function isStale(project: Project): boolean {
  return project.idleDays !== null && project.idleDays >= STALE_DAYS;
}

/**
 * Measured bytes and how many figures were missing.
 *
 * Separate from `projectTotal` because a project whose every folder was
 * unreadable must render "Not measured", and a bare sum renders "0 B".
 */
export function measure(sizes: readonly (number | null)[]): {
  readonly bytes: number;
  readonly unmeasured: number;
  readonly count: number;
} {
  let bytes = 0;
  let unmeasured = 0;
  for (const size of sizes) {
    if (size === null) unmeasured += 1;
    else bytes += size;
  }
  return { bytes, unmeasured, count: sizes.length };
}

/**
 * The translation key (inside the storage namespace) for a `selectables`
 * label kind: `nodeModules`, `worktree:removable`, `cache:pnpm`,
 * `docker:buildCache`, `vdisk:wsl`.
 */
export function selectableLabelKey(kind: string): string {
  const colon = kind.indexOf(':');
  if (colon < 0) return `dev.artefact.${kind}`;
  const group = kind.slice(0, colon);
  const key = kind.slice(colon + 1);
  switch (group) {
    case 'worktree':
      return `dev.worktreeState.${key}`;
    case 'cache':
      return `dev.cache.${key}`;
    case 'docker':
      return `dev.docker.${key}`;
    default:
      return `dev.vdisk.${key}`;
  }
}

export type DevSectionKey = 'projects' | 'worktrees' | 'caches' | 'docker' | 'vdisks';

export const devSections: readonly DevSectionKey[] = [
  'projects',
  'worktrees',
  'caches',
  'docker',
  'vdisks',
];

/** The selectable ids of each section, for per-section select all / none. */
export function sectionIds(
  scan: DevScan,
  projects: readonly Project[] = scan.projects,
): Readonly<Record<DevSectionKey, readonly string[]>> {
  const ids = (values: readonly (string | null)[]) =>
    values.filter((id): id is string => id !== null);
  return {
    projects: projects.flatMap((p) => p.artefacts.map((a) => a.id)),
    worktrees: ids(scan.worktrees.map((w) => w.id)),
    caches: ids(scan.caches.map((c) => c.id)),
    // Volumes hold databases: never selectable, even if one arrived with an id.
    docker: ids(scan.docker.items.map((d) => (d.kind === 'volumes' ? null : d.id))),
    vdisks: scan.vdisks.map((v) => v.id),
  };
}

/** A project's box: ticked, empty, or mixed when only some folders are. */
export function projectCheckState(
  project: Project,
  selected: ReadonlySet<string>,
): boolean | 'indeterminate' {
  const ticked = project.artefacts.filter((a) => selected.has(a.id)).length;
  if (ticked === 0) return false;
  return ticked === project.artefacts.length ? true : 'indeterminate';
}

/**
 * The scan with the given ids taken out: what a clean-up finished leaves
 * the list, without a rescan. A project with no folders left goes with them.
 */
export function withoutIds(scan: DevScan, ids: ReadonlySet<string>): DevScan {
  const keep = (id: string | null) => id === null || !ids.has(id);
  return {
    ...scan,
    projects: scan.projects
      .map((p) => ({ ...p, artefacts: p.artefacts.filter((a) => keep(a.id)) }))
      .filter((p) => p.artefacts.length > 0),
    worktrees: scan.worktrees.filter((w) => keep(w.id)),
    caches: scan.caches.filter((c) => keep(c.id)),
    docker: { ...scan.docker, items: scan.docker.items.filter((d) => keep(d.id)) },
    vdisks: scan.vdisks.filter((v) => keep(v.id)),
  };
}
