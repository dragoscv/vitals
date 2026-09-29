import { describe, expect, it } from 'vitest';

import {
  addToBasket,
  basketTotal,
  filterDirectories,
  groupBySafety,
  inBasket,
  leafName,
  measuredFreed,
  needsQualifier,
  reclaimableTotal,
  removeFromBasket,
  sortDirectories,
  sortVolumes,
  unmeasuredReason,
  usedBytes,
  usedPercent,
  type BasketItem,
  type CleanupCandidate,
  type DirectoryEntry,
  type ScanSnapshot,
  type Volume,
} from './model';

const GB = 1024 ** 3;

function item(path: string, allocated = GB): BasketItem {
  return { path, name: leafName(path), kind: 'folder', allocated };
}

describe('the review basket', () => {
  it('sums what is in it', () => {
    expect(basketTotal([item('C:\\a', GB), item('C:\\b', 2 * GB)])).toBe(3 * GB);
    expect(basketTotal([])).toBe(0);
  });

  it('adding a folder absorbs what is already in it, so nothing counts twice', () => {
    const basket = addToBasket(
      [item('C:\\Users\\me\\Videos\\a.mp4'), item('C:\\Users\\me\\Other')],
      item('C:\\Users\\me\\Videos'),
    );
    expect(basket.map((b) => b.path)).toEqual(['C:\\Users\\me\\Other', 'C:\\Users\\me\\Videos']);
  });

  it('adding something inside a folder already there changes nothing', () => {
    const start = [item('D:\\Games')];
    expect(addToBasket(start, item('d:\\games\\old'))).toBe(start);
    expect(addToBasket(start, item('D:\\GAMES'))).toBe(start);
  });

  it('a sibling with the same prefix is not inside', () => {
    expect(addToBasket([item('D:\\Games')], item('D:\\Games2'))).toHaveLength(2);
  });

  it('matches paths case-insensitively, as Windows does', () => {
    const basket = [item('C:\\Temp\\X')];
    expect(inBasket(basket, 'c:\\temp\\x')).toBe(true);
    expect(removeFromBasket(basket, 'C:\\TEMP\\X')).toEqual([]);
  });

  it('names the last component, including under a drive root', () => {
    expect(leafName('C:\\Users\\me\\big.iso')).toBe('big.iso');
    expect(leafName('D:\\Movies\\')).toBe('Movies');
  });
});

function volume(overrides: Partial<Volume> = {}): Volume {
  return {
    mount: 'C:\\',
    label: 'System',
    fileSystem: 'NTFS',
    kind: 'nvme',
    total: 100 * GB,
    available: 25 * GB,
    strategy: 'directoryWalk',
    ...overrides,
  };
}

function entry(overrides: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return {
    path: 'C:\\Windows',
    allocated: 10 * GB,
    logical: 9 * GB,
    files: 100,
    incomplete: null,
    ...overrides,
  };
}

function candidate(overrides: Partial<CleanupCandidate> = {}): CleanupCandidate {
  return {
    path: 'C:\\Temp',
    kind: 'userTemp',
    size: GB,
    safety: 'safe',
    allowsOneClick: true,
    tool: null,
    irreversible: false,
    needsElevation: false,
    ...overrides,
  };
}

function snapshot(overrides: Partial<ScanSnapshot> = {}): ScanSnapshot {
  return {
    root: 'C:\\',
    largest: [entry()],
    allocated: 10 * GB,
    logical: 9 * GB,
    clusterBytes: 4096,
    filesScanned: 100,
    directoriesScanned: 10,
    hardLinkDuplicates: 0,
    hardLinkBytesSaved: 0,
    cancelled: false,
    complete: true,
    elapsedMs: 1200,
    skipped: [],
    skippedTotal: 0,
    linksNotFollowed: 0,
    scanId: 1,
    rootNode: 0,
    largestFiles: [],
    ...overrides,
  };
}

