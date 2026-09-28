/**
 * The watchdog panel writes a file another process obeys while Vitals is
 * closed, so the tests are about the two ways it could lie: showing a
 * setting that was refused, and turning a stray config into the defaults
 * that then overwrite what the user chose.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n, i18n } from '@vitals/i18n';

import { registerShellStrings } from '../shell/strings';
import { WatchdogPanel } from './WatchdogPanel';
import {
  defaultWatchdogConfig,
  parseConfig,
  soundFromKey,
  soundKey,
  systemSounds,
  type WatchdogApi,
  type WatchdogStatus,
} from './watchdog';

const host = vi.hoisted(() => ({ present: true }));
vi.mock('../shell/host', () => ({ hasTauriHost: () => host.present }));

const running: WatchdogStatus = { available: true, registered: true, running: true };

function api(overrides: Partial<WatchdogApi> = {}): WatchdogApi {
  return {
    status: vi.fn(() => Promise.resolve(running)),
    save: vi.fn(() => Promise.resolve(running)),
    test: vi.fn(() => Promise.resolve()),
    pickFile: vi.fn(() => Promise.resolve(null)),
    ...overrides,
  };
}

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  host.present = true;
  localStorage.clear();
});

describe('parseConfig', () => {
  it('keeps every valid field and replaces only the broken ones', () => {
    // A hand-edited file with one typo must not reset the sound the user chose.
    const parsed = parseConfig({
      enabled: false,
      sensitivity: 'paranoid',
      sound: { kind: 'file', value: String.raw`C:\alarm.mp3` },
      volume: 250,
    });
    expect(parsed).toEqual({
      enabled: false,
      sensitivity: defaultWatchdogConfig.sensitivity,
      sound: { kind: 'file', value: String.raw`C:\alarm.mp3` },
      volume: defaultWatchdogConfig.volume,
    });
  });

  it('rejects a system sound Windows does not ship', () => {
    expect(parseConfig({ sound: { kind: 'system', value: 'Doorbell' } }).sound).toEqual({
      kind: 'default',
    });
  });

  it('falls back to the defaults for something that is not an object', () => {
    expect(parseConfig('on')).toEqual(defaultWatchdogConfig);
    expect(parseConfig(null)).toEqual(defaultWatchdogConfig);
  });
});

describe('sound picker keys', () => {
  it('round-trips every system sound', () => {
    for (const value of systemSounds) {
      const sound = { kind: 'system', value } as const;
      expect(soundFromKey(soundKey(sound), { kind: 'default' })).toEqual(sound);
    }
  });

  it('asks for a file when "choose a file" is picked without one', () => {
    // null is the signal to open the file dialog rather than save a file sound with no path.
    expect(soundFromKey('file', { kind: 'default' })).toBeNull();
    const current = { kind: 'file', value: 'a.wav' } as const;
    expect(soundFromKey('file', current)).toBe(current);
  });
});

describe('WatchdogPanel', () => {
  it('says so when this build has no watchdog, and locks the controls', async () => {
    const a = api({
      status: vi.fn(() => Promise.resolve({ available: false, registered: false, running: false })),
    });
    render(<WatchdogPanel api={a} />);
    await screen.findByText('This copy of Vitals does not include the watchdog.');
    const toggle = screen.getByRole('switch');
    expect(toggle.hasAttribute('disabled')).toBe(true);
    expect(toggle.getAttribute('aria-checked')).toBe('false');
  });

  it('saves the switch through the backend and shows what the backend then reports', async () => {
    const stopped: WatchdogStatus = { available: true, registered: false, running: false };
    const a = api({ save: vi.fn(() => Promise.resolve(stopped)) });
    render(<WatchdogPanel api={a} />);
    await screen.findByText('Running, and starts when you sign in.');

    fireEvent.click(screen.getByRole('switch'));

    await screen.findByText(/^Not running\./);
    expect(a.save).toHaveBeenCalledWith({ ...defaultWatchdogConfig, enabled: false });
    expect(JSON.parse(localStorage.getItem('vitals.watchdog') ?? '{}')).toMatchObject({
      enabled: false,
    });
  });

  it('puts the switch back and shows the reason when the backend refuses', async () => {
    const a = api({ save: vi.fn(() => Promise.reject(new Error('could not write the file'))) });
    render(<WatchdogPanel api={a} />);
    await screen.findByText('Running, and starts when you sign in.');

    fireEvent.click(screen.getByRole('switch'));

    await screen.findByText('could not write the file');
    await waitFor(() =>
      expect(screen.getByRole('switch').getAttribute('aria-checked')).toBe('true'),
    );
    expect(localStorage.getItem('vitals.watchdog')).toBeNull();
  });

  it('plays the chosen sound when Test is pressed', async () => {
    const a = api();
    render(<WatchdogPanel api={a} />);
    await screen.findByText('Running, and starts when you sign in.');

    fireEvent.click(screen.getByRole('button', { name: 'Test' }));

    await waitFor(() => expect(a.test).toHaveBeenCalledWith({ kind: 'default' }, 80));
  });

  it('saves a picked file as the sound', async () => {
    const path = String.raw`C:\Users\me\Music\alarm.mp3`;
    const a = api({ pickFile: vi.fn(() => Promise.resolve(path)) });
    render(<WatchdogPanel api={a} />);
    await screen.findByText('Running, and starts when you sign in.');

    fireEvent.click(screen.getByRole('button', { name: 'Browse…' }));

    await waitFor(() =>
      expect(a.save).toHaveBeenCalledWith({
        ...defaultWatchdogConfig,
        sound: { kind: 'file', value: path },
      }),
    );
  });
});
