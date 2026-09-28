import { beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n, i18n } from '@vitals/i18n';

import type { SettingsBackend } from './persistence';
import { defaultSettings } from './schema';
import { flushSettings, resetSettingsForTests, setSettingsBackend, useSettings } from './store';

function memoryBackend(seed: unknown = undefined) {
  let stored: unknown = seed;
  const saves: unknown[] = [];

  const backend: SettingsBackend = {
    load: () => Promise.resolve(stored),
    save: (value) => {
      stored = value;
      saves.push(value);
      return Promise.resolve();
    },
  };

  return { backend, saves, read: () => stored };
}

beforeEach(async () => {
  await initI18n();
  await i18n.changeLanguage('en');
  resetSettingsForTests();
  vi.useRealTimers();
});

describe('settings store', () => {
  it('starts from defaults before hydration', () => {
    expect(useSettings.getState().hydrated).toBe(false);
    expect(useSettings.getState().settings).toEqual(defaultSettings);
  });

  it('restores persisted values', async () => {
    const { backend } = memoryBackend({
      theme: { mode: 'dark', accent: 'green', density: 'compact' },
      sidebarCollapsed: true,
      lastRoute: 'processes',
    });
    setSettingsBackend(backend);

    await useSettings.getState().hydrate();

    const state = useSettings.getState();
    expect(state.hydrated).toBe(true);
    expect(state.settings.theme.accent).toBe('green');
    expect(state.settings.sidebarCollapsed).toBe(true);
    // The route is restored, not just recorded — reopening on the section you
    // were last using is the whole point of persisting it.
    expect(state.route).toBe('processes');
  });

  it('survives a corrupt store rather than failing to start', async () => {
    setSettingsBackend({
      load: () => Promise.reject(new Error('the file is not JSON')),
      save: () => Promise.resolve(),
    });
    vi.spyOn(console, 'error').mockImplementation(() => {});

    await useSettings.getState().hydrate();

    expect(useSettings.getState().hydrated).toBe(true);
    expect(useSettings.getState().settings).toEqual(defaultSettings);
  });

  it('applies a persisted locale to i18n', async () => {
    const { backend } = memoryBackend({ locale: 'ro' });
    setSettingsBackend(backend);

    await useSettings.getState().hydrate();

    expect(i18n.language).toBe('ro');
  });

  it('writes a change back to the backend', async () => {
    const { backend, read } = memoryBackend();
    setSettingsBackend(backend);
    await useSettings.getState().hydrate();

    useSettings.getState().patch({ historyEnabled: true });
    await flushSettings();

    expect((read() as { historyEnabled: boolean }).historyEnabled).toBe(true);
  });

  it('coalesces a burst of changes into one write', async () => {
    // Dragging a slider or arrowing through accents emits a change per frame.
    // One fsync per frame in an app that promises to cost nothing is a bug.
    vi.useFakeTimers();
    const { backend, saves } = memoryBackend();
    setSettingsBackend(backend);

    useSettings.getState().patch({ retentionDays: 1 });
    useSettings.getState().patch({ retentionDays: 7 });
    useSettings.getState().patch({ retentionDays: 30 });

    expect(saves).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(400);
    expect(saves).toHaveLength(1);
    expect((saves[0] as { retentionDays: number }).retentionDays).toBe(30);
    vi.useRealTimers();
  });

  it('keeps route and lastRoute in step', async () => {
    const { backend } = memoryBackend();
    setSettingsBackend(backend);
    await useSettings.getState().hydrate();

    useSettings.getState().navigate('benchmarks');

    expect(useSettings.getState().route).toBe('benchmarks');
    expect(useSettings.getState().settings.lastRoute).toBe('benchmarks');
  });

  it('toggles the sidebar as a persisted preference', async () => {
    const { backend, read } = memoryBackend();
    setSettingsBackend(backend);
    await useSettings.getState().hydrate();

    useSettings.getState().toggleSidebar();
    await flushSettings();

    expect(useSettings.getState().settings.sidebarCollapsed).toBe(true);
    expect((read() as { sidebarCollapsed: boolean }).sidebarCollapsed).toBe(true);
  });
});
