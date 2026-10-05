import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { Connection } from '@vitals/protocol';

import { makeProcess, makeProcessMap, makeSystem } from '../dashboard/test-fixtures';
import { createManualSystemSource } from '../dashboard/useSystemSnapshot';
import { ConnectionsScreen } from './ConnectionsScreen';
import { registerConnectionStrings } from './strings';
import type { ConnectionsSnapshot } from './useConnections';

beforeAll(async () => {
  await initI18n();
  registerConnectionStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function makeConnection(overrides: Partial<Connection> = {}): Connection {
  return {
    protocol: 'tcp',
    localAddress: '192.168.1.20',
    localPort: 51234,
    remoteAddress: '140.82.121.4',
    remotePort: 443,
    state: 'established',
    ownerPid: 1000,
    ownerName: null,
    remoteHost: null,
    country: null,
    bytesSent: null,
    bytesReceived: null,
    ...overrides,
  };
}

/** A process source that resolves the PIDs used below to names. */
function processSource() {
  const source = createManualSystemSource();
  source.push({
    system: makeSystem(),
    processes: makeProcessMap([
      makeProcess({ key: { pid: 1000, startTime: 1 }, name: 'chrome.exe' }),
      makeProcess({ key: { pid: 1001, startTime: 1 }, name: 'chrome.exe' }),
      makeProcess({ key: { pid: 2000, startTime: 1 }, name: 'sshd.exe' }),
    ]),
  });
  return source;
}

async function mount(snapshot: ConnectionsSnapshot) {
  const reader = vi.fn<() => Promise<ConnectionsSnapshot>>().mockResolvedValue(snapshot);
  render(<ConnectionsScreen reader={reader} processSource={processSource()} />);
  await screen.findByRole('heading', { name: 'Network connections' });
  return { reader };
}

const DEFAULT: ConnectionsSnapshot = {
  connections: [
    makeConnection({ ownerPid: 1000, localPort: 51234, remoteAddress: '140.82.121.4' }),
    makeConnection({ ownerPid: 1001, localPort: 51235, remoteAddress: '140.82.121.4' }),
    makeConnection({
      ownerPid: 2000,
      localPort: 22,
      localAddress: '0.0.0.0',
      remoteAddress: null,
      remotePort: null,
      state: 'listen',
    }),
  ],
  byProcess: [],
};

describe('ConnectionsScreen', () => {
  it('shows skeletons until the first read completes', () => {
    // A promise that never settles: the point is that the screen renders a
    // loading state rather than an empty list claiming no connections exist.
    render(
      <ConnectionsScreen
        reader={() => new Promise(() => undefined)}
        processSource={processSource()}
      />,
    );

    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('groups sockets by the program that owns them', async () => {
    // Two chrome PIDs collapse into one row. A flat socket list is what
    // `netstat -ano` already gives and it answers nothing.
    await mount(DEFAULT);

    expect(screen.getByText('chrome.exe')).toBeTruthy();
    expect(screen.getByText('sshd.exe')).toBeTruthy();
    expect(screen.getByText('2 connections')).toBeTruthy();
  });

  it('names a group whose owner could not be resolved', async () => {
    await mount({
      connections: [makeConnection({ ownerPid: 99999 })],
      byProcess: [],
    });

    expect(screen.getByText('Unknown program')).toBeTruthy();
  });

  it('reports a listener reachable from the network', async () => {
    // An observation, not an accusation: this module has no threat
    // intelligence, and a false positive teaches the user to ignore badges.
    await mount(DEFAULT);
    expect(screen.getByText('Reachable from the network')).toBeTruthy();
  });

  it('reveals the sockets when a group is expanded', async () => {
    await mount(DEFAULT);

    expect(screen.queryByRole('table')).toBeNull();
    fireEvent.click(screen.getByRole('button', { expanded: false, name: /sshd\.exe/ }));

    const table = screen.getByRole('table');
    expect(within(table).getByText('0.0.0.0:22')).toBeTruthy();
  });

  describe('filtering', () => {
    it('narrows to listening sockets', async () => {
      await mount(DEFAULT);

      fireEvent.click(screen.getByRole('radio', { name: 'Listening' }));

      expect(screen.getByText('sshd.exe')).toBeTruthy();
      expect(screen.queryByText('chrome.exe')).toBeNull();
    });

    it('narrows to external peers', async () => {
      await mount({
        connections: [
          makeConnection({ ownerPid: 1000, remoteAddress: '8.8.8.8' }),
          makeConnection({ ownerPid: 2000, localPort: 9, remoteAddress: '10.0.0.5' }),
        ],
        byProcess: [],
      });

      fireEvent.click(screen.getByRole('radio', { name: 'External' }));

      expect(screen.getByText('chrome.exe')).toBeTruthy();
      expect(screen.queryByText('sshd.exe')).toBeNull();
    });

    it('searches by application name', async () => {
      await mount(DEFAULT);

      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'sshd' } });

      expect(screen.getByText('sshd.exe')).toBeTruthy();
      expect(screen.queryByText('chrome.exe')).toBeNull();
    });

    it('searches a port without matching it as a substring', async () => {
      // "22" must not also match 51234 or any port containing 22.
      await mount(DEFAULT);

      fireEvent.change(screen.getByRole('searchbox'), { target: { value: '22' } });

      expect(screen.getByText('sshd.exe')).toBeTruthy();
      expect(screen.queryByText('chrome.exe')).toBeNull();
    });

    it('says nothing matched rather than showing an empty list', async () => {
      await mount(DEFAULT);

      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'nothing-here' } });

      expect(screen.getByText('No connection matches')).toBeTruthy();
    });
  });

  describe('failure handling', () => {
    it('reaches a terminal state when the very first read fails', async () => {
      // The rule this project keeps relearning: a loading state must resolve
      // either way. A failure before any success has nothing to show, so it
      // must at least stop claiming to be loading.
      const reader = vi
        .fn<() => Promise<ConnectionsSnapshot>>()
        .mockRejectedValue(new Error('table read failed'));

      render(<ConnectionsScreen reader={reader} processSource={processSource()} />);

      // Header present means it left the skeleton branch.
      expect(await screen.findByRole('heading', { name: 'Network connections' })).toBeTruthy();
      expect(document.querySelector('[aria-busy="true"]')).toBeNull();
      expect(screen.getByRole('alert')).toBeTruthy();
    });

    it('keeps the last good list when a refresh fails', async () => {
      // Blanking a list the user is reading is worse than showing old data
      // that is labelled as old.
      const reader = vi
        .fn<() => Promise<ConnectionsSnapshot>>()
        .mockResolvedValueOnce(DEFAULT)
        .mockRejectedValue(new Error('table read failed'));

      render(<ConnectionsScreen reader={reader} processSource={processSource()} />);
      await screen.findByText('chrome.exe');

      fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
      expect(await screen.findByRole('alert')).toBeTruthy();

      // Still there.
      expect(screen.getByText('chrome.exe')).toBeTruthy();
    });
  });

  it('renders in Romanian without falling back to key paths', async () => {
    await i18n.changeLanguage('ro');

    const reader = vi.fn<() => Promise<ConnectionsSnapshot>>().mockResolvedValue(DEFAULT);
    render(<ConnectionsScreen reader={reader} processSource={processSource()} />);

    expect(await screen.findByRole('heading', { name: 'Conexiuni de rețea' })).toBeTruthy();
    expect(screen.getByRole('radio', { name: 'În ascultare' })).toBeTruthy();
  });
});

