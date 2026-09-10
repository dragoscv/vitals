import { render, screen, waitFor } from '@testing-library/react';
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
