/**
 * Hands the alert engine the two things it cannot work out for itself.
 *
 * The engine runs in Rust on the sampler thread, but the notification
 * switches live in the webview's settings store and the translations live in
 * i18next. A toast has to fire while the window is hidden, so it cannot ask
 * for either at the moment it needs them — both are pushed instead.
 *
 * Failures are swallowed for the same reason as in `settingsSync.ts`: a
 * dropped push means the previous values stay in force, which is never worth
 * an unhandled rejection.
 */

import type { TFunction } from 'i18next';

import { hasTauriHost } from '../../shell/host';
import { ALERT_KINDS } from './alertText';

/** The four switches the Notifications panel offers. Matches `AlertPrefs`. */
export interface AlertPrefs {
  readonly notificationsEnabled: boolean;
  readonly notifyHighCpu: boolean;
  readonly notifyHighMemory: boolean;
  readonly notifyThermal: boolean;
}

/** Tells the engine which alerts the user wants to hear about. */
export async function syncAlertPrefs(prefs: AlertPrefs): Promise<void> {
  if (!hasTauriHost()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_alert_prefs', { prefs });
  } catch {
    // Intentionally ignored; see module doc.
  }
}

/**
 * Hands over a toast title for every kind in the current language.
 *
 * All thirteen, unconditionally: Rust has no fallback and skips any kind it
 * has no string for, so an omission is a notification the user simply never
 * receives — silent, and indistinguishable from the condition not occurring.
 */
export async function syncAlertStrings(t: TFunction): Promise<void> {
  if (!hasTauriHost()) return;

  const titles: Record<string, string> = {};
  for (const kind of ALERT_KINDS) {
    titles[kind] = t(`alert.${kind}.title`);
  }

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    // Both spellings deliberately. `NotificationStrings` in
    // `src-tauri/src/alerts.rs` carries no `rename_all = "camelCase"`, so
    // serde wants `cleared_suffix` — while every other command on this
    // boundary is camelCase, and the field is one attribute away from
    // becoming so. Serde ignores the unknown one, so this survives the change
    // in either direction; a missing required field would fail the whole
    // command and silently cost every toast its title.
    await invoke('set_alert_strings', {
      strings: {
        titles,
        cleared_suffix: t('alert.cleared'),
        clearedSuffix: t('alert.cleared'),
      },
    });
  } catch {
    // Intentionally ignored; see module doc.
  }
}
