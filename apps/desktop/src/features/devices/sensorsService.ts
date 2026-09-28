/**
 * The optional CPU sensors service: status and install / remove.
 *
 * The one place the two command names are spelled. Taken as an interface by
 * the panel so it is testable without a Tauri host — the same seam as
 * `SensorsReader`.
 *
 * Why a service exists at all is ADR-0031: CPU temperature and package power
 * live in model-specific registers (ring 0), the signed PawnIO driver is the
 * way in, and opening it needs administrator rights, so a small SYSTEM
 * service reads them and the app only reads its pipe.
 */

export interface SensorsServiceStatus {
  /** The service answered on its pipe, even if only with an error. */
  readonly installed: boolean;
  /** It answered with a reading. */
  readonly running: boolean;
  readonly error: string | null;
  /** When false, installing also downloads the PawnIO driver. */
  readonly pawnioInstalled: boolean;
  /** This build ships the helper; a bare dev build does not. */
  readonly helperAvailable: boolean;
}

export interface SensorsServiceApi {
  status(): Promise<SensorsServiceStatus>;
  /** Resolves once the elevated helper has finished; rejects with a CommandError. */
  setup(install: boolean): Promise<void>;
}

export const tauriSensorsService: SensorsServiceApi = {
  async status() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<SensorsServiceStatus>('get_sensors_service');
  },
  async setup(install) {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('setup_sensors_service', { install });
  },
};

/** Which affordance the panel offers. */
export type ServiceAction = 'install' | 'remove' | 'unavailable' | 'none';

/**
 * - Not installed, helper present → install.
 * - Installed (reading or not) → remove, so a service that cannot read this
 *   CPU is never left running without a way to take it off.
 * - Not installed, no helper → say so; a button that fails after a UAC
 *   prompt is worse than no button.
 */
export function serviceAction(status: SensorsServiceStatus | null): ServiceAction {
  if (status === null) return 'none';
  if (status.installed) return 'remove';
  return status.helperAvailable ? 'install' : 'unavailable';
}
