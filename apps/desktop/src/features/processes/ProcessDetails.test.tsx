/**
 * The detail panel's on-demand sections.
 *
 * What these guard: that an unreadable efficiency state is never rendered as
 * "off"; that the handle and module lists cost nothing until the user asks
 * for them; and that affinity presets appear only when the machine can say
 * which cores are which.
 */

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';
import type { HandleInfo, HostInfo } from '@vitals/protocol';

import type { ProcessActionsApi } from './actions';
import type { ProcessRow } from './model';
import { LAZY_LIST_CAP, ProcessDetails } from './ProcessDetails';
import { registerProcessesStrings } from './strings';
import { makeProcess } from './test-fixtures';

beforeAll(async () => {
  await initI18n('en');
  registerProcessesStrings();
});

afterEach(cleanup);

function stubActions(overrides: Partial<ProcessActionsApi> = {}): ProcessActionsApi {
  return {
    planTerminate: vi.fn(),
    planSuspend: vi.fn(),
    terminate: vi.fn(async () => undefined),
    suspend: vi.fn(async () => undefined),
    resume: vi.fn(async () => undefined),
    setPriority: vi.fn(async () => undefined),
    setAffinity: vi.fn(async () => undefined),
    getEfficiencyMode: vi.fn(async () => null),
    setEfficiencyMode: vi.fn(async () => undefined),
    getHandles: vi.fn(async () => []),
    getModules: vi.fn(async () => []),
    getExecutablePath: vi.fn(async () => null),
    openFileLocation: vi.fn(async () => undefined),
    showFileProperties: vi.fn(async () => undefined),
    runAsAdmin: vi.fn(async () => undefined),
    ...overrides,
  };
}

function row(pid = 300): ProcessRow {
  const process = makeProcess({ pid, name: 'chrome.exe' });
  return {
    id: `${pid}:${process.key.startTime}`,
    process,
    depth: 0,
    childIds: [],
    descendantCount: 0,
    rolledCpu: process.cpu,
    rolledMemory: process.memoryPrivate,
    rolledDisk: 0,
    rolledNetwork: null,
    rolledGpu: null,
  };
}

function host(overrides: Partial<HostInfo> = {}): HostInfo {
  return {
    hostname: 'box',
    osName: 'Windows',
    osVersion: '11',
    kernelVersion: '22631',
    architecture: 'x86_64',
    cpuModel: 'test',
    cpuVendor: 'test',
    physicalCores: 4,
    logicalCores: 8,
    coreTopology: null,
    totalMemory: 0,
    bootTimeMs: 0,
    isVirtualMachine: false,
    motherboard: null,
    biosVersion: null,
    ...overrides,
  };
}

function mount(
  actions: ProcessActionsApi = stubActions(),
  options: { host?: HostInfo | null; path?: string | null } = {},
) {
  const onFailure = vi.fn();
  render(
    <ProcessDetails
      row={row()}
      locale="en"
      actions={actions}
      host={options.host ?? null}
      executablePath={options.path ?? null}
      onFailure={onFailure}
    />,
  );
  return { actions, onFailure };
}

/** Lets the pending `getEfficiencyMode` promise settle into state. */
async function flush(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe('efficiency mode', () => {
  it('renders an unreadable state as unavailable with a disabled switch, never as off', async () => {
    // `null` from the backend means "we were denied a handle". Showing "Off"
    // would offer to turn on a throttle the same denial will refuse to set.
    mount(stubActions({ getEfficiencyMode: vi.fn(async () => null) }));
    await flush();

    expect(screen.getByText('Cannot be read for this process')).toBeTruthy();
    expect(screen.queryByText('Off')).toBeNull();
    const control = screen.getByRole('switch');
    expect(control.hasAttribute('disabled')).toBe(true);
    expect(control.getAttribute('aria-checked')).toBe('false');
  });

  it('renders a readable false as Off with an enabled switch', async () => {
    mount(stubActions({ getEfficiencyMode: vi.fn(async () => false) }));
    await flush();

    expect(screen.getByText('Off')).toBeTruthy();
    expect(screen.getByRole('switch').hasAttribute('disabled')).toBe(false);
  });

  it('toggling sends the pid and start time, then re-reads the state', async () => {
    const getEfficiencyMode = vi
      .fn<ProcessActionsApi['getEfficiencyMode']>()
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    const { actions } = mount(stubActions({ getEfficiencyMode }));
    await flush();

    fireEvent.click(screen.getByRole('switch'));

    await waitFor(() => expect(actions.setEfficiencyMode).toHaveBeenCalledOnce());
    const [process, enabled] = vi.mocked(actions.setEfficiencyMode).mock.calls[0] ?? [];
    expect(process?.key).toEqual({ pid: 300, startTime: 300_000 });
    expect(enabled).toBe(true);

    // The re-read is what makes the switch show the truth rather than the
    // click — the backend can refuse and leave the state unchanged.
    await waitFor(() => expect(getEfficiencyMode).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByText('On')).toBeTruthy());
  });

  it('reports a refused toggle through the screen failure surface', async () => {
    const { onFailure } = mount(
      stubActions({
        getEfficiencyMode: vi.fn(async () => false),
        setEfficiencyMode: vi.fn(async () => {
          throw { kind: 'access-denied', message: 'no' };
        }),
      }),
    );
    await flush();

    fireEvent.click(screen.getByRole('switch'));

    await waitFor(() => expect(onFailure).toHaveBeenCalledOnce());
    expect(onFailure.mock.calls[0]?.[0]).toContain('no');
  });
});

