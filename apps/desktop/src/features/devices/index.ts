export { DevicesScreen, formatReading, type DevicesScreenProps } from './DevicesScreen';
export { registerDevicesStrings, DEVICES_NS } from './strings';
export {
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
  type ThermalAvailability,
} from './model';
export { DEFAULT_CADENCE_MS, NO_HOST, useSensors, type SensorsReader } from './useSensors';
