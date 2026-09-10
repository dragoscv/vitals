import { describe, expect, it } from 'vitest';

import type { HostInfo } from '@vitals/protocol';

import { affinityPresets } from './affinity';

function host(logicalCores: number, coreTopology: HostInfo['coreTopology']): HostInfo {
  return {
    hostname: 'box',
    osName: 'Windows',
    osVersion: '11',
    kernelVersion: '22631',
    architecture: 'x86_64',
    cpuModel: 'test',
    cpuVendor: 'test',
    physicalCores: logicalCores,
    logicalCores,
    coreTopology,
    totalMemory: 0,
    bootTimeMs: 0,
    isVirtualMachine: false,
    motherboard: null,
    biosVersion: null,
  };
}

describe('affinityPresets', () => {
  it('returns nothing without a host, or on a single core where every preset is the same', () => {
    expect(affinityPresets(null)).toEqual([]);
    expect(affinityPresets(host(1, null))).toEqual([]);
  });

  it('omits the class presets on a uniform machine rather than inventing a split', () => {
    const ids = affinityPresets(host(8, null)).map((p) => p.id);
    expect(ids).toEqual(['all', 'firstHalf', 'secondHalf']);
  });

  it('builds masks by logical index, so the top cores of a wide machine are reachable', () => {
    // 64 logical processors: bit 63 does not fit in a double. The mask must
    // be a bigint or the "second half" preset silently drops the top cores.
    const presets = affinityPresets(host(64, null));
    const all = presets.find((p) => p.id === 'all');
    const second = presets.find((p) => p.id === 'secondHalf');
    expect(all?.mask).toBe((1n << 64n) - 1n);
    expect(second?.mask).toBe(((1n << 32n) - 1n) << 32n);
    expect(second?.cores).toBe(32);
  });

  it('selects exactly the cores of each class from the topology', () => {
    const presets = affinityPresets(
      host(6, ['performance', 'performance', 'efficiency', 'efficiency', 'lowPower', 'lowPower']),
    );
    const byId = new Map(presets.map((p) => [p.id, p]));
    expect(byId.get('performance')?.mask).toBe(0b000011n);
    expect(byId.get('efficiency')?.mask).toBe(0b001100n);
    // lowPower is in neither bucket: "efficiency cores" means the E-cores a
    // user has heard of, not the SoC tile.
    expect(byId.get('all')?.mask).toBe(0b111111n);
  });

  it('ignores a topology whose length disagrees with the logical core count', () => {
    // Indexing a 4-entry topology as if it described 8 processors would
    // build a mask for the wrong cores. Better to offer no class presets.
    const ids = affinityPresets(
      host(8, ['performance', 'performance', 'efficiency', 'efficiency']),
    ).map((p) => p.id);
    expect(ids).toEqual(['all', 'firstHalf', 'secondHalf']);
  });
});
