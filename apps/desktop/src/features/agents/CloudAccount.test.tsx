/**
 * The codai sign-in (S21): the webview passes only an opaque handle back to
 * Rust (the device code never reaches it), a sign-in can be cancelled, and a
 * sign-out that could not reach codai says so instead of pretending the key
 * was switched off.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n, i18n } from '@vitals/i18n';

import { CloudAccount } from './CloudAccount';
import { registerAgentsStrings } from './strings';

vi.mock('../../shell/host', () => ({ hasTauriHost: () => true }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(() => Promise.resolve()) }));

interface Backend {
  answers: Record<string, unknown>;
  calls: { cmd: string; args: unknown }[];
  release: (() => void) | null;
}
const backend = vi.hoisted((): Backend => ({ answers: {}, calls: [], release: null }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: unknown) => {
    backend.calls.push({ cmd, args });
    if (cmd === 'finish_cloud_sign_in') {
      // stays pending until the test cancels, like a poll waiting on the browser
      return new Promise((resolve) => {
        backend.release = () => resolve(false);
      });
    }
    if (cmd === 'cancel_cloud_sign_in') {
      backend.release?.();
      return Promise.resolve(true);
    }
    return Promise.resolve(backend.answers[cmd] ?? null);
  }),
}));

const signedOut = { signedIn: false, hosts: [], error: null };
const signedIn = { signedIn: true, hosts: [], error: null };

beforeAll(async () => {
  await initI18n();
  registerAgentsStrings();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
  backend.calls = [];
  backend.release = null;
  backend.answers = {
    get_cloud_status: signedOut,
    start_cloud_sign_in: {
      handle: 's1',
      userCode: 'ABCD-EFGH',
      verificationUri: 'https://auth.codai.ro/device',
      verificationUriComplete: null,
    },
  };
});

describe('CloudAccount', () => {
  it('says the access is read-only, sends only the handle back and can be cancelled', async () => {
    render(<CloudAccount />);
    expect(await screen.findByText(/read-only access/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Sign in with codai' }));
    expect(await screen.findByText('ABCD-EFGH')).toBeTruthy();
    await waitFor(() =>
      expect(backend.calls.find((c) => c.cmd === 'finish_cloud_sign_in')?.args).toEqual({
        handle: 's1',
      }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByText('ABCD-EFGH')).toBeNull());
    expect(backend.calls.find((c) => c.cmd === 'cancel_cloud_sign_in')?.args).toEqual({
      handle: 's1',
    });
  });

  it('a sign-out codai did not confirm still signs out here and tells the user to revoke the key', async () => {
    backend.answers.get_cloud_status = signedIn;
    backend.answers.cloud_sign_out = { revoked: false };
    render(<CloudAccount />);
    const out = await screen.findByRole('button', { name: 'Sign out' });
    backend.answers.get_cloud_status = signedOut;
    fireEvent.click(out);
    expect(await screen.findByText(/could not be reached to switch the key off/)).toBeTruthy();
  });

  it('a confirmed revoke shows no warning', async () => {
    backend.answers.get_cloud_status = signedIn;
    backend.answers.cloud_sign_out = { revoked: true };
    render(<CloudAccount />);
    const out = await screen.findByRole('button', { name: 'Sign out' });
    backend.answers.get_cloud_status = signedOut;
    fireEvent.click(out);
    await screen.findByRole('button', { name: 'Sign in with codai' });
    expect(screen.queryByText(/could not be reached/)).toBeNull();
  });
});