describe('ConnectionsScreen row menus', () => {
  const menuItems = async (): Promise<(string | null)[]> =>
    within(await screen.findByRole('menu'))
      .getAllByRole('menuitem')
      .map((item) => item.textContent);

  it('opens a group menu on a right click with expand, copies and refresh', async () => {
    await mount(DEFAULT);

    fireEvent.contextMenu(screen.getByRole('button', { name: /chrome\.exe/ }), {
      button: 2,
      clientX: 5,
      clientY: 5,
    });
    await act(async () => {});

    expect(await menuItems()).toEqual([
      'Show connections',
      'Copy program name',
      'Copy process IDs',
      'Copy remote addresses',
      'Refresh',
    ]);
  });

  it('does not open a group menu on a left-button contextmenu', async () => {
    await mount(DEFAULT);

    fireEvent.contextMenu(screen.getByRole('button', { name: /chrome\.exe/ }), {
      button: 0,
      clientX: 0,
      clientY: 0,
    });
    await act(async () => {});

    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('opens a socket menu whose remote copy is disabled for a listener with no peer', async () => {
    await mount(DEFAULT);
    fireEvent.click(screen.getByRole('button', { expanded: false, name: /sshd\.exe/ }));

    const row = within(screen.getByRole('table')).getByText('0.0.0.0:22').closest('tr');
    if (row === null) throw new Error('no socket row');
    fireEvent.contextMenu(row, { button: 2, clientX: 5, clientY: 5 });
    await act(async () => {});

    expect(await menuItems()).toEqual([
      'Copy remote address',
      'Copy local address',
      'Copy program name and PID',
    ]);
    const remote = within(screen.getByRole('menu'))
      .getByText('Copy remote address')
      .closest('[role="menuitem"]');
    expect(remote?.getAttribute('aria-disabled')).toBe('true');
  });
});
