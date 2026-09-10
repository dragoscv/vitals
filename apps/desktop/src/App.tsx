import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { syncAlertPrefs, syncAlertStrings } from './features/alerts/pushPrefs';
import { DASHBOARD_NS } from './features/dashboard/strings';
import { signalReady, wasAutostarted } from './lib/ready';
import { effectiveRate, pushSampleRate } from './lib/sampleRate';
import {
  pushCloseToTray,
  pushHistoryEnabled,
  pushRetentionDays,
  pushTrayStrings,
  reconcileAutostart,
} from './lib/settingsSync';
import { useHud } from './lib/useHud';
import { useSettings } from './settings/store';
import { AppShell } from './shell/AppShell';
import { SHELL_NS } from './shell/strings';
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
 * Pushes the settings that take effect outside the webview.
 *
 * Waits for hydration: before it, `settings` is the defaults, and pushing
 * `historyEnabled: false` from the defaults would briefly turn off a store the
 * user has enabled, and `startWithWindows: false` would remove their Run-key
 * entry on every launch.
 */
function useSettingsSync(
  settings: ReturnType<typeof useSettings.getState>['settings'],
  hydrated: boolean,
): void {
  const { historyEnabled, retentionDays, startWithWindows, closeToTray } = settings;
  const patch = useSettings((state) => state.patch);

  useEffect(() => {
    if (!hydrated) return;
    void pushHistoryEnabled(historyEnabled);
  }, [hydrated, historyEnabled]);

  useEffect(() => {
    if (!hydrated) return;
    void pushRetentionDays(retentionDays);
  }, [hydrated, retentionDays]);

  useEffect(() => {
    if (!hydrated) return;
    void pushCloseToTray(closeToTray);
  }, [hydrated, closeToTray]);

  useEffect(() => {
    if (!hydrated) return;
    void reconcileAutostart(startWithWindows).then((actual) => {
      // The OS is the source of truth. If it refused, or Task Manager had
      // already disabled the entry, the switch must show that rather than
      // the value we wished for.
      if (actual !== null && actual !== startWithWindows) {
        patch({ startWithWindows: actual });
      }
    });
  }, [hydrated, startWithWindows, patch]);
}

/**
 * Keeps the Rust alert engine's notification switches in step with settings.
 *
 * Waits for hydration for the same reason as `useSettingsSync`: before it the
 * values are the defaults, which are all off, and pushing those would silence
 * notifications a user had enabled for as long as the settings file takes to
 * read.
 */
function useAlertPrefsSync(
  settings: ReturnType<typeof useSettings.getState>['settings'],
  hydrated: boolean,
): void {
  const { notificationsEnabled, notifyHighCpu, notifyHighMemory, notifyThermal } = settings;

  useEffect(() => {
    if (!hydrated) return;
    void syncAlertPrefs({
      notificationsEnabled,
      notifyHighCpu,
      notifyHighMemory,
      notifyThermal,
    });
  }, [hydrated, notificationsEnabled, notifyHighCpu, notifyHighMemory, notifyThermal]);
}

/**
 * Hands the engine a toast title for every alert kind, in the current
 * language.
 *
 * Toasts are rendered in Rust because they must fire while the window is
 * hidden, so the strings have to be pushed rather than fetched. Re-pushed on
 * a language change: without it, a user who switches to Romanian keeps
 * receiving English notifications until the next restart, with nothing on
 * screen to explain why.
 */
function useAlertStringsSync(): void {
  const { t, i18n } = useTranslation(DASHBOARD_NS);

  useEffect(() => {
    const push = (): void => {
      void syncAlertStrings(t);
    };

    push();
    i18n.on('languageChanged', push);
    return () => {
      i18n.off('languageChanged', push);
    };
  }, [t, i18n]);
}

/**
 * Hands the tray its menu labels and tooltip words.
 *
 * Identical argument to `useAlertStringsSync`, and re-pushed on a language
 * change for the same reason: the tray menu is the only way to quit once the
 * × hides the window, so leaving it in the wrong language is worse than a
 * mislabelled toast.
 */
function useTrayStringsSync(): void {
  const { t, i18n } = useTranslation(SHELL_NS);

  useEffect(() => {
    const push = (): void => {
      void pushTrayStrings({
        show: t('tray.show'),
        pause: t('tray.pause'),
        quit: t('tray.quit'),
        cpu: t('tray.cpu'),
        memory: t('tray.memory'),
        gpu: t('tray.gpu'),
        stillRunning: t('tray.stillRunning'),
      });
    };

    push();
    i18n.on('languageChanged', push);
    return () => {
      i18n.off('languageChanged', push);
    };
  }, [t, i18n]);
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
  useSettingsSync(settings, hydrated);
  useAlertPrefsSync(settings, hydrated);
  useAlertStringsSync();
  useTrayStringsSync();
  useHud(hydrated);

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  // Reveal two frames after the shell commits — unless this launch came from
  // the autostart entry and the user asked to start minimised, in which case
  // the window stays hidden and the tray is the way back in.
  //
  // A user launch reveals without waiting for hydration: a slow or corrupt
  // settings file must never cost them the window. An autostarted launch
  // waits, because the decision depends on a setting, and there is no user in
  // front of the screen to notice the delay.
  const startMinimised = settings.startMinimised;
  useEffect(() => {
    let live = true;
    void wasAutostarted().then((auto) => {
      if (!live) return;
      if (!auto) {
        signalReady();
        return;
      }
      if (hydrated && !startMinimised) signalReady();
    });
    return () => {
      live = false;
    };
  }, [hydrated, startMinimised]);

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
