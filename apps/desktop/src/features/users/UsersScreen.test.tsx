import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import type { LogonSession, UsersSnapshot } from './model';
import { registerUsersStrings } from './strings';
import { UsersScreen } from './UsersScreen';

beforeAll(async () => {
  await initI18n();
  registerUsersStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function session(overrides: Partial<LogonSession> = {}): LogonSession {
  return {
    sessionId: 1,
    userName: 'Alice',
    domain: 'CORP',
    clientName: null,
    state: 'active',
    logonTime: null,
    isServices: false,
    ...overrides,
  };
}

function snapshot(overrides: Partial<UsersSnapshot> = {}): UsersSnapshot {
  return { sessions: [session()], rollups: [], ...overrides };
}

async function mount(data = snapshot()) {
  const reader = vi.fn<() => Promise<UsersSnapshot>>().mockResolvedValue(data);
  render(<UsersScreen reader={reader} />);
  await screen.findByRole('heading', { level: 2 });
  return { reader };
}

describe('UsersScreen', () => {
  it('shows skeletons until the read settles', () => {
    render(<UsersScreen reader={() => new Promise(() => undefined)} />);
    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('reaches a terminal state when the read fails', async () => {
    // The rule this project keeps relearning: a loading state must resolve
    // either way, or a broken app looks merely busy.
    const reader = vi
      .fn<() => Promise<UsersSnapshot>>()
      .mockRejectedValue(new Error('WTS enumeration failed'));

    render(<UsersScreen reader={reader} />);

    await screen.findByText(/Failed to load sessions/);
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('renders an empty state when no sessions exist', async () => {
    const reader = vi
      .fn<() => Promise<UsersSnapshot>>()
      .mockResolvedValue(snapshot({ sessions: [] }));
    render(<UsersScreen reader={reader} />);
    expect(await screen.findByText(/No sessions/)).toBeTruthy();
  });

  it('lists sessions with their display names', async () => {
    await mount(
      snapshot({
        sessions: [
          session({ sessionId: 1, userName: 'Alice', domain: 'CORP' }),
          session({ sessionId: 2, userName: 'Bob', domain: null }),
        ],
      }),
    );

    expect(screen.getByText('CORP\\Alice')).toBeTruthy();
    expect(screen.getByText('Bob')).toBeTruthy();
  });

  it('shows session state with appropriate badge', async () => {
    await mount(
      snapshot({
        sessions: [session({ state: 'active' }), session({ sessionId: 2, state: 'disconnected' })],
      }),
    );

    expect(screen.getByText('Active')).toBeTruthy();
    expect(screen.getByText('Disconnected')).toBeTruthy();
  });

  it('shows client name for remote sessions', async () => {
    await mount(
      snapshot({
        sessions: [session({ clientName: 'LAPTOP-001' })],
      }),
    );

    expect(screen.getByText('LAPTOP-001')).toBeTruthy();
  });

  it('marks session 0 as Services', async () => {
    await mount(
      snapshot({
        sessions: [session({ sessionId: 0, userName: null, isServices: true })],
      }),
    );

    expect(screen.getByText('Services')).toBeTruthy();
    expect(screen.getByText('Session 0 (Services)')).toBeTruthy();
  });

  it('shows logon time when available', async () => {
    const logonTime = Math.floor(new Date('2024-01-01T10:00:00Z').getTime() / 1000);
    await mount(
      snapshot({
        sessions: [session({ logonTime })],
      }),
    );

    // The exact format depends on the locale, but some representation of the
    // date should appear. Check for the date components (year can be 2-digit).
    const body = document.body.textContent ?? '';
    expect(body).toMatch(/1\/1\/24|2024/);
  });

  it('shows placeholder when logon time is unavailable', async () => {
    await mount(
      snapshot({
        sessions: [session({ logonTime: null })],
      }),
    );

    // The field should exist with a dash or placeholder, not an actual time.
    const logonTimeField = screen.getByText('Logon time').parentElement;
    expect(logonTimeField?.textContent).toMatch(/—/);
  });

  it('shows process count and resource rollup when available', async () => {
    await mount(
      snapshot({
        sessions: [session({ sessionId: 1 })],
        rollups: [
          {
            sessionId: 1,
            processCount: 42,
            cpuPercent: 12.5,
            memoryBytes: 1024 * 1024 * 512, // 512 MB
          },
        ],
      }),
    );

    expect(screen.getByText('42')).toBeTruthy(); // process count
    // CPU and memory are formatted, check for the raw numbers in the text
    const body = document.body.textContent ?? '';
    expect(body).toMatch(/12/); // CPU %
    expect(body).toMatch(/512/); // Memory MB
  });

  it('shows placeholder when rollup is unavailable', async () => {
    await mount(
      snapshot({
        sessions: [session({ sessionId: 1 })],
        rollups: [], // No rollup for this session
      }),
    );

    // Process, CPU, and memory fields should have placeholders, not zeros.
    const processField = screen.getByText('Processes').parentElement;
    expect(processField?.textContent).toMatch(/—/);
  });

  it('counts interactive sessions correctly', async () => {
    await mount(
      snapshot({
        sessions: [
          session({ sessionId: 1, state: 'active' }),
          session({ sessionId: 2, state: 'disconnected' }),
          session({ sessionId: 0, isServices: true }),
          session({ sessionId: 65536, state: 'listen' }),
        ],
      }),
    );

    expect(screen.getByText(/2 interactive/)).toBeTruthy();
  });

  it('filters sessions by search query', async () => {
    await mount(
      snapshot({
        sessions: [
          session({ sessionId: 1, userName: 'Alice' }),
          session({ sessionId: 2, userName: 'Bob' }),
        ],
      }),
    );

    expect(screen.getByText('CORP\\Alice')).toBeTruthy();
    expect(screen.getByText('CORP\\Bob')).toBeTruthy();

    // Typing has to be the thing under test. An earlier version of this
    // asserted only that the input existed, which would have passed just as
    // happily if filtering did nothing at all.
    fireEvent.change(screen.getByPlaceholderText(/Filter sessions/), {
      target: { value: 'bob' },
    });

    await waitFor(() => {
      expect(screen.queryByText('CORP\\Alice')).toBeNull();
    });
    expect(screen.getByText('CORP\\Bob')).toBeTruthy();
  });
});
