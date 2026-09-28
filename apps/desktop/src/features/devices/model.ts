/**
 * Devices and sensors: the data shape, and what an absence means.
 *
 * # This screen is mostly about what is missing
 *
 * On a typical desktop, unelevated, this backend measures nothing at all: the
 * ACPI thermal provider refuses the query, there is no battery, and every
 * remaining quantity a user would want — core temperature, fan RPM, package
 * power, rail voltages — is behind ring 0. So the interesting content is not
 * a table of readings; it is the enumerated list of gaps, each with the
 * concrete mechanism that would close it.
 *
 * That inverts the usual empty-state logic. "No readings" is not a degraded
 * state to apologise for once and hide; it is the expected state, and the
 * screen's job is to make it legible.
 *
 * # An absence is never rounded to zero
 *
 * Every optional field here is optional because the hardware or the OS
 * genuinely declined to answer. None of them may be rendered as `0`:
 *
 * - A battery with `secondsRemaining: null` is one whose estimator has not
 *   settled. "0 minutes remaining" on a full pack is a lie the user acts on.
 * - `cycleCount: null` means the gauge does not count cycles. "0 cycles" on a
 *   five-year-old laptop is a lie the user believes.
 * - An empty `zones` list with `accessDenied` is a permissions problem, not a
 *   board without sensors — and only one of those is fixed by elevating.
 */

/** Why the thermal zone list is the length it is. */
export type ThermalAvailability =
  'available' | 'accessDenied' | 'noZonesPresent' | 'providerMissing';

export type LineStatus = 'ac' | 'battery' | 'unknown';

export type PowerModeKey = 'bestPowerEfficiency' | 'balanced' | 'bestPerformance' | 'custom';

export type SensorUnit = 'temperature' | 'power' | 'voltage' | 'fanSpeed' | 'charge' | 'percent';

export type SensorSourceKey =
  | 'acpiThermalZone'
  | 'batteryMiniport'
  | 'systemPowerStatus'
  | 'vendorLibrary'
  | 'kernelDriver'
  | 'storageDevice';

export type SensorQuality = 'measured' | 'derived' | 'nameplate';

export type ChargeStateKey = 'charging' | 'discharging' | 'idle' | 'unknown';

export type CapabilityKey = 'thermals' | 'powerDraw' | 'fanControl' | 'other';

export type UnavailableKey =
  | 'notSupportedOnPlatform'
  | 'noSuchHardware'
  | 'needsElevation'
  | 'needsHelper'
  | 'needsPlugin'
  | 'disabledByUser';

export interface SensorReading {
  readonly key: string;
  readonly label: string;
  readonly value: number;
  readonly unit: SensorUnit;
  readonly source: SensorSourceKey;
  readonly quality: SensorQuality;
}

export interface DriverGap {
  readonly capability: CapabilityKey;
  readonly reason: UnavailableKey;
  readonly label: string;
  readonly requirement: string;
  /** Whether the user can do something about it right now. */
  readonly actionable: boolean;
}

export interface PowerState {
  readonly line: LineStatus;
  readonly mode: PowerModeKey;
  readonly schemeGuid: string | null;
  readonly hasBattery: boolean;
  readonly powerSaver: boolean;
  readonly batteryPercent: number | null;
  readonly secondsRemaining: number | null;
}

export interface AggregateBattery {
  readonly onAc: boolean;
  readonly charging: boolean;
  readonly discharging: boolean;
  readonly maxCapacity: number | null;
  readonly remainingCapacity: number | null;
  readonly estimatedSeconds: number | null;
}

export interface Battery {
  readonly devicePath: string;
  readonly chemistry: string;
  readonly charge: number | null;
  readonly health: number | null;
  readonly rateWatts: number | null;
  readonly voltage: number | null;
  readonly state: ChargeStateKey;
  readonly designCapacityMwh: number | null;
  readonly fullChargeCapacityMwh: number | null;
  readonly cycleCount: number | null;
  readonly secondsToEmpty: number | null;
  /** When set, the mWh figures are gauge counts and must carry no Wh suffix. */
  readonly capacityIsRelative: boolean;
  /** A UPS: its runtime means minutes to shutdown, not hours of portable use. */
  readonly isShortTerm: boolean;
}

