import { useEffect, useState } from 'react';

import { signalReady } from './lib/ready';
import { effectiveRate, pushSampleRate } from './lib/sampleRate';
import { useSettings } from './settings/store';
import { AppShell } from './shell/AppShell';
import { hasTauriHost } from './shell/host';
import { ThemeProvider } from './theme/ThemeProvider';

// Shell strings are registered in `main.tsx`, after `initI18n` has resolved.
// They cannot be registered here at module scope: this module is imported
// before that await runs, and i18next does not define `addResourceBundle`
// until it is initialised.

/**
 * Reads the version from the Tauri host.
 *
 * At runtime rather than baked in at build time, so the About panel can never
 * disagree with what the installer registered — those have drifted before,
 * and a wrong version in a bug report costs more than one async call.
 */
function useAppVersion(): string {
  const [version, setVersion] = useState('');

  useEffect(() => {
    if (!hasTauriHost()) return;
    let live = true;
    void import('@tauri-apps/api/app')
      .then((module) => module.getVersion())
      .then((value) => {
        if (live) setVersion(value);
      })
      .catch(() => {
        // A missing version is cosmetic. Failing the render over it would
        // take down the whole settings dialog.
      });
    return () => {
      live = false;
    };
  }, []);

  return version;
}

/**
 * Keeps the Rust sampler's cadence in step with the settings and the window.
 *
 * Sends on change rather than on a timer, and only when the resolved rate
 * actually differs — dragging the rate control through three options should
 * not produce three round trips to the backend for two rates nobody stopped
 * on.
 *
 * `visibilitychange` covers minimise and occlusion. It fires in a Tauri
 * webview the same way it does in a browser, so no Tauri-specific window
 * event is needed.
 */
function useSampleRate(settings: ReturnType<typeof useSettings.getState>['settings']): void {
  const { samplingRate, throttleWhenHidden } = settings;

  useEffect(() => {
    let last: string | null = null;

    const send = () => {
      const rate = effectiveRate(
        { samplingRate, throttleWhenHidden },
        document.visibilityState === 'visible',
      );

      if (rate === last) return;
      last = rate;
      void pushSampleRate(rate);
    };

    send();
    document.addEventListener('visibilitychange', send);
    return () => {
      document.removeEventListener('visibilitychange', send);
    };
  }, [samplingRate, throttleWhenHidden]);
}

/**
 * The application shell.
 *
 * Settings hydration and the ready signal are separated on purpose. The window
 * must be revealed as soon as the shell has painted, whether or not the store
 * has been read — a settings file that is slow, locked or corrupt would
 * otherwise leave the user with no window at all, and the Rust fallback would
 * reveal a blank one five seconds later.
 */
export function App() {
  const settings = useSettings((state) => state.settings);
  const hydrated = useSettings((state) => state.hydrated);
  const setTheme = useSettings((state) => state.setTheme);
  const hydrate = useSettings((state) => state.hydrate);
  const version = useAppVersion();

  useSampleRate(settings);

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  // Two frames after the shell commits, whether or not settings have loaded.
  useEffect(() => {
    signalReady();
  }, []);

  return (
    <ThemeProvider
      // Remounted once hydration completes so the stored theme becomes the
      // provider's initial state rather than a post-mount transition. Applying
      // it as a change would flash the default palette on every launch.
      key={hydrated ? 'hydrated' : 'defaults'}
      initial={settings.theme}
      onChange={setTheme}
    >
      <AppShell version={version} />
    </ThemeProvider>
  );
}
