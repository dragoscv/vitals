import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerShellStrings } from '../../shell/strings';
import type { LanApi, LanStatus, Pairing, PairingCode, PairingCodeStatus } from './api';
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
    createCode: () => Promise.resolve(code()),
    cancelCode: () => Promise.resolve(),
    codeStatus: () => Promise.resolve(ACTIVE),
    ...overrides,
  };
}

function code(expiresInMs = 5 * 60 * 1000): PairingCode {
  return { code: '482913', expiresAtMs: Date.now() + expiresInMs };
}

const ACTIVE: PairingCodeStatus = { active: true, expiresAtMs: Date.now() + 300_000 };
const INACTIVE: PairingCodeStatus = { active: false, expiresAtMs: null };
const TV = { prefix: 'TVTVTVTV', scope: 'read' as const, label: 'Living room TV', created: 0 };

/** Polls fast so a test waits milliseconds, not the real two seconds. */
const FAST_POLL = 20;

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
    const pair = vi.fn<LanApi['pair']>(() => Promise.resolve(PAIRING));
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

  it('shows a TV code grouped three and three with a countdown, created read-only by default', async () => {
    const createCode = vi.fn<LanApi['createCode']>(() => Promise.resolve(code()));
    render(<RemoteAccessPanel api={fakeApi({ createCode }, RUNNING)} codePollMs={FAST_POLL} />);

    (await screen.findByRole('button', { name: /pair a tv/i })).click();

    expect(await screen.findByText('482 913')).toBeTruthy();
    // Read one digit at a time by a screen reader, not as "four hundred
    // and eighty-two thousand".
    expect(screen.getByLabelText('Pairing code 4 8 2 9 1 3')).toBeTruthy();
    expect(screen.getByText(/expires in [45]:\d\d/i)).toBeTruthy();
    expect(screen.getByRole('status').textContent).toMatch(/waiting for the tv/i);
    expect(createCode).toHaveBeenCalledWith('read');
  });

  it('creates a control code only when the control switch is on', async () => {
    const createCode = vi.fn<LanApi['createCode']>(() => Promise.resolve(code()));
    render(<RemoteAccessPanel api={fakeApi({ createCode }, RUNNING)} codePollMs={FAST_POLL} />);

    (await screen.findByRole('switch', { name: /allow this device to end processes/i })).click();
    await waitFor(() => {
      expect(
        screen
          .getByRole('switch', { name: /allow this device to end processes/i })
          .getAttribute('aria-checked'),
      ).toBe('true');
    });
    screen.getByRole('button', { name: /pair a tv/i }).click();

    await waitFor(() => {
      expect(createCode).toHaveBeenCalledWith('control');
    });
  });

  it('says the TV paired and lists it when the code is used before it expires', async () => {
    let used = false;
    const api = fakeApi({
      status: () => Promise.resolve(used ? { ...RUNNING, tokens: [TV] } : RUNNING),
      codeStatus: () => Promise.resolve(used ? INACTIVE : ACTIVE),
    });
    render(<RemoteAccessPanel api={api} codePollMs={FAST_POLL} />);
    (await screen.findByRole('button', { name: /pair a tv/i })).click();
    await screen.findByText('482 913');

    used = true;

    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toMatch(/the tv is paired/i);
    });
    expect(screen.getByText('Living room TV')).toBeTruthy();
    // The digits are a spent credential now; leaving them up invites
    // someone to try them.
    expect(screen.queryByText('482 913')).toBeNull();
  });

  it('does not claim a pairing when the code died without adding a device', async () => {
    // Five wrong guesses burn the code early. Inactive-before-expiry alone
    // would read as "paired"; only a new token proves it.
    let burnt = false;
    const api = fakeApi(
      {
        codeStatus: () => Promise.resolve(burnt ? INACTIVE : ACTIVE),
      },
      RUNNING,
    );
    render(<RemoteAccessPanel api={api} codePollMs={FAST_POLL} />);
    (await screen.findByRole('button', { name: /pair a tv/i })).click();
    await screen.findByText('482 913');

    burnt = true;

    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toMatch(/too many wrong attempts/i);
    });
    expect(screen.queryByText(/the tv is paired/i)).toBeNull();
  });

  it('says a code expired rather than waiting forever', async () => {
    const api = fakeApi({ createCode: () => Promise.resolve(code(-1)) }, RUNNING);
    render(<RemoteAccessPanel api={api} codePollMs={FAST_POLL} />);
    (await screen.findByRole('button', { name: /pair a tv/i })).click();

    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toMatch(/expired/i);
    });
    expect(screen.queryByText('482 913')).toBeNull();
  });

  it('cancelling withdraws the code on the server and removes it from the screen', async () => {
    const cancelCode = vi.fn<LanApi['cancelCode']>(() => Promise.resolve());
    render(<RemoteAccessPanel api={fakeApi({ cancelCode }, RUNNING)} codePollMs={FAST_POLL} />);
    (await screen.findByRole('button', { name: /pair a tv/i })).click();
    await screen.findByText('482 913');

    screen.getByRole('button', { name: /^cancel$/i }).click();

    await waitFor(() => {
      expect(cancelCode).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(screen.queryByText('482 913')).toBeNull();
    });
  });

  it('surfaces a failure to start rather than silently staying off', async () => {
    // On Windows the first bind raises a firewall prompt; declining it must
    // not look like a switch that does nothing.
    const start = vi.fn(() => Promise.reject(new Error('port 7331 is already in use')));
    render(<RemoteAccessPanel api={fakeApi({ start })} />);

    (await screen.findByRole('switch', { name: /allow devices on this network/i })).click();

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('already in use');
  });
});
