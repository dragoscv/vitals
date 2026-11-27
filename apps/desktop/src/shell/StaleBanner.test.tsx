/**
 * Guards the stale-data banner.
 *
 * The failure it defends against is silent: when the sampler stops pushing
 * frames the meters freeze on their last value and look exactly as they do on
 * a genuinely idle machine. Nothing else in the UI would ever notice, because
 * nothing else is woken by the absence of an event. This test drives the
 * clock past the threshold and back, so the banner must both appear and clear
 * itself without any user action.
 */

import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import type { SettingsBackend } from '../settings/persistence';
import { samplingIntervalMs } from '../settings/schema';
import { resetSettingsForTests, setSettingsBackend } from '../settings/store';
import { STALE_AFTER_INTERVALS, StaleBanner, TICK_MS } from './StaleBanner';

/** Controls whether the module under test believes a Tauri host exists. */
const hasHost = vi.hoisted(() => ({ value: true }));

vi.mock('./host', () => ({
  hasTauriHost: () => hasHost.value,
}));

interface Listener {
  onSnapshot: (snapshot: { timestampMs: number }) => void;
}

const listeners = vi.hoisted(() => [] as Listener[]);
const subscribeToMetrics = vi.hoisted(() =>
  vi.fn((listener: Listener) => {
    listeners.push(listener);
    return Promise.resolve(() => undefined);
  }),
);
const latestMetrics = vi.hoisted(() => vi.fn<() => unknown>(() => null));

vi.mock('@/lib/metrics', () => ({ subscribeToMetrics, latestMetrics }));

const nullBackend: SettingsBackend = {
  load: () => Promise.resolve(undefined),
  save: () => Promise.resolve(),
};

beforeAll(async () => {
  await initI18n('en');
  const { registerShellStrings } = await import('./strings');
  registerShellStrings();
});

beforeEach(async () => {
  vi.useFakeTimers();
  hasHost.value = true;
  listeners.length = 0;
  subscribeToMetrics.mockClear();
  latestMetrics.mockReset();
  latestMetrics.mockReturnValue(null);
  await i18n.changeLanguage('en');
  resetSettingsForTests();
  setSettingsBackend(nullBackend);
});

afterEach(() => {
  vi.useRealTimers();
});

/** The default sampling rate is `normal`, so this is the interval in play. */
const interval = samplingIntervalMs.normal;

describe('the stale-data banner', () => {
  it('appears once the readings pass twice the interval, and hides itself on the next frame', async () => {
    const t0 = 1_700_000_000_000;
    vi.setSystemTime(t0);
    render(<StaleBanner />);
    await act(async () => undefined);

    // No frame has ever arrived, so there is nothing to call stale.
    expect(screen.queryByRole('status')).toBeNull();

    // A frame lands now; the banner stays away.
    await act(async () => {
      listeners[0]?.onSnapshot({ timestampMs: t0 });
    });
    expect(screen.queryByRole('status')).toBeNull();

    // The stream goes quiet. Past twice the interval the banner appears on
    // its own, with no interaction.
    await act(async () => {
      vi.advanceTimersByTime(STALE_AFTER_INTERVALS * interval + TICK_MS);
    });
    expect(screen.getByRole('status')).toBeTruthy();
    expect(screen.getByText(/stopped updating/i)).toBeTruthy();

    // A fresh frame must clear it, again with no interaction.
    await act(async () => {
      listeners[0]?.onSnapshot({ timestampMs: Date.now() });
    });
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('stays quiet while the readings merely arrive slowly', async () => {
    const t0 = 1_700_000_000_000;
    vi.setSystemTime(t0);
    render(<StaleBanner />);
    await act(async () => undefined);
    await act(async () => {
      listeners[0]?.onSnapshot({ timestampMs: t0 });
    });

    // Just under the threshold: a late frame is not a stopped one.
    await act(async () => {
      vi.advanceTimersByTime(STALE_AFTER_INTERVALS * interval - TICK_MS);
    });
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('shows nothing when there is no Tauri host to be stale about', async () => {
    hasHost.value = false;
    vi.setSystemTime(1_700_000_000_000);
    render(<StaleBanner />);
    await act(async () => undefined);

    await act(async () => {
      vi.advanceTimersByTime(STALE_AFTER_INTERVALS * interval + TICK_MS);
    });
    expect(screen.queryByRole('status')).toBeNull();
    expect(subscribeToMetrics).not.toHaveBeenCalled();
  });
});