describe('volume capacity', () => {
  it('reports the used fraction', () => {
    expect(usedPercent(volume())).toBe(75);
    expect(usedBytes(volume())).toBe(75 * GB);
  });

  it('reports unknown rather than zero when capacity is not available', () => {
    // A card reader with no card, or a mount that refused GetDiskFreeSpaceEx.
    // Dividing by zero yields NaN, and a meter drawn from NaN reads as a
    // confident 0% — an empty-looking bar asserting a fact nothing supports.
    expect(usedPercent(volume({ total: 0, available: 0 }))).toBeNull();
  });

  it('never reports more than full, even if free space exceeds capacity', () => {
    // Quota-backed and network volumes can report available > total.
    expect(usedPercent(volume({ total: GB, available: 2 * GB }))).toBe(0);
    expect(usedBytes(volume({ total: GB, available: 2 * GB }))).toBe(0);
  });

  it('orders volumes by mount point, not by size', () => {
    // Ordering by capacity would move the system drive around every time a
    // USB stick is plugged in.
    const sorted = sortVolumes([
      volume({ mount: 'D:\\', total: 4000 * GB }),
      volume({ mount: 'C:\\' }),
    ]);
    expect(sorted.map((item) => item.mount)).toEqual(['C:\\', 'D:\\']);
  });
});

describe('sortDirectories', () => {
  const small = entry({ path: 'B', allocated: GB, logical: 5 * GB, files: 1 });
  const large = entry({ path: 'A', allocated: 5 * GB, logical: GB, files: 50 });

  it('puts the biggest on-disk folder first', () => {
    expect(sortDirectories([small, large], 'allocated', 'en').map((e) => e.path)).toEqual([
      'A',
      'B',
    ]);
  });

  it('sorts by logical size independently of allocated size', () => {
    // The two genuinely disagree — cluster rounding and hard-link dedup pull
    // in opposite directions — so a shared comparator would be wrong for one.
    expect(sortDirectories([large, small], 'logical', 'en').map((e) => e.path)).toEqual(['B', 'A']);
  });

  it('sorts by file count', () => {
    expect(sortDirectories([small, large], 'files', 'en').map((e) => e.path)).toEqual(['A', 'B']);
  });

  it('sorts by path', () => {
    expect(sortDirectories([small, large], 'path', 'en').map((e) => e.path)).toEqual(['A', 'B']);
  });

  it('breaks ties by path so the order is stable between renders', () => {
    const a = entry({ path: 'zeta', allocated: GB });
    const b = entry({ path: 'alpha', allocated: GB });
    expect(sortDirectories([a, b], 'allocated', 'en').map((e) => e.path)).toEqual([
      'alpha',
      'zeta',
    ]);
  });

  it('does not mutate the input', () => {
    const input = [small, large];
    sortDirectories(input, 'allocated', 'en');
    expect(input.map((e) => e.path)).toEqual(['B', 'A']);
  });
});

describe('filterDirectories', () => {
  it('matches a path fragment case-insensitively', () => {
    const rows = [entry({ path: 'C:\\Windows\\WinSxS' }), entry({ path: 'C:\\Users' })];
    expect(filterDirectories(rows, 'winsxs').map((e) => e.path)).toEqual(['C:\\Windows\\WinSxS']);
  });

  it('returns everything for an empty or whitespace query', () => {
    const rows = [entry({ path: 'A' }), entry({ path: 'B' })];
    expect(filterDirectories(rows, '   ')).toHaveLength(2);
  });
});

