import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { resetSettingsForTests, useSettings } from '../../../settings/store';
import { registerStorageStrings } from '../strings';
import { DriveCleanupDialog } from './DriveCleanupDialog';
import type {
  DriveAnalyseProgress,
  DriveCleanProgress,
  DriveCleanupPlan,
  DriveCleanupReport,
  PlanItem,
} from './model';
import type { DriveCleanupSource } from './useDriveCleanup';

const GB = 1024 ** 3;

beforeAll(async () => {
  await initI18n();
  registerStorageStrings();
});

beforeEach(() => {
  resetSettingsForTests();
});

afterEach(async () => {
  cleanup();
  await i18n.changeLanguage('en');
});

function item(overrides: Partial<PlanItem> & Pick<PlanItem, 'id' | 'kind'>): PlanItem {
  return {
    label: null,
    size: GB,
    safety: 'safe',
    defaultSelected: true,
    irreversible: false,
    needsElevation: false,
    stopsWsl: false,
    stopsDocker: false,
    detail: null,
    ...overrides,
  };
}

const TEMP = item({ id: 't1', kind: 'tempOld', size: 2 * GB });
const NPM = item({
  id: 'c1',
  kind: 'npmCache',
  size: 3 * GB,
  safety: 'review',
  defaultSelected: false,
});
const PNPM = item({ id: 'c2', kind: 'pnpmStore', size: null, defaultSelected: false });
const VOLUME = item({
  id: 'd1',
  kind: 'dockerNamedVolume',
  label: 'pgdata',
  size: 5 * GB,
  safety: 'danger',
  defaultSelected: true,
  irreversible: true,
  stopsDocker: true,
});
const IMAGES = item({
  id: 'd2',
  kind: 'dockerOldImages',
  size: 4 * GB,
  defaultSelected: false,
  stopsDocker: true,
});
const COMPACT = item({
  id: 'v1',
  kind: 'compactVhd',
  label: 'Ubuntu-24.04',
  size: 40 * GB,
  defaultSelected: false,
  needsElevation: true,
  stopsWsl: true,
  stopsDocker: true,
});

function plan(overrides: Partial<DriveCleanupPlan> = {}): DriveCleanupPlan {
  return {
    scanId: 7,
    mount: 'C:\\',
    role: 'system',
    roleReasons: ['role.reason.holdsWindows'],
    freeBytes: 10 * GB,
    totalBytes: 500 * GB,
    groups: [
      { key: 'temp', items: [TEMP] },
      { key: 'caches', items: [NPM, PNPM] },
      { key: 'docker', items: [VOLUME, IMAGES] },
      { key: 'compaction', items: [COMPACT] },
    ],
    blockers: [
      { kind: 'wslDistro', name: 'Ubuntu-24.04', restartAfter: true },
      { kind: 'dockerDesktop', name: 'Docker Desktop', restartAfter: true },
      { kind: 'container', name: 'pg', restartAfter: true },
      { kind: 'scheduledTask', name: 'keepalive', restartAfter: true },
    ],
    ...overrides,
  };
}

function report(overrides: Partial<DriveCleanupReport> = {}): DriveCleanupReport {
  return {
    items: [
      { id: 't1', status: 'done', freed: 2 * GB, message: null },
      { id: 'v1', status: 'failed', freed: null, message: 'Optimize-VHD failed' },
    ],
    driveFreedBytes: 2 * GB,
    restarted: [{ kind: 'wslDistro', name: 'Ubuntu-24.04', restartAfter: true }],
    restartFailed: [{ kind: 'dockerDesktop', name: 'Docker Desktop', restartAfter: true }],
    ...overrides,
  };
}

interface Harness {
  readonly source: DriveCleanupSource;
  analyse: (progress: DriveAnalyseProgress) => void;
  clean: (progress: DriveCleanProgress) => void;
}

