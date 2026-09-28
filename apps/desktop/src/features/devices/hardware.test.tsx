import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { DeviceTreePanel } from './DeviceTreePanel';
import { HardwarePanel } from './HardwarePanel';
import {
  filterTree,
  installedMemory,
  problemCount,
  type DeviceClass,
  type DeviceInfo,
  type HardwareApi,
  type HardwareInfo,
} from './hardwareApi';
import { registerDevicesStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerDevicesStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function device(overrides: Partial<DeviceInfo> = {}): DeviceInfo {
  return {
    instanceId: 'PCI\\VEN_10DE&DEV_2489\\4&17F9',
    name: 'NVIDIA GeForce RTX 3060 Ti',
    manufacturer: 'NVIDIA',
    driverProvider: 'NVIDIA',
    driverVersion: '32.0.16.1692',
    driverDate: '2026-09-04',
    location: 'PCI bus 1, device 0, function 0',
    enumerator: 'PCI',
    status: 'ok',
    problemCode: null,
    problem: null,
    present: true,
    hidden: false,
    ...overrides,
  };
}

const classes: readonly DeviceClass[] = [
  {
    guid: '{display}',
    name: 'Display',
    description: 'Display adapters',
    devices: [device()],
  },
  {
    guid: '{bt}',
    name: 'Bluetooth',
    description: 'Bluetooth',
    devices: [
      device({ instanceId: 'BTH\\1', name: 'Bedroom display', enumerator: 'BTHENUM' }),
      device({
        instanceId: 'BTH\\2',
        name: 'Old headset',
        status: 'problem',
        problemCode: 28,
        problem: 'The drivers for this device are not installed',
      }),
    ],
  },
];

function hardware(overrides: Partial<HardwareInfo> = {}): HardwareInfo {
  return {
    cpus: [
      {
        name: 'Intel(R) Core(TM) i9-14900K',
        manufacturer: 'GenuineIntel',
        socket: 'U3E1',
        cores: 24,
        threads: 32,
        baseClockMhz: 3200,
        l2CacheKb: 32768,
        l3CacheKb: 36864,
        virtualization: true,
      },
    ],
    memory: {
      usableBytes: 205_939_000_000,
      slots: 4,
      maxCapacityBytes: null,
      modules: [
        {
          slot: 'DDR5-A1',
          capacityBytes: 48 * 1024 ** 3,
          kind: 'DDR5',
          formFactor: 'DIMM',
          speedMts: 5200,
          configuredSpeedMts: 5200,
          manufacturer: 'Corsair',
          partNumber: 'CMH96GX5M2B5200C38',
          serialTail: null,
          voltageMv: 1250,
        },
      ],
    },
    gpus: [
      {
        name: 'NVIDIA GeForce RTX 3060 Ti',
        manufacturer: 'NVIDIA',
        driverVersion: '32.0.16.1692',
        driverDate: '2026-09-04',
        videoMemoryBytes: 8 * 1024 ** 3,
        resolution: '3440 × 1440',
        refreshHz: 180,
      },
    ],
    drives: [
      {
        index: 1,
        model: 'CT2000P3PSSD8',
        media: 'ssd',
        bus: 'NVMe',
        sizeBytes: 2_000_398_934_016,
        health: 'Healthy',
        firmware: 'P9CR40A',
        serialTail: '052.',
        spindleRpm: null,
        temperatureCelsius: 45,
      },
      {
        index: 3,
        model: 'WD Elements 2620',
        media: 'hdd',
        bus: 'USB',
        sizeBytes: 2_000_365_289_472,
        health: 'Healthy',
        firmware: '1023',
        serialTail: '07SS',
        spindleRpm: null,
        temperatureCelsius: null,
      },
    ],
    board: {
      manufacturer: 'Gigabyte Technology Co., Ltd.',
      product: 'Z790 AORUS ELITE AX',
      version: null,
      biosVendor: 'American Megatrends International, LLC.',
      biosVersion: 'FM',
      biosDate: '2025-09-18',
      systemManufacturer: 'Gigabyte Technology Co., Ltd.',
      systemModel: 'Z790 AORUS ELITE AX',
    },
    elapsedMs: 702.4,
    ...overrides,
  };
}

function api(overrides: Partial<HardwareApi> = {}): HardwareApi {
  return {
    hardware: vi.fn<HardwareApi['hardware']>().mockResolvedValue(hardware()),
    devices: vi
      .fn<HardwareApi['devices']>()
      .mockResolvedValue({ classes: [...classes], elapsedMs: 40 }),
    ...overrides,
  };
}

describe('filterTree', () => {
  it('keeps a whole class when its heading matches', () => {
    const out = filterTree(classes, 'bluetooth');
    expect(out).toHaveLength(1);
    expect(out[0]?.devices).toHaveLength(2);
  });

  it('keeps only matching devices, and drops classes left empty', () => {
    const out = filterTree(classes, 'headset');
    expect(out.map((c) => c.description)).toEqual(['Bluetooth']);
    expect(out[0]?.devices.map((d) => d.name)).toEqual(['Old headset']);
  });

  it('matches the bus and the instance id, not only the name', () => {
    expect(filterTree(classes, 'bthenum')[0]?.devices.map((d) => d.name)).toEqual([
      'Bedroom display',
    ]);
    expect(filterTree(classes, 'VEN_10DE')).toHaveLength(1);
  });

  it('returns everything for a blank query', () => {
    expect(filterTree(classes, '   ')).toBe(classes);
  });
});

describe('hardware helpers', () => {
  it('counts devices with a problem code, not disabled ones', () => {
    expect(problemCount(classes)).toBe(1);
  });

  it('sums installed memory, and reports none as unknown rather than zero', () => {
    expect(installedMemory(hardware().memory)).toBe(48 * 1024 ** 3);
    expect(
      installedMemory({ usableBytes: null, slots: null, maxCapacityBytes: null, modules: [] }),
    ).toBeNull();
  });
});

describe('HardwarePanel', () => {
  it('lists processor, memory, graphics, drives and board', async () => {
    render(<HardwarePanel api={api()} />);

    await screen.findByText('Intel(R) Core(TM) i9-14900K');
    for (const title of [
      'Processor',
      'Memory (RAM)',
      'Graphics',
      'Drives',
      'Motherboard and firmware',
    ]) {
      expect(screen.getByRole('region', { name: title })).toBeTruthy();
    }
    expect(screen.getByText('CMH96GX5M2B5200C38', { exact: false })).toBeTruthy();
    const board = screen.getByRole('region', { name: 'Motherboard and firmware' });
    expect(
      within(board).getAllByText('Gigabyte Technology Co., Ltd. Z790 AORUS ELITE AX'),
    ).toHaveLength(2);
    expect(within(board).getByText(/American Megatrends.*FM \(2025-09-18\)/)).toBeTruthy();
  });

  it('shows a drive that reports no temperature as unavailable, not 0 °C', async () => {
    render(<HardwarePanel api={api()} />);
    const drives = await screen.findByRole('region', { name: 'Drives' });

    expect(within(drives).getByText('45°C', { exact: false })).toBeTruthy();
    expect(within(drives).queryByText(/0\s?°C/)).toBeNull();
    expect(within(drives).getAllByText('Not available').length).toBeGreaterThan(0);
  });

  it('never shows more than the last four characters of a serial', async () => {
    render(<HardwarePanel api={api()} />);
    const drives = await screen.findByRole('region', { name: 'Drives' });
    expect(within(drives).getByText('…07SS')).toBeTruthy();
  });

  it('reaches a terminal state when the read fails', async () => {
    render(
      <HardwarePanel
        api={api({
          hardware: vi.fn<HardwareApi['hardware']>().mockRejectedValue(new Error('WMI down')),
        })}
      />,
    );
    await screen.findByText('The hardware details could not be read');
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });
});

describe('DeviceTreePanel', () => {
  it('groups devices by class and opens a class with a problem by itself', async () => {
    render(<DeviceTreePanel api={api()} />);

    const bt = await screen.findByRole('button', { name: /Bluetooth/ });
    expect(bt.getAttribute('aria-expanded')).toBe('true');
    expect(
      screen.getByText(/Problem 28: The drivers for this device are not installed/),
    ).toBeTruthy();

    const display = screen.getByRole('button', { name: /Display adapters/ });
    expect(display.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(display);
    expect(display.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByText('NVIDIA GeForce RTX 3060 Ti')).toBeTruthy();
  });

  it('asks again with disconnected devices when the box is ticked', async () => {
    const hw = api();
    render(<DeviceTreePanel api={hw} />);
    await screen.findByRole('button', { name: /Bluetooth/ });

    fireEvent.click(screen.getByRole('checkbox', { name: 'Show devices that are not connected' }));

    await vi.waitFor(() => expect(hw.devices).toHaveBeenLastCalledWith(true));
  });

  it('filters across every class and says so when nothing matches', async () => {
    render(<DeviceTreePanel api={api()} />);
    await screen.findByRole('button', { name: /Bluetooth/ });

    const search = screen.getByRole('searchbox', { name: 'Search devices' });
    fireEvent.change(search, { target: { value: 'zzzz' } });
    expect(screen.getByText('No device matches that search.')).toBeTruthy();
  });

  it('renders in Romanian without leaking key paths', async () => {
    await i18n.changeLanguage('ro');
    const { container } = render(<DeviceTreePanel api={api()} />);
    await screen.findByRole('button', { name: /Bluetooth/ });
    expect(screen.getByText(/3 dispozitive în 2 grupuri/)).toBeTruthy();
    expect(container.textContent ?? '').not.toMatch(/\btree\.[a-z]/i);
  });
});
