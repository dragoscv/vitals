import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import type { ServiceEntry, StartupEntry, StartupSnapshot } from './model';
import type { StartupActions } from './startupActions';
import { StartupScreen } from './StartupScreen';
import { registerStartupStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerStartupStrings();
});

beforeEach(async () => {
  // The Microsoft box lives in the URL fragment; a test that unticks it must
  // not leave the next one starting from "show".
  window.history.replaceState(null, '', '#');
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
    impact: null,
    company: null,
    microsoft: false,
    risk: 'safe',
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
    company: null,
    microsoft: false,
    risk: 'safe',
    imagePath: null,
    ...overrides,
  };
}

function snapshot(overrides: Partial<StartupSnapshot> = {}): StartupSnapshot {
  return {
    entries: [entry()],
    services: [service()],
    unreadableTasks: 0,
    impactMeasuredAtMs: null,
    ...overrides,
  };
}

function fakeActions(overrides: Partial<StartupActions> = {}) {
  return {
    setStartupEnabled: vi.fn<StartupActions['setStartupEnabled']>().mockResolvedValue(undefined),
    controlService: vi.fn<StartupActions['controlService']>().mockResolvedValue(undefined),
    setServiceStartType: vi
      .fn<StartupActions['setServiceStartType']>()
      .mockResolvedValue(undefined),
    openFileLocation: vi.fn<StartupActions['openFileLocation']>().mockResolvedValue(undefined),
    showFileProperties: vi.fn<StartupActions['showFileProperties']>().mockResolvedValue(undefined),
    ...overrides,
  };
}

async function mount(
  mode: 'startup' | 'services',
  data = snapshot(),
  actions: StartupActions = fakeActions(),
) {
  const reader = vi.fn<(withConfig: boolean) => Promise<StartupSnapshot>>().mockResolvedValue(data);
  render(<StartupScreen mode={mode} reader={reader} actions={actions} />);
  await screen.findByRole('heading');
  return { reader };
}

