/**
 * The update panel is the one screen whose job is to be honest about not
 * knowing. These tests are mostly about the states that are easy to fake:
 * a build that cannot check must never claim to be up to date, and the check
 * button must not be clickable twice while the first check is in flight.
 *
 * No jest-dom in this workspace, so assertions go through plain DOM
 * properties — `.disabled`, `getAttribute`, `queryByText`.
 */

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n, i18n } from '@vitals/i18n';

import { UpdateSection } from './UpdatePanel';

const host = vi.hoisted(() => ({ present: true }));

vi.mock('../shell/host', () => ({
  hasTauriHost: () => host.present,
}));

// The plugin modules do not exist outside a webview; the real ones would
// throw on import, so the state machine must never see them here.
const plugin = vi.hoisted(() => ({
  check: vi.fn(),
  relaunch: vi.fn(() => Promise.resolve()),
}));

vi.mock('@tauri-apps/plugin-updater', () => ({ check: plugin.check }));
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: plugin.relaunch }));

type DownloadEvent =
  | { event: 'Started'; data: { contentLength?: number } }
  | { event: 'Progress'; data: { chunkLength: number } }
  | { event: 'Finished' };

function checkButton(): HTMLButtonElement {
  return screen.getByRole('button', { name: 'Check for updates' });
}

beforeAll(async () => {
  await initI18n();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  host.present = true;
  plugin.check.mockReset();
  vi.unstubAllEnvs();
  // Production-like by default: DEV is true under vitest, which would send
  // every test down the `unconfigured` path and prove nothing.
  vi.stubEnv('DEV', false);
});

describe('the update section', () => {
  it('offers to check before anything has been asked', () => {
    render(<UpdateSection />);
    expect(checkButton().disabled).toBe(false);
    expect(screen.queryByText('You are running the latest version.')).toBeNull();
  });

  it('says updates are not configured with no Tauri host, rather than up to date', async () => {
    host.present = false;
    render(<UpdateSection />);

    fireEvent.click(checkButton());

    await waitFor(() => {
      expect(screen.getByText('Updates are not configured for this build.')).toBeTruthy();
    });
    expect(screen.queryByText('You are running the latest version.')).toBeNull();
    expect(plugin.check).not.toHaveBeenCalled();
  });

  it('says updates are not configured in a development build', async () => {
    vi.stubEnv('DEV', true);
    render(<UpdateSection />);

    fireEvent.click(checkButton());

    await waitFor(() => {
      expect(screen.getByText('Updates are not configured for this build.')).toBeTruthy();
    });
    expect(plugin.check).not.toHaveBeenCalled();
  });

  it('disables the button while a check is in flight', async () => {
    let settle: (value: null) => void = () => undefined;
    plugin.check.mockReturnValue(
      new Promise<null>((resolve) => {
        settle = resolve;
      }),
    );

    render(<UpdateSection />);
    fireEvent.click(checkButton());

    await waitFor(() => {
      expect(checkButton().disabled).toBe(true);
    });

    settle(null);
    await waitFor(() => {
      expect(screen.getByText('You are running the latest version.')).toBeTruthy();
    });
    expect(checkButton().disabled).toBe(false);
  });

  it('reports the version and notes when an update is available', async () => {
    plugin.check.mockResolvedValue({
      version: '2.0.0',
      body: 'Faster sampling.',
      downloadAndInstall: vi.fn(() => Promise.resolve()),
    });

    render(<UpdateSection />);
    fireEvent.click(checkButton());

    await waitFor(() => {
      expect(screen.getByText('Version 2.0.0 is available.')).toBeTruthy();
    });
    expect(screen.getByText('Faster sampling.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Download update' })).toBeTruthy();
  });

  it('shows a determinate bar only once the download size is known', async () => {
    let emit: (event: DownloadEvent) => void = () => undefined;
    let finish: () => void = () => undefined;

    plugin.check.mockResolvedValue({
      version: '2.0.0',
      downloadAndInstall: (onEvent: (event: DownloadEvent) => void) =>
        new Promise<void>((resolve) => {
          emit = onEvent;
          finish = resolve;
        }),
    });

    render(<UpdateSection />);
    fireEvent.click(checkButton());
    fireEvent.click(await screen.findByRole('button', { name: 'Download update' }));

    // No Content-Length yet, so the bar must not pretend to know: an
    // indeterminate bar omits aria-valuenow entirely.
    await waitFor(() => {
      expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBeNull();
    });

    // A server that omits Content-Length must keep the bar indeterminate
    // through the whole download rather than inventing a denominator. Without
    // this the "unmeasured is not zero" rule is untested: the assertion above
    // passes purely because no event has arrived yet.
    //
    // Asserted after an explicit flush rather than inside `waitFor`: proving
    // something *stays* absent cannot use a retry loop, which succeeds on its
    // first poll against state React has not yet committed. That version of
    // this test passed against a build that reported 100%.
    emit({ event: 'Started', data: {} });
    emit({ event: 'Progress', data: { chunkLength: 100 } });
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBeNull();

    emit({ event: 'Started', data: { contentLength: 200 } });
    emit({ event: 'Progress', data: { chunkLength: 100 } });

    await waitFor(() => {
      // 100 bytes arrived before the size was known and 100 after, against a
      // declared 200 — the counter is cumulative, so this is 100%.
      expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('100');
    });

    finish();
    await waitFor(() => {
      expect(screen.getByText('Version 2.0.0 is ready to install.')).toBeTruthy();
    });
    expect(screen.getByRole('button', { name: 'Restart to update' })).toBeTruthy();
  });

  it('treats a signature failure as unconfigured and a network failure as retryable', async () => {
    plugin.check.mockRejectedValueOnce(new Error('failed to verify signature'));
    const first = render(<UpdateSection />);
    fireEvent.click(checkButton());
    await waitFor(() => {
      expect(screen.getByText('Updates are not configured for this build.')).toBeTruthy();
    });
    first.unmount();

    plugin.check.mockRejectedValueOnce(new Error('network unreachable'));
    render(<UpdateSection />);
    fireEvent.click(checkButton());
    await waitFor(() => {
      expect(screen.getByText('The update check failed: network unreachable')).toBeTruthy();
    });
    const retry: HTMLButtonElement = screen.getByRole('button', { name: 'Try again' });
    expect(retry.disabled).toBe(false);
  });
});
