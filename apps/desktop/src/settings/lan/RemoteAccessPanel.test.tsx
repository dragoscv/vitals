import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerShellStrings } from '../../shell/strings';
import type { LanApi, LanStatus, Pairing } from './api';
import { RemoteAccessPanel } from './RemoteAccessPanel';

const OFF: LanStatus = { running: false, port: null, interfaces: [], tokens: [] };

const RUNNING: LanStatus = {
  running: true,
  port: 7331,
  interfaces: [
    { name: 'Wi-Fi', address: '192.168.1.20', score: 40, virtualised: false },
    { name: 'vEthernet (WSL)', address: '172.30.96.1', score: -95, virtualised: true },
  ],
  tokens: [],
};

const PAIRING: Pairing = {
  url: 'http://192.168.1.20:7331/mobile.html#t=SECRETVALUE',
  qrSvg: '<svg viewBox="0 0 10 10"></svg>',
  token: { prefix: 'SECRETVA', scope: 'read', label: 'My phone', created: 0 },
};

function fakeApi(overrides: Partial<LanApi> = {}, status: LanStatus = OFF): LanApi {
  return {
    status: () => Promise.resolve(status),
    start: () => Promise.resolve(7331),
    stop: () => Promise.resolve(),
    pair: () => Promise.resolve(PAIRING),
    revoke: () => Promise.resolve(),
    revokeAll: () => Promise.resolve(),
    ...overrides,
  };
}

beforeEach(async () => {
  await initI18n('en');
  registerShellStrings();
});

describe('RemoteAccessPanel', () => {
  it('is off by default and offers no pairing until the server runs', async () => {
    // The product promise: nothing is listening until the user says so, and
    // there is no way to hand out a credential for a server that is off.
    render(<RemoteAccessPanel api={fakeApi()} />);

    const toggle = await screen.findByRole('switch', {
      name: /allow devices on this network/i,
    });
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    expect(screen.queryByRole('button', { name: /create/i })).toBeNull();
    expect(screen.queryByRole('img', { name: /pairing qr/i })).toBeNull();
  });

  it('shows the listening port and the adapter choice once running', async () => {
    render(<RemoteAccessPanel api={fakeApi({}, RUNNING)} />);

    expect(await screen.findByText(/listening on port 7331/i)).toBeTruthy();
    // Both adapters are offered, named, so the user can tell the Wi-Fi from
    // the WSL bridge — picking the wrong one is the failure this prevents.
    expect(screen.getByRole('combobox', { name: /network adapter/i })).toBeTruthy();
  });

  it('defaults a new pairing to read-only', async () => {
    // A phone left on a desk must not be a remote kill switch by default.
    const pair = vi.fn(() => Promise.resolve(PAIRING));
    render(<RemoteAccessPanel api={fakeApi({ pair }, RUNNING)} />);

    const control = await screen.findByRole('switch', {
      name: /allow this device to end processes/i,
    });
    expect(control.getAttribute('aria-checked')).toBe('false');

    // Named by its SettingsRow label, not its own text — that is what a
    // screen reader announces.
    (await screen.findByRole('button', { name: /pairing code/i })).click();

    await waitFor(() => {
      expect(pair).toHaveBeenCalledTimes(1);
    });
    expect(pair.mock.calls[0]?.[0]).toMatchObject({ scope: 'read' });
  });

  it('renders the QR and says the secret is shown only once', async () => {
    render(<RemoteAccessPanel api={fakeApi({}, RUNNING)} />);
    (await screen.findByRole('button', { name: /pairing code/i })).click();

    const qr = await screen.findByRole('img', { name: /pairing qr/i });
    expect(qr.innerHTML).toContain('<svg');
    expect(screen.getByText(/shown once/i)).toBeTruthy();
  });

  it('lists paired devices by prefix, never by secret', async () => {
    const paired: LanStatus = {
      ...RUNNING,
      tokens: [{ prefix: 'ABCD1234', scope: 'control', label: 'Pixel', created: 0 }],
    };
    render(<RemoteAccessPanel api={fakeApi({}, paired)} />);

    expect(await screen.findByText('Pixel')).toBeTruthy();
    expect(screen.getByText(/ABCD1234…/)).toBeTruthy();
    // The scope must be legible: "can watch and control" is a different
    // thing to consent to than "can watch".
    expect(screen.getByText(/can watch and control/i)).toBeTruthy();
  });

  it('surfaces a failure to start rather than silently staying off', async () => {
    // On Windows the first bind raises a firewall prompt; declining it must
    // not look like a switch that does nothing.
    const start = vi.fn(() => Promise.reject(new Error('port 7331 is already in use')));
    render(<RemoteAccessPanel api={fakeApi({ start })} />);

    (await screen.findByRole('switch', { name: /allow devices on this network/i })).click();

    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      expect.stringContaining('already in use') as unknown as string,
    );
  });
});