function makeSource(overrides: Partial<DriveCleanupSource> = {}): Harness {
  const harness: Harness = {
    analyse: () => undefined,
    clean: () => undefined,
    source: {
      analyse: vi.fn<DriveCleanupSource['analyse']>().mockResolvedValue(plan()),
      cancelAnalysis: vi.fn<DriveCleanupSource['cancelAnalysis']>().mockResolvedValue(undefined),
      run: vi.fn<DriveCleanupSource['run']>().mockResolvedValue(report()),
      onAnalyseProgress: vi.fn<DriveCleanupSource['onAnalyseProgress']>((listener) => {
        harness.analyse = listener;
        return Promise.resolve(() => undefined);
      }),
      onCleanProgress: vi.fn<DriveCleanupSource['onCleanProgress']>((listener) => {
        harness.clean = listener;
        return Promise.resolve(() => undefined);
      }),
      ...overrides,
    },
  };
  return harness;
}

async function opened(overrides: Partial<DriveCleanupSource> = {}, onClose = vi.fn()) {
  const harness = makeSource(overrides);
  render(<DriveCleanupDialog mount="C:\" locale="en" source={harness.source} onClose={onClose} />);
  await screen.findByRole('region', { name: 'Temporary files' });
  return harness;
}

function box(name: string | RegExp) {
  return screen.getByRole('checkbox', { name });
}

function checked(name: string | RegExp): boolean {
  return box(name).getAttribute('aria-checked') === 'true';
}

function next() {
  fireEvent.click(screen.getByRole('button', { name: /^Next/ }));
}

describe('drive clean-up wizard', () => {
  it('analyses the drive with live progress, then lists the plan', async () => {
    const harness = makeSource({
      analyse: vi.fn<DriveCleanupSource['analyse']>(() => new Promise(() => undefined)),
    });
    render(
      <DriveCleanupDialog mount="C:\" locale="en" source={harness.source} onClose={vi.fn()} />,
    );
    expect(harness.source.analyse).toHaveBeenCalledWith('C:\\');
    await act(async () => {
      await Promise.resolve();
      harness.analyse({ stage: 'docker', done: 3, total: 9 });
    });
    const status = screen.getByRole('status');
    expect(within(status).getByText('Asking Docker')).toBeTruthy();
    expect(screen.getByRole('progressbar').getAttribute('aria-valuetext')).toBe('3 of 9 checks');
    fireEvent.click(screen.getByRole('button', { name: 'Stop measuring' }));
    expect(harness.source.cancelAnalysis).toHaveBeenCalled();
  });

  it('preselects only safe defaults, never a danger item', async () => {
    await opened();
    expect(checked('Old temporary files')).toBe(true);
    expect(checked('npm cache')).toBe(false);
    // defaultSelected, but danger: a fresh decision every time.
    expect(checked('Named Docker volume (pgdata)')).toBe(false);
    expect(screen.getByText('System drive')).toBeTruthy();
    expect(screen.getByText('Windows is installed here.')).toBeTruthy();
  });

  it('shows the gain per group and the selected total before running', async () => {
    await opened();
    const docker = screen.getByRole('region', { name: 'Docker' });
    expect(within(docker).getByText('Up to 9.00 GB')).toBeTruthy();
    const caches = screen.getByRole('region', { name: 'Package caches' });
    expect(within(caches).getByText('Up to 3.00 GB, plus 1 not measured')).toBeTruthy();
    expect(screen.getByText('1 item selected, about 2.00 GB')).toBeTruthy();
  });

  it('select-all in a group ticks everything but the danger item', async () => {
    await opened();
    fireEvent.click(box('Tick everything safe in Docker'));
    expect(checked('Unused Docker images')).toBe(true);
    expect(checked('Named Docker volume (pgdata)')).toBe(false);
  });

  it('renders an unmeasured size as an em dash with its reason, never 0', async () => {
    await opened();
    const caches = screen.getByRole('region', { name: 'Package caches' });
    const dash = within(caches).getByLabelText('Not measured');
    expect(dash.textContent).toBe('—');
    expect(caches.textContent).not.toContain('0 B');
  });

  it('a ticked danger item needs its own acknowledgement before Next', async () => {
    await opened();
    fireEvent.click(box('Named Docker volume (pgdata)'));
    expect(screen.getByRole('button', { name: /^Next/ })).toHaveProperty('disabled', true);
    fireEvent.click(box('I understand Named Docker volume (pgdata) cannot be undone'));
    expect(screen.getByRole('button', { name: /^Next/ })).toHaveProperty('disabled', false);
  });

  it('previews what will stop only when a selected item stops WSL or Docker', async () => {
    await opened();
    expect(screen.queryByText('These will be stopped and started again:')).toBeNull();

    fireEvent.click(box('Unused Docker images'));
    const note = screen.getByRole('note');
    expect(within(note).getByText('Docker Desktop')).toBeTruthy();
    expect(within(note).getByText('Container pg')).toBeTruthy();
    expect(within(note).queryByText('Linux distribution Ubuntu-24.04')).toBeNull();

    fireEvent.click(box('Shrink a virtual disk (Ubuntu-24.04)'));
    expect(
      within(screen.getByRole('note')).getByText('Linux distribution Ubuntu-24.04'),
    ).toBeTruthy();
    expect(within(screen.getByRole('note')).getByText('Scheduled task keepalive')).toBeTruthy();

    fireEvent.click(box('Unused Docker images'));
    fireEvent.click(box('Shrink a virtual disk (Ubuntu-24.04)'));
    expect(screen.queryByText('These will be stopped and started again:')).toBeNull();
  });

  it('runs exactly the selected ids, confirmed, after the irreversible tick', async () => {
    const harness = await opened();
    fireEvent.click(box('Shrink a virtual disk (Ubuntu-24.04)'));
    fireEvent.click(box('Named Docker volume (pgdata)'));
    fireEvent.click(box('I understand Named Docker volume (pgdata) cannot be undone'));
    next();

    expect(screen.getByText('Windows will ask once for administrator approval.')).toBeTruthy();
    const run = screen.getByRole('button', { name: 'Clean up now' });
    expect(run).toHaveProperty('disabled', true);
    fireEvent.click(box('I understand this cannot be undone'));
    fireEvent.click(run);

    expect(harness.source.run).toHaveBeenCalledWith(7, ['t1', 'd1', 'v1'], true);
  });

  it('names an unlisted Hyper-V blocker as unknown machines, not a machine called unknown', async () => {
    const unknown = { kind: 'hyperVVm', name: 'unknown', restartAfter: false } as const;
    await opened({
      analyse: vi
        .fn<DriveCleanupSource['analyse']>()
        .mockResolvedValue(plan({ blockers: [...plan().blockers, unknown] })),
    });
    fireEvent.click(box('Shrink a virtual disk (Ubuntu-24.04)'));
    const note = screen.getByRole('note');
    expect(within(note).getByText(/Any running Hyper-V virtual machines/)).toBeTruthy();
    expect(within(note).queryByText('Virtual machine unknown')).toBeNull();
  });

  it('shows an unavailable item with its reason but never lets it be ticked', async () => {
    const orphan = item({
      id: 'h1',
      kind: 'hiberfilOrphan',
      size: 12 * GB,
      defaultSelected: false,
      available: false,
      detail: 'Windows refuses to turn hibernation on here',
    });
    await opened({
      analyse: vi
        .fn<DriveCleanupSource['analyse']>()
        .mockResolvedValue(
          plan({ groups: [...plan().groups, { key: 'windows', items: [orphan] }] }),
        ),
    });
    const windows = screen.getByRole('region', { name: 'Windows' });
    expect(within(windows).getByText('Not possible here')).toBeTruthy();
    expect(within(windows).getByText('Windows refuses to turn hibernation on here')).toBeTruthy();
    const tick = box('Leftover hibernation file');
    expect(tick.hasAttribute('disabled') || tick.getAttribute('aria-disabled') === 'true').toBe(
      true,
    );
  });

  it('shows each stage live and cannot be closed while it runs', async () => {
    const onClose = vi.fn();
    const harness = await opened(
      { run: vi.fn<DriveCleanupSource['run']>(() => new Promise(() => undefined)) },
      onClose,
    );
    next();
    fireEvent.click(screen.getByRole('button', { name: 'Clean up now' }));
    await act(async () => {
      await Promise.resolve();
    });

    const stages: readonly [DriveCleanProgress, string][] = [
      [
        { stage: 'cleaning', current: 't1', done: 0, total: 5, diskIndex: null, diskCount: null },
        'Cleaning up the chosen items',
      ],
      [
        { stage: 'stopping', current: null, done: 0, total: 5, diskIndex: null, diskCount: null },
        'Stopping programs that use the disks',
      ],
      [
        { stage: 'trimming', current: null, done: 1, total: 5, diskIndex: null, diskCount: null },
        'Freeing unused space inside the disks',
      ],
      [
        // The backend counts disks from 1.
        { stage: 'compacting', current: null, done: 2, total: 5, diskIndex: 2, diskCount: 3 },
        'Shrinking virtual disk 2 of 3',
      ],
      [
        { stage: 'restarting', current: null, done: 3, total: 5, diskIndex: null, diskCount: null },
        'Starting programs again',
      ],
      [
        {
          stage: 'remeasuring',
          current: 't1',
          done: 4,
          total: 5,
          diskIndex: null,
          diskCount: null,
        },
        'Measuring the drive again',
      ],
    ];
    for (const [progress, text] of stages) {
      act(() => {
        harness.clean(progress);
      });
      expect(within(screen.getByRole('status')).getByText(text)).toBeTruthy();
    }
    expect(screen.getByText('Now: Old temporary files')).toBeTruthy();

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('reports each item, the drive gain and the programs that did not restart', async () => {
    await opened();
    next();
    fireEvent.click(screen.getByRole('button', { name: 'Clean up now' }));
    await screen.findByRole('dialog', { name: 'What was cleaned up' });

    expect(screen.getByText('C:\\ gained 2.00 GB')).toBeTruthy();
    expect(screen.getByText('Gave back 2.00 GB')).toBeTruthy();
    expect(screen.getByText('Failed')).toBeTruthy();
    expect(screen.getByText('Optimize-VHD failed')).toBeTruthy();
    expect(screen.getByText('Started again:')).toBeTruthy();
    const failed = screen.getByRole('alert');
    expect(failed.className).toContain('danger');
    expect(within(failed).getByText(/Start Docker Desktop from the Start menu\./)).toBeTruthy();
  });

  it('says a refused run changed nothing', async () => {
    await opened({
      run: vi
        .fn<DriveCleanupSource['run']>()
        .mockRejectedValue({ kind: 'refused', message: 'The UAC prompt was declined.' }),
    });
    next();
    fireEvent.click(screen.getByRole('button', { name: 'Clean up now' }));
    expect(await screen.findByRole('dialog', { name: 'Nothing was cleaned up' })).toBeTruthy();
    expect(screen.getByText('Nothing was changed. The UAC prompt was declined.')).toBeTruthy();
  });

  it('remembers the chosen kinds, never a danger kind, and preselects them next time', async () => {
    await opened();
    fireEvent.click(box('npm cache'));
    fireEvent.click(box('Named Docker volume (pgdata)'));
    fireEvent.click(box('I understand Named Docker volume (pgdata) cannot be undone'));
    next();
    fireEvent.click(box('Remember my choices for this drive'));
    fireEvent.click(box('I understand this cannot be undone'));
    fireEvent.click(screen.getByRole('button', { name: 'Clean up now' }));
    await screen.findByRole('dialog', { name: 'What was cleaned up' });

    expect(useSettings.getState().settings.driveCleanupProfiles).toEqual({
      'C:': ['tempOld', 'npmCache'],
    });

    cleanup();
    await opened();
    expect(checked('npm cache')).toBe(true);
    expect(checked('Old temporary files')).toBe(true);
    expect(checked('Named Docker volume (pgdata)')).toBe(false);
  });

  it('the one-click usual clean-up still stops at the confirmation', async () => {
    useSettings.getState().patch({ driveCleanupProfiles: { 'C:': ['npmCache'] } });
    const harness = await opened();
    fireEvent.click(screen.getByRole('button', { name: 'Run my usual clean-up' }));

    expect(harness.source.run).not.toHaveBeenCalled();
    expect(screen.getByText('Step 3 of 3: Confirm')).toBeTruthy();
    expect(screen.getByText('You chose')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Clean up now' }));
    expect(harness.source.run).toHaveBeenCalledWith(7, ['c1'], true);
  });

  it('speaks Romanian', async () => {
    await i18n.changeLanguage('ro');
    const harness = makeSource();
    render(
      <DriveCleanupDialog mount="C:\" locale="ro" source={harness.source} onClose={vi.fn()} />,
    );
    expect(await screen.findByRole('region', { name: 'Fișiere temporare' })).toBeTruthy();
    expect(screen.getByText('Unitatea de sistem')).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: 'Fișiere temporare vechi' })).toBeTruthy();
    expect(screen.getByRole('button', { name: /^Înainte/ })).toBeTruthy();
  });
});
