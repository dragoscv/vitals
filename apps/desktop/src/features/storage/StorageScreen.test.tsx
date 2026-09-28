import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { StorageScreen } from './StorageScreen';
import type { CleanupCandidate, DirectoryEntry, ScanProgress, ScanSnapshot, Volume } from './model';
import { registerStorageStrings } from './strings';
import type { StorageSource } from './useStorage';

const GB = 1024 ** 3;

beforeAll(async () => {
  await initI18n();
  registerStorageStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
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
  await screen.findByRole('heading', { name: 'Largest folders' });
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
      await screen.findByRole('heading', { name: 'Largest folders' });
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
      await screen.findByRole('heading', { name: 'Largest folders' });

      fireEvent.click(screen.getByRole('button', { name: /Scan again/ }));

      expect(await screen.findByRole('alert')).toBeTruthy();
      expect(screen.getByText(/last completed scan/)).toBeTruthy();
      expect(screen.getByText('C:\\Windows\\WinSxS')).toBeTruthy();
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
      await scanned();
      expect(screen.getByText('10.0 GB')).toBeTruthy();
      expect(screen.getByText('9.00 GB')).toBeTruthy();
    });

    it('marks a folder that could not be fully read', async () => {
      await scanned(snapshot({ largest: [entry({ incomplete: 'accessDenied' })] }));
      expect(screen.getByText('Incomplete')).toBeTruthy();
    });

    it('sorts by the chosen column', async () => {
      await scanned(
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
      await scanned(
        snapshot({ largest: [entry({ path: 'C:\\Games' }), entry({ path: 'C:\\Users' })] }),
      );

      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'games' } });

      expect(screen.getByText('C:\\Games')).toBeTruthy();
      expect(screen.queryByText('C:\\Users')).toBeNull();
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
      // There is no deletion backend. A live-looking button that does nothing
      // teaches the user the app is broken.
      await withCleanup([candidate()]);

      const remove = await screen.findByRole('button', { name: 'Delete' });
      expect(remove.hasAttribute('disabled')).toBe(true);
      expect(remove.getAttribute('title')).toMatch(/cannot delete anything yet/);
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
