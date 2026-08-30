/**
 * App history: data shape, sorting, and view-model transforms.
 *
 * # Data provenance
 *
 * This is Vitals' own accumulated history, not Windows' SRUM database. It
 * starts empty on a fresh install and builds from the moment Vitals first
 * runs. The UI must say so explicitly — presenting it as "what Task Manager
 * shows" would be misleading.
 */

export interface AppHistoryRecord {
  readonly executable: string;
  readonly name: string;
  readonly cpuSeconds: number;
  readonly diskReadBytes: number;
  readonly diskWriteBytes: number;
  readonly peakPrivateBytes: number;
  /**
   * Milliseconds since the Unix epoch.
   *
   * A number rather than a formatted string: formatting belongs to the view,
   * which is the only layer that knows the user's locale.
   */
  readonly firstSeen: number;
  /** Milliseconds since the Unix epoch. */
  readonly lastSeen: number;
  readonly sessions: number;
}

export interface AppHistorySnapshot {
  readonly records: readonly AppHistoryRecord[];
}

export const historySorts = ['cpu', 'disk', 'memory', 'lastSeen'] as const;
export type HistorySort = (typeof historySorts)[number];

/**
 * Sorts history records by the selected criterion.
 *
 * All sorts are descending: "what used the most" is the question for every
 * column. Names are the tiebreaker when values match.
 */
export function sortHistory(
  records: readonly AppHistoryRecord[],
  sort: HistorySort,
  locale: string,
): readonly AppHistoryRecord[] {
  const byName = (a: AppHistoryRecord, b: AppHistoryRecord) => a.name.localeCompare(b.name, locale);

  return [...records].sort((a, b) => {
    switch (sort) {
      case 'cpu':
        return b.cpuSeconds !== a.cpuSeconds ? b.cpuSeconds - a.cpuSeconds : byName(a, b);

      case 'disk': {
        const aTotal = a.diskReadBytes + a.diskWriteBytes;
        const bTotal = b.diskReadBytes + b.diskWriteBytes;
        return bTotal !== aTotal ? bTotal - aTotal : byName(a, b);
      }

      case 'memory':
        return b.peakPrivateBytes !== a.peakPrivateBytes
          ? b.peakPrivateBytes - a.peakPrivateBytes
          : byName(a, b);

      case 'lastSeen': {
        return b.lastSeen !== a.lastSeen ? b.lastSeen - a.lastSeen : byName(a, b);
      }
    }
  });
}

/**
 * Filters history records by a search query.
 *
 * Matches against the display name and the full executable path, case-
 * insensitive. An empty query matches everything.
 */
export function filterHistory(
  records: readonly AppHistoryRecord[],
  query: string,
): readonly AppHistoryRecord[] {
  if (query.trim() === '') return records;

  const lowerQuery = query.toLowerCase();
  return records.filter(
    (record) =>
      record.name.toLowerCase().includes(lowerQuery) ||
      record.executable.toLowerCase().includes(lowerQuery),
  );
}

/**
 * Totals for the summary bar.
 */
export interface HistoryTotals {
  readonly totalCpuSeconds: number;
  readonly totalDiskReadBytes: number;
  readonly totalDiskWriteBytes: number;
}

export function computeTotals(records: readonly AppHistoryRecord[]): HistoryTotals {
  return records.reduce(
    (acc, record) => ({
      totalCpuSeconds: acc.totalCpuSeconds + record.cpuSeconds,
      totalDiskReadBytes: acc.totalDiskReadBytes + record.diskReadBytes,
      totalDiskWriteBytes: acc.totalDiskWriteBytes + record.diskWriteBytes,
    }),
    { totalCpuSeconds: 0, totalDiskReadBytes: 0, totalDiskWriteBytes: 0 },
  );
}

/**
 * Formats CPU seconds as a human-readable duration.
 *
 * Examples: "1.2 s", "3 min 5 s", "2h 15 min", "48h 30 min".
 */
export function formatCpuTime(seconds: number, locale: string): string {
  if (seconds < 1) {
    return (
      new Intl.NumberFormat(locale, {
        style: 'decimal',
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      }).format(seconds) + ' s'
    );
  }

  if (seconds < 60) {
    return Math.floor(seconds) + ' s';
  }

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    const secs = Math.floor(seconds % 60);
    return secs > 0 ? `${minutes} min ${secs} s` : `${minutes} min`;
  }

  const hours = Math.floor(minutes / 60);
  const mins = Math.floor(minutes % 60);
  return mins > 0 ? `${hours}h ${mins} min` : `${hours}h`;
}