export interface ThermalZone {
  readonly instance: string;
  readonly celsius: number;
  readonly criticalCelsius: number | null;
  readonly activeCooling: boolean | null;
}

export interface SensorsSnapshot {
  readonly power: PowerState;
  readonly aggregateBattery: AggregateBattery | null;
  readonly batteries: readonly Battery[];
  readonly zones: readonly ThermalZone[];
  readonly thermalAvailability: ThermalAvailability;
  readonly readings: readonly SensorReading[];
  readonly gaps: readonly DriverGap[];
  /** Backend-chosen refresh period. Honoured rather than second-guessed. */
  readonly cadenceMs: number;
  readonly elapsedMs: number;
}

/**
 * Whether elevating could plausibly change the thermal answer.
 *
 * Only `accessDenied` qualifies. Offering "restart as administrator" for a
 * board that exposes no zones sends the user through a UAC prompt to see the
 * same empty list, which teaches them that the prompt means nothing.
 */
export function thermalsNeedElevation(availability: ThermalAvailability): boolean {
  return availability === 'accessDenied';
}

/**
 * Whether anything at all was measured.
 *
 * Drives the choice between a readings table and the "nothing is measurable
 * here, and here is why" state. Note that a power *state* is not a
 * measurement: knowing the machine is on mains is a fact from the OS, not a
 * sensor reading, and counting it would make the screen claim a sensor it
 * does not have.
 */
export function hasMeasurements(snapshot: SensorsSnapshot): boolean {
  return snapshot.readings.length > 0;
}

/** Gaps grouped by the capability they block, preserving backend order. */
export function groupGapsByCapability(
  gaps: readonly DriverGap[],
): readonly (readonly [CapabilityKey, readonly DriverGap[]])[] {
  const groups = new Map<CapabilityKey, DriverGap[]>();

  for (const gap of gaps) {
    const bucket = groups.get(gap.capability);
    if (bucket === undefined) groups.set(gap.capability, [gap]);
    else bucket.push(gap);
  }

  return [...groups.entries()].map(([capability, items]) => [capability, items] as const);
}

/**
 * Gaps the user could act on today, and those they could not.
 *
 * Separated because the two demand different UI: an actionable gap can carry
 * a next step ("install the sensor plugin"), whereas a permanent one must not
 * — a button that cannot help is worse than no button.
 */
export function partitionGaps(gaps: readonly DriverGap[]): {
  readonly actionable: readonly DriverGap[];
  readonly permanent: readonly DriverGap[];
} {
  return {
    actionable: gaps.filter((gap) => gap.actionable),
    permanent: gaps.filter((gap) => !gap.actionable),
  };
}

/**
 * Battery health, only when both capacities were reported.
 *
 * Returns `null` rather than 100 when either is missing. A gauge that
 * declines to report design capacity is not a pack in perfect condition, and
 * a "100% health" badge is precisely the figure someone checks before
 * deciding whether to replace it.
 */
export function batteryHealthPercent(pack: Battery): number | null {
  if (pack.health !== null) return pack.health;

  const design = pack.designCapacityMwh;
  const full = pack.fullChargeCapacityMwh;
  if (design === null || full === null || design === 0) return null;

  return (full / design) * 100;
}

/**
 * Sorts readings so the same sensor keeps its row between refreshes.
 *
 * By key, not by value: ordering a live list by magnitude makes rows swap
 * places as temperatures drift, which is unreadable and makes a stable
 * machine look unstable.
 */
export function sortReadings(readings: readonly SensorReading[]): readonly SensorReading[] {
  return [...readings].sort((a, b) => a.key.localeCompare(b.key));
}

/**
 * Whether the aggregate battery view adds anything over the per-pack list.
 *
 * On a single-battery laptop the two say the same thing, and printing both is
 * how a user comes to believe there are two packs.
 */
export function aggregateIsRedundant(snapshot: SensorsSnapshot): boolean {
  return snapshot.aggregateBattery !== null && snapshot.batteries.length === 1;
}
