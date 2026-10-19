import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { registerStorageStrings } from '../../strings';
import { MoveDiskDialog } from './MoveDiskDialog';
import type {
  DockerDisk,
  MovableDisk,
  MovePlan,
  MoveProgress,
  MoveReport,
  MoveTarget,
} from './model';
import type { DriveMoveSource } from './useDriveMove';

const GB = 1024 ** 3;

beforeAll(async () => {
  await initI18n();
  registerStorageStrings();
});

afterEach(async () => {
  cleanup();
  await i18n.changeLanguage('en');
});

function target(mount: string, refusal: string | null, free = 500 * GB): MoveTarget {
  return {
    mount,
    label: null,
    fileSystem: 'NTFS',
    freeBytes: free,
    totalBytes: 1000 * GB,
    refusal,
  };
}

const UBUNTU: MovableDisk = {
  id: '1-0',
  owner: 'distro',
  name: 'Ubuntu-24.04',
  path: 'C:\\wsl\\ext4.vhdx',
  size: 120 * GB,
  requiredBytes: 134 * GB,
  running: true,
  unavailable: null,
  targets: [
    target('C:', 'sameDrive', 10 * GB),
    target('D:', 'notFixed'),
    target('E:', null),
    target('F:', 'tooLittleSpace', 20 * GB),
  ],
  blockers: [{ kind: 'wslDistro', name: 'Ubuntu-24.04', restartAfter: true }],
};

const DOCKER_DISK: MovableDisk = {
  id: '1-1',
  owner: 'dockerDesktop',
  name: 'docker-desktop',
  path: 'C:\\Users\\v\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx',
  size: 190 * GB,
  requiredBytes: 211 * GB,
  running: true,
  unavailable: null,
  targets: [target('C:', 'sameDrive'), target('E:', null)],
  blockers: [],
};

const DOCKER: DockerDisk = {
  installed: true,
  path: DOCKER_DISK.path,
  size: DOCKER_DISK.size,
  location: null,
  version: '4.95.0',
  crossDriveOk: true,
  suggested: [{ mount: 'E:', folder: 'E:\\Vitals\\Docker' }],
};

function plan(disks: readonly MovableDisk[]): MovePlan {
  return { planId: 1, mount: 'C:', wslVersion: '2.7.13.0', disks, docker: DOCKER };
}

const MOVED: MoveReport = {
  status: 'moved',
  message: null,
  refusal: null,
  command: 'wsl --manage Ubuntu-24.04 --move E:\\Vitals\\WSL\\Ubuntu-24.04',
  from: UBUNTU.path,
  to: 'E:\\Vitals\\WSL\\Ubuntu-24.04\\ext4.vhdx',
  verification: {
    registeredAtTarget: true,
    fileAtTarget: true,
    oldFileGone: true,
    starts: true,
    freedBytes: 120 * GB,
  },
  restarted: [{ kind: 'wslDistro', name: 'Ubuntu-24.04', restartAfter: true }],
  restartFailed: [],
};

function source(overrides: Partial<DriveMoveSource> = {}): DriveMoveSource {
  return {
    plan: vi.fn(async () => plan([UBUNTU, DOCKER_DISK])),
    run: vi.fn(async () => MOVED),
    openDocker: vi.fn(async () => undefined),
    checkDocker: vi.fn(async () => DOCKER),
    onProgress: vi.fn(async () => () => undefined),
    ...overrides,
  };
}

async function open(src: DriveMoveSource, focus: string | null = 'Ubuntu-24.04') {
  const onClose = vi.fn();
  render(<MoveDiskDialog mount="C:" focus={focus} locale="en" onClose={onClose} source={src} />);
  await screen.findByText('Which disk');
  return onClose;
}

