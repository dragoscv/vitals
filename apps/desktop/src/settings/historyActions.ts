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
 * Rust shows the save dialog and writes the file itself, so the destination
 * never crosses the IPC boundary and a compromised webview cannot be made to
 * write to an arbitrary path. It also serialises the two-minute recording
 * once rather than routing the bytes through the webview.
 *
 * Returns the chosen path, or `null` when the dialog was cancelled — a normal
 * outcome, not an error.
 */
export async function exportFlightRecording(): Promise<string | null> {
  if (!hasTauriHost()) return null;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string | null>('write_flight_recording');
}
