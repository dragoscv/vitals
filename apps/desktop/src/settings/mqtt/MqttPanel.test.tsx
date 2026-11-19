/**
 * The MQTT section's behaviour.
 *
 * The panel's job is to make the backend do the right thing at the right
 * moment: not start for an address it does not understand, not carry a
 * password, and not claim to be publishing on its own. Each test drives the
 * real panel against a fake backend and checks what the backend was asked.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerShellStrings } from '../../shell/strings';
import { resetSettingsForTests, useSettings } from '../store';
import type { MqttApi, MqttStatus } from './api';
import { MqttPanel } from './MqttPanel';

// No host in this suite; the panel is driven entirely through the injected API.
vi.mock('../../shell/host', () => ({ hasTauriHost: () => false }));

const OFF: MqttStatus = { running: false, error: null, passwordSet: false };

function fakeApi(overrides: Partial<MqttApi> = {}, status: MqttStatus = OFF): MqttApi {
  return {
    status: () => Promise.resolve(status),
    start: () => Promise.resolve({ ...status, running: true }),
    stop: () => Promise.resolve(OFF),
    setPassword: () => Promise.resolve(true),
    ...overrides,
  };
}

function storedMqtt(overrides: Partial<{ enabled: boolean; url: string; username: string }>): void {
  const current = useSettings.getState().settings.mqtt;
  useSettings.getState().patch({ mqtt: { ...current, ...overrides } });
}

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

beforeEach(() => {
  resetSettingsForTests();
});

describe('the MQTT section', () => {
  it('is off by default and asks the backend for nothing', async () => {
    const start = vi.fn<MqttApi['start']>(() => Promise.resolve(OFF));
    render(<MqttPanel api={fakeApi({ start })} />);

    const toggle = await screen.findByRole('switch', { name: /publish to a broker/i });
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    expect(start).not.toHaveBeenCalled();
  });

  it('refuses an address that is not mqtt or mqtts, and does not start on it', async () => {
    storedMqtt({ url: 'http://broker.local' });
    const start = vi.fn<MqttApi['start']>(() => Promise.resolve(OFF));
    render(<MqttPanel api={fakeApi({ start })} />);

    expect(await screen.findByText(/must start with mqtt:\/\/ or mqtts:\/\//i)).toBeTruthy();

    (await screen.findByRole('switch', { name: /publish to a broker/i })).click();

    // The refusal is on the row; the switch must not have opened a connection
    // the user did not describe.
    expect(start).not.toHaveBeenCalled();
  });

  it('starts the publisher with the address and never sends a password', async () => {
    storedMqtt({ url: 'mqtt://broker.local:1883', username: 'vitals' });
    const start = vi.fn<MqttApi['start']>(() => Promise.resolve({ ...OFF, running: true }));
    render(<MqttPanel api={fakeApi({ start })} />);

    (await screen.findByRole('switch', { name: /publish to a broker/i })).click();

    await waitFor(() => {
      expect(start).toHaveBeenCalledTimes(1);
    });
    const request = start.mock.calls[0]?.[0];
    expect(request?.url).toBe('mqtt://broker.local:1883');
    expect(request?.username).toBe('vitals');
    // The password is entered elsewhere and stored by the backend; it must
    // never travel with the start request.
    expect(Object.keys(request ?? {})).not.toContain('password');
  });

  it('stores the password through the secret-store command, not the settings', async () => {
    const setPassword = vi.fn<MqttApi['setPassword']>(() => Promise.resolve(true));
    render(<MqttPanel api={fakeApi({ setPassword })} />);

    fireEvent.change(await screen.findByLabelText('Password'), { target: { value: 's3cret' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(setPassword).toHaveBeenCalledWith('s3cret');
    });
    expect(useSettings.getState().settings.mqtt).not.toHaveProperty('password');
  });

  it('offers no way to remove a password that is not stored', async () => {
    render(<MqttPanel api={fakeApi()} />);

    await screen.findByRole('switch', { name: /publish to a broker/i });
    expect(screen.queryByRole('button', { name: 'Remove' })).toBeNull();
  });

  it('removes a stored password through the secret-store command', async () => {
    const setPassword = vi.fn<MqttApi['setPassword']>(() => Promise.resolve(false));
    render(<MqttPanel api={fakeApi({ setPassword }, { ...OFF, passwordSet: true })} />);

    const remove = await screen.findByRole('button', { name: 'Remove' });
    fireEvent.click(remove);
    await waitFor(() => {
      expect(setPassword).toHaveBeenCalledWith(null);
    });
  });

  it('reports the backend refusal instead of showing a publisher that is not running', async () => {
    // A broker that refuses is not an address problem: the panel learns it
    // from the status the backend kept, and must not pretend otherwise.
    const start = vi.fn<MqttApi['start']>(() => Promise.reject(new Error('broker refused')));
    storedMqtt({ url: 'mqtt://broker.local' });
    render(<MqttPanel api={fakeApi({ start })} />);

    (await screen.findByRole('switch', { name: /publish to a broker/i })).click();

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('broker refused');
    expect(screen.queryByText(/publishing readings/i)).toBeNull();
  });
});
