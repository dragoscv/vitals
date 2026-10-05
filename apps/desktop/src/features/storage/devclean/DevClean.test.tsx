import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { registerStorageStrings } from '../strings';
import { DevClean } from './DevClean';
import { GB, devScan } from './fixtures.test-utils';
import type { DevCleanProgress, DevCleanReport, DevScan, DevScanProgress } from './model';
import type { DevCleanSource } from './useDevClean';

beforeAll(async () => {
  await initI18n();
  registerStorageStrings();
});

afterEach(async () => {
  cleanup();
  await i18n.changeLanguage('en');
});

interface Harness {
  readonly source: DevCleanSource;
  scanProgress: (progress: DevScanProgress) => void;
  cleanProgress: (progress: DevCleanProgress) => void;
}

function makeSource(overrides: Partial<DevCleanSource> = {}): Harness {
  const harness: Harness = {
    scanProgress: () => undefined,
    cleanProgress: () => undefined,
    source: {
      defaultRoots: vi.fn<DevCleanSource['defaultRoots']>().mockResolvedValue(['E:\\gh']),
      scan: vi.fn<DevCleanSource['scan']>().mockResolvedValue(devScan()),
      cancelScan: vi.fn<DevCleanSource['cancelScan']>().mockResolvedValue(undefined),
      run: vi
        .fn<DevCleanSource['run']>()
        .mockResolvedValue({ items: [], drives: [] } satisfies DevCleanReport),
      onScanProgress: vi.fn<DevCleanSource['onScanProgress']>((listener) => {
        harness.scanProgress = listener;
        return Promise.resolve(() => undefined);
      }),
      onCleanProgress: vi.fn<DevCleanSource['onCleanProgress']>((listener) => {
        harness.cleanProgress = listener;
        return Promise.resolve(() => undefined);
      }),
      pickFolders: vi.fn<DevCleanSource['pickFolders']>().mockResolvedValue(null),
      ...overrides,
    },
  };
  return harness;
}

async function scanned(overrides: Partial<DevCleanSource> = {}): Promise<Harness> {
  const harness = makeSource(overrides);
  render(<DevClean locale="en" source={harness.source} />);
  await screen.findByText('E:\\gh');
  fireEvent.click(screen.getByRole('button', { name: 'Scan projects' }));
  await screen.findByRole('region', { name: 'Projects' });
  return harness;
}

function expand(name: string) {
  fireEvent.click(screen.getByRole('button', { name: `Show the folders in ${name}` }));
}

function checkbox(name: string | RegExp) {
  return screen.getByRole('checkbox', { name });
}

async function openConfirm() {
  fireEvent.click(screen.getByRole('button', { name: 'Clean up…' }));
  return screen.findByRole('dialog', { name: 'Clean up these items?' });
}

