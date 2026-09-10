/**
 * Affinity presets, computed from the machine's core topology.
 *
 * A free-form core picker is the obvious design and the wrong one: a
 * twenty-four-checkbox grid asks the user to know which of their logical
 * processors are performance cores, which is exactly the fact they came here
 * to be told. The presets answer the questions people actually have — "keep
 * this off my fast cores", "give this everything" — and each one is a mask
 * the backend already accepts.
 *
 * Presets that need a hybrid topology are omitted, not disabled, when the
 * machine is uniform. `HostInfo.coreTopology` is `null` there on purpose: a
 * "performance cores only" button on a machine with no such distinction
 * would be inventing one.
 */

import type { CoreClass, HostInfo } from '@vitals/protocol';

export type AffinityPresetId = 'all' | 'performance' | 'efficiency' | 'firstHalf' | 'secondHalf';

export interface AffinityPreset {
  readonly id: AffinityPresetId;
  /** The bitfield to send, one bit per logical processor. */
  readonly mask: bigint;
  /** How many logical processors the mask selects, for the label. */
  readonly cores: number;
}

/**
 * A mask with the bits at `indices` set.
 *
 * `bigint`, not `number`: a machine with more than 53 logical processors
 * cannot express its top cores in a double, and those machines exist.
 */
function maskOf(indices: readonly number[]): bigint {
  let mask = 0n;
  for (const index of indices) mask |= 1n << BigInt(index);
  return mask;
}

function indicesOf(topology: readonly CoreClass[], wanted: CoreClass): number[] {
  const found: number[] = [];
  topology.forEach((core, index) => {
    if (core === wanted) found.push(index);
  });
  return found;
}

/**
 * The presets worth offering for this machine.
 *
 * Returns an empty list when the logical core count is unknown or below two:
 * every preset on a single-core machine is "all cores", and five buttons
 * that do the same thing is worse than none.
 */
export function affinityPresets(host: HostInfo | null): readonly AffinityPreset[] {
  if (host === null) return [];

  const logical = host.logicalCores;
  if (logical < 2) return [];

  const all = Array.from({ length: logical }, (_, i) => i);
  const presets: AffinityPreset[] = [{ id: 'all', mask: maskOf(all), cores: logical }];

  const topology = host.coreTopology;
  if (topology !== null && topology.length === logical) {
    // `lowPower` is deliberately folded into neither bucket. Windows reports
    // it as a third class on Meteor Lake and later, and a user asking for
    // "efficiency cores" means the E-cores they have heard of, not the SoC
    // tile — putting both under one label would make the preset mean
    // different things on different machines.
    const performance = indicesOf(topology, 'performance');
    const efficiency = indicesOf(topology, 'efficiency');

    if (performance.length > 0) {
      presets.push({
        id: 'performance',
        mask: maskOf(performance),
        cores: performance.length,
      });
    }
    if (efficiency.length > 0) {
      presets.push({
        id: 'efficiency',
        mask: maskOf(efficiency),
        cores: efficiency.length,
      });
    }
  }

  // The halves are topology-free — they are the crude "keep these two things
  // apart" tool, and they work on any machine with more than one core.
  const half = Math.floor(logical / 2);
  presets.push(
    { id: 'firstHalf', mask: maskOf(all.slice(0, half)), cores: half },
    { id: 'secondHalf', mask: maskOf(all.slice(half)), cores: logical - half },
  );

  return presets;
}
