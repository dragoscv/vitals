/**
 * How this copy was installed: the direct installer or the Microsoft Store.
 *
 * The Store build is the same binary inside an MSIX package, and Store policy
 * is applied at run time (`distribution.rs`): no self-update, no Task Manager
 * hook, no sensors service. The UI hides those rows rather than offering a
 * switch that can only fail.
 */

import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';

import { hasTauriHost } from '../shell/host';

export interface Distribution {
  readonly packaged: boolean;
}

let cached: Promise<Distribution> | null = null;

/** One IPC round trip per window: package identity cannot change while running. */
export function readDistribution(): Promise<Distribution> {
  if (!hasTauriHost()) return Promise.resolve({ packaged: false });
  cached ??= invoke<Distribution>('get_distribution').catch(() => ({ packaged: false }));
  return cached;
}

/** `false` until answered: the direct build is the default, so nothing flickers in for it. */
export function useIsStoreBuild(): boolean {
  const [packaged, setPackaged] = useState(false);
  useEffect(() => {
    let live = true;
    void readDistribution().then((d) => {
      if (live) setPackaged(d.packaged);
    });
    return () => {
      live = false;
    };
  }, []);
  return packaged;
}
