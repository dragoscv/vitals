/**
 * How this copy was installed: the direct installer, the Microsoft Store or
 * the Mac App Store.
 *
 * The Microsoft Store build is the same binary inside an MSIX package; the
 * Mac App Store build is compiled without the updater. Either way store
 * policy comes from `distribution.rs`: no self-update, no Task Manager hook,
 * no sensors service, no login item. The UI hides those rows rather than
 * offering a switch that can only fail.
 */

import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';

import { hasTauriHost } from '../shell/host';

export type Store = 'microsoft' | 'macAppStore';

export interface Distribution {
  readonly packaged: boolean;
  /** Absent from a host older than the field, which only shipped to Microsoft. */
  readonly store?: Store | null;
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

/** Which store updates this copy, or `null` for the direct build. */
export function useStore(): Store | null {
  const [store, setStore] = useState<Store | null>(null);
  useEffect(() => {
    let live = true;
    void readDistribution().then((d) => {
      if (live) setStore(d.packaged ? (d.store ?? 'microsoft') : null);
    });
    return () => {
      live = false;
    };
  }, []);
  return store;
}
