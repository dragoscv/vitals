/**
 * The commands behind the Startup and Services row menus.
 *
 * An interface rather than bare functions so the screen can be tested with
 * fakes: every call here changes the machine, and a test that reaches the
 * real IPC module either fails for want of a host or, worse, does not.
 *
 * # `confirmed` travels with every call
 *
 * The backend refuses a risky change that arrives with `confirmed: false`.
 * The UI decides when to ask, but the flag makes the backend the final word,
 * so a future caller that forgets the dialog gets a refusal instead of a
 * machine that no longer signs in.
 *
 * Each import is dynamic so a browser build never evaluates the IPC module,
 * the same reason `readStartup` does it.
 */

import type { ServiceEntry, StartupEntry } from './model';

export type ServiceControl = 'start' | 'stop' | 'restart';
export type ServiceStartType = 'automatic' | 'manual' | 'disabled';

export interface StartupActions {
  setStartupEnabled(entry: StartupEntry, enabled: boolean, confirmed: boolean): Promise<void>;
  controlService(service: ServiceEntry, action: ServiceControl, confirmed: boolean): Promise<void>;
  setServiceStartType(
    service: ServiceEntry,
    startType: ServiceStartType,
    confirmed: boolean,
  ): Promise<void>;
  openFileLocation(path: string): Promise<void>;
  showFileProperties(path: string): Promise<void>;
}

/** Loaded per call; `import()` caches the module, so only the first call pays. */
const ipc = () => import('@tauri-apps/api/core');

/**
 * The real implementation. The backend raises the UAC prompt itself when
 * needed.
 *
 * Each command name is a literal `invoke('…')` rather than a string passed to
 * a shared helper: `scripts/check-drift.ps1` finds commands by that literal
 * shape, and a helper would hide every name from it.
 */
export const tauriStartupActions: StartupActions = {
  async setStartupEnabled(entry, enabled, confirmed) {
    const { invoke } = await ipc();
    await invoke<void>('set_startup_enabled', {
      source: entry.source,
      name: entry.name,
      enabled,
      confirmed,
    });
  },
  async controlService(service, action, confirmed) {
    const { invoke } = await ipc();
    await invoke<void>('control_service', { name: service.name, action, confirmed });
  },
  async setServiceStartType(service, startType, confirmed) {
    const { invoke } = await ipc();
    await invoke<void>('set_service_start_type', { name: service.name, startType, confirmed });
  },
  async openFileLocation(path) {
    const { invoke } = await ipc();
    await invoke<void>('open_file_location', { path });
  },
  async showFileProperties(path) {
    const { invoke } = await ipc();
    await invoke<void>('show_file_properties', { path });
  },
};
