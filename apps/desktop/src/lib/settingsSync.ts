/**
 * Pushes settings that live outside the webview to where they take effect.
 *
 * Every function here is the "unfed" half of a setting that once wrote to the
 * store and did nothing else. Each is idempotent and swallows failures for
 * the same reason `pushSampleRate` does: a dropped update means the previous
 * value stays in force, which is never worth an unhandled rejection.
 */

import { hasTauriHost } from '../shell/host';

/** Tells the sampler whether to accumulate per-app history. */
export async function pushHistoryEnabled(enabled: boolean): Promise<void> {
  if (!hasTauriHost()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_history_enabled', { enabled });
  } catch {
    // Intentionally ignored; see module doc.
  }
}

/** Tells the store how long to keep recorded history. */
export async function pushRetentionDays(days: number): Promise<void> {
  if (!hasTauriHost()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_retention_days', { days });
  } catch {
    // Intentionally ignored; see module doc.
  }
}

/**
 * Reconciles the OS autostart entry with the setting.
 *
 * Reads the current state first and only writes on a mismatch. Task Manager's
 * Startup tab can disable the entry behind our back, and `enable()` on every
 * launch would silently override a choice the user made there. On a mismatch
 * we follow the setting because that is the one they just changed in front of
 * us; the read-back is what lets the UI show the truth if the OS disagrees.
 *
 * Returns the state after reconciliation, or `null` when it could not be read.
 */
export async function reconcileAutostart(wanted: boolean): Promise<boolean | null> {
  if (!hasTauriHost()) return null;
  try {
    const autostart = await import('@tauri-apps/plugin-autostart');
    const current = await autostart.isEnabled();
    if (current === wanted) return current;
    if (wanted) await autostart.enable();
    else await autostart.disable();
    return autostart.isEnabled();
  } catch {
    return null;
  }
}
