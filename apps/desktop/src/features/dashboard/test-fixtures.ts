/**
 * Fixtures for the dashboard tests.
 *
 * Every builder takes a deep-partial override and fills the rest with a
 * plausible idle machine. Writing a full `SystemMetrics` literal per test is
 * forty lines of noise around the one field the test is about, and — worse —
 * when the protocol gains a field every one of those literals fails to compile
 * at once, which is how fixtures end up cast to `any`.
 *
 * The defaults describe a healthy machine deliberately, so a test that expects
 * an alert has to say which condition it is creating.
 */

import type {
  BatteryMetrics,
  CpuMetrics,
  DiskMetrics,
  GpuMetrics,
  MemoryMetrics,
  NetworkMetrics,
  Process,
  SystemMetrics,
} from '@vitals/protocol';

type Deep<T> = {
  [K in keyof T]?: T[K] extends object | null ? Deep<NonNullable<T[K]>> | null : T[K];
};

const GIB = 1024 ** 3;

export function makeCpu(overrides: Deep<CpuMetrics> = {}): CpuMetrics {
  return {
    total: 8,
    perCore: [6, 9, 7, 10],
    kernel: 2,
    effectiveClock: 3_200_000_000,
    maxClock: 4_800_000_000,
    temperature: 45,
    power: 22,
    throttled: null,
    processCount: 240,
    threadCount: 3100,
    handleCount: 98_000,
    uptimeSecs: 7200,
    contextSwitches: 12_000,
    interrupts: 9000,
    ...overrides,
  } as CpuMetrics;
}

export function makeMemory(overrides: Deep<MemoryMetrics> = {}): MemoryMetrics {
  return {
    total: 32 * GIB,
    used: 12 * GIB,
    available: 20 * GIB,
    cached: 6 * GIB,
    pagedPool: GIB / 2,
    nonPagedPool: GIB / 4,
    committed: 14 * GIB,
    commitLimit: 40 * GIB,
    swapTotal: 8 * GIB,
    swapUsed: GIB,
    hardwareReserved: GIB / 8,
    pageFaultsPerSec: 120,
    speed: 3_200_000_000,
    slotsUsed: 2,
    slotsTotal: 4,
    formFactor: 'DIMM',
    ...overrides,
  };
}

export function makeDisk(overrides: Deep<DiskMetrics> = {}): DiskMetrics {
  return {
    id: 0,
    name: 'Samsung SSD 980 PRO',
    model: '980 PRO',
    mount: 'C:',
    kind: 'ssd',
    total: 1000 * GIB,
    free: 400 * GIB,
    read: 1_200_000,
    write: 800_000,
    activeTime: 4,
    responseMs: 0.4,
    queueDepth: 1,
    temperature: 38,
    health: null,
    ...overrides,
  } as DiskMetrics;
}

export function makeNetwork(overrides: Deep<NetworkMetrics> = {}): NetworkMetrics {
  return {
    id: 0,
    name: 'Ethernet',
    adapter: 'Intel I225-V',
    kind: 'ethernet',
    rx: 120_000,
    tx: 40_000,
    rxTotal: 4 * GIB,
    txTotal: GIB,
    linkSpeed: 1_000_000_000,
    ipv4: '192.168.1.20',
    ipv6: null,
    mac: '00:11:22:33:44:55',
    connected: true,
    signal: null,
    ssid: null,
    errorsPerSec: 0,
    ...overrides,
  };
}

export function makeGpu(overrides: Deep<GpuMetrics> = {}): GpuMetrics {
  return {
    id: 0,
    name: 'NVIDIA GeForce RTX 3060 Ti',
    vendor: 'nvidia',
    engines: [],
    utilization: 12,
    memoryUsed: 2 * GIB,
    memoryTotal: 8 * GIB,
    sharedMemoryUsed: null,
    coreClock: 1_700_000_000,
    memoryClock: 7_000_000_000,
    temperature: 52,
    hotspotTemperature: null,
    power: 90,
    powerLimit: 200,
    fanPercent: 40,
    fanRpm: 1200,
    throttled: null,
    driverVersion: '551.23',
    ...overrides,
  } as GpuMetrics;
}

export function makeBattery(overrides: Deep<BatteryMetrics> = {}): BatteryMetrics {
  return {
    charge: 78,
    charging: false,
    timeRemainingSecs: 9000,
    power: 12,
    health: 92,
    cycleCount: 140,
    temperature: 31,
    ...overrides,
  };
}

export interface SystemOverrides {
  readonly cpu?: Deep<CpuMetrics>;
  readonly memory?: Deep<MemoryMetrics>;
  readonly disks?: readonly Deep<DiskMetrics>[];
  readonly networks?: readonly Deep<NetworkMetrics>[];
  readonly gpus?: readonly Deep<GpuMetrics>[];
  readonly battery?: Deep<BatteryMetrics> | null;
  readonly powerDraw?: number | null;
  readonly fans?: SystemMetrics['fans'];
}

/** A healthy idle desktop, unless a test says otherwise. */
export function makeSystem(overrides: SystemOverrides = {}): SystemMetrics {
  return {
    cpu: makeCpu(overrides.cpu),
    memory: makeMemory(overrides.memory),
    disks: (overrides.disks ?? [{}]).map(makeDisk),
    networks: (overrides.networks ?? [{}]).map(makeNetwork),
    gpus: (overrides.gpus ?? [{}]).map(makeGpu),
    powerDraw: overrides.powerDraw ?? null,
    fans: overrides.fans ?? [],
    battery:
      overrides.battery === null
        ? null
        : overrides.battery === undefined
          ? null
          : makeBattery(overrides.battery),
  };
}

let nextPid = 1000;

export function makeProcess(overrides: Partial<Process> = {}): Process {
  const pid = overrides.key?.pid ?? nextPid++;
  return {
    key: { pid, startTime: 1 },
    parent: null,
    name: 'app.exe',
    kind: 'app',
    state: 'running',
    flags: [],
    integrity: 'medium',
    protection: 'none',
    cpu: 0,
    memoryPrivate: 0,
    memoryWorkingSet: 0,
    diskRead: 0,
    diskWrite: 0,
    netRx: 0,
    netTx: 0,
    gpu: null,
    gpuMemory: null,
    threadCount: 4,
    handleCount: 100,
    user: 'user',
    uptimeSecs: 60,
    ...overrides,
  } as Process;
}

/** Keyed the way `lib/metrics` keys them: `pid:startTime`. */
export function makeProcessMap(processes: readonly Process[]): ReadonlyMap<string, Process> {
  return new Map(
    processes.map((process) => [`${process.key.pid}:${process.key.startTime}`, process]),
  );
}
