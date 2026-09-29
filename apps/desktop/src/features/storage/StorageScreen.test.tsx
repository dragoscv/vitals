import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { StorageScreen } from './StorageScreen';
import type {
  CleanupCandidate,
  DirectoryEntry,
  MapCell,
  RecycleReport,
  ScanProgress,
  ScanSnapshot,
  StorageListing,
  StorageNode,
  Volume,
} from './model';
import { registerStorageStrings } from './strings';
import type { StorageSource } from './useStorage';

const GB = 1024 ** 3;

beforeAll(async () => {
  await initI18n();
  registerStorageStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  // View state lives in the hash; one test's view must not leak into the next.
  history.replaceState(null, '', location.pathname);
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
    path: 'C:\\Windows\\WinSxS',
    allocated: 10 * GB,
    logical: 9 * GB,
    files: 1000,
    incomplete: null,
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
    filesScanned: 1000,
    directoriesScanned: 40,
    hardLinkDuplicates: 0,
    hardLinkBytesSaved: 0,
    cancelled: false,
    complete: true,
    elapsedMs: 1500,
    skipped: [],
    skippedTotal: 0,
    linksNotFollowed: 0,
    scanId: 1,
    rootNode: 0,
    largestFiles: [],
    ...overrides,
  };
}

function node(overrides: Partial<StorageNode> = {}): StorageNode {
  return {
    node: 0,
    name: 'C:\\',
    path: 'C:\\',
    allocated: 10 * GB,
    logical: 9 * GB,
    files: 1000,
    ownAllocated: 0,
    ownFiles: 0,
    hasChildren: true,
    incomplete: null,
    ...overrides,
  };
}

const ROOT = node();
const USERS = node({ node: 1, name: 'Users', path: 'C:\\Users', allocated: 6 * GB });
const WINDOWS = node({ node: 2, name: 'Windows', path: 'C:\\Windows', allocated: 4 * GB });
const ME = node({
  node: 3,
  name: 'me',
  path: 'C:\\Users\\me',
  allocated: 6 * GB,
  hasChildren: false,
});

/** A two-level tree: C:\ -> Users -> me, and Windows. */
function listingFor(id: number): StorageListing {
  if (id === 1) return { ancestry: [ROOT, USERS], children: [ME] };
  if (id === 2) return { ancestry: [ROOT, WINDOWS], children: [] };
  return { ancestry: [ROOT], children: [USERS, WINDOWS] };
}

function cell(overrides: Partial<MapCell> = {}): MapCell {
  return {
    kind: 'directory',
    node: 1,
    depth: 1,
    x0: 0,
    y0: 0.2,
    x1: 0.6,
    y1: 0.4,
    allocated: 6 * GB,
    count: 10,
    openable: true,
    name: 'Users',
    incomplete: null,
    ...overrides,
  };
}

function candidate(overrides: Partial<CleanupCandidate> = {}): CleanupCandidate {
  return {
    path: 'C:\\Users\\me\\AppData\\Local\\Temp',
    kind: 'userTemp',
    size: 2 * GB,
    safety: 'safe',
    allowsOneClick: true,
    needsElevation: false,
    ...overrides,
  };
}

interface SourceOverrides {
  readonly volumes?: readonly Volume[];
  /** Replaces the whole reader, for the pending and rejecting cases. */
  readonly readVolumes?: StorageSource['volumes'];
  readonly scan?: StorageSource['scan'];
  readonly cleanup?: StorageSource['cleanup'];
  readonly children?: StorageSource['children'];
  readonly recycle?: StorageSource['recycle'];
}

type ProgressListener = Parameters<StorageSource['onProgress']>[0];

/** A source whose progress events a test can fire by hand. */
type TestSource = StorageSource & { emit: (progress: ScanProgress) => void };

