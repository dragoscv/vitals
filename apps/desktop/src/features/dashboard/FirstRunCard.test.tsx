/**
 * The first-run card asks before the watchdog is turned on — the guarantee
 * that replaced it switching itself on at install. So the tests are about
 * asking honestly: the card goes away for good when dismissed, the watchdog
 * changes only when its button is pressed, and the Task Manager offer is a
 * route to the setting, never a silent registry write.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { TaskManagerReplacement } from '@vitals/protocol';

import { OPEN_SETTINGS_EVENT } from '../../shell/navigation';
import {
  flushSettings,
  resetSettingsForTests,
  setSettingsBackend,
  useSettings,
} from '../../settings/store';
import type { TaskManagerReplacementApi } from '../../settings/useTaskManagerReplacement';
import type { WatchdogApi, WatchdogStatus } from '../../settings/watchdog';
import { FirstRunCard } from './FirstRunCard';
import { registerDashboardStrings } from './strings';

const host = vi.hoisted(() => ({ present: true, packaged: false }));
vi.mock('../../shell/host', () => ({ hasTauriHost: () => host.present }));
// The real hook caches its one IPC answer for the module's lifetime, so a
// per-test Store/direct switch has to be mocked at the hook.
vi.mock('../../settings/useDistribution', () => ({ useIsStoreBuild: () => host.packaged }));

const off: WatchdogStatus = { available: true, registered: false, running: false };
const on: WatchdogStatus = { available: true, registered: true, running: true };

function watchdogApi(overrides: Partial<WatchdogApi> = {}): WatchdogApi {
  return {
    status: vi.fn(() => Promise.resolve(off)),
    save: vi.fn(() => Promise.resolve(on)),
    test: vi.fn(() => Promise.resolve()),
    pickFile: vi.fn(() => Promise.resolve(null)),
    ...overrides,
  };
}

function taskManagerApi(status: TaskManagerReplacement): TaskManagerReplacementApi {
  return {
    read: vi.fn(() => Promise.resolve(status)),
    write: vi.fn(() => Promise.resolve(status)),
  };
}

const notReplaced: TaskManagerReplacement = { enabled: false, replacedBy: null, path: null };

let stored: unknown;

async function hydrate(seed: unknown = undefined): Promise<void> {
  stored = seed;
  setSettingsBackend({
    load: () => Promise.resolve(stored),
    save: (value) => {
      stored = value;
      return Promise.resolve();
    },
  });
  await useSettings.getState().hydrate();
}

beforeAll(async () => {
  await initI18n();
  registerDashboardStrings();
});

beforeEach(async () => {
  resetSettingsForTests();
  await i18n.changeLanguage('en');
  host.present = true;
  host.packaged = false;
  localStorage.clear();
});

describe('FirstRunCard', () => {
  it('offers all three choices on a fresh direct install without changing anything', async () => {
    await hydrate();
    const wd = watchdogApi();
    const tm = taskManagerApi(notReplaced);
    render(<FirstRunCard onDiagnose={vi.fn()} watchdogApi={wd} taskManagerApi={tm} />);

    expect(screen.getByRole('heading', { name: 'Welcome to Vitals' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Find out why your PC is slow' })).toBeTruthy();
    await screen.findByRole('button', { name: 'Watch for freezes and warn me' });
    await screen.findByRole('button', { name: 'Open Vitals with Ctrl+Shift+Esc' });
    // Showing the offer is not accepting it.
    expect(wd.save).not.toHaveBeenCalled();
    expect(tm.write).not.toHaveBeenCalled();
  });

  it('stays dismissed: the choice is saved and the card does not come back', async () => {
    await hydrate();
    const view = render(
      <FirstRunCard
        onDiagnose={vi.fn()}
        watchdogApi={watchdogApi()}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss the welcome card' }));
    expect(screen.queryByRole('heading', { name: 'Welcome to Vitals' })).toBeNull();
    await flushSettings();
    expect((stored as { firstRunDismissed: boolean }).firstRunDismissed).toBe(true);

    // A restart: the store is read back from what was saved.
    view.unmount();
    resetSettingsForTests();
    await hydrate(stored);
    render(
      <FirstRunCard
        onDiagnose={vi.fn()}
        watchdogApi={watchdogApi()}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );
    expect(screen.queryByRole('heading', { name: 'Welcome to Vitals' })).toBeNull();
  });

  it('turns the watchdog on through the same command as Settings, only when pressed', async () => {
    await hydrate();
    const wd = watchdogApi();
    render(
      <FirstRunCard
        onDiagnose={vi.fn()}
        watchdogApi={wd}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Watch for freezes and warn me' }));

    await screen.findByText('The freeze watchdog is on. You can turn it off in Settings.');
    expect(wd.save).toHaveBeenCalledTimes(1);
    expect(wd.save).toHaveBeenCalledWith({
      enabled: true,
      sensitivity: 'normal',
      sound: { kind: 'default' },
      volume: 80,
    });
    // The Settings panel starts from this mirror, so it shows the switch on.
    expect(JSON.parse(localStorage.getItem('vitals.watchdog') ?? '{}')).toMatchObject({
      enabled: true,
    });
  });

  it('says why when the watchdog could not be turned on, and offers it again', async () => {
    await hydrate();
    const wd = watchdogApi({ save: vi.fn(() => Promise.reject(new Error('no logon entry'))) });
    render(
      <FirstRunCard
        onDiagnose={vi.fn()}
        watchdogApi={wd}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Watch for freezes and warn me' }));

    expect((await screen.findByRole('alert')).textContent).toContain('no logon entry');
    expect(screen.getByRole('button', { name: 'Watch for freezes and warn me' })).toBeTruthy();
    expect(localStorage.getItem('vitals.watchdog')).toBeNull();
  });

  it('opens the diagnosis when asked', async () => {
    await hydrate();
    const onDiagnose = vi.fn();
    render(
      <FirstRunCard
        onDiagnose={onDiagnose}
        watchdogApi={watchdogApi()}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Find out why your PC is slow' }));
    expect(onDiagnose).toHaveBeenCalledTimes(1);
  });

  it('sends the Task Manager choice to Settings rather than changing the registry itself', async () => {
    await hydrate();
    const tm = taskManagerApi(notReplaced);
    const opened = vi.fn();
    window.addEventListener(OPEN_SETTINGS_EVENT, opened);
    render(<FirstRunCard onDiagnose={vi.fn()} watchdogApi={watchdogApi()} taskManagerApi={tm} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Open Vitals with Ctrl+Shift+Esc' }));

    expect(opened).toHaveBeenCalledTimes(1);
    expect(tm.write).not.toHaveBeenCalled();
    window.removeEventListener(OPEN_SETTINGS_EVENT, opened);
  });

  it('does not offer Task Manager replacement in the Store build, where it cannot work', async () => {
    host.packaged = true;
    await hydrate();
    render(
      <FirstRunCard
        onDiagnose={vi.fn()}
        watchdogApi={watchdogApi()}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );

    await screen.findByRole('button', { name: 'Watch for freezes and warn me' });
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Open Vitals with Ctrl+Shift+Esc' })).toBeNull(),
    );
  });

  it('does not offer the watchdog in a build that does not include it', async () => {
    await hydrate();
    const wd = watchdogApi({
      status: vi.fn(() => Promise.resolve({ available: false, registered: false, running: false })),
    });
    render(
      <FirstRunCard
        onDiagnose={vi.fn()}
        watchdogApi={wd}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );

    await waitFor(() => expect(wd.status).toHaveBeenCalled());
    expect(screen.queryByRole('button', { name: 'Watch for freezes and warn me' })).toBeNull();
  });

  it('shows nothing before the saved settings are read, so a dismissed card never flashes', () => {
    render(
      <FirstRunCard
        onDiagnose={vi.fn()}
        watchdogApi={watchdogApi()}
        taskManagerApi={taskManagerApi(notReplaced)}
      />,
    );
    expect(screen.queryByRole('heading', { name: 'Welcome to Vitals' })).toBeNull();
  });
});