describe('reclaimableTotal', () => {
  it('sums the measured candidates', () => {
    const total = reclaimableTotal([candidate({ size: GB }), candidate({ size: 2 * GB })]);
    expect(total.bytes).toBe(3 * GB);
    expect(total.measured).toBe(2);
    expect(total.unmeasured).toBe(0);
  });

  it('counts an unmeasured candidate separately instead of adding zero', () => {
    // This is the whole point. Folding an unmeasurable 8 GB Windows Update
    // cache in as zero is how a cleanup tool talks someone out of reclaiming
    // it: the headline reads as complete when it is a floor.
    const total = reclaimableTotal([candidate({ size: GB }), candidate({ size: null })]);
    expect(total.bytes).toBe(GB);
    expect(total.measured).toBe(1);
    expect(total.unmeasured).toBe(1);
  });

  it('excludes risky candidates from the headline by default', () => {
    // A 34 GB hibernation file is worth SHOWING, but it is not something the
    // user can simply free — deleting hiberfil.sys by hand breaks fast
    // startup. Including it in "you could free this much" would be a promise.
    const total = reclaimableTotal([
      candidate({ safety: 'safe', size: GB }),
      candidate({ safety: 'review', size: 2 * GB }),
      candidate({ safety: 'risky', size: 34 * GB }),
    ]);
    expect(total.bytes).toBe(3 * GB);
  });

  it('includes risky candidates when explicitly asked', () => {
    const total = reclaimableTotal([candidate({ safety: 'risky', size: 34 * GB })], 'risky');
    expect(total.bytes).toBe(34 * GB);
  });

  it('counts only safe items when the tier is safe', () => {
    const total = reclaimableTotal(
      [candidate({ safety: 'safe', size: GB }), candidate({ safety: 'review', size: 9 * GB })],
      'safe',
    );
    expect(total.bytes).toBe(GB);
  });

  it('does not count an unmeasured risky item as unmeasured in the safe total', () => {
    // The count must describe the same scope as the bytes, or the caption
    // ("N could not be measured, so the real figure is higher") refers to
    // items the figure never intended to include.
    const total = reclaimableTotal([candidate({ safety: 'risky', size: null })], 'review');
    expect(total.unmeasured).toBe(0);
  });
});

describe('groupBySafety', () => {
  it('orders tiers safest first and drops empty ones', () => {
    const groups = groupBySafety([
      candidate({ path: 'r', safety: 'risky' }),
      candidate({ path: 's', safety: 'safe' }),
    ]);
    expect(groups.map((g) => g.safety)).toEqual(['safe', 'risky']);
    expect(groups[0]?.items).toHaveLength(1);
  });

  it('returns nothing for an empty list', () => {
    expect(groupBySafety([])).toHaveLength(0);
  });
});

describe('unmeasuredReason', () => {
  it('says nothing when the size is known', () => {
    expect(unmeasuredReason(candidate({ size: GB }))).toBeNull();
  });

  it('distinguishes an elevation problem from an unreadable location', () => {
    // Conflating the two leaves the user with no idea whether running as
    // admin would help, which is the only action available to them.
    expect(unmeasuredReason(candidate({ size: null, needsElevation: true }))).toBe(
      'needsElevation',
    );
    expect(unmeasuredReason(candidate({ size: null, needsElevation: false }))).toBe('unreadable');
  });
});

describe('needsQualifier', () => {
  it('is false for a clean, complete scan', () => {
    expect(needsQualifier(snapshot())).toBe(false);
  });

  it('is true when the scan was incomplete', () => {
    expect(needsQualifier(snapshot({ complete: false }))).toBe(true);
  });

  it('is true when the cluster size could not be read', () => {
    // No cluster size means no rounding, so the totals are an under-count.
    // Presenting them as exact would be a small lie repeated on every row.
    expect(needsQualifier(snapshot({ clusterBytes: null }))).toBe(true);
  });
});

describe('measuredFreed', () => {
  const base = {
    path: 'C:\\x',
    outcome: 'done',
    code: null,
    locationBefore: 10,
    locationAfter: 4,
    locationFreed: 6,
    driveFreed: 9,
    elapsedMs: 1,
  } as const;

  it('prefers what the location lost over what the drive gained', () => {
    expect(measuredFreed(base)).toEqual({ bytes: 6, from: 'location' });
    expect(measuredFreed({ ...base, locationFreed: null })).toEqual({ bytes: 9, from: 'drive' });
  });

  it('falls back to the drive when the location did not shrink', () => {
    // Update Cleanup trims the component store, not the folder shown.
    expect(measuredFreed({ ...base, locationFreed: 0 })).toEqual({ bytes: 9, from: 'drive' });
    expect(measuredFreed({ ...base, locationFreed: 0, driveFreed: null })).toEqual({
      bytes: 0,
      from: 'location',
    });
  });

  it('is unknown, not zero, when neither was measured', () => {
    expect(measuredFreed({ ...base, locationFreed: null, driveFreed: null })).toBeNull();
  });

  it('never reports a negative figure as space freed', () => {
    expect(measuredFreed({ ...base, locationFreed: -5, driveFreed: -3 })).toEqual({
      bytes: 0,
      from: 'drive',
    });
  });
});
