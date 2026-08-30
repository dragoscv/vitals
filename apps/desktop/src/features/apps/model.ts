/**
 * Installed applications: the data shape, sorting, and what the size means.
 *
 * # `estimatedSize` is advisory, and the UI must say so
 *
 * `EstimatedSize` is whatever the installer chose to write into the registry
 * at install time. It is frequently absent, frequently wrong, and Windows
 * never recomputes it — an application that has since downloaded 40 GB of
 * game data still reports its original figure.
 *
 * Sorting by it is still useful, because the biggest offenders usually did
 * declare a size. But presenting it as a measurement would be false: the real
 * disk footprint needs a directory walk, which is what the Storage feature is
 * for. So the size column is labelled as reported-by-installer, and apps with
 * no figure sort last rather than as zero — zero would rank a 10 GB product
 * that omitted the value below a 2 MB utility that supplied one.
 */

export type AppSourceKey = 'machineNative' | 'machineWow64' | 'userNative' | 'userWow64';

export type RejectReasonKey =
  'noDisplayName' | 'systemComponent' | 'updateOrHotfix' | 'childOfAnotherEntry' | 'orphanPatch';

export interface InstalledApp {
  /** Registry subkey name. Stable across runs, so it works as a React key. */
  readonly keyName: string;
  readonly name: string;
  readonly publisher: string | null;
  readonly version: string | null;
  /** ISO `YYYY-MM-DD`, or null. Formatted per locale at render time. */
  readonly installDate: string | null;
  readonly installLocation: string | null;
  /** As the installer reported it. Advisory — see the module note. */
  readonly estimatedSize: number | null;
  readonly uninstallString: string | null;
  readonly quietUninstallString: string | null;
  readonly isMsi: boolean;
  readonly perUser: boolean;
  readonly source: AppSourceKey;
}

export interface Rejection {
  readonly reason: RejectReasonKey;
  readonly count: number;
}

export interface AppsSnapshot {
  readonly apps: readonly InstalledApp[];
  readonly examined: number;
  readonly rejected: number;
  readonly rejectedByReason: readonly Rejection[];
  readonly duplicatesCollapsed: number;
}

export const appSorts = ['name', 'size', 'date', 'publisher'] as const;
export type AppSort = (typeof appSorts)[number];

/**
 * Whether an uninstaller can be launched at all.
 *
 * A surprising number of entries publish no `UninstallString` — Store apps,
 * some MSI child products, entries left behind by a failed removal. The
 * button is disabled for those rather than hidden, so the user can see that
 * the option exists and that this particular product does not offer it.
 */
export function canUninstall(app: InstalledApp): boolean {
  return app.uninstallString !== null && app.uninstallString.trim() !== '';
}

/**
 * Sorts, with absent values always last.
 *
 * The rule matters most for size: treating a missing figure as zero ranks a
 * 10 GB product that omitted it below a 2 MB utility that supplied one, which
 * inverts the exact ordering the user asked for.
 */
export function sortApps(
  apps: readonly InstalledApp[],
  sort: AppSort,
  locale: string,
): readonly InstalledApp[] {
  const byName = (a: InstalledApp, b: InstalledApp) => a.name.localeCompare(b.name, locale);

  return [...apps].sort((a, b) => {
    switch (sort) {
      case 'name':
        return byName(a, b);

      case 'size': {
        // Descending: "what is taking up space" is the question, so the
        // biggest belongs at the top.
        if (a.estimatedSize === null && b.estimatedSize === null) return byName(a, b);
        if (a.estimatedSize === null) return 1;
        if (b.estimatedSize === null) return -1;
        return b.estimatedSize !== a.estimatedSize
          ? b.estimatedSize - a.estimatedSize
          : byName(a, b);
      }

      case 'date': {
        // Newest first: recently installed is what someone looks for when
        // something started misbehaving.
        if (a.installDate === null && b.installDate === null) return byName(a, b);
        if (a.installDate === null) return 1;
        if (b.installDate === null) return -1;
        // ISO dates compare correctly as strings, which is why the Rust side
        // normalises the registry's `YYYYMMDD` rather than passing it raw.
        return a.installDate === b.installDate
          ? byName(a, b)
          : b.installDate.localeCompare(a.installDate);
      }

      case 'publisher': {
        const left = a.publisher ?? '';
        const right = b.publisher ?? '';
        if (left === '' && right === '') return byName(a, b);
        if (left === '') return 1;
        if (right === '') return -1;
        return left === right ? byName(a, b) : left.localeCompare(right, locale);
      }

      default: {
        const exhaustive: never = sort;
        return exhaustive;
      }
    }
  });
}

export function filterApps(apps: readonly InstalledApp[], query: string): readonly InstalledApp[] {
  const needle = query.trim().toLowerCase();
  if (needle === '') return apps;

  return apps.filter(
    (app) =>
      app.name.toLowerCase().includes(needle) ||
      app.publisher?.toLowerCase().includes(needle) === true ||
      app.version?.toLowerCase().includes(needle) === true ||
      app.installLocation?.toLowerCase().includes(needle) === true,
  );
}

/**
 * Total declared size across the list.
 *
 * Explicitly a sum of *declared* sizes, not of disk usage, and the UI must
 * label it that way. Apps with no figure contribute nothing, so this is a
 * floor — presenting it as "total space used by applications" would be
 * wrong by however much the silent ones occupy.
 */
export function declaredTotal(apps: readonly InstalledApp[]): {
  readonly bytes: number;
  readonly withSize: number;
  readonly withoutSize: number;
} {
  let bytes = 0;
  let withSize = 0;
  let withoutSize = 0;

  for (const app of apps) {
    if (app.estimatedSize === null) withoutSize += 1;
    else {
      bytes += app.estimatedSize;
      withSize += 1;
    }
  }

  return { bytes, withSize, withoutSize };
}
