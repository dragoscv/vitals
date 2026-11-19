/**
 * Which General rows this build shows.
 *
 * Two of them are Windows-only in the backend — the Task Manager replacement
 * (IFEO) and Start with Windows — and a switch that moves while doing nothing
 * is the failure this file exists to prevent. The platform arrives from the
 * same store the shell fills from `get_capabilities`.
 */

import { act, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerShellStrings } from '../shell/strings';
import { useNavVisibility } from '../shell/navigation';
import { GeneralPanel } from './panels';

// The panel reads the Task Manager hook, which asks the host. The host is off
// by default — the hook answers "unavailable", which is what most rows here
// are tested around — and switched on only for the replacement tests below.
const host = vi.hoisted(() => ({ present: false }));
vi.mock('../shell/host', () => ({ hasTauriHost: () => host.present }));

const backend = vi.hoisted((): { answers: Record<string, unknown> } => ({ answers: {} }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => Promise.resolve(backend.answers[cmd] ?? null)),
}));

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

// The direct build, so the panel renders its own rows rather than the
// "updates come from the Store" path.
beforeEach(() => {
  host.present = false;
  backend.answers = { get_distribution: { packaged: false } };
});

afterEach(() => {
  host.present = false;
  useNavVisibility.setState({ platform: null });
});

function setPlatform(platform: 'windows' | 'macos' | null): void {
  useNavVisibility.setState({ platform });
}

describe('General rows on a platform that has no command for them', () => {
  it('shows the Windows rows when the platform is Windows', () => {
    setPlatform('windows');
    render(<GeneralPanel />);
    expect(screen.getByText('Start with Windows')).toBeTruthy();
    // By role: the row label and the button carry the same words.
    expect(screen.getByRole('button', { name: 'Open Windows Task Manager' })).toBeTruthy();
  });

  it('shows them while the platform is still unknown, so a Windows launch never flickers', () => {
    setPlatform(null);
    render(<GeneralPanel />);
    expect(screen.getByText('Start with Windows')).toBeTruthy();
  });

  it('hides them on macOS', () => {
    setPlatform('macos');
    render(<GeneralPanel />);
    expect(screen.queryByText('Start with Windows')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Open Windows Task Manager' })).toBeNull();
    expect(screen.queryByText('Replace Task Manager')).toBeNull();
    // The rows a Mac does have are still there.
    expect(screen.getByText('Close to tray')).toBeTruthy();
  });
});

describe('the Task Manager row while the replacement is on', () => {
  it('warns at the switch that moving to the account will turn it off', async () => {
    // The move lives in About; nothing there is visible from this row, so the
    // coupling is only discoverable if it is stated where the setting lives.
    setPlatform('windows');
    host.present = true;
    backend.answers.get_taskmgr_replacement = {
      enabled: true,
      replacedBy: null,
      path: 'C:\\Program Files\\Vitals\\vitals-desktop.exe',
    };

    render(<GeneralPanel />);

    await waitFor(() => {
      expect(screen.getByText(/Moving Vitals to your account turns this off/)).toBeTruthy();
    });
  });

  it('says nothing about moving while the replacement is off', async () => {
    setPlatform('windows');
    host.present = true;
    backend.answers.get_taskmgr_replacement = { enabled: false, replacedBy: null, path: null };

    render(<GeneralPanel />);
    // Absence after an explicit flush: a `waitFor` would pass on its first
    // poll against state React has not committed yet.
    await act(async () => {
      await Promise.resolve();
    });

    expect(screen.getByText('Replace Task Manager')).toBeTruthy();
    expect(screen.queryByText(/Moving Vitals to your account turns this off/)).toBeNull();
  });
});
