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
}));

vi.mock('@tauri-apps/plugin-updater', () => ({ check: plugin.check }));

type DownloadEvent =
  | { event: 'Started'; data: { contentLength?: number } }
  | { event: 'Progress'; data: { chunkLength: number } }
  | { event: 'Finished' };

interface FakeChannel {
  onmessage: (event: DownloadEvent) => void;
}

// The backend commands the panel asks, answered per test. `download` plays
// the backend's `download_update`: it receives the progress channel.
interface Backend {
  answers: Record<string, unknown>;
  calls: { cmd: string; args: unknown }[];
  download: ((channel: FakeChannel) => Promise<unknown>) | null;
}
const backend = vi.hoisted((): Backend => ({ answers: {}, calls: [], download: null }));
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage: (event: DownloadEvent) => void = () => undefined;
  },
  invoke: vi.fn((cmd: string, args?: { onEvent?: FakeChannel }) => {
    backend.calls.push({ cmd, args });
    if (cmd === 'download_update' && backend.download !== null && args?.onEvent) {
      return backend.download(args.onEvent);
    }
    const answer = backend.answers[cmd];
    return answer instanceof Error ? Promise.reject(answer) : Promise.resolve(answer ?? null);
  }),
}));

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
  backend.answers = {};
  backend.calls = [];
  backend.download = null;
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
    plugin.check.mockResolvedValue({ version: '2.0.0', body: 'Faster sampling.' });

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

    plugin.check.mockResolvedValue({ version: '2.0.0' });
    backend.download = (channel) =>
      new Promise((resolve) => {
        emit = (event) => {
          channel.onmessage(event);
        };
        finish = () => {
          resolve('2.0.0');
        };
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

  it('downloads only, and installs through the backend that stops and restores the app around the setup', async () => {
    // ENG2-5: the plugin's own install started the setup with the sampler,
    // the local API and the attach pipe still running, and with no way back
    // when the prompt was declined. The handle the check returns has no
    // download or install here: if the panel reached for either, it throws.
    plugin.check.mockResolvedValue({ version: '2.0.0' });
    backend.answers.download_update = '2.0.0';

    render(<UpdateSection />);
    fireEvent.click(checkButton());
    fireEvent.click(await screen.findByRole('button', { name: 'Download update' }));

    await waitFor(() => {
      expect(screen.getByText('Version 2.0.0 is ready to install.')).toBeTruthy();
    });
    const download = backend.calls.find((c) => c.cmd === 'download_update');
    expect((download?.args as { version?: string } | undefined)?.version).toBe('2.0.0');
    expect(backend.calls.some((c) => c.cmd === 'install_update_now')).toBe(false);

    fireEvent.click(screen.getByRole('button', { name: 'Restart to update' }));
    await waitFor(() => {
      expect(backend.calls.some((c) => c.cmd === 'install_update_now')).toBe(true);
    });
  });

  it('says why a declined prompt after a manual download did not install, and keeps the app usable', async () => {
    plugin.check.mockResolvedValue({ version: '2.0.0' });
    backend.answers.download_update = '2.0.0';
    backend.answers.install_update_now = new Error('the update was not installed: os error 1223');

    render(<UpdateSection />);
    fireEvent.click(checkButton());
    fireEvent.click(await screen.findByRole('button', { name: 'Download update' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Restart to update' }));

    await waitFor(() => {
      expect(screen.getByText(/os error 1223/)).toBeTruthy();
    });
    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
  });

  it('says a Program Files copy needs approval and offers the click instead of waiting for quit', async () => {
    backend.answers.get_pending_update = '2.0.0';
    backend.answers.get_update_install_mode = 'needsApproval';

    render(<UpdateSection />);

    await waitFor(() => {
      expect(screen.getByText(/asks an administrator to approve the update/)).toBeTruthy();
    });
    fireEvent.click(screen.getByRole('button', { name: 'Restart to update' }));
    await waitFor(() => {
      expect(backend.calls.some((c) => c.cmd === 'install_update_now')).toBe(true);
    });
  });

  it('says why when the administrator approval is declined, instead of doing nothing', async () => {
    backend.answers.get_pending_update = '2.0.0';
    backend.answers.get_update_install_mode = 'needsApproval';
    backend.answers.install_update_now = new Error('the update was not installed: os error 1223');

    render(<UpdateSection />);
    fireEvent.click(await screen.findByRole('button', { name: 'Restart to update' }));

    await waitFor(() => {
      expect(screen.getByText(/os error 1223/)).toBeTruthy();
    });
  });

  it('offers to remove an old Program Files copy only when one is left behind', async () => {
    const { unmount } = render(<UpdateSection />);
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.queryByTestId('update-leftover')).toBeNull();
    unmount();

    backend.answers.get_leftover_install = 'C:\\Program Files\\Vitals';
    render(<UpdateSection />);
    fireEvent.click(await screen.findByRole('button', { name: 'Remove the old copy' }));
    await waitFor(() => {
      expect(screen.queryByTestId('update-leftover')).toBeNull();
    });
    expect(backend.calls.some((c) => c.cmd === 'remove_leftover_install')).toBe(true);
  });
});