describe('MoveDiskDialog', () => {
  it('lists every drive and names why the ones that cannot take the disk are refused', async () => {
    await open(source());
    const e = screen.getByRole<HTMLInputElement>('radio', { name: /^E:/ });
    expect(e.disabled).toBe(false);
    expect(e.checked).toBe(true);
    expect(screen.getByRole<HTMLInputElement>('radio', { name: /^C:/ }).disabled).toBe(true);
    expect(screen.getByRole<HTMLInputElement>('radio', { name: /^D:/ }).disabled).toBe(true);
    expect(screen.getByText(/already on this drive/)).toBeTruthy();
    expect(screen.getByText(/can be unplugged/)).toBeTruthy();
    expect(screen.getByText(/not enough free space/)).toBeTruthy();
  });

  it('says no drive can take the disk and offers no way forward when none is eligible', async () => {
    const stuck: MovableDisk = {
      ...UBUNTU,
      targets: [target('C:', 'sameDrive'), target('D:', 'notNtfs')],
    };
    await open(source({ plan: vi.fn(async () => plan([stuck])) }));
    expect(screen.getByText(/No drive on this PC can take this disk/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull();
    expect(screen.getByRole<HTMLInputElement>('radio', { name: /^C:/ }).disabled).toBe(true);
    expect(screen.getByRole<HTMLInputElement>('radio', { name: /^D:/ }).disabled).toBe(true);
  });

  it('will not move until the backup box is ticked, then sends only the plan, disk and drive ids', async () => {
    const src = source();
    await open(src);
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByText(/cannot be interrupted safely/)).toBeTruthy();
    expect(screen.getByText(/You can move it back/)).toBeTruthy();
    expect(screen.getByText(/Ubuntu-24.04 will be stopped while it moves/)).toBeTruthy();
    const run = screen.getByRole<HTMLButtonElement>('button', { name: /Move now/ });
    expect(run.disabled).toBe(true);
    fireEvent.click(screen.getByRole('checkbox', { name: /I have a backup/ }));
    await act(async () => {
      fireEvent.click(run);
    });
    expect(src.run).toHaveBeenCalledWith(1, '1-0', 'E:', true);
    expect(await screen.findByText('Ubuntu-24.04 now lives on E:')).toBeTruthy();
    expect(screen.getAllByText('Passed')).toHaveLength(4);
  });

  it('shows the copy progress and cannot be closed while the disk is copying', async () => {
    let emit: (p: MoveProgress) => void = () => undefined;
    let finish: (r: MoveReport) => void = () => undefined;
    const src = source({
      onProgress: vi.fn(async (listener) => {
        emit = listener;
        return () => undefined;
      }),
      run: vi.fn(
        () =>
          new Promise<MoveReport>((resolve) => {
            finish = resolve;
          }),
      ),
    });
    const onClose = await open(src);
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    fireEvent.click(screen.getByRole('checkbox', { name: /I have a backup/ }));
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Move now/ }));
    });
    act(() => {
      emit({ stage: 'moving', done: 30 * GB, total: 120 * GB });
    });
    expect(screen.getByText('Copying the disk to E:')).toBeTruthy();
    expect(screen.getByRole('progressbar').getAttribute('aria-valuetext')).toMatch(/copied/);
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    await act(async () => {
      finish(MOVED);
    });
    expect(await screen.findByText('Ubuntu-24.04 now lives on E:')).toBeTruthy();
  });

  it('reports a move WSL rolled back as nothing moved, with what WSL said', async () => {
    const failed: MoveReport = {
      ...MOVED,
      status: 'notMoved',
      message: 'Wsl/Service/MoveDistro/E_ACCESSDENIED',
      verification: {
        registeredAtTarget: null,
        fileAtTarget: null,
        oldFileGone: null,
        starts: null,
        freedBytes: null,
      },
    };
    await open(source({ run: vi.fn(async () => failed) }));
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    fireEvent.click(screen.getByRole('checkbox', { name: /I have a backup/ }));
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Move now/ }));
    });
    expect(await screen.findByText('Nothing was moved')).toBeTruthy();
    expect(screen.getByText('WSL said: Wsl/Service/MoveDistro/E_ACCESSDENIED')).toBeTruthy();
    expect(screen.getByText(/Full control of the new folder/)).toBeTruthy();
    expect(screen.queryByText('Passed')).toBeNull();
  });

  it('says a refused move stopped nothing', async () => {
    await open(
      source({
        run: vi.fn(async () => {
          throw { kind: 'refused', message: 'E: cannot take Ubuntu-24.04' };
        }),
      }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    fireEvent.click(screen.getByRole('checkbox', { name: /I have a backup/ }));
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Move now/ }));
    });
    expect((await screen.findByRole('alert')).textContent).toMatch(
      /refused before anything stopped/,
    );
  });

  it('guides Docker Desktop through its own setting and checks where the disk went', async () => {
    const moved: DockerDisk = { ...DOCKER, path: 'E:\\Vitals\\Docker\\disk\\docker_data.vhdx' };
    const src = source({ checkDocker: vi.fn(async () => moved) });
    await open(src, 'docker-desktop');
    expect(screen.getByText('Docker Desktop moves its own disk')).toBeTruthy();
    expect(screen.getByText(/pick E:\\Vitals\\Docker/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Move now|Next/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Open Docker Desktop' }));
    expect(src.openDocker).toHaveBeenCalledOnce();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Check again/ }));
    });
    expect(await screen.findByText('Docker Desktop’s disk is no longer on C:.')).toBeTruthy();
    expect(src.run).not.toHaveBeenCalled();
  });

  it('warns that an old Docker Desktop fails moves to another drive', async () => {
    const old: MovePlan = {
      ...plan([DOCKER_DISK]),
      docker: { ...DOCKER, version: '4.91.0', crossDriveOk: false },
    };
    await open(source({ plan: vi.fn(async () => old) }), 'docker-desktop');
    expect(screen.getByText(/\(4\.91\.0\) fails moves to another drive/)).toBeTruthy();
  });

  it('explains why a WSL that is too old cannot move a distro', async () => {
    const old: MovableDisk = { ...UBUNTU, unavailable: 'wslTooOld' };
    await open(source({ plan: vi.fn(async () => plan([old])) }));
    expect(screen.getByText(/2\.3\.11 or later is needed/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull();
  });

  it('ends a failed plan in an error that can be closed', async () => {
    const onClose = vi.fn();
    render(
      <MoveDiskDialog
        mount="C:"
        focus={null}
        locale="en"
        onClose={onClose}
        source={source({
          plan: vi.fn(async () => {
            throw { kind: 'not-found', message: 'C: is not a mounted drive' };
          }),
        })}
      />,
    );
    expect((await screen.findByRole('alert')).textContent).toMatch(/not a mounted drive/);
    const footerClose = screen.getAllByRole('button', { name: 'Close' }).at(-1);
    if (footerClose === undefined) throw new Error('no Close button');
    fireEvent.click(footerClose);
    expect(onClose).toHaveBeenCalled();
  });

  it('speaks Romanian', async () => {
    await i18n.changeLanguage('ro');
    render(
      <MoveDiskDialog mount="C:" focus={null} locale="ro" onClose={vi.fn()} source={source()} />,
    );
    expect(await screen.findByText('Ce disc')).toBeTruthy();
    expect(screen.getByText('Unde')).toBeTruthy();
  });
});
