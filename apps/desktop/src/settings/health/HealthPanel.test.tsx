import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import type { HealthStatus } from '../../features/health/api';
import { fakeHealthApi } from '../../features/health/fixtures.test-utils';
import { useNavVisibility } from '../../shell/navigation';
import { registerShellStrings } from '../../shell/strings';
import { HealthPanel } from './HealthPanel';

const OFF: HealthStatus = { configured: false, base: null, tokenPrefix: null };
const ON: HealthStatus = {
  configured: true,
  base: 'https://vmui.example.com',
  tokenPrefix: 'vmd_abcd',
};

/** Polls fast so a test waits milliseconds, not the real three seconds. */
const FAST_POLL = 20;

beforeEach(async () => {
  await initI18n('en');
  registerShellStrings();
});

afterEach(() => {
  act(() => {
    useNavVisibility.setState({ health: null });
  });
});

describe('HealthPanel', () => {
  it('shows the code, waits for approval, then reports the connection and reveals the page', async () => {
    let status = OFF;
    let polls = 0;
    const pairStart = vi.fn(() => Promise.resolve({ id: 'abcdef0123456789', code: '4821' }));
    const api = fakeHealthApi({
      status: () => Promise.resolve(status),
      pairStart,
      pairPoll: () => {
        polls += 1;
        if (polls < 2) return Promise.resolve('pending');
        status = ON;
        return Promise.resolve('approved');
      },
    });
    render(<HealthPanel api={api} pollMs={FAST_POLL} />);

    const address = await screen.findByLabelText('vmui address');
    fireEvent.change(address, { target: { value: 'https://vmui.example.com' } });
    fireEvent.click(screen.getByRole('button', { name: 'Pair with code' }));

    expect(await screen.findByText('4821')).toBeTruthy();
    expect(screen.getByText('Waiting for approval…')).toBeTruthy();
    expect(pairStart).toHaveBeenCalledWith('https://vmui.example.com', 'Vitals');
    expect(useNavVisibility.getState().health).toBe(false);

    expect(await screen.findByText('Connected to https://vmui.example.com')).toBeTruthy();
    expect(screen.getByText('Key starting vmd_abcd…')).toBeTruthy();
    expect(screen.queryByText('4821')).toBeNull();
    expect(useNavVisibility.getState().health).toBe(true);
  });

  it('says so when the owner declines the code in vmui', async () => {
    const api = fakeHealthApi({ pairPoll: () => Promise.resolve('rejected') });
    render(<HealthPanel api={api} pollMs={FAST_POLL} />);
    fireEvent.change(await screen.findByLabelText('vmui address'), {
      target: { value: 'https://vmui.example.com' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Pair with code' }));
    expect(await screen.findByText('The pairing was declined in vmui.')).toBeTruthy();
  });

  it('explains an insecure address in plain words', async () => {
    const api = fakeHealthApi({ pairStart: () => Promise.reject(new Error('insecureUrl')) });
    render(<HealthPanel api={api} pollMs={FAST_POLL} />);
    fireEvent.change(await screen.findByLabelText('vmui address'), {
      target: { value: 'http://vmui.example.com' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Pair with code' }));
    expect((await screen.findByRole('alert')).textContent).toMatch(/must start with https/i);
  });

  it('signs in with a password field and clears the password from the form at once', async () => {
    const signIn = vi.fn(() => Promise.resolve('approved' as const));
    render(<HealthPanel api={fakeHealthApi({ signIn })} pollMs={FAST_POLL} />);
    fireEvent.change(await screen.findByLabelText('vmui address'), {
      target: { value: 'https://vmui.example.com' },
    });
    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'me@example.com' } });
    const password = screen.getByLabelText<HTMLInputElement>('Password');
    expect(password.type).toBe('password');
    fireEvent.change(password, { target: { value: 'hunter2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => {
      expect(signIn).toHaveBeenCalledWith('https://vmui.example.com', 'me@example.com', 'hunter2');
    });
    expect(password.value).toBe('');
  });

  it('disconnects only after a confirmation and hides the page again', async () => {
    let status = ON;
    const disconnect = vi.fn(() => {
      status = OFF;
      return Promise.resolve();
    });
    render(
      <HealthPanel
        api={fakeHealthApi({ status: () => Promise.resolve(status), disconnect })}
        pollMs={FAST_POLL}
      />,
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Disconnect' }));
    expect(disconnect).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Yes, disconnect' }));

    expect(await screen.findByText('Not connected')).toBeTruthy();
    expect(disconnect).toHaveBeenCalledTimes(1);
    expect(useNavVisibility.getState().health).toBe(false);
  });
});
