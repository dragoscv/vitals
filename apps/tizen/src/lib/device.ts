/**
 * What this TV tells a web app about itself, through Samsung's official
 * `tizen.systeminfo` API — and nothing else. No private APIs, no shell.
 *
 * Every property is read independently: one that the firmware refuses or
 * does not have must turn into an em dash on its own row, not blank the
 * whole screen. `null` means "not measured", never zero.
 */

export interface StorageUnit {
  readonly type: string;
  readonly capacity: number | null;
  readonly available: number | null;
  readonly removable: boolean;
}

export interface DeviceSnapshot {
  /** Percent, 0–100. */
  readonly cpuLoad: number | null;
  readonly memoryTotal: number | null;
  readonly memoryAvailable: number | null;
  readonly storage: readonly StorageUnit[] | null;
  /** `WIFI`, `ETHERNET`, `NONE` and so on, as Tizen spells them. */
  readonly networkType: string | null;
  readonly ipAddress: string | null;
  /** Percent, 0–100. */
  readonly wifiSignal: number | null;
  readonly ssid: string | null;
  readonly resolution: { readonly width: number; readonly height: number } | null;
  readonly model: string | null;
  readonly manufacturer: string | null;
  readonly firmware: string | null;
  readonly platformVersion: string | null;
}

export const EMPTY_DEVICE: DeviceSnapshot = {
  cpuLoad: null,
  memoryTotal: null,
  memoryAvailable: null,
  storage: null,
  networkType: null,
  ipAddress: null,
  wifiSignal: null,
  ssid: null,
  resolution: null,
  model: null,
  manufacturer: null,
  firmware: null,
  platformVersion: null,
};

type Info = NonNullable<NonNullable<typeof tizen>['systeminfo']>;

export function systemInfo(): Info | null {
  return typeof tizen !== 'undefined' && tizen?.systeminfo !== undefined ? tizen.systeminfo : null;
}

function property(info: Info, name: string): Promise<Record<string, unknown> | null> {
  return new Promise((resolve) => {
    try {
      info.getPropertyValue(
        name,
        (value) =>
          resolve(
            typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null,
          ),
        () => resolve(null),
      );
    } catch {
      resolve(null);
    }
  });
}

function num(v: unknown): number | null {
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

function str(v: unknown): string | null {
  return typeof v === 'string' && v.trim() !== '' ? v : null;
}

function safe<T>(read: () => T): T | null {
  try {
    return read();
  } catch {
    return null;
  }
}

/** The fast-changing readings, polled while a screen shows them. */
export async function readLoad(
  info: Info,
): Promise<Pick<DeviceSnapshot, 'cpuLoad' | 'memoryTotal' | 'memoryAvailable'>> {
  const cpu = await property(info, 'CPU');
  const load = num(cpu?.load);
  return {
    // Tizen reports load as a 0–1 fraction; everything else in Vitals is percent.
    cpuLoad: load === null ? null : load * 100,
    memoryTotal: safe(() => num(info.getTotalMemory())),
    memoryAvailable: safe(() => num(info.getAvailableMemory())),
  };
}

function capabilityString(info: Info, key: string): string | null {
  return safe(() => str(info.getCapability(key)));
}

export async function readDevice(info: Info): Promise<DeviceSnapshot> {
  const [load, storage, network, wifi, ethernet, display, build] = await Promise.all([
    readLoad(info),
    property(info, 'STORAGE'),
    property(info, 'NETWORK'),
    property(info, 'WIFI_NETWORK'),
    property(info, 'ETHERNET_NETWORK'),
    property(info, 'DISPLAY'),
    property(info, 'BUILD'),
  ]);
  const units = Array.isArray(storage?.units)
    ? (storage.units as unknown[]).map((u): StorageUnit => {
        const unit = (typeof u === 'object' && u !== null ? u : {}) as Record<string, unknown>;
        return {
          type: str(unit.type) ?? 'UNKNOWN',
          capacity: num(unit.capacity),
          available: num(unit.availableCapacity),
          removable: unit.isRemovable === true,
        };
      })
    : null;
  const networkType = str(network?.networkType);
  const wifiOn = wifi?.status === 'ON';
  const ethernetIp = str(ethernet?.ipAddress);
  const signal = num(wifi?.signalStrength);
  const width = num(display?.resolutionWidth);
  const height = num(display?.resolutionHeight);
  return {
    ...load,
    storage: units,
    networkType,
    ipAddress: networkType === 'ETHERNET' ? ethernetIp : wifiOn ? str(wifi?.ipAddress) : ethernetIp,
    wifiSignal: wifiOn && signal !== null ? signal * 100 : null,
    ssid: wifiOn ? str(wifi?.ssid) : null,
    resolution: width !== null && height !== null ? { width, height } : null,
    model: str(build?.model),
    manufacturer: str(build?.manufacturer),
    firmware: str(build?.buildVersion),
    platformVersion: capabilityString(info, 'http://tizen.org/feature/platform.version'),
  };
}