describe('developer clean-up', () => {
  it('shows the default roots and does not scan until asked', async () => {
    const { source } = makeSource();
    render(<DevClean locale="en" source={source} />);

    expect(await screen.findByText('E:\\gh')).toBeTruthy();
    expect(source.scan).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Scan projects' }));
    // Unchanged roots ask for the backend's defaults rather than a copy of them.
    expect(source.scan).toHaveBeenCalledWith(null);
  });

  it('scans a picked folder alongside the defaults, and drops a removed root', async () => {
    const { source } = makeSource({
      pickFolders: vi.fn<DevCleanSource['pickFolders']>().mockResolvedValue(['D:\\work']),
    });
    render(<DevClean locale="en" source={source} />);
    await screen.findByText('E:\\gh');

    fireEvent.click(screen.getByRole('button', { name: 'Add folder…' }));
    await screen.findByText('D:\\work');
    fireEvent.click(screen.getByRole('button', { name: 'Stop looking in E:\\gh' }));
    fireEvent.click(screen.getByRole('button', { name: 'Scan projects' }));

    expect(source.scan).toHaveBeenCalledWith(['D:\\work']);
  });

  it('shows the phase, count and path while scanning, and Stop asks the backend to cancel', async () => {
    const harness = makeSource({
      scan: vi.fn<DevCleanSource['scan']>(() => new Promise<DevScan>(() => undefined)),
    });
    render(<DevClean locale="en" source={harness.source} />);
    await screen.findByText('E:\\gh');
    fireEvent.click(screen.getByRole('button', { name: 'Scan projects' }));

    act(() => {
      harness.scanProgress({ phase: 'sizing', found: 12, currentPath: 'E:\\gh\\memorai' });
    });
    const status = screen.getByRole('status');
    expect(within(status).getByText('Measuring folders')).toBeTruthy();
    expect(within(status).getByText('12 items found so far')).toBeTruthy();
    expect(within(status).getByText('Reading E:\\gh\\memorai')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Stop scanning projects' }));
    expect(harness.source.cancelScan).toHaveBeenCalledTimes(1);
  });

  it('ends the scanning state when the scan fails, and says why', async () => {
    const { source } = makeSource({
      scan: vi.fn<DevCleanSource['scan']>().mockRejectedValue({
        kind: 'internal',
        message: 'walk failed',
      }),
    });
    render(<DevClean locale="en" source={source} />);
    await screen.findByText('E:\\gh');
    fireEvent.click(screen.getByRole('button', { name: 'Scan projects' }));

    expect((await screen.findByRole('alert')).textContent).toMatch(/walk failed/);
    expect(screen.queryByRole('button', { name: 'Stop scanning projects' })).toBeNull();
  });

  it('pre-ticks folders of stale projects and leaves active ones unticked', async () => {
    await scanned();

    expect(checkbox('Select every folder in memorai').getAttribute('aria-checked')).toBe('true');
    expect(checkbox('Select every folder in codai').getAttribute('aria-checked')).toBe('false');
    expect(screen.getByText('Idle 376 days')).toBeTruthy();
    expect(screen.getByText('Active')).toBeTruthy();
  });

  it('says Not measured for an unreadable folder, never 0 B', async () => {
    await scanned();
    expand('memorai');

    const row = checkbox(/Rust target/).closest('li');
    if (row === null) throw new Error('no row');
    expect(within(row).getByText('Not measured')).toBeTruthy();
    expect(row.textContent).not.toMatch(/\b0 B\b/);
    expect(within(row).getByText('cargo build')).toBeTruthy();
  });

  it('warns that pnpm node_modules frees little until the store is pruned', async () => {
    await scanned();
    expand('memorai');
    expect(screen.getByText(/shared with the pnpm store/)).toBeTruthy();
  });

  it('totals the selection, counting unmeasured items apart rather than as zero', async () => {
    await scanned();

    expect(screen.getByText(/2 items selected/).textContent).toMatch(/\(\+ 1 not measured\)/);

    fireEvent.click(checkbox('Select every folder in codai'));
    expect(screen.getByText(/3 items selected/)).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Select none in Projects' }));
    expect(screen.getByText(/0 items selected/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Clean up…' })).toHaveProperty('disabled', true);
  });

  it('shows why a worktree is kept and gives it no checkbox', async () => {
    await scanned();

    expect(screen.getByText('Kept: 408 uncommitted changes')).toBeTruthy();
    expect(screen.getByText('Kept: active 3 hours ago')).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: /E:\\gh\\\.wt\\codai\\old/ })).toBeTruthy();
    expect(screen.queryByRole('checkbox', { name: /brivio\\busy/ })).toBeNull();
    expect(screen.queryByRole('checkbox', { name: /codai\\live/ })).toBeNull();
  });

  it('never offers a Docker volume, even by select all', async () => {
    const scan = devScan();
    const harness = await scanned({
      scan: vi.fn<DevCleanSource['scan']>().mockResolvedValue({
        ...scan,
        // A volume that arrives with an id by mistake is still not offered.
        docker: {
          state: 'ok',
          items: scan.docker.items.map((d) => (d.kind === 'volumes' ? { ...d, id: 'd-vol' } : d)),
        },
      }),
    });

    expect(screen.queryByRole('checkbox', { name: 'Select Volumes' })).toBeNull();
    expect(screen.getByText(/they hold databases/)).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Select all in Docker' }));
    await openConfirm();
    fireEvent.click(checkbox(/cannot be undone/));
    fireEvent.click(screen.getByRole('button', { name: 'Delete permanently' }));
    const ids = vi.mocked(harness.source.run).mock.calls[0]?.[1];
    expect(ids).toContain('d-build');
    expect(ids).not.toContain('d-vol');
  });

  it('says a package cache whose tool is missing cannot be run', async () => {
    await scanned();
    expect(screen.getByText('Tool not installed')).toBeTruthy();
    expect(screen.queryByRole('checkbox', { name: 'Select Go module cache' })).toBeNull();
    expect(screen.getByText('pnpm store prune')).toBeTruthy();
  });

  it('keeps the confirm button disabled until the cannot-be-undone box is ticked', async () => {
    await scanned();
    const dialog = await openConfirm();

    const run = within(dialog).getByRole('button', { name: 'Delete permanently' });
    expect(run).toHaveProperty('disabled', true);
    expect(within(dialog).getByText(/do not go to the Recycle Bin/)).toBeTruthy();

    fireEvent.click(within(dialog).getByRole('checkbox', { name: /cannot be undone/ }));
    expect(run).toHaveProperty('disabled', false);
  });

  it('warns that WSL and Docker stop only when a virtual disk is selected', async () => {
    await scanned();
    let dialog = await openConfirm();
    expect(
      within(dialog).queryByText(
        /WSL, Docker Desktop and any scheduled task that starts WSL will stop/,
      ),
    ).toBeNull();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));

    fireEvent.click(checkbox(/Compact Docker Desktop disk/));
    dialog = await openConfirm();
    expect(
      within(dialog).getByText(
        /WSL, Docker Desktop and any scheduled task that starts WSL will stop/,
      ),
    ).toBeTruthy();
  });

  it('runs exactly the selected ids with confirmed true', async () => {
    const harness = await scanned();
    fireEvent.click(checkbox('Select Build cache'));
    const dialog = await openConfirm();

    expect(within(dialog).getByText('pnpm install')).toBeTruthy();
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /cannot be undone/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete permanently' }));

    expect(harness.source.run).toHaveBeenCalledWith(
      7,
      ['a-stale-nm', 'a-stale-target', 'd-build'],
      true,
    );
  });

  it('shows k of n and the current path while cleaning, and cannot be closed', async () => {
    const harness = await scanned({
      run: vi.fn<DevCleanSource['run']>(() => new Promise<DevCleanReport>(() => undefined)),
    });
    const dialog = await openConfirm();
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /cannot be undone/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete permanently' }));

    act(() => {
      harness.cleanProgress({ id: 'a-stale-target', index: 1, total: 2 });
    });
    const status = within(dialog).getByRole('status');
    expect(within(status).getByText('2 of 2')).toBeTruthy();
    expect(within(status).getByText('E:\\gh\\memorai\\target')).toBeTruthy();

    fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(screen.getByRole('dialog', { name: 'Clean up these items?' })).toBeTruthy();
  });

  it('says nothing was changed when the backend refuses the run', async () => {
    await scanned({
      run: vi.fn<DevCleanSource['run']>().mockRejectedValue({
        kind: 'refused',
        message: 'Administrator approval was declined.',
      }),
    });
    const dialog = await openConfirm();
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /cannot be undone/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete permanently' }));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toBe('Nothing was changed. Administrator approval was declined.');
    expect(await screen.findByRole('dialog', { name: 'Nothing was cleaned up' })).toBeTruthy();
  });

  it('reports each outcome, freed space and drive gain, and removes what was done', async () => {
    const report: DevCleanReport = {
      items: [
        {
          id: 'a-stale-nm',
          path: 'E:\\gh\\memorai\\node_modules',
          outcome: 'done',
          freed: GB,
          toolFreed: null,
          message: null,
          holders: null,
        },
        {
          id: 'a-stale-target',
          path: 'E:\\gh\\memorai\\target',
          outcome: 'partial',
          freed: null,
          toolFreed: null,
          message: 'Some files are in use.',
          holders: [{ pid: 4242, name: 'rust-analyzer.exe' }],
        },
        {
          id: 'v-docker',
          path: 'docker_data.vhdx',
          outcome: 'refused',
          freed: null,
          toolFreed: null,
          message: null,
          holders: null,
        },
      ],
      drives: [{ drive: 'E:', freed: 3 * GB }],
    };
    await scanned({ run: vi.fn<DevCleanSource['run']>().mockResolvedValue(report) });
    fireEvent.click(checkbox(/Compact Docker Desktop disk/));
    const dialog = await openConfirm();
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /cannot be undone/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete permanently' }));

    const done = await screen.findByRole('dialog', { name: 'What was cleaned up' });
    const removed = within(done).getByText('E:\\gh\\memorai\\node_modules').closest('li');
    if (removed === null) throw new Error('no report row');
    expect(within(removed).getByText('Done')).toBeTruthy();
    expect(within(done).getByText('Partly done')).toBeTruthy();
    expect(within(done).getByText('Not done')).toBeTruthy();
    expect(within(done).getByText(/^Gave back 1(\.0+)? GB$/)).toBeTruthy();
    expect(within(done).getByText('Space given back: not measured')).toBeTruthy();
    expect(within(done).getByText('rust-analyzer.exe (process 4242)')).toBeTruthy();
    expect(within(done).getByText('Nothing was changed for this item.')).toBeTruthy();
    expect(within(done).getByText(/^Drive E: gained 3(\.0+)? GB$/)).toBeTruthy();

    fireEvent.click(within(done).getByRole('button', { name: 'Done' }));
    expand('memorai');
    expect(screen.queryByText('E:\\gh\\memorai\\node_modules')).toBeNull();
    expect(screen.getByText('E:\\gh\\memorai\\target')).toBeTruthy();
    // The rows that stayed keep their report line.
    expect(screen.getByText('Partly done')).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: /Compact Docker Desktop disk/ })).toBeTruthy();
  });

  it('opens a row menu on a right click that ticks, reveals and copies the path', async () => {
    const reveal = vi.fn<NonNullable<DevCleanSource['reveal']>>().mockResolvedValue(undefined);
    await scanned({ reveal });
    expand('memorai');
    const row = checkbox(/Rust target/).closest('li');
    if (row === null) throw new Error('no row');
    const before = checkbox(/Rust target/).getAttribute('aria-checked');

    fireEvent.contextMenu(row, { button: 2 });
    await act(async () => {});
    const menu = await screen.findByRole('menu');
    expect(within(menu).getByRole('menuitem', { name: 'Copy path' })).toBeTruthy();
    fireEvent.click(within(menu).getByRole('menuitem', { name: 'Show in File Explorer' }));
    expect(reveal).toHaveBeenCalledWith('E:\\gh\\memorai\\target');

    fireEvent.contextMenu(row, { button: 2 });
    await act(async () => {});
    const again = await screen.findByRole('menu');
    fireEvent.click(within(again).getByRole('menuitem', { name: /^(Tick for clean-up|Untick)$/ }));
    expect(checkbox(/Rust target/).getAttribute('aria-checked')).not.toBe(before);
  });

  it('does not open a row menu for a contextmenu that is neither a right click nor the menu key', async () => {
    await scanned();

    fireEvent.contextMenu(screen.getAllByTestId('dev-project')[0] as HTMLElement, { button: 0 });
    await act(async () => {});

    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('ticks a whole project from its right-click menu', async () => {
    await scanned();
    const header = checkbox('Select every folder in codai').closest('[data-testid="dev-project"]');
    if (header === null) throw new Error('no project header');

    fireEvent.contextMenu(header, { button: 2 });
    await act(async () => {});
    const menu = await screen.findByRole('menu');
    expect(within(menu).getByRole('menuitem', { name: 'Show its folders' })).toBeTruthy();
    fireEvent.click(within(menu).getByRole('menuitem', { name: 'Tick for clean-up' }));

    expect(screen.getByText(/3 items selected/)).toBeTruthy();
  });

  it('renders in Romanian without falling back to key paths', async () => {
    await i18n.changeLanguage('ro');
    const { source } = makeSource();
    render(<DevClean locale="ro" source={source} />);
    await screen.findByText('E:\\gh');
    fireEvent.click(screen.getByRole('button', { name: 'Scanează proiectele' }));
    await screen.findByRole('region', { name: 'Proiecte' });
    expand_ro('memorai');

    expect(screen.getByText('Curățenie pentru dezvoltatori')).toBeTruthy();
    expect(screen.getByText('Neatins de 376 de zile')).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/dev\./);
  });
});

function expand_ro(name: string) {
  fireEvent.click(screen.getByRole('button', { name: `Arată folderele din ${name}` }));
}
