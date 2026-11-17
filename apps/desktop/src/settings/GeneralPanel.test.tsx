/**
 * Which General rows this build shows.
 *
 * Two of them are Windows-only in the backend — the Task Manager replacement
 * (IFEO) and Start with Windows — and a switch that moves while doing nothing
 * is the failure this file exists to prevent. The platform arrives from the
 * same store the shell fills from `get_capabilities`.
 */

import { render, screen } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerShellStrings } from '../shell/strings';
import { useNavVisibility } from '../shell/navigation';
import { GeneralPanel } from './panels';

// The panel reads the Task Manager hook, which asks the host. No host here:
// the hook answers "unavailable", which is what the panel renders around.
vi.mock('../shell/host', () => ({ hasTauriHost: () => false }));

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

afterEach(() => {
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
