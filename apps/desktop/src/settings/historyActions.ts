/**
 * The two destructive-ish history actions, kept out of the panel component so
 * they can be tested and reused (the command palette will want both).
 */

import { hasTauriHost } from '../shell/host';

/** Deletes the per-app tally and the time-series store, immediately. */
export async function clearHistory(): Promise<void> {
  if (!hasTauriHost()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('clear_app_history');
}

/**
 * Writes the flight recording to a file the user chooses.
 *
 * A save dialog rather than a fixed path: this exists to be attached to a bug
 * report, so it must land somewhere the reporter can find it. Cancelling the
 * dialog is a normal outcome, not an error.
 *
 * Rust does the writing. Routing the bytes through the webview would mean
 * granting the filesystem plugin write access to arbitrary paths for one
 * feature, and serialising a two-minute recording twice.
 */
export async function exportFlightRecording(): Promise<string | null> {
  if (!hasTauriHost()) return null;

  const [{ invoke }, { save }] = await Promise.all([
    import('@tauri-apps/api/core'),
    import('@tauri-apps/plugin-dialog'),
  ]);

  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  const path = await save({
    defaultPath: `vitals-recording-${stamp}.json`,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  });
  if (path === null) return null;

  await invoke<void>('write_flight_recording', { path });
  return path;
}