describe('handles and modules', () => {
  it('does not fetch handles or modules until the section is expanded', async () => {
    // The backend walks the whole system handle table. Fetching it for every
    // row the user arrows past would make the panel the most expensive thing
    // on the screen.
    const { actions } = mount();
    await flush();

    expect(actions.getHandles).not.toHaveBeenCalled();
    expect(actions.getModules).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('handles-toggle'));
    await waitFor(() => expect(actions.getHandles).toHaveBeenCalledOnce());
    expect(actions.getModules).not.toHaveBeenCalled();
  });

  it('shows the count in the header once loaded and caps the rendered rows', async () => {
    const many: HandleInfo[] = Array.from({ length: LAZY_LIST_CAP + 50 }, (_, i) => ({
      pid: 300,
      value: i,
      kind: 'File',
      name: `\\Device\\HarddiskVolume3\\file-${i}`,
      grantedAccess: 0x12_00_89,
    }));
    mount(stubActions({ getHandles: vi.fn(async () => many) }));
    await flush();

    fireEvent.click(screen.getByTestId('handles-toggle'));

    await waitFor(() =>
      expect(
        screen.getByText(`Handles · ${(LAZY_LIST_CAP + 50).toLocaleString('en')}`),
      ).toBeTruthy(),
    );
    expect(screen.getAllByRole('listitem')).toHaveLength(LAZY_LIST_CAP);
    expect(
      screen.getByText(`Showing the first ${LAZY_LIST_CAP} of ${LAZY_LIST_CAP + 50}.`),
    ).toBeTruthy();
  });

  it('renders a handle without a name or type honestly rather than blank', async () => {
    mount(
      stubActions({
        getHandles: vi.fn(async () => [
          { pid: 300, value: 4, kind: null, name: null, grantedAccess: 0 },
        ]),
      }),
    );
    await flush();
    fireEvent.click(screen.getByTestId('handles-toggle'));

    expect(await screen.findByText('(unknown type)')).toBeTruthy();
    expect(screen.getByText('(unnamed)')).toBeTruthy();
  });

  it('shows the backend error instead of an empty list when the fetch fails', async () => {
    mount(
      stubActions({
        getModules: vi.fn(async () => {
          throw { kind: 'access-denied', message: 'PROCESS_VM_READ denied' };
        }),
      }),
    );
    await flush();
    fireEvent.click(screen.getByTestId('modules-toggle'));

    expect((await screen.findByRole('alert')).textContent).toContain('PROCESS_VM_READ denied');
  });
});

describe('affinity presets', () => {
  it('offers nothing without host info', async () => {
    mount(stubActions(), { host: null });
    await flush();
    expect(screen.queryByTestId('affinity-presets')).toBeNull();
  });

  it('offers the topology-free presets on a uniform machine', async () => {
    mount(stubActions(), { host: host({ coreTopology: null, logicalCores: 8 }) });
    await flush();

    expect(screen.getByRole('button', { name: 'All cores (8)' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'First half (4)' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Second half (4)' })).toBeTruthy();
    // No P/E split exists on this machine, so no button may claim one.
    expect(screen.queryByRole('button', { name: /Performance cores/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /Efficiency cores/ })).toBeNull();
  });

  it('offers performance and efficiency presets when the topology is known', async () => {
    const { actions } = mount(stubActions(), {
      host: host({
        logicalCores: 6,
        coreTopology: [
          'performance',
          'performance',
          'performance',
          'performance',
          'efficiency',
          'efficiency',
        ],
      }),
    });
    await flush();

    fireEvent.click(screen.getByRole('button', { name: 'Efficiency cores only (2)' }));

    await waitFor(() => expect(actions.setAffinity).toHaveBeenCalledOnce());
    const [, mask] = vi.mocked(actions.setAffinity).mock.calls[0] ?? [];
    // Bits 4 and 5: the two E-cores, nothing else.
    expect(mask).toBe(0b110000n);
  });
});

describe('file actions', () => {
  it('disables both buttons when the path is unknown', async () => {
    mount(stubActions(), { path: null });
    await flush();

    const open = screen.getByRole('button', { name: 'Open file location' });
    const properties = screen.getByRole('button', { name: 'Properties' });
    expect(open.hasAttribute('disabled')).toBe(true);
    expect(properties.hasAttribute('disabled')).toBe(true);
  });

  it('passes the known path to the shell actions', async () => {
    const { actions } = mount(stubActions(), { path: 'C:\\Apps\\chrome.exe' });
    await flush();

    fireEvent.click(screen.getByRole('button', { name: 'Properties' }));
    await waitFor(() =>
      expect(actions.showFileProperties).toHaveBeenCalledWith('C:\\Apps\\chrome.exe'),
    );
  });
});
