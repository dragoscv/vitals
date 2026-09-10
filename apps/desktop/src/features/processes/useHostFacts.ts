/**
 * The two slow facts the Processes screen needs once: the machine's static
 * description (for affinity presets) and the capability report (for how the
 * Disk column must be labelled).
 *
 * Fetched together and once per mount rather than per row: neither changes
 * while the screen is open, and the sampler's push channel deliberately does
 * not carry them.
 */

import { useEffect, useState } from 'react';

import type { HostInfo } from '@vitals/protocol';

import { hasTauriHost } from '../../shell/host';
import type { CapabilityReport } from './actions';

export interface HostFacts {
  readonly host: HostInfo | null;
  readonly capabilities: CapabilityReport | null;
}

/** Injectable so tests supply facts without a Tauri host. */
export type HostFactsReader = () => Promise<HostFacts>;

export const EMPTY_FACTS: HostFacts = { host: null, capabilities: null };

async function tauriHostFacts(): Promise<HostFacts> {
  const { invoke } = await import('@tauri-apps/api/core');
  // Independent failures degrade independently: a platform whose host info
  // command answers `Unsupported` still has a capability report, and losing
  // the disk tooltip because the About panel's data is missing would be an
  // odd coupling.
  const [host, capabilities] = await Promise.all([
    invoke<HostInfo>('get_host_info').catch(() => null),
    invoke<CapabilityReport>('get_capabilities').catch(() => null),
  ]);
  return { host, capabilities };
}

export function useHostFacts(reader?: HostFactsReader): HostFacts {
  const [facts, setFacts] = useState<HostFacts>(EMPTY_FACTS);

  // The injection seam bypasses the production guard rather than sitting
  // behind it: a test that injects a reader must have it called even though
  // there is no Tauri host.
  const injected = reader !== undefined;

  useEffect(() => {
    if (!injected && !hasTauriHost()) return;

    let live = true;
    const read = reader ?? tauriHostFacts;
    void read()
      .then((value) => {
        if (live) setFacts(value);
      })
      .catch(() => {
        // Nothing to show is a legitimate state, already rendered as such.
      });

    return () => {
      live = false;
    };
  }, [injected, reader]);

  return facts;
}
