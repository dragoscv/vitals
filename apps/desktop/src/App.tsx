import { useEffect, useState } from 'react';

import { signalReady } from './lib/ready';
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