/** Opens the "⋯" menu the way a mouse does; Radix needs the pointerDown. */
async function openRowActions(name: string): Promise<HTMLElement> {
  const trigger = await screen.findByRole('button', { name: `Actions for ${name}` });
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: 'mouse' });
  fireEvent.click(trigger);
  await act(async () => {});
  return screen.findByRole('menu');
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

    it('shows an em dash for an entry whose startup cost was not measured, never a zero', async () => {
      // Vitals launched ten minutes into the session has nothing to say
      // about boot. "0.0 s" would tell the user this item is free to keep.
      await mount('startup', snapshot({ entries: [entry({ impact: null })] }));
      const cell = screen.getByTestId('startup-impact');
      expect(cell.textContent).toBe('——');
      expect(cell.textContent).not.toMatch(/0/);
      expect(screen.getByText(/has not been measured yet/)).toBeTruthy();
    });

    it('shows measured CPU seconds and disk bytes with the boot they describe', async () => {
      await mount(
        'startup',
        snapshot({
          entries: [
            entry({
              impact: { cpuMs: 4213, diskBytes: 15_728_640, measuredAtMs: 1_700_000_000_000 },
            }),
          ],
          impactMeasuredAtMs: 1_700_000_000_000,
        }),
      );
      const cell = screen.getByTestId('startup-impact');
      expect(cell.textContent).toContain('4.2 s');
      expect(cell.textContent).toContain('15.0 MB');
      expect(screen.getByText(/measured over the first 2 minutes after boot on/)).toBeTruthy();
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
    expect(screen.getByText('Ascunde serviciile Microsoft')).toBeTruthy();
  });

  describe('Microsoft filter', () => {
    it('hides Microsoft rows by default and says how many, and unticking reveals them', async () => {
      await mount(
        'services',
        snapshot({
          services: [
            service(),
            service({ name: 'WinDefend', displayName: 'Defender', microsoft: true }),
            service({ name: 'Wuauserv', displayName: 'Windows Update', microsoft: true }),
          ],
        }),
      );

      expect(screen.getByText('Print Spooler')).toBeTruthy();
      expect(screen.queryByText('Defender')).toBeNull();
      expect(screen.getByText('2 Microsoft items hidden')).toBeTruthy();

      const box = screen.getByRole('checkbox', { name: 'Hide Microsoft services' });
      expect(box.getAttribute('aria-checked')).toBe('true');
      fireEvent.click(box);

      expect(screen.getByText('Defender')).toBeTruthy();
      expect(screen.getByText('Windows Update')).toBeTruthy();
      expect(screen.queryByText(/Microsoft items? hidden/)).toBeNull();
    });

    it('labels the box for entries on the Startup screen', async () => {
      await mount('startup', snapshot({ entries: [entry({ microsoft: true })] }));
      expect(screen.getByRole('checkbox', { name: 'Hide Microsoft entries' })).toBeTruthy();
      expect(screen.getByText('1 Microsoft item hidden')).toBeTruthy();
    });
  });

  describe('row actions', () => {
    it('disables a safe entry straight from the row button, without a dialog', async () => {
      const actions = fakeActions();
      const target = entry();
      await mount('startup', snapshot({ entries: [target] }), actions);

      const menu = await openRowActions('Steam');
      fireEvent.click(within(menu).getByRole('menuitem', { name: 'Disable' }));
      await act(async () => {});

      expect(actions.setStartupEnabled).toHaveBeenCalledWith(target, false, false);
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(await screen.findByRole('status')).toHaveProperty('textContent', 'Disabled Steam');
    });

    it('asks before stopping a system-critical service, and confirming passes confirmed', async () => {
      const actions = fakeActions();
      const target = service({ risk: 'systemCritical' });
      await mount('services', snapshot({ services: [target] }), actions);

      const menu = await openRowActions('Print Spooler');
      fireEvent.click(within(menu).getByRole('menuitem', { name: 'Stop' }));

      const dialog = await screen.findByRole('dialog', { name: 'Stop Print Spooler?' });
      expect(within(dialog).getByText(/Windows depends on this/)).toBeTruthy();
      expect(actions.controlService).not.toHaveBeenCalled();

      fireEvent.click(within(dialog).getByRole('button', { name: 'Continue' }));
      await act(async () => {});
      expect(actions.controlService).toHaveBeenCalledWith(target, 'stop', true);
    });

    it('does nothing when the confirmation is cancelled', async () => {
      const actions = fakeActions();
      await mount(
        'services',
        snapshot({ services: [service({ risk: 'systemCritical' })] }),
        actions,
      );

      const menu = await openRowActions('Print Spooler');
      fireEvent.click(within(menu).getByRole('menuitem', { name: 'Stop' }));
      const dialog = await screen.findByRole('dialog');
      fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
      await act(async () => {});

      expect(screen.queryByRole('dialog')).toBeNull();
      expect(actions.controlService).not.toHaveBeenCalled();
    });

    it('disables Disable on an entry Windows forbids changing', async () => {
      await mount('startup', snapshot({ entries: [entry({ risk: 'forbidden' })] }));

      const menu = await openRowActions('Steam');
      const item = within(menu).getByRole('menuitem', { name: 'Disable' });
      expect(item.getAttribute('aria-disabled')).toBe('true');
      expect(item.getAttribute('title')).toBe('Windows does not allow this to be changed');
    });

    it('shows the refusal message when the user declines the prompt', async () => {
      const actions = fakeActions({
        setStartupEnabled: vi
          .fn<StartupActions['setStartupEnabled']>()
          .mockRejectedValue({ kind: 'refused', message: 'The administrator prompt was declined' }),
      });
      await mount('startup', snapshot(), actions);

      const menu = await openRowActions('Steam');
      fireEvent.click(within(menu).getByRole('menuitem', { name: 'Disable' }));

      expect(await screen.findByText('The administrator prompt was declined')).toBeTruthy();
      expect(screen.getByRole('alert').textContent).toBe('The administrator prompt was declined');
    });

    it('opens the same menu on right-click of the row', async () => {
      await mount('startup');

      const row = await screen.findByTestId('startup-row');
      expect(row.tabIndex, 'a row that cannot be focused cannot take Shift+F10').toBe(0);
      fireEvent.contextMenu(row, { button: 2 });
      await act(async () => {});

      const menu = await screen.findByRole('menu');
      expect(within(menu).getByRole('menuitem', { name: 'Disable' })).toBeTruthy();
      expect(within(menu).getByRole('menuitem', { name: 'Open file location' })).toBeTruthy();
    });
  });
});
