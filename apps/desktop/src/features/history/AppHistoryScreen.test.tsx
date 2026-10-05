import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { AppHistoryScreen } from './AppHistoryScreen';
import { registerHistoryStrings } from './strings';
import type { AppHistorySnapshot } from './model';

// Mock Tooltip components to avoid complex UI interactions
vi.mock('@vitals/ui', async () => {
  const actual = await vi.importActual('@vitals/ui');
  return {
    ...actual,
    Tooltip: ({ children }: { children: React.ReactNode }) => <>{children}</>,
    TooltipTrigger: ({ children }: { children: React.ReactNode }) => <>{children}</>,
    TooltipContent: () => null,
    TooltipProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  };
});

beforeEach(async () => {
  await initI18n('en');
  registerHistoryStrings();
});

const stubSnapshot = (count: number): AppHistorySnapshot => ({
  records: Array.from({ length: count }, (_, i) => ({
    executable: `C:\\app${i}.exe`,
    name: `app${i}.exe`,
    cpuSeconds: 100 + i * 10,
    diskReadBytes: 1024 * (i + 1),
    diskWriteBytes: 2048 * (i + 1),
    peakPrivateBytes: 4096 * (i + 1),
    firstSeen: Date.parse('2026-01-01T00:00:00Z'),
    lastSeen: Date.parse(`2026-01-0${i + 1}T12:00:00Z`),
    sessions: i + 1,
  })),
});

describe('AppHistoryScreen', () => {
  it('renders populated history', async () => {
    const reader = async () => stubSnapshot(3);

    render(<AppHistoryScreen reader={reader} />);

    await waitFor(() => {
      expect(screen.getByText('app0.exe')).toBeDefined();
    });

    expect(screen.getByText('app1.exe')).toBeDefined();
    expect(screen.getByText('app2.exe')).toBeDefined();
  });

  it('shows empty state when history is empty', async () => {
    const reader = async () => ({ records: [] });

    render(<AppHistoryScreen reader={reader} />);

    await waitFor(() => {
      expect(screen.getByText(i18n.t('empty.title', { ns: 'history' }))).toBeDefined();
    });
  });

  it('displays summary with totals', async () => {
    const reader = async () => stubSnapshot(2);

    render(<AppHistoryScreen reader={reader} />);

    await waitFor(() => {
      const text = screen.getByText(/2 applications/i);
      expect(text).toBeDefined();
    });
  });
});

describe('AppHistoryScreen row menu', () => {
  const rowOf = (name: string): HTMLElement => {
    const row = screen.getByText(name).closest('tr');
    if (row === null) throw new Error(`no row for ${name}`);
    return row;
  };

  it('opens on a right click with copy, search, export, refresh and clear', async () => {
    render(<AppHistoryScreen reader={async () => stubSnapshot(2)} />);
    await screen.findByText('app0.exe');

    fireEvent.contextMenu(rowOf('app0.exe'), { button: 2, clientX: 5, clientY: 5 });
    await act(async () => {});

    const items = within(await screen.findByRole('menu'))
      .getAllByRole('menuitem')
      .map((item) => item.textContent);
    expect(items).toEqual([
      'Copy details',
      'Search online',
      'Export the list as CSV',
      'Export the list as JSON',
      'Refresh',
      'Clear history',
    ]);
  });

  it('does not open on a left-button contextmenu', async () => {
    render(<AppHistoryScreen reader={async () => stubSnapshot(1)} />);
    await screen.findByText('app0.exe');

    fireEvent.contextMenu(rowOf('app0.exe'), { button: 0, clientX: 0, clientY: 0 });
    await act(async () => {});

    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('asks for the same confirmation as the toolbar before clearing from the menu', async () => {
    const clearer = vi.fn(async () => undefined);
    render(<AppHistoryScreen reader={async () => stubSnapshot(1)} clearer={clearer} />);
    await screen.findByText('app0.exe');

    fireEvent.contextMenu(rowOf('app0.exe'), { button: 2, clientX: 5, clientY: 5 });
    await act(async () => {});
    fireEvent.click(within(await screen.findByRole('menu')).getByText('Clear history'));

    expect(await screen.findByRole('dialog')).toBeTruthy();
    expect(clearer).not.toHaveBeenCalled();
  });
});
