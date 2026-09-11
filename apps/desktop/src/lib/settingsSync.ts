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
 * Drives the overlay window to match the setting.
 *
 * Absolute rather than a toggle: this runs on hydration to restore an overlay
 * the user left on, and a toggle there would close one that the global
 * shortcut had already opened. Returns what the backend reports, or `null`
 * when there is no host to ask.
 */
export async function pushHudVisible(visible: boolean): Promise<boolean | null> {
  if (!hasTauriHost()) return null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<boolean>('set_hud_visible', { visible });
  } catch {
    return null;
  }
}

/** Shows the overlay if hidden, hides it if shown. Returns the state after. */
export async function toggleHud(): Promise<boolean | null> {
  if (!hasTauriHost()) return null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<boolean>('toggle_hud');
  } catch {
    return null;
  }
}

/**
 * Tells the backend whether the × button hides the window.
 *
 * The decision has to live in Rust: `CloseRequested` is answered before the
 * webview hears about it, so asking the frontend at that moment would be too
 * late and the window would close regardless of the setting.
 */
export async function pushCloseToTray(enabled: boolean): Promise<void> {
  if (!hasTauriHost()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_close_to_tray', { enabled });
  } catch {
    // Intentionally ignored; see module doc.
  }
}

/**
 * Hands the tray its labels.
 *
 * Same shape as the alert strings: the tray renders natively so it keeps
 * working with the window hidden, but every translation lives in the webview,
 * so the webview pushes them at start and on a language change.
 */
export async function pushTrayStrings(strings: {
  show: string;
  pause: string;
  taskManager: string;
  quit: string;
  cpu: string;
  memory: string;
  gpu: string;
  stillRunning: string;
}): Promise<void> {
  if (!hasTauriHost()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_tray_strings', { strings });
  } catch {
    // Intentionally ignored; see module doc.
  }
}

/**
 * Opens the real Windows Task Manager.
 *
 * Goes through the backend rather than a shell `taskmgr` launch: while
 * Vitals is registered as the Task Manager replacement, launching
 * `taskmgr.exe` normally would start a second Vitals, and the user who
 * pressed this button wanted the other program. Not swallowed for the same
 * reason as `quitApp`.
 */
export async function openWindowsTaskManager(): Promise<void> {
  if (!hasTauriHost()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('launch_real_taskmgr');
}

/**
 * Exits Vitals.
 *
 * Exists because `closeToTray` turns the × into a hide, so the window needs a
 * quit path that does not require finding the tray icon. Not swallowed: if
 * quitting fails the user should know, because the alternative is Task
 * Manager.
 */
export async function quitApp(): Promise<void> {
  if (!hasTauriHost()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('quit_app');
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
