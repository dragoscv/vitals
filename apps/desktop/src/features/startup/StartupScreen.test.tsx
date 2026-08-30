import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import type { ServiceEntry, StartupEntry, StartupSnapshot } from './model';
import { StartupScreen } from './StartupScreen';
import { registerStartupStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerStartupStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function entry(overrides: Partial<StartupEntry> = {}): StartupEntry {
  return {
    name: 'Steam',
    displayName: null,
    command: 'C:\\Steam\\steam.exe',
    imagePath: null,
    publisher: 'Valve',
    source: 'userRun',
    state: 'enabled',
    pid: null,
    ...overrides,
  };
}

function service(overrides: Partial<ServiceEntry> = {}): ServiceEntry {
  return {
    name: 'Spooler',
    displayName: 'Print Spooler',
    state: 'running',
    startType: 'automatic',
    pid: 900,
    binaryPath: null,
    svchostGroup: null,
    ...overrides,
  };
}

function snapshot(overrides: Partial<StartupSnapshot> = {}): StartupSnapshot {
  return { entries: [entry()], services: [service()], unreadableTasks: 0, ...overrides };
}

async function mount(mode: 'startup' | 'services', data = snapshot()) {
  const reader = vi.fn<(withConfig: boolean) => Promise<StartupSnapshot>>().mockResolvedValue(data);
  render(<StartupScreen mode={mode} reader={reader} />);
  await screen.findByRole('heading');
  return { reader };
}

describe('StartupScreen', () => {
  it('shows skeletons until the read settles', () => {
    render(<StartupScreen mode="startup" reader={() => new Promise(() => undefined)} />);
    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('reaches a terminal state when the read fails', () => {
    // The rule this project keeps relearning: a loading state must resolve
    // either way, or a broken app looks merely busy.
    const reader = vi
      .fn<(withConfig: boolean) => Promise<StartupSnapshot>>()
      .mockRejectedValue(new Error('registry read failed'));

    render(<StartupScreen mode="startup" reader={reader} />);

    return screen.findByRole('alert').then(() => {
      expect(document.querySelector('[aria-busy="true"]')).toBeNull();
    });
  });

  it('only pays for service configuration on the Services screen', async () => {
    // Reading start types opens the SCM once per service — on ~370 services
    // that is the difference between a fast list and a slow one, and the
    // Startup tab does not use the result.
    const startup = await mount('startup');
    expect(startup.reader).toHaveBeenCalledWith(false);

    const services = await mount('services');
    expect(services.reader).toHaveBeenCalledWith(true);
  });

  describe('startup mode', () => {
    it('lists entries with their source', async () => {
      await mount('startup');
      expect(screen.getByText('Steam')).toBeTruthy();
      expect(screen.getByText(/Registry \(your account\)/)).toBeTruthy();
    });

    it('marks a machine-wide entry, because changing it needs elevation', async () => {
      await mount('startup', snapshot({ entries: [entry({ source: 'machineRun' })] }));
      expect(screen.getByText('All users')).toBeTruthy();
    });

    it('says how many states it could not read, rather than guessing', async () => {
      // The distinctive behaviour: every other tool either omits these
      // silently or reports them as enabled. Both are fabrications, and this
      // is the exact number a user counts when deciding what to disable.
      await mount(
        'startup',
        snapshot({
          entries: [entry({ state: 'enabled' }), entry({ name: 'Opaque', state: 'unknown' })],
        }),
      );

      expect(screen.getByText(/1 of 2 will run at sign-in/)).toBeTruthy();
      expect(screen.getByText(/whose state could not be read/)).toBeTruthy();
      expect(screen.getByText(/a floor, not a total/)).toBeTruthy();
    });

    it('reports unreadable scheduled tasks', async () => {
      // Unelevated, some definitions under \Microsoft\Windows are ACL'd to
      // SYSTEM. Presenting a partial list as complete would be a lie.
      await mount('startup', snapshot({ unreadableTasks: 40 }));
      expect(screen.getByText(/40 scheduled tasks could not be read/)).toBeTruthy();
    });

    it('stays silent about caveats when there are none', async () => {
      await mount('startup');
      expect(screen.queryByText(/could not be read/)).toBeNull();
    });

    it('filters to enabled without including unknown', async () => {
      await mount(
        'startup',
        snapshot({
          entries: [
            entry({ name: 'Sure', state: 'enabled' }),
            entry({ name: 'Maybe', state: 'unknown' }),
          ],
        }),
      );

      fireEvent.click(screen.getByRole('radio', { name: 'Enabled' }));
      expect(screen.getByText('Sure')).toBeTruthy();
      expect(screen.queryByText('Maybe')).toBeNull();
    });

    it('searches the command line', async () => {
      await mount(
        'startup',
        snapshot({ entries: [entry({ name: 'Opaque', command: 'C:\\Weird\\hidden.exe' })] }),
      );

      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'hidden' } });
      expect(screen.getByText('Opaque')).toBeTruthy();
    });
  });

  describe('services mode', () => {
    it('lists services with their start type', async () => {
      await mount('services');
      expect(screen.getByText('Print Spooler')).toBeTruthy();
      expect(screen.getByText('Automatic')).toBeTruthy();
    });

    it('marks a service that shares a host process', async () => {
      // Grouped services cannot have CPU or memory attributed individually,
      // and saying so beats showing the host's whole footprint against each.
      await mount('services', snapshot({ services: [service({ svchostGroup: 'netsvcs' })] }));
      expect(screen.getByText('Shares a process')).toBeTruthy();
    });

    it('says how many start types it could not read', async () => {
      await mount(
        'services',
        snapshot({
          services: [
            service({ startType: 'automatic' }),
            service({ name: 'Opaque', startType: 'unknown' }),
          ],
        }),
      );

      expect(screen.getByText(/1 start at boot/)).toBeTruthy();
      expect(screen.getByText(/whose start type could not be read/)).toBeTruthy();
    });

    it('filters to services that start at boot', async () => {
      await mount(
        'services',
        snapshot({
          services: [
            service({ name: 'Auto', displayName: 'Auto svc', startType: 'automatic' }),
            service({ name: 'Man', displayName: 'Manual svc', startType: 'manual' }),
          ],
        }),
      );

      fireEvent.click(screen.getByRole('radio', { name: 'Starts at boot' }));
      expect(screen.getByText('Auto svc')).toBeTruthy();
      expect(screen.queryByText('Manual svc')).toBeNull();
    });
  });

  it('says nothing matched rather than showing an empty table', async () => {
    await mount('startup');
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'nothing-here' } });
    expect(screen.getByText('Nothing matches')).toBeTruthy();
  });

  it('keeps the last good list when a refresh fails', async () => {
    const reader = vi
      .fn<(withConfig: boolean) => Promise<StartupSnapshot>>()
      .mockResolvedValueOnce(snapshot())
      .mockRejectedValue(new Error('registry read failed'));

    render(<StartupScreen mode="startup" reader={reader} />);
    await screen.findByText('Steam');

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(screen.getByText('Steam')).toBeTruthy();
  });

  it('renders in Romanian without falling back to key paths', async () => {
    await i18n.changeLanguage('ro');
    await mount('services');

    expect(screen.getByRole('heading', { name: 'Servicii' })).toBeTruthy();
    expect(screen.getByRole('radio', { name: 'Pornesc la boot' })).toBeTruthy();
  });
});
