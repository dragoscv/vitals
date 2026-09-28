import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { DevicesScreen } from './DevicesScreen';
import type { Battery, DriverGap, SensorReading, SensorsSnapshot } from './model';
import { registerDevicesStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerDevicesStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function gap(overrides: Partial<DriverGap> = {}): DriverGap {
  return {
    capability: 'fanControl',
    reason: 'needsPlugin',
    label: 'Fan speed (RPM)',
    requirement: 'Super-I/O tachometer registers, ring-0 port I/O.',
    actionable: true,
    ...overrides,
  };
}

function reading(overrides: Partial<SensorReading> = {}): SensorReading {
  return {
    key: 'acpi.tz.0',
    label: 'Thermal zone 0',
    value: 41.5,
    unit: 'temperature',
    source: 'acpiThermalZone',
    quality: 'measured',
    ...overrides,
  };
}

function battery(overrides: Partial<Battery> = {}): Battery {
  return {
    devicePath: '\\\\?\\battery#0',
    chemistry: 'LION',
    charge: 82,
    health: null,
    rateWatts: null,
    voltage: null,
    state: 'discharging',
    designCapacityMwh: null,
    fullChargeCapacityMwh: null,
    cycleCount: null,
    secondsToEmpty: null,
    capacityIsRelative: false,
    isShortTerm: false,
    ...overrides,
  };
}

/** This machine's actual state: no zones, no battery, AC, BestPerformance. */
function snapshot(overrides: Partial<SensorsSnapshot> = {}): SensorsSnapshot {
  return {
    power: {
      line: 'ac',
      mode: 'bestPerformance',
      schemeGuid: '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c',
      hasBattery: false,
      powerSaver: false,
      batteryPercent: null,
      secondsRemaining: null,
    },
    aggregateBattery: null,
    batteries: [],
    zones: [],
    thermalAvailability: 'accessDenied',
    readings: [],
    gaps: [gap()],
    cadenceMs: 5000,
    elapsedMs: 11.2,
    ...overrides,
  };
}

async function mount(data = snapshot()) {
  const reader = vi.fn<() => Promise<SensorsSnapshot>>().mockResolvedValue(data);
  render(<DevicesScreen reader={reader} />);
  await screen.findByRole('heading', { name: 'Devices & sensors' });
  return { reader };
}

describe('DevicesScreen', () => {
  it('shows skeletons until the read settles', () => {
    render(<DevicesScreen reader={() => new Promise(() => undefined)} />);
    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('reaches a terminal state when the read fails', async () => {
    // The rule this project keeps relearning: a loading state must resolve
    // either way, or a broken app merely looks busy.
    const reader = vi
      .fn<() => Promise<SensorsSnapshot>>()
      .mockRejectedValue(new Error('WMI connect failed'));

    render(<DevicesScreen reader={reader} />);

    await screen.findByRole('alert');
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('keeps the last good data when a later read fails', async () => {
    const reader = vi
      .fn<() => Promise<SensorsSnapshot>>()
      .mockResolvedValueOnce(snapshot({ readings: [reading({ label: 'Thermal zone 0' })] }))
      .mockRejectedValue(new Error('transient'));

    render(<DevicesScreen reader={reader} />);
    await screen.findByText('Thermal zone 0');

    screen.getByRole('button', { name: /Refresh/ }).click();
    await screen.findByRole('alert');

    // Blanking a panel on a transient WMI hiccup would make a working machine
    // look broken and cost a cold-start read to find out otherwise.
    expect(screen.getByText('Thermal zone 0')).toBeTruthy();
  });

  describe('the no-hardware machine, which is the common case', () => {
    it('explains an empty zone list as a permissions result, not absent sensors', async () => {
      await mount();

      expect(screen.getByText(/Windows refused the query/)).toBeTruthy();
      expect(screen.getByText(/needs administrator rights on most builds/)).toBeTruthy();
    });

    it('does not offer elevation for a board that genuinely has no zones', async () => {
      // A UAC prompt that changes nothing teaches the user that prompts
      // change nothing.
      await mount(snapshot({ thermalAvailability: 'noZonesPresent' }));

      expect(screen.getByText(/firmware exposes no thermal zones/)).toBeTruthy();
      expect(screen.queryByText('Needs administrator rights')).toBeNull();
    });

    it('says nothing is measurable and points at the gap list', async () => {
      await mount();

      expect(screen.getByText('Nothing measurable')).toBeTruthy();
      expect(screen.getByText(/what each one would need, is listed below/)).toBeTruthy();
    });

    it('states plainly that there is no battery', async () => {
      await mount();
      expect(screen.getByText(/No battery/)).toBeTruthy();
    });

    it('renders no battery section at all when there is no pack', async () => {
      await mount();
      expect(screen.queryByRole('region', { name: 'Battery' })).toBeNull();
    });

    it('still reports the power state, which is a fact rather than a sensor', async () => {
      await mount();

      expect(screen.getByText('Mains (AC)')).toBeTruthy();
      expect(screen.getByText('Best performance')).toBeTruthy();
      expect(screen.getByText('8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c')).toBeTruthy();
    });
  });

  describe('the gap list', () => {
    it('renders each gap with its concrete requirement', async () => {
      // This is the screen's actual value: it explains the absences instead
      // of showing an empty page.
      await mount();

      expect(screen.getByText('Fan speed (RPM)')).toBeTruthy();
      expect(screen.getByText(/Super-I\/O tachometer registers/)).toBeTruthy();
      expect(
        screen.getByText('Needs a kernel driver or vendor SDK Vitals does not ship'),
      ).toBeTruthy();
    });

    it('separates what could be unlocked from what could not', async () => {
      await mount(
        snapshot({
          gaps: [
            gap({ label: 'Fan speed (RPM)', actionable: true }),
            gap({
              label: 'Something permanent',
              reason: 'notSupportedOnPlatform',
              actionable: false,
            }),
          ],
        }),
      );

      expect(screen.getByRole('heading', { name: 'Could be unlocked' })).toBeTruthy();
      expect(screen.getByRole('heading', { name: 'Not possible in this build' })).toBeTruthy();
    });
  });

  describe('readings', () => {
    it('formats each reading in its own unit', async () => {
      await mount(
        snapshot({
          readings: [
            reading({ key: 'acpi.tz.0', value: 41.5, unit: 'temperature' }),
            reading({
              key: 'battery.0.rate',
              label: 'Battery 0 discharge rate',
              value: 12.5,
              unit: 'power',
              source: 'batteryMiniport',
            }),
          ],
        }),
      );

      expect(screen.getByText('42°C')).toBeTruthy();
      expect(screen.getByText('12.5 W')).toBeTruthy();
    });

    it('marks a derived figure as derived', async () => {
      // Battery health is a ratio of two firmware constants, not a live
      // measurement, and users make hardware decisions on the difference.
      await mount(
        snapshot({
          readings: [
            reading({ key: 'battery.0.health', unit: 'charge', value: 88, quality: 'derived' }),
          ],
        }),
      );

      expect(screen.getByText('Derived')).toBeTruthy();
    });

    it('shows GPU readings from the graphics driver with their source named', async () => {
      // Found live 2026-09-28: the RTX 3060 Ti's temperature, fan and power
      // were readable unelevated through the driver's nvml.dll, and the
      // screen listed them as "cannot measure".
      await mount(
        snapshot({
          readings: [
            reading({
              key: 'nvidia.0.temperature',
              label: 'NVIDIA GeForce RTX 3060 Ti temperature',
              value: 47,
              source: 'vendorLibrary',
            }),
            reading({
              key: 'nvidia.0.fan',
              label: 'NVIDIA GeForce RTX 3060 Ti fan',
              value: 83,
              unit: 'percent',
              source: 'vendorLibrary',
            }),
          ],
        }),
      );

      expect(screen.getByText('NVIDIA GeForce RTX 3060 Ti temperature')).toBeTruthy();
      expect(screen.getByText('47°C')).toBeTruthy();
      expect(screen.getByText('83%')).toBeTruthy();
      expect(screen.getAllByText('GPU driver').length).toBe(2);
    });
  });

  describe('layout', () => {
    it('keeps power and thermal zones as separate cards, outside any scrolling region', async () => {
      // S12-29: the two fact cards shared one scrolling column, and the
      // thermal zones were cut off below the power card.
      await mount(
        snapshot({
          zones: [{ instance: 'TZ00', celsius: 28, criticalCelsius: null, activeCooling: null }],
          thermalAvailability: 'available',
        }),
      );

      for (const name of ['Power', 'Thermal zones']) {
        const card = screen.getByRole('region', { name });
        expect(card.closest('.pane-stack, .pane-scroll')).toBeNull();
        expect(card.parentElement?.classList.contains('devices-facts')).toBe(true);
      }
    });
  });

  describe('battery', () => {
    it('never renders an unreported cycle count as zero', async () => {
      // "0 cycles" on a five-year-old laptop is a lie the user believes.
      await mount(snapshot({ batteries: [battery({ cycleCount: null })] }));

      expect(screen.queryByText('0')).toBeNull();
      expect(screen.getAllByText('Not available').length).toBeGreaterThan(0);
    });

    it('never renders an unsettled runtime estimate as zero minutes', async () => {
      await mount(
        snapshot({
          power: { ...snapshot().power, hasBattery: true, secondsRemaining: null },
        }),
      );

      expect(screen.queryByText('0s')).toBeNull();
    });

    it('marks a UPS so its runtime is not read as portable use', async () => {
      await mount(snapshot({ batteries: [battery({ isShortTerm: true })] }));
      expect(screen.getByText('Uninterruptible power supply')).toBeTruthy();
    });

    it('drops the Wh suffix when the gauge reports relative capacity', async () => {
      await mount(
        snapshot({
          batteries: [
            battery({
              capacityIsRelative: true,
              designCapacityMwh: 100,
              fullChargeCapacityMwh: 92,
            }),
          ],
        }),
      );

      expect(screen.queryByText('100 mWh')).toBeNull();
      expect(screen.getByText(/counts rather than energy/)).toBeTruthy();
    });
  });

  it('honours the backend cadence rather than choosing its own', async () => {
    const { reader } = await mount();

    // The cadence is stated to the user so they know the screen is not stuck.
    expect(screen.getByText(/Refreshes every 5s/)).toBeTruthy();
    expect(reader).toHaveBeenCalledTimes(1);
  });

  it('renders in Romanian without leaking key paths', async () => {
    await i18n.changeLanguage('ro');

    const reader = vi.fn<() => Promise<SensorsSnapshot>>().mockResolvedValue(snapshot());
    const { container } = render(<DevicesScreen reader={reader} />);
    await screen.findByRole('heading', { name: 'Dispozitive și senzori' });

    expect(screen.getByText('Rețea (AC)')).toBeTruthy();
    expect(screen.getByText('Performanță maximă')).toBeTruthy();
    expect(screen.getByText('Nimic măsurabil')).toBeTruthy();

    // A missing key renders as its own path. Catching that here is the only
    // thing standing between a translation gap and a user seeing
    // "availability.accessDenied" on screen.
    const text = container.textContent ?? '';
    expect(text).not.toMatch(/\b(availability|reason|capability|quality|source|mode|line)\.[a-z]/i);
  });
});