function makeSource(overrides: SourceOverrides = {}): TestSource {
  const listeners = new Set<ProgressListener>();
  return {
    volumes:
      overrides.readVolumes ??
      vi.fn<StorageSource['volumes']>().mockResolvedValue(overrides.volumes ?? [volume()]),
    scan: overrides.scan ?? vi.fn<StorageSource['scan']>().mockResolvedValue(snapshot()),
    cancelScan: vi.fn<StorageSource['cancelScan']>().mockResolvedValue(undefined),
    cleanup: overrides.cleanup ?? vi.fn<StorageSource['cleanup']>().mockResolvedValue([]),
    cancelCleanup: vi.fn<StorageSource['cancelCleanup']>().mockResolvedValue(undefined),
    children:
      overrides.children ??
      vi.fn<StorageSource['children']>((_scan, id) => Promise.resolve(listingFor(id))),
    map: vi.fn<StorageSource['map']>().mockResolvedValue([cell()]),
    reveal: vi.fn<StorageSource['reveal']>().mockResolvedValue(undefined),
    recycle:
      overrides.recycle ??
      vi.fn<StorageSource['recycle']>((paths) =>
        Promise.resolve<RecycleReport>({
          items: paths.map((path) => ({
            path,
            outcome: 'recycled',
            protection: null,
            holders: null,
            code: null,
            freed: GB,
          })),
          scan: null,
        }),
      ),
    holders: vi.fn<StorageSource['holders']>().mockResolvedValue([]),
    onProgress: (listener) => {
      listeners.add(listener);
      return Promise.resolve(() => {
        listeners.delete(listener);
      });
    },
    emit: (progress) => {
      for (const listener of listeners) listener(progress);
    },
  };
}

function progress(overrides: Partial<ScanProgress> = {}): ScanProgress {
  return {
    root: 'C:\\',
    filesSeen: 120_000,
    directoriesSeen: 9_000,
    bytesSeen: 30 * GB,
    elapsedMs: 4_000,
    currentPath: 'C:\\Users\\me\\Documents',
    ...overrides,
  };
}

async function mount(source = makeSource()) {
  render(<StorageScreen source={source} />);
  await screen.findByRole('heading', { name: 'Storage' });
  return source;
}

async function scanned(data = snapshot()) {
  const source = makeSource({ scan: vi.fn<StorageSource['scan']>().mockResolvedValue(data) });
  await mount(source);
  fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
  await screen.findByRole('heading', { name: 'Explore' });
  return source;
}

/** Scans, then switches to the flat largest-folders table. */
async function tabled(data = snapshot()) {
  const source = await scanned(data);
  fireEvent.click(screen.getByRole('radio', { name: 'Largest folders' }));
  return source;
}

