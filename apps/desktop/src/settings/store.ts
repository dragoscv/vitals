import { create } from 'zustand';

import { i18n } from '@vitals/i18n';

import type { RouteId } from '../shell/navigation';
import type { ThemeSettings } from '../theme/types';
import { createBackend, type SettingsBackend } from './persistence';
import { defaultSettings, parseSettings, type AppSettings } from './schema';

/**
 * Delay before a change reaches disk.
 *
 * Dragging the density selector or arrowing through ten accent colours emits a
 * change per keypress. Writing each one is a file open, serialise and fsync
 * per frame, in an app whose entire pitch is that it costs nothing to run.
 */
const WRITE_DEBOUNCE_MS = 250;

/**
 * Where the overlay window reads the language from.
 *
 * The HUD's capability grants it no store access, on purpose, so the one
 * setting it needs is mirrored into `localStorage`, which every webview of
 * this app shares. Must match `LOCALE_KEY` in `src/hud/main.tsx`.
 */
const HUD_LOCALE_KEY = 'vitals.locale.v1';

function mirrorLocale(locale: string): void {
  try {
    window.localStorage.setItem(HUD_LOCALE_KEY, locale);
  } catch {
    // Storage can be unavailable or full. The overlay then falls back to
    // English, which is a nuisance rather than a failure.
  }
}

interface SettingsState {
  readonly settings: AppSettings;
  /**
   * Whether the on-disk state has been read.
   *
   * Exposed rather than hidden because rendering the shell with defaults and
   * then swapping to the user's real theme is a visible flash of the wrong
   * colours on every launch.
   */
  readonly hydrated: boolean;
  readonly route: RouteId;

  hydrate: () => Promise<void>;
  patch: (changes: Partial<AppSettings>) => void;
  setTheme: (theme: ThemeSettings) => void;
  navigate: (route: RouteId) => void;
  toggleSidebar: () => void;
}

/**
 * Overridable so tests can drive hydration and persistence without a Tauri
 * host, and so a future portable mode can swap in a file next to the binary.
 */
let backend: SettingsBackend = createBackend();

export function setSettingsBackend(next: SettingsBackend): void {
  backend = next;
}

let writeTimer: ReturnType<typeof setTimeout> | undefined;

function scheduleWrite(settings: AppSettings): void {
  clearTimeout(writeTimer);
  writeTimer = setTimeout(() => {
    void backend.save(settings).catch((error: unknown) => {
      // Never surface this as a thrown error. Losing a preference is a minor
      // annoyance; an unhandled rejection in a settings dialog is a crash.
      console.error('failed to save settings', error);
    });
  }, WRITE_DEBOUNCE_MS);
}

/** Flushes any pending write. Test seam, and useful before a deliberate quit. */
export async function flushSettings(): Promise<void> {
  clearTimeout(writeTimer);
  await backend.save(useSettings.getState().settings);
}

export const useSettings = create<SettingsState>((set, get) => ({
  settings: defaultSettings,
  hydrated: false,
  route: defaultSettings.lastRoute,

  async hydrate() {
    let settings = defaultSettings;
    try {
      settings = parseSettings(await backend.load());
    } catch (error) {
      console.error('failed to read settings; falling back to defaults', error);
    }

    if (settings.locale !== i18n.language) {
      await i18n.changeLanguage(settings.locale);
    }
    mirrorLocale(settings.locale);

    set({ settings, hydrated: true, route: settings.lastRoute });
  },

  patch(changes) {
    const settings = { ...get().settings, ...changes };
    set({ settings });
    scheduleWrite(settings);

    if (changes.locale !== undefined && changes.locale !== i18n.language) {
      void i18n.changeLanguage(changes.locale);
    }
    if (changes.locale !== undefined) mirrorLocale(changes.locale);
  },

  setTheme(theme) {
    get().patch({ theme });
  },

  navigate(route) {
    // `lastRoute` mirrors `route` so the window reopens where it was left.
    // Kept as one action so the two can never disagree.
    set({ route });
    get().patch({ lastRoute: route });
  },

  toggleSidebar() {
    const { settings } = get();
    get().patch({ sidebarCollapsed: !settings.sidebarCollapsed });
  },
}));

/** Test seam: returns the store to its pre-hydration state. */
export function resetSettingsForTests(): void {
  clearTimeout(writeTimer);
  useSettings.setState({
    settings: defaultSettings,
    hydrated: false,
    route: defaultSettings.lastRoute,
  });
}
