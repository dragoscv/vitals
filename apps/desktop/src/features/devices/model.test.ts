import { describe, expect, it } from 'vitest';

import {
  aggregateIsRedundant,
  batteryHealthPercent,
  groupGapsByCapability,
  hasMeasurements,
  partitionGaps,
  sortReadings,
  thermalsNeedElevation,
  type Battery,
  type DriverGap,
  type SensorReading,
  type SensorsSnapshot,
} from './model';

function battery(overrides: Partial<Battery> = {}): Battery {
  return {
    devicePath: '\\\\?\\battery#0',
    chemistry: 'LION',
    charge: 82,
    health: null,
    rateWatts: null,
    voltage: null,
    state: 'idle',
    designCapacityMwh: null,
    fullChargeCapacityMwh: null,
    cycleCount: null,
    secondsToEmpty: null,
    capacityIsRelative: false,
    isShortTerm: false,
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

function gap(overrides: Partial<DriverGap> = {}): DriverGap {
  return {
    capability: 'thermals',
    reason: 'needsPlugin',
    label: 'CPU core temperature',
    requirement: 'RDMSR of IA32_THERM_STATUS (0x19C).',
    actionable: true,
    ...overrides,
  };
}

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

describe('thermalsNeedElevation', () => {
  it('offers elevation only when the query was actually refused', () => {
    expect(thermalsNeedElevation('accessDenied')).toBe(true);
  });

  it('does not send the user through UAC for a board with no zones', () => {
    // Elevating changes nothing here, and a prompt that achieves nothing
    // teaches the user that prompts achieve nothing.
    expect(thermalsNeedElevation('noZonesPresent')).toBe(false);
    expect(thermalsNeedElevation('providerMissing')).toBe(false);
    expect(thermalsNeedElevation('available')).toBe(false);
  });
});

describe('hasMeasurements', () => {
  it('is false on a machine that measures nothing', () => {
    // This machine's real state: no zones, no battery, mains power.
    expect(hasMeasurements(snapshot())).toBe(false);
  });

  it('does not count the power state as a measurement', () => {
    // Knowing the machine is on mains is a fact from the OS, not a sensor.
    // Counting it would make the screen claim a sensor it does not have.
    expect(hasMeasurements(snapshot({ power: { ...snapshot().power, line: 'battery' } }))).toBe(
      false,
    );
  });

  it('is true once something was read', () => {
    expect(hasMeasurements(snapshot({ readings: [reading()] }))).toBe(true);
  });
});

describe('batteryHealthPercent', () => {
  it('prefers the gauge figure when the firmware reports one', () => {
    expect(batteryHealthPercent(battery({ health: 88 }))).toBe(88);
  });

  it('derives health from the two capacities', () => {
    const derived = batteryHealthPercent(
      battery({ designCapacityMwh: 50_000, fullChargeCapacityMwh: 40_000 }),
    );
    expect(derived).toBeCloseTo(80);
  });

  it('returns nothing rather than a healthy-looking 100 when capacities are missing', () => {
    // A gauge that declines to report design capacity is not a pack in
    // perfect condition, and "100% health" is precisely the figure someone
    // checks before deciding whether to replace it.
    expect(batteryHealthPercent(battery())).toBeNull();
    expect(batteryHealthPercent(battery({ designCapacityMwh: 50_000 }))).toBeNull();
    expect(batteryHealthPercent(battery({ fullChargeCapacityMwh: 40_000 }))).toBeNull();
  });

  it('does not divide by a zero design capacity', () => {
    expect(
      batteryHealthPercent(battery({ designCapacityMwh: 0, fullChargeCapacityMwh: 40_000 })),
    ).toBeNull();
  });
});

describe('partitionGaps', () => {
  it('separates what the user can act on from what they cannot', () => {
    // The two demand different UI: an actionable gap can carry a next step,
    // a permanent one must not — a button that cannot help is worse than
    // no button.
    const { actionable, permanent } = partitionGaps([
      gap({ label: 'Fan speed', actionable: true }),
      gap({ label: 'Nothing doing', actionable: false }),
    ]);

    expect(actionable.map((entry) => entry.label)).toEqual(['Fan speed']);
    expect(permanent.map((entry) => entry.label)).toEqual(['Nothing doing']);
  });

  it('handles an empty list without inventing groups', () => {
    const { actionable, permanent } = partitionGaps([]);
    expect(actionable).toEqual([]);
    expect(permanent).toEqual([]);
  });
});

describe('groupGapsByCapability', () => {
  it('groups without reordering within a capability', () => {
    const grouped = groupGapsByCapability([
      gap({ capability: 'thermals', label: 'A' }),
      gap({ capability: 'fanControl', label: 'B' }),
      gap({ capability: 'thermals', label: 'C' }),
    ]);

    expect(grouped.map(([capability]) => capability)).toEqual(['thermals', 'fanControl']);
    expect(grouped[0]?.[1].map((entry) => entry.label)).toEqual(['A', 'C']);
  });
});

describe('sortReadings', () => {
  it('orders by key so a row does not move as its value drifts', () => {
    // Sorting a live list by magnitude makes rows swap places as temperatures
    // change, which is unreadable and makes a stable machine look unstable.
    const sorted = sortReadings([
      reading({ key: 'battery.0.charge', value: 5 }),
      reading({ key: 'acpi.tz.0', value: 90 }),
    ]);

    expect(sorted.map((entry) => entry.key)).toEqual(['acpi.tz.0', 'battery.0.charge']);
  });

  it('does not mutate the input', () => {
    const input = [reading({ key: 'z' }), reading({ key: 'a' })];
    sortReadings(input);
    expect(input.map((entry) => entry.key)).toEqual(['z', 'a']);
  });
});

describe('aggregateIsRedundant', () => {
  it('is redundant against a single pack', () => {
    const state = snapshot({
      batteries: [battery()],
      aggregateBattery: {
        onAc: true,
        charging: false,
        discharging: false,
        maxCapacity: null,
        remainingCapacity: null,
        estimatedSeconds: null,
      },
    });

    expect(aggregateIsRedundant(state)).toBe(true);
  });

  it('is not redundant across two packs', () => {
    const state = snapshot({
      batteries: [battery(), battery({ devicePath: '\\\\?\\battery#1' })],
      aggregateBattery: {
        onAc: false,
        charging: false,
        discharging: true,
        maxCapacity: 90_000,
        remainingCapacity: 45_000,
        estimatedSeconds: 3600,
      },
    });

    expect(aggregateIsRedundant(state)).toBe(false);
  });
});