describe('StorageScreen', () => {
  it('shows skeletons until the volume read settles', () => {
    const source = makeSource({
      readVolumes: vi.fn<StorageSource['volumes']>(() => new Promise(() => undefined)),
    });
    render(<StorageScreen source={source} />);
    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('reaches a terminal state when the volume read fails', async () => {
    // The recurring failure class in this project: a pending flag with no
    // resolution makes a broken app look busy, so nobody reports it.
    const source = makeSource({
      readVolumes: vi
        .fn<StorageSource['volumes']>()
        .mockRejectedValue(new Error('volume enumeration failed')),
    });

    render(<StorageScreen source={source} />);

    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('lists volumes with their free space', async () => {
    await mount();
    expect(screen.getByText('C:\\ System')).toBeTruthy();
    expect(screen.getByText(/25.0 GB free/)).toBeTruthy();
  });

  it('says a capacity is unreported rather than drawing an empty meter', async () => {
    await mount(makeSource({ volumes: [volume({ total: 0, available: 0 })] }));
    expect(screen.getByText('Capacity not reported')).toBeTruthy();
    expect(screen.queryByRole('meter')).toBeNull();
  });

  it('marks a drive that supports fast discovery', async () => {
    await mount(makeSource({ volumes: [volume({ strategy: 'mftAssisted' })] }));
    expect(screen.getByText('Fast discovery available')).toBeTruthy();
  });

  it('invites a scan before one has run, without a spinner', async () => {
    await mount();
    expect(screen.getByText('Nothing scanned yet')).toBeTruthy();
    // No progress bar: an idle screen must not look busy. (`role="status"`
    // is unusable as the assertion here — EmptyState itself carries one.)
    expect(screen.queryByRole('progressbar')).toBeNull();
  });

  describe('scanning', () => {
    it('reports progress and offers cancel while running', async () => {
      const source = makeSource({
        scan: vi.fn<StorageSource['scan']>(() => new Promise(() => undefined)),
      });
      await mount(source);

      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));

      expect(await screen.findByText(/Scanning C:\\/)).toBeTruthy();
      expect(screen.getByRole('button', { name: /Stop the scan/ })).toBeTruthy();
    });

    it('shows files, bytes and a real fraction of the drive as the scan climbs', async () => {
      // The drive fixture has 75 GB used; 30 GB seen is 40 %.
      const source = makeSource({
        scan: vi.fn<StorageSource['scan']>(() => new Promise(() => undefined)),
      });
      await mount(source);
      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      expect(await screen.findByText('Starting…')).toBeTruthy();

      act(() => {
        source.emit(progress());
      });

      const bar = await screen.findByRole('progressbar');
      expect(bar.getAttribute('aria-valuenow')).toBe('40');
      expect(screen.getAllByText(/120,000 files/).length).toBeGreaterThan(0);
      expect(screen.getByText(/Reading C:\\Users\\me\\Documents/)).toBeTruthy();
    });

    it('ignores progress from a scan of a different root', async () => {
      const source = makeSource({
        scan: vi.fn<StorageSource['scan']>(() => new Promise(() => undefined)),
      });
      await mount(source);
      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      await screen.findByText('Starting…');

      act(() => {
        source.emit(progress({ root: 'D:\\' }));
      });

      expect(screen.getByText('Starting…')).toBeTruthy();
    });

    it('locks the drive choice while a scan runs', async () => {
      // Changing the selection mid-scan would leave the running scan with no
      // visible owner and the next result under the wrong drive.
      const source = makeSource({
        scan: vi.fn<StorageSource['scan']>(() => new Promise(() => undefined)),
      });
      await mount(source);
      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      await screen.findByRole('button', { name: /Stop the scan/ });

      const drive = screen.getByRole('button', { pressed: true });
      expect((drive as HTMLButtonElement).disabled).toBe(true);
    });

    it('scans the whole drive: there is no depth choice to make', async () => {
      const source = makeSource();
      await mount(source);
      expect(screen.queryByText(/levels/)).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      await screen.findByRole('heading', { name: 'Explore' });
      expect(source.scan).toHaveBeenCalledWith('C:\\');
    });

    it('does not call links that were not followed unreadable folders', async () => {
      // A full C: scan records ~97,000 links. They are counted where they
      // point; only unreadable folders make the total a floor.
      await scanned(snapshot({ linksNotFollowed: 97_293 }));
      expect(screen.queryByText(/could not be read/)).toBeNull();
      expect(screen.getByText(/97,293 links/)).toBeTruthy();
    });

    it('asks the backend to stop when cancel is pressed', async () => {
      const source = makeSource({
        scan: vi.fn<StorageSource['scan']>(() => new Promise(() => undefined)),
      });
      await mount(source);

      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      fireEvent.click(await screen.findByRole('button', { name: /Stop the scan/ }));

      expect(source.cancelScan).toHaveBeenCalledOnce();
    });

    it('keeps the last good result when a rescan fails', async () => {
      // A failed refresh must not blank a result the user was reading. The
      // error line says the data is stale instead.
      const scan = vi
        .fn<StorageSource['scan']>()
        .mockResolvedValueOnce(snapshot())
        .mockRejectedValueOnce(new Error('the drive went away'));
      const source = makeSource({ scan });
      await mount(source);

      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      await screen.findByRole('heading', { name: 'Explore' });

      fireEvent.click(screen.getByRole('button', { name: /Scan again/ }));

      expect(await screen.findByRole('alert')).toBeTruthy();
      expect(screen.getByText(/last completed scan/)).toBeTruthy();
      expect(await screen.findByRole('button', { name: 'Open Users' })).toBeTruthy();
      expect(document.querySelector('[aria-busy="true"]')).toBeNull();
    });

    it('surfaces a first-scan failure as a message, not a spinner', async () => {
      const source = makeSource({
        scan: vi.fn<StorageSource['scan']>().mockRejectedValue(new Error('access denied')),
      });
      await mount(source);

      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));

      expect(await screen.findByText(/The scan did not finish/)).toBeTruthy();
      expect(screen.queryByRole('progressbar')).toBeNull();
    });
  });

  describe('the largest-folders list', () => {
    it('shows both the on-disk and the file-size figure', async () => {
      // Only one of the two, compared against Explorer, makes the user
      // conclude Vitals is wrong. They measure different things.
      await tabled();
      expect(screen.getByText('10.0 GB')).toBeTruthy();
      expect(screen.getByText('9.00 GB')).toBeTruthy();
    });

    it('marks a folder that could not be fully read', async () => {
      await tabled(snapshot({ largest: [entry({ incomplete: 'accessDenied' })] }));
      expect(screen.getByText('Incomplete')).toBeTruthy();
    });

    it('sorts by the chosen column', async () => {
      await tabled(
        snapshot({
          largest: [
            entry({ path: 'C:\\Small', allocated: GB, files: 9000 }),
            entry({ path: 'C:\\Big', allocated: 50 * GB, files: 3 }),
          ],
        }),
      );

      const paths = () =>
        [...document.querySelectorAll('tbody tr td:first-child span:first-child')].map(
          (cell) => cell.textContent,
        );

      expect(paths()).toEqual(['C:\\Big', 'C:\\Small']);

      fireEvent.click(screen.getByRole('radio', { name: 'Files' }));
      expect(paths()).toEqual(['C:\\Small', 'C:\\Big']);
    });

    it('filters by path', async () => {
      await tabled(
        snapshot({ largest: [entry({ path: 'C:\\Games' }), entry({ path: 'C:\\Users' })] }),
      );

      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'games' } });

      expect(screen.getByText('C:\\Games')).toBeTruthy();
      expect(screen.queryByText('C:\\Users')).toBeNull();
    });
  });

  describe('exploring a finished scan', () => {
    it('opens the scanned root largest first, with a breadcrumb', async () => {
      const source = await scanned();
      const users = await screen.findByRole('button', { name: 'Open Users' });
      const windows = screen.getByRole('button', { name: 'Open Windows' });
      // Largest first, in document order.
      expect(
        users.compareDocumentPosition(windows) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(source.children).toHaveBeenCalledWith(1, 0);
      const crumbs = screen.getByRole('navigation', { name: 'Where you are' });
      expect(crumbs.textContent).toContain('C:\\');
    });

    it('goes into a folder and back up through the breadcrumb', async () => {
      const source = await scanned();
      fireEvent.click(await screen.findByRole('button', { name: 'Open Users' }));
      expect(await screen.findByRole('button', { name: 'me' })).toBeTruthy();
      expect(source.children).toHaveBeenLastCalledWith(1, 1);
      const crumbs = screen.getByRole('navigation', { name: 'Where you are' });
      expect(crumbs.querySelector('[aria-current="location"]')?.textContent).toBe('Users');

      fireEvent.click(screen.getByRole('button', { name: 'Up one level' }));
      expect(await screen.findByRole('button', { name: 'Open Windows' })).toBeTruthy();
      expect(source.children).toHaveBeenLastCalledWith(1, 0);
    });

    it('asks for the map of the folder in view, in the chosen shape', async () => {
      const source = await scanned();
      await screen.findByRole('img', { name: /Map of C:/ });
      expect(source.map).toHaveBeenCalledWith(1, 0, 'icicle', expect.any(Number));
      fireEvent.click(screen.getByRole('radio', { name: 'Blocks' }));
      await vi.waitFor(() => {
        expect(source.map).toHaveBeenCalledWith(1, 0, 'treemap', expect.any(Number));
      });
    });

    it('explains a released scan instead of showing an error', async () => {
      // The backend lets go of the tree after fifteen idle minutes and answers
      // not-found. That is a state to explain, not a failure in red.
      await scanned();
      cleanup();
      const source = makeSource({
        children: vi
          .fn<StorageSource['children']>()
          .mockRejectedValue({ kind: 'not-found', message: 'gone' }),
      });
      await mount(source);
      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      expect(await screen.findByText(/no longer in memory/)).toBeTruthy();
      expect(screen.queryByRole('alert')).toBeNull();
    });

    it('lists the largest files and reveals one in Explorer', async () => {
      const source = await scanned(
        snapshot({
          largestFiles: [
            { path: 'C:\\Users\\me\\big.iso', allocated: 5 * GB, logical: 5 * GB, dirNode: 3 },
          ],
        }),
      );
      fireEvent.click(screen.getByRole('radio', { name: 'Largest files' }));
      fireEvent.click(await screen.findByText('big.iso'));
      expect(source.reveal).toHaveBeenCalledWith('C:\\Users\\me\\big.iso');
    });
  });

  describe('the review basket', () => {
    it('adds from the list, shows the total, and takes an item back out', async () => {
      await scanned();
      fireEvent.click(await screen.findByRole('button', { name: 'Add Users to review' }));
      fireEvent.click(screen.getByRole('button', { name: 'Add Windows to review' }));
      const bar = screen.getByRole('region', { name: 'To review' });
      expect(bar.textContent).toContain('2 items in review');
      expect(bar.textContent).toContain('10');

      fireEvent.click(screen.getByRole('button', { name: 'Remove Users from review' }));
      expect(bar.textContent).toContain('1 item in review');
    });

    it('adds from the largest-files and largest-folders views', async () => {
      await scanned(
        snapshot({
          largestFiles: [
            { path: 'C:\\Users\\me\\big.iso', allocated: 5 * GB, logical: 5 * GB, dirNode: 3 },
          ],
        }),
      );
      fireEvent.click(screen.getByRole('radio', { name: 'Largest files' }));
      fireEvent.click(await screen.findByRole('button', { name: 'Add big.iso to review' }));
      fireEvent.click(screen.getByRole('radio', { name: 'Largest folders' }));
      fireEvent.click(screen.getByRole('button', { name: 'Add WinSxS to review' }));
      expect(screen.getByRole('region', { name: 'To review' }).textContent).toContain(
        '2 items in review',
      );
    });

    it('adds a folder from the map with a right-click', async () => {
      await scanned();
      const map = await screen.findByRole('img', { name: /Map of C:/ });
      vi.spyOn(map, 'getBoundingClientRect').mockReturnValue({
        x: 0,
        y: 0,
        left: 0,
        top: 0,
        width: 100,
        height: 100,
        right: 100,
        bottom: 100,
        toJSON: () => ({}),
      });
      // The mock cell spans x 0..0.6, y 0.2..0.4: (30, 30) is inside it.
      await vi.waitFor(() => {
        fireEvent.contextMenu(map, { clientX: 30, clientY: 30 });
        expect(screen.getByRole('region', { name: 'To review' }).textContent).toContain(
          '1 item in review',
        );
      });
      expect(screen.getByRole('button', { name: 'Remove Users from review' })).toBeTruthy();
    });

    it('does not count a folder twice when its parent is added', async () => {
      await scanned();
      fireEvent.click(await screen.findByRole('button', { name: 'Open Users' }));
      fireEvent.click(await screen.findByRole('button', { name: 'Add me to review' }));
      fireEvent.click(screen.getByRole('button', { name: 'Up one level' }));
      fireEvent.click(await screen.findByRole('button', { name: 'Add Users to review' }));
      expect(screen.getByRole('region', { name: 'To review' }).textContent).toContain(
        '1 item in review',
      );
    });

    it('asks once, recycles through the backend, and reports each item', async () => {
      const source = makeSource({
        scan: vi.fn<StorageSource['scan']>().mockResolvedValue(snapshot()),
        recycle: vi.fn<StorageSource['recycle']>().mockResolvedValue({
          items: [
            {
              path: 'C:\\Users',
              outcome: 'locked',
              protection: null,
              holders: [{ pid: 4242, name: 'Word', service: null, kind: 'window' }],
              code: null,
              freed: null,
            },
            {
              path: 'C:\\Windows',
              outcome: 'refused',
              protection: 'systemFolder',
              holders: null,
              code: null,
              freed: null,
            },
          ],
          scan: null,
        }),
      });
      await mount(source);
      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      fireEvent.click(await screen.findByRole('button', { name: 'Add Users to review' }));
      fireEvent.click(screen.getByRole('button', { name: 'Add Windows to review' }));
      fireEvent.click(screen.getByRole('button', { name: 'Review…' }));

      const dialog = await screen.findByRole('dialog', { name: 'Send to the Recycle Bin?' });
      expect(dialog.textContent).toContain('C:\\Users');
      expect(source.recycle).not.toHaveBeenCalled();
      fireEvent.click(screen.getByRole('button', { name: /Recycle 2 items/ }));

      await screen.findByRole('dialog', { name: 'What happened' });
      expect(source.recycle).toHaveBeenCalledTimes(1);
      expect(source.recycle).toHaveBeenCalledWith(['C:\\Users', 'C:\\Windows'], 1);
      expect(screen.getByText('Word')).toBeTruthy();
      expect(screen.getByText('process 4242')).toBeTruthy();
      expect(screen.getByText(/part of Windows or an installed program/)).toBeTruthy();
      expect(screen.getByText(/Nothing was moved\. Every item is still where it was/)).toBeTruthy();
      // Neither item went, so both stay in review.
      fireEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(screen.getByRole('region', { name: 'To review' }).textContent).toContain(
        '2 items in review',
      );
    });

    it('cancelling the confirmation recycles nothing', async () => {
      const source = await scanned();
      fireEvent.click(await screen.findByRole('button', { name: 'Add Users to review' }));
      fireEvent.click(screen.getByRole('button', { name: 'Review…' }));
      await screen.findByRole('dialog');
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
      expect(source.recycle).not.toHaveBeenCalled();
    });

    it('applies the new totals and reloads the folder, without a rescan', async () => {
      const source = makeSource({
        recycle: vi.fn<StorageSource['recycle']>().mockResolvedValue({
          items: [
            {
              path: 'C:\\Users',
              outcome: 'recycled',
              protection: null,
              holders: null,
              code: null,
              freed: 6 * GB,
            },
          ],
          scan: {
            allocated: 4 * GB,
            logical: 3 * GB,
            filesScanned: 400,
            largest: [entry({ path: 'C:\\Windows', allocated: 4 * GB })],
            largestFiles: [],
          },
        }),
      });
      await mount(source);
      fireEvent.click(screen.getByRole('button', { name: /Scan this drive/ }));
      fireEvent.click(await screen.findByRole('button', { name: 'Add Users to review' }));
      const reads = vi.mocked(source.children).mock.calls.length;
      fireEvent.click(screen.getByRole('button', { name: 'Review…' }));
      fireEvent.click(await screen.findByRole('button', { name: /Recycle 1 item/ }));
      await screen.findByRole('dialog', { name: 'What happened' });

      expect(screen.getByText(/on disk across 400 files/)).toBeTruthy();
      await vi.waitFor(() => {
        expect(vi.mocked(source.children).mock.calls.length).toBeGreaterThan(reads);
      });
      expect(source.scan).toHaveBeenCalledTimes(1);
      fireEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(screen.queryByRole('region', { name: 'To review' })).toBeNull();
    });

    it('does not offer the scanned drive itself', async () => {
      await tabled(snapshot({ largest: [entry({ path: 'C:\\' }), entry()] }));
      expect(screen.getAllByRole('button', { name: /to review$/ })).toHaveLength(1);
    });
  });

  describe('honesty about an incomplete scan', () => {
    it('says how many folders were left out of the totals', async () => {
      // A total that silently omits unreadable folders is a wrong number
      // presented as a right one, with no way for the user to tell.
      await scanned(
        snapshot({
          complete: false,
          skippedTotal: 412,
          skipped: [
            {
              path: 'C:\\System Volume Information',
              reason: 'accessDenied',
              elevationFixable: true,
            },
          ],
        }),
      );

      expect(screen.getByText(/412 folders could not be read/)).toBeTruthy();
      expect(screen.getByText(/readable if Vitals ran as admin/)).toBeTruthy();
    });

    it('says the figures are a lower bound after a cancel', async () => {
      await scanned(snapshot({ cancelled: true, complete: false }));
      expect(screen.getByText(/lower bound/)).toBeTruthy();
    });

    it('says when cluster rounding was disabled', async () => {
      // No cluster size means no rounding, so the totals under-report.
      await scanned(snapshot({ clusterBytes: null }));
      expect(screen.getByText(/were not rounded up/)).toBeTruthy();
    });

    it('stays quiet on a clean, complete scan', async () => {
      await scanned();
      expect(screen.queryByText(/could not be read/)).toBeNull();
      expect(screen.queryByText(/lower bound/)).toBeNull();
    });
  });

  describe('cleanup candidates', () => {
    async function withCleanup(items: readonly CleanupCandidate[]) {
      const source = makeSource({
        cleanup: vi.fn<StorageSource['cleanup']>().mockResolvedValue(items),
      });
      await mount(source);
      fireEvent.click(screen.getByRole('button', { name: /Look for reclaimable space/ }));
      return source;
    }

    it('does not run until asked', async () => {
      const source = await mount();
      expect(source.cleanup).not.toHaveBeenCalled();
      expect(screen.getByText('Not checked yet')).toBeTruthy();
    });

    it('groups by safety tier and explains each one', async () => {
      await withCleanup([
        candidate({ path: 'C:\\Temp', safety: 'safe' }),
        candidate({ path: 'C:\\Windows.old', kind: 'previousWindows', safety: 'risky' }),
      ]);

      expect(await screen.findByRole('heading', { name: 'Safe to remove' })).toBeTruthy();
      expect(screen.getByRole('heading', { name: 'Risky' })).toBeTruthy();
      expect(screen.getByText(/Regenerated automatically/)).toBeTruthy();
    });

    it('excludes risky items from the headline total but still lists them', async () => {
      await withCleanup([
        candidate({ path: 'C:\\Temp', size: 2 * GB, safety: 'safe' }),
        candidate({
          path: 'C:\\hiberfil.sys',
          kind: 'hibernation',
          size: 34 * GB,
          safety: 'risky',
        }),
      ]);

      expect(await screen.findByText(/About 2.00 GB looks reclaimable/)).toBeTruthy();
      expect(screen.getByText('Hibernation file')).toBeTruthy();
    });

    it('says an unmeasured location was not measured, never zero', async () => {
      // Showing "0 B" for an 8 GB cache we could not read is how a cleanup
      // tool talks a user out of reclaiming real space.
      await withCleanup([candidate({ size: null, needsElevation: true })]);

      expect(await screen.findByText('Not measured')).toBeTruthy();
      expect(screen.queryByText('0 B')).toBeNull();
    });

    it('says WHY an unmeasured item has no size', async () => {
      await withCleanup([candidate({ size: null, needsElevation: true })]);
      expect(await screen.findByText(/without administrator rights/)).toBeTruthy();
    });

    it('distinguishes unreadable from needing elevation', async () => {
      await withCleanup([candidate({ size: null, needsElevation: false })]);
      expect(await screen.findByText(/could not be measured. Its size is unknown/)).toBeTruthy();
    });

    it('turns the total into a floor when something is unmeasured', async () => {
      await withCleanup([
        candidate({ path: 'C:\\Temp', size: 2 * GB }),
        candidate({ path: 'C:\\SoftwareDistribution', kind: 'windowsUpdateCache', size: null }),
      ]);

      expect(await screen.findByText(/At least 2.00 GB looks reclaimable/)).toBeTruthy();
      expect(screen.getByText(/1 location could not be measured/)).toBeTruthy();
    });

    it('offers deletion as unavailable rather than faking it', async () => {
      // The catalogue has no removal path of its own yet (Windows-managed
      // cleanup is its own slice). A live-looking button that does nothing
      // teaches the user the app is broken; the tooltip points to the basket.
      await withCleanup([candidate()]);

      const remove = await screen.findByRole('button', { name: 'Delete' });
      expect(remove.hasAttribute('disabled')).toBe(true);
      expect(remove.getAttribute('title')).toMatch(/Recycle Bin/);
    });

    it('surfaces a cleanup failure without a stuck spinner', async () => {
      const source = makeSource({
        cleanup: vi.fn<StorageSource['cleanup']>().mockRejectedValue(new Error('probe failed')),
      });
      await mount(source);

      fireEvent.click(screen.getByRole('button', { name: /Look for reclaimable space/ }));

      expect(await screen.findByText(/Could not check for reclaimable space/)).toBeTruthy();
      expect(screen.queryByRole('progressbar')).toBeNull();
    });

    it('offers its own stop, and stopping it leaves any scan alone', async () => {
      // They shared one cancel flag: "Stop the scan" stopped this too, and
      // starting this cleared a stop the user had asked for.
      const source = makeSource({
        cleanup: vi.fn<StorageSource['cleanup']>(() => new Promise(() => undefined)),
      });
      await mount(source);

      fireEvent.click(screen.getByRole('button', { name: /Look for reclaimable space/ }));
      fireEvent.click(await screen.findByRole('button', { name: /Stop checking/ }));

      expect(source.cancelCleanup).toHaveBeenCalledOnce();
      expect(source.cancelScan).not.toHaveBeenCalled();
    });

    it('says so when there is genuinely nothing to reclaim', async () => {
      await withCleanup([]);
      expect(await screen.findByText('Nothing to reclaim')).toBeTruthy();
    });
  });

  it('renders in Romanian without falling back to key paths', async () => {
    await i18n.changeLanguage('ro');
    const source = makeSource();
    render(<StorageScreen source={source} />);

    expect(await screen.findByRole('heading', { name: 'Stocare' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Scanează această unitate' })).toBeTruthy();
    expect(screen.getByText('Unități')).toBeTruthy();
    // A missing key renders as its own path, which is the failure this guards.
    expect(document.body.textContent).not.toMatch(/storage\.|cleanup\.|kindLabel\./);
  });
});
