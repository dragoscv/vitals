/**
 * What the row menu offers, given what the host can actually do.
 *
 * The guarantee under test is the one the panel cannot express on its own: an
 * action whose command is not in this build must not appear at all, and when
 * none of them can run the menu says why rather than opening as a heading over
 * a blank list.
 */

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';
import { ContextMenu, ContextMenuTrigger } from '@vitals/ui';

import { ProcessFlag } from './constants';
import { ProcessMenu, type ProcessMenuProps } from './ProcessMenu';
import { registerProcessesStrings } from './strings';
import { makeProcess } from './test-fixtures';

beforeAll(async () => {
  await initI18n('en');
  registerProcessesStrings();
});

afterEach(cleanup);

/** Windows with every action available; a test narrows what it needs. */
function props(overrides: Partial<ProcessMenuProps> = {}): ProcessMenuProps {
  return {
    // HasWindow so the "Bring to front" item is a candidate at all.
    process: makeProcess({ pid: 40, flags: ProcessFlag.HasWindow }),
    descendantCount: 0,
    onTerminate: vi.fn(),
    onTerminateTree: vi.fn(),
    onSuspend: vi.fn(),
    onResume: vi.fn(),
    onSetPriority: vi.fn(),
    executablePath: 'C:\\Apps\\chrome.exe',
    onOpenFileLocation: vi.fn(),
    onShowProperties: vi.fn(),
    onSearchOnline: vi.fn(),
    onCopyDetails: vi.fn(),
    canBringToFront: true,
    onBringToFront: vi.fn(),
    canTerminate: true,
    canSuspend: true,
    canSetPriority: true,
    canUseShellActions: true,
    unsupportedReason: null,
    ...overrides,
  };
}

/** Opens the menu on a throwaway trigger and waits for Radix to portal it. */
async function open(overrides: Partial<ProcessMenuProps> = {}): Promise<void> {
  render(
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button type="button">row</button>
      </ContextMenuTrigger>
      <ProcessMenu {...props(overrides)} />
    </ContextMenu>,
  );
  fireEvent.contextMenu(screen.getByRole('button', { name: 'row' }), {
    button: 2,
    clientX: 5,
    clientY: 5,
  });
  await act(async () => {});
}

describe('ProcessMenu', () => {
  it('offers no action this build cannot perform, and names the reason instead', async () => {
    // macOS in the App Store sandbox: no terminate, no suspend, no priority,
    // and the shell integrations are Windows-only. Every one of these would be
    // a menu item that rejects on the press.
    await open({
      canBringToFront: false,
      canTerminate: false,
      canSuspend: false,
      canSetPriority: false,
      canUseShellActions: false,
      unsupportedReason: 'appSandbox',
    });

    expect(await screen.findByRole('menu')).toBeTruthy();
    for (const name of ['End task', 'Suspend', 'Priority', 'Open file location', 'Properties']) {
      expect(screen.queryByText(name)).toBeNull();
    }
    // The sandbox reason, in the same words the capability report uses.
    expect(screen.getByText(/App Store version/)).toBeTruthy();
  });

  it('shows the actions it can do and stays silent about the reason', async () => {
    // A direct-build Mac: terminate, suspend and priority exist, so they are
    // offered; only affinity and the shell integrations are missing, and no
    // blanket notice belongs on a menu that still works.
    await open({ canUseShellActions: false });

    expect(await screen.findByRole('menu')).toBeTruthy();
    expect(screen.getByText('End task')).toBeTruthy();
    expect(screen.getByText('Suspend')).toBeTruthy();
    expect(screen.getByText('Priority')).toBeTruthy();
    expect(screen.queryByText('Open file location')).toBeNull();
    expect(screen.queryByText('Not available on this system')).toBeNull();
  });
});
