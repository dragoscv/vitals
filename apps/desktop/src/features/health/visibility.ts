/**
 * Feeds the nav visibility store from `health_status`.
 *
 * Its own small module so the shell can import it without pulling the Health
 * screen into the entry chunk; the command itself is a dynamic import.
 */

import { useEffect } from 'react';

import { useNavVisibility } from '../../shell/navigation';
import { defaultHealthApi, type HealthApi } from './api';

/** Reads whether a vmui connection is stored and publishes it. */
export async function refreshHealthVisibility(api: HealthApi = defaultHealthApi()): Promise<void> {
  let configured = false;
  try {
    configured = (await api.status()).configured;
  } catch {
    // An unreadable secret store is "not connected": the page stays hidden
    // rather than appearing with nothing it can show.
  }
  useNavVisibility.getState().setHealth(configured);
}

/** Once per shell mount. Settings refreshes it again after every change. */
export function useHealthVisibility(api?: HealthApi): void {
  useEffect(() => {
    void refreshHealthVisibility(api);
  }, [api]);
}
