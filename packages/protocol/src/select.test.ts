import { describe, expect, it } from 'vitest';

import type { GpuMetrics, SystemMetrics } from './generated';
import { busiestNic, gpuPercent, headlineTemperature, memoryPercent, primaryGpu } from './select';

function gpu(name: string, utilization: number | null): GpuMetrics {
  return {
    id: name.length,
    name,
    vendor: 'unknown',
    engines: [],
    utilization,
    memoryUsed: null,
    memoryTotal: null,
    sharedMemoryUsed: null,
    coreClock: null,
    memoryClock: null,
    temperature: null,
    hotspotTemperature: null,
    power: null,
    powerLimit: null,
    fanPercent: null,
    fanRpm: null,
    throttled: null,
    driverVersion: null,
  };
}

function system(overrides: Partial<SystemMetrics>): SystemMetrics {
  return {
    cpu: { total: 10, temperature: null },
    memory: { used: 4, total: 16 },
    disks: [],
    networks: [],
    gpus: [],
    powerDraw: null,
    battery: null,
    ...overrides,
  } as SystemMetrics;
}

describe('primaryGpu', () => {
  it('skips a phantom adapter that reports nothing in favour of the real card', () => {
    // Measured on the dev machine: the kernel enumerates a Parsec virtual
    // display first, then the RTX, then a render-only device. Taking
    // `gpus[0]` showed a dash while the card was at 17 %.
    const s = system({
      gpus: [
        gpu('Display adapter 0x0003', null),
        gpu('RTX 3060 Ti', 17),
        gpu('Display 0x0002', null),
      ],
    });
    expect(primaryGpu(s)?.name).toBe('RTX 3060 Ti');
    expect(gpuPercent(s)).toBe(17);
  });

  it('prefers the busiest card, not the first, when several report', () => {
    const s = system({ gpus: [gpu('iGPU', 3), gpu('dGPU', 64)] });
    expect(primaryGpu(s)?.name).toBe('dGPU');
    expect(gpuPercent(s)).toBe(64);
  });

  it('still names the hardware when the only adapter is unmeasured', () => {
    // The card exists and its name is worth showing; the number is not.
    const s = system({ gpus: [gpu('RTX 3060 Ti', null)] });
    expect(primaryGpu(s)?.name).toBe('RTX 3060 Ti');
    expect(gpuPercent(s)).toBeNull();
  });

  it('is null, never zero, on a machine with no GPU at all', () => {
    const s = system({ gpus: [] });
    expect(primaryGpu(s)).toBeNull();
    expect(gpuPercent(s)).toBeNull();
  });
});

describe('memoryPercent', () => {
  it('is null rather than NaN when the total is unknown', () => {
    expect(
      memoryPercent(system({ memory: { used: 1, total: 0 } as SystemMetrics['memory'] })),
    ).toBeNull();
  });

  it('is a percentage of the total', () => {
    expect(memoryPercent(system({}))).toBe(25);
  });
});

describe('busiestNic', () => {
  it('ranks on total throughput, not on order', () => {
    const s = system({
      networks: [
        { id: 1, name: 'Ethernet', rx: 10, tx: 10 },
        { id: 2, name: 'Wi-Fi', rx: 900, tx: 5 },
      ] as unknown as SystemMetrics['networks'],
    });
    expect(busiestNic(s)?.name).toBe('Wi-Fi');
  });

  it('is undefined when there are no adapters', () => {
    expect(busiestNic(system({}))).toBeUndefined();
  });
});

describe('headlineTemperature', () => {
  it('prefers the CPU package', () => {
    const s = system({ cpu: { total: 10, temperature: 61 } as SystemMetrics['cpu'] });
    expect(headlineTemperature(s)).toBe(61);
  });

  it('falls back to the primary GPU, which is not necessarily the first', () => {
    const cool = gpu('phantom', null);
    const hot = { ...gpu('RTX', 17), temperature: 72 } as GpuMetrics;
    expect(headlineTemperature(system({ gpus: [cool, hot] }))).toBe(72);
  });

  it('is null when nothing reports a temperature', () => {
    expect(headlineTemperature(system({ gpus: [gpu('x', 5)] }))).toBeNull();
  });
});
