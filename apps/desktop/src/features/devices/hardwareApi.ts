/**
 * Hardware inventory and the Device Manager list — the two on-demand reads
 * behind the Hardware and Devices tabs.
 *
 * The one place the command names are spelled; taken as an interface by the
 * panels so they are testable without a Tauri host. Shapes mirror
 * `apps/desktop/src-tauri/src/hardware.rs`. Every optional field is `null`
 * when the firmware or driver did not say, never `0` or `""`.
 */

export interface CpuInfo {
  readonly name: string;
  readonly manufacturer: string | null;
  readonly socket: string | null;
  readonly cores: number | null;
  readonly threads: number | null;
  readonly baseClockMhz: number | null;
  readonly l2CacheKb: number | null;
  readonly l3CacheKb: number | null;
  readonly virtualization: boolean | null;
}

export interface MemoryModule {
  readonly slot: string | null;
  readonly capacityBytes: number | null;
  readonly kind: string | null;
  readonly formFactor: string | null;
  readonly speedMts: number | null;
  readonly configuredSpeedMts: number | null;
  readonly manufacturer: string | null;
  readonly partNumber: string | null;
  /** The last four characters only; the backend withholds the rest. */
  readonly serialTail: string | null;
  readonly voltageMv: number | null;
}

export interface MemoryInfo {
  readonly usableBytes: number | null;
  readonly slots: number | null;
  readonly maxCapacityBytes: number | null;
  readonly modules: readonly MemoryModule[];
}

export interface GpuInfo {
  readonly name: string;
  readonly manufacturer: string | null;
  readonly driverVersion: string | null;
  readonly driverDate: string | null;
  readonly videoMemoryBytes: number | null;
  readonly resolution: string | null;
  readonly refreshHz: number | null;
}

export type DriveMedia = 'ssd' | 'hdd' | 'unknown';

export interface DriveInfo {
  readonly index: number;
  readonly model: string;
  readonly media: DriveMedia;
  readonly bus: string | null;
  readonly sizeBytes: number | null;
  readonly health: string | null;
  readonly firmware: string | null;
  readonly serialTail: string | null;
  readonly spindleRpm: number | null;
  readonly temperatureCelsius: number | null;
}

export interface BoardInfo {
  readonly manufacturer: string | null;
  readonly product: string | null;
  readonly version: string | null;
  readonly biosVendor: string | null;
  readonly biosVersion: string | null;
  readonly biosDate: string | null;
  readonly systemManufacturer: string | null;
  readonly systemModel: string | null;
}

export interface HardwareInfo {
  readonly cpus: readonly CpuInfo[];
  readonly memory: MemoryInfo;
  readonly gpus: readonly GpuInfo[];
  readonly drives: readonly DriveInfo[];
  readonly board: BoardInfo;
  readonly elapsedMs: number;
}

export type DeviceStatus = 'ok' | 'disabled' | 'problem' | 'notPresent';

export interface DeviceInfo {
  readonly instanceId: string;
  readonly name: string;
  readonly manufacturer: string | null;
  readonly driverProvider: string | null;
  readonly driverVersion: string | null;
  readonly driverDate: string | null;
  readonly location: string | null;
  readonly enumerator: string | null;
  readonly status: DeviceStatus;
  readonly problemCode: number | null;
  /** Windows' own wording for `problemCode`, in English. */
  readonly problem: string | null;
  readonly present: boolean;
  readonly hidden: boolean;
}

export interface DeviceClass {
  readonly guid: string;
  readonly name: string;
  /** The heading Device Manager prints, localised by Windows. */
  readonly description: string;
  readonly devices: readonly DeviceInfo[];
}

export interface DeviceTree {
  readonly classes: readonly DeviceClass[];
  readonly elapsedMs: number;
}

export interface HardwareApi {
  hardware(): Promise<HardwareInfo>;
  devices(includeHidden: boolean): Promise<DeviceTree>;
}

export const tauriHardwareApi: HardwareApi = {
  async hardware() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<HardwareInfo>('get_hardware');
  },
  async devices(includeHidden) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DeviceTree>('get_device_tree', { includeHidden });
  },
};

/**
 * Filters the tree by a search string over name, manufacturer, driver
 * provider and instance id, keeping only classes that still have devices.
 * A class heading that matches keeps all its devices — searching "Bluetooth"
 * should show the Bluetooth group, not nothing.
 */
export function filterTree(classes: readonly DeviceClass[], query: string): readonly DeviceClass[] {
  const q = query.trim().toLowerCase();
  if (q === '') return classes;
  const hit = (value: string | null) => value !== null && value.toLowerCase().includes(q);
  return classes.flatMap((cls) => {
    if (hit(cls.description) || hit(cls.name)) return [cls];
    const devices = cls.devices.filter(
      (d) =>
        hit(d.name) ||
        hit(d.manufacturer) ||
        hit(d.driverProvider) ||
        hit(d.instanceId) ||
        hit(d.enumerator),
    );
    return devices.length > 0 ? [{ ...cls, devices }] : [];
  });
}

/** Devices needing attention (a problem code), for the header count. */
export function problemCount(classes: readonly DeviceClass[]): number {
  return classes.reduce(
    (n, cls) => n + cls.devices.filter((d) => d.status === 'problem').length,
    0,
  );
}

/**
 * Sums installed module capacity; `null` when no module reported one — a
 * total of 0 GB would read as "no memory".
 */
export function installedMemory(memory: MemoryInfo): number | null {
  const known = memory.modules.flatMap((m) => (m.capacityBytes === null ? [] : [m.capacityBytes]));
  return known.length === 0 ? null : known.reduce((a, b) => a + b, 0);
}
