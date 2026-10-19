import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import type { Page } from '@playwright/test';

/**
 * A stand-in Tauri host for layout tests that need content on screen.
 *
 * The no-host fallback renders every screen as an empty state, which is
 * exactly the case where nothing overflows and no inner panel scrolls: the
 * Storage scroll-trap (S22-04) and every long-Romanian-string overflow were
 * invisible to it. This answers the handful of reads those screens make with
 * plausible, deliberately long data, through the same `__TAURI_INTERNALS__`
 * seam `@tauri-apps/api/mocks` uses — injected by Playwright, so the app
 * bundle carries none of it.
 *
 * The frame is the Rust-written contract fixture the Android client decodes,
 * so the shape cannot drift from what the sampler sends; only the process list
 * is widened, to give the Processes table rows to lay out.
 */

const here = dirname(fileURLToPath(import.meta.url));
const CONTRACT = resolve(here, '../../android/core/src/test/resources/contract');

const GIB = 1024 ** 3;

/** Paths long enough to need truncating at every width the app supports. */
const LONG =
  'C:\\Users\\developer\\AppData\\Local\\Packages\\Microsoft.WindowsTerminal_8wekyb3d8bbwe';

function storageData() {
  const largest = Array.from({ length: 30 }, (_, i) => ({
    path: `${LONG}\\LocalCache\\Roaming\\project-${String(i)}\\node_modules\\.pnpm\\very-long-package-name@1.2.${String(i)}`,
    allocated: (40 - i) * GIB,
    logical: (39 - i) * GIB,
    files: 120_000 - i * 1000,
    incomplete: i % 7 === 3 ? 'accessDenied' : null,
  }));
  const children = Array.from({ length: 24 }, (_, i) => ({
    node: i + 2,
    name:
      i % 5 === 0
        ? `A folder with a deliberately very long name number ${String(i)} for truncation`
        : `Folder ${String(i)}`,
    path: `C:\\Folder ${String(i)}`,
    allocated: (30 - i) * GIB,
    logical: (29 - i) * GIB,
    files: 5000 - i * 10,
    ownAllocated: 0,
    ownFiles: 0,
    hasChildren: true,
    incomplete: null,
  }));
  const root = {
    node: 1,
    name: 'C:\\',
    path: 'C:\\',
    allocated: 800 * GIB,
    logical: 780 * GIB,
    files: 1_400_000,
    ownAllocated: 2 * GIB,
    ownFiles: 12,
    hasChildren: true,
    incomplete: null,
  };
  const cells = children.slice(0, 12).map((child, i) => ({
    kind: 'directory',
    node: child.node,
    depth: 1,
    x0: i / 12,
    y0: 0.2,
    x1: (i + 1) / 12,
    y1: 1,
    allocated: child.allocated,
    count: child.files,
    openable: true,
    name: child.name,
    incomplete: null,
  }));
  return {
    volumes: [
      {
        mount: 'C:\\',
        label: 'Windows',
        fileSystem: 'NTFS',
        kind: 'nvme',
        total: 1000 * GIB,
        available: 120 * GIB,
        strategy: 'mftAssisted',
      },
      {
        mount: 'D:\\',
        label: 'A very long volume label for the data drive',
        fileSystem: 'NTFS',
        kind: 'ssd',
        total: 2000 * GIB,
        available: 900 * GIB,
        strategy: 'directoryWalk',
      },
      {
        mount: 'E:\\',
        label: null,
        fileSystem: 'exFAT',
        kind: 'removable',
        total: 64 * GIB,
        available: 63 * GIB,
        strategy: 'directoryWalk',
      },
    ],
    snapshot: {
      root: 'C:\\',
      largest,
      allocated: 800 * GIB,
      logical: 780 * GIB,
      clusterBytes: 4096,
      filesScanned: 1_400_000,
      directoriesScanned: 210_000,
      hardLinkDuplicates: 1200,
      hardLinkBytesSaved: 3 * GIB,
      cancelled: false,
      complete: true,
      elapsedMs: 42_000,
      skipped: [{ path: `${LONG}\\Secret`, reason: 'accessDenied', elevationFixable: true }],
      skippedTotal: 37,
      linksNotFollowed: 4,
      scanId: 7,
      rootNode: 1,
      largestFiles: largest.slice(0, 20).map((entry, i) => ({
        path: `${entry.path}\\pagefile-${String(i)}.sys`,
        allocated: entry.allocated / 4,
        logical: entry.logical / 4,
        dirNode: 1,
      })),
    },
    listing: { ancestry: [root], children },
    cells,
    candidates: [
      {
        path: `${LONG}\\Temp`,
        kind: 'userTemp',
        size: 3 * GIB,
        safety: 'safe',
        allowsOneClick: true,
        needsElevation: false,
        tool: null,
        irreversible: false,
      },
      {
        path: 'C:\\Windows\\SoftwareDistribution\\Download',
        kind: 'windowsUpdateCache',
        size: 6 * GIB,
        safety: 'safe',
        allowsOneClick: true,
        needsElevation: true,
        tool: 'diskCleanup',
        irreversible: false,
      },
      {
        path: 'C:\\Windows\\WinSxS',
        kind: 'componentStore',
        size: 9 * GIB,
        safety: 'review',
        allowsOneClick: false,
        needsElevation: true,
        tool: 'componentCleanup',
        irreversible: true,
      },
      {
        path: 'C:\\hiberfil.sys',
        kind: 'hibernation',
        size: 12 * GIB,
        safety: 'risky',
        allowsOneClick: false,
        needsElevation: true,
        tool: 'hibernateOff',
        irreversible: false,
      },
      {
        path: `${LONG}\\INetCache`,
        kind: 'browserCache',
        size: null,
        safety: 'review',
        allowsOneClick: false,
        needsElevation: false,
        tool: null,
        irreversible: false,
      },
    ],
  };
}

function frameData() {
  const keyframe = JSON.parse(readFileSync(resolve(CONTRACT, 'keyframe_full.json'), 'utf8')) as {
    payload: { processes: Record<string, unknown>[] };
  };
  const base = keyframe.payload.processes[0] ?? {};
  const names = [
    'chrome.exe',
    'Code - Insiders.exe',
    'msedgewebview2.exe',
    'svchost.exe',
    'A program with an extraordinarily long executable name.exe',
    'explorer.exe',
  ];
  keyframe.payload.processes = Array.from({ length: 60 }, (_, i) => ({
    ...base,
    key: { pid: 1000 + i * 4, startTime: 133_700_000_000_000_000 + i },
    parent: 4,
    name: names[i % names.length],
    description: `${String(names[i % names.length])} description that is rather long`,
    cpu: (i * 7) % 40,
    memoryPrivate: (60 - i) * 50_000_000,
    memoryWorkingSet: (60 - i) * 60_000_000,
    state: 'running',
  }));
  return keyframe;
}

/** Rows for the table screens, with the long names real machines have. */
function tableData() {
  const vendor = 'Contoso Software Engineering and Hardware Solutions Corporation';
  const now = Date.now();
  const startupEntries = Array.from({ length: 18 }, (_, i) => ({
    name: `StartupItem${String(i)}`,
    displayName:
      i % 4 === 0
        ? `${vendor} Background Update Helper Service ${String(i)}`
        : `Helper ${String(i)}`,
    command: `"${LONG}\\helper-${String(i)}.exe" --background --minimized --profile-directory="Default"`,
    imagePath: `${LONG}\\helper-${String(i)}.exe`,
    publisher: vendor,
    source: i % 3 === 0 ? 'scheduledTask' : 'userRun',
    state: i % 5 === 0 ? 'disabled' : 'enabled',
    pid: i % 2 === 0 ? 2000 + i : null,
    impact: { cpuMs: 1200 * i, diskBytes: 3_000_000 * i, measuredAtMs: now },
    company: vendor,
    microsoft: false,
    risk: 'safe',
  }));
  const services = Array.from({ length: 30 }, (_, i) => ({
    name: `Svc${String(i)}`,
    displayName: `${vendor} Telemetry and Diagnostics Collector ${String(i)}`,
    state: i % 3 === 0 ? 'stopped' : 'running',
    startType: i % 2 === 0 ? 'automatic' : 'manual',
    pid: i % 3 === 0 ? null : 3000 + i,
    binaryPath: `C:\\Windows\\System32\\svchost.exe -k LocalSystemNetworkRestricted -p -s Svc${String(i)}`,
    svchostGroup: 'LocalSystemNetworkRestricted',
    company: vendor,
    microsoft: i % 2 === 0,
    risk: i % 4 === 0 ? 'systemCritical' : 'safe',
    imagePath: 'C:\\Windows\\System32\\svchost.exe',
  }));
  const apps = Array.from({ length: 40 }, (_, i) => ({
    keyName: `{APP-${String(i)}}`,
    name:
      i % 3 === 0
        ? `${vendor} Ultimate Developer Toolkit Edition ${String(i)} (x64)`
        : `App ${String(i)}`,
    publisher: vendor,
    version: `10.${String(i)}.12345.67890`,
    installDate: '2026-09-15',
    installLocation: `${LONG}\\App${String(i)}`,
    estimatedSize: (i + 1) * 120_000,
    uninstallString: `"${LONG}\\uninstall.exe"`,
    quietUninstallString: null,
    isMsi: i % 2 === 0,
    perUser: i % 3 === 0,
    source: 'machineNative',
  }));
  const records = Array.from({ length: 30 }, (_, i) => ({
    executable: `${LONG}\\program-${String(i)}.exe`,
    name:
      i % 4 === 0
        ? `${vendor} Very Long Application Name ${String(i)}`
        : `program-${String(i)}.exe`,
    cpuSeconds: 3600 * (30 - i),
    diskReadBytes: 9 * GIB * (i + 1),
    diskWriteBytes: 2 * GIB * (i + 1),
    peakPrivateBytes: GIB * (i + 1),
    firstSeen: now - 86_400_000 * 20,
    lastSeen: now - 60_000 * i,
    sessions: 40 - i,
  }));
  const connections = Array.from({ length: 40 }, (_, i) => ({
    protocol: i % 4 === 0 ? 'tcp6' : 'tcp',
    localAddress: i % 4 === 0 ? 'fe80::1c2d:3e4f:5a6b:7c8d' : '192.168.100.123',
    localPort: 50_000 + i,
    remoteAddress: i % 4 === 0 ? '2a00:1450:4001:82f::200e' : '142.250.185.78',
    remotePort: 443,
    state: i % 6 === 0 ? 'listen' : 'established',
    ownerPid: 1000 + (i % 10) * 4,
    ownerName:
      i % 5 === 0 ? 'A program with an extraordinarily long executable name.exe' : 'chrome.exe',
    remoteHost: `edge-${String(i)}.very-long-subdomain.content-delivery.example-network.com`,
    country: 'RO',
    bytesSent: 1_000_000 * i,
    bytesReceived: 9_000_000 * i,
  }));
  return {
    get_startup: { entries: startupEntries, services, unreadableTasks: 2, impactMeasuredAtMs: now },
    get_installed_apps: {
      apps,
      examined: 60,
      rejected: 20,
      rejectedByReason: [{ reason: 'systemComponent', count: 20 }],
      duplicatesCollapsed: 3,
    },
    get_app_history: { records },
    get_connections: { connections, byProcess: [] },
    get_users: {
      sessions: [
        {
          sessionId: 0,
          userName: null,
          domain: null,
          clientName: null,
          state: 'disconnected',
          logonTime: null,
          isServices: true,
        },
        {
          sessionId: 1,
          userName: 'developer-with-a-long-account-name',
          domain: 'CONTOSO-WORKSTATION-01',
          clientName: null,
          state: 'active',
          logonTime: Math.floor(now / 1000) - 3600,
          isServices: false,
        },
      ],
      rollups: [
        { sessionId: 0, processCount: 140, cpuPercent: 3.2, memoryBytes: 2 * GIB },
        { sessionId: 1, processCount: 180, cpuPercent: 21.5, memoryBytes: 9 * GIB },
      ],
    },
    get_sensors: {
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
      readings: Array.from({ length: 40 }, (_, i) => ({
        key: `acpi.tz.${String(i)}`,
        label:
          i % 6 === 0
            ? `Thermal zone \\_TZ.THM${String(i)} on the chipset of a very long motherboard name`
            : `Thermal zone ${String(i)}`,
        value: 30 + i,
        unit: 'temperature',
        source: 'acpiThermalZone',
        quality: 'measured',
      })),
      gaps: [
        {
          capability: 'fanControl',
          reason: 'needsPlugin',
          label: 'Fan speed (RPM)',
          requirement: 'Super-I/O tachometer registers, ring-0 port I/O.',
          actionable: true,
        },
      ],
      cadenceMs: 5000,
      elapsedMs: 11.2,
    },
  };
}

export interface FakeHostOptions {
  readonly locale: 'en' | 'ro';
  /**
   * Make the optional readings flicker: every other frame reports CPU power,
   * temperature and GPU power as `null`, the way real sensors skip a tick.
   * The S22-10 defect — rows vanishing and every cell after them moving —
   * only shows with this on.
   */
  readonly flicker?: boolean;
  /** The first AI-page snapshot; later ones are pushed with `pushAgents`. */
  readonly agents?: unknown;
}

/** Installs the fake host before the app's first script runs. */
export async function installFakeHost(page: Page, options: FakeHostOptions): Promise<void> {
  const data = {
    locale: options.locale,
    storage: storageData(),
    keyframe: frameData(),
    tables: tableData(),
    flicker: options.flicker === true,
    agents: options.agents ?? null,
  };
  await page.addInitScript((d) => {
    type Handler = (event: { event: string; id: number; payload: unknown }) => void;
    const callbacks = new Map<number, Handler>();
    const listeners = new Map<string, number[]>();
    let nextId = 1;
    let seq = 1;
    const emit = (event: string, payload: unknown) => {
      for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
    };
    const sendFrame = () => {
      seq += 1;
      let frame: typeof d.keyframe = d.keyframe;
      if (d.flicker && seq % 2 === 1) {
        frame = JSON.parse(JSON.stringify(d.keyframe)) as typeof d.keyframe;
        const system = (frame.payload as unknown as Record<string, Record<string, unknown>>)[
          'system'
        ] as { cpu: Record<string, unknown>; gpus: Record<string, unknown>[] };
        system.cpu['power'] = null;
        system.cpu['temperature'] = null;
        for (const gpu of system.gpus) gpu['power'] = null;
      }
      emit('vitals://frame', { ...frame, seq, timestampMs: Date.now() });
    };
    // An Error carrying `kind`, so `isCommandError` reads it as the shape
    // Tauri commands reject with and the screen shows its own failure state.
    const unsupported = (cmd: string) =>
      Promise.reject(
        Object.assign(new Error(`${cmd} is not part of the layout fake host`), {
          kind: 'unsupported',
        }),
      );

    const answers: Record<string, unknown> = {
      get_volumes: d.storage.volumes,
      scan_storage: d.storage.snapshot,
      get_storage_children: d.storage.listing,
      get_storage_map: d.storage.cells,
      find_cleanup_candidates: d.storage.candidates,
      default_dev_roots: ['C:\\Users\\developer\\source\\repos', 'D:\\work'],
      get_launch_options: { autostarted: false, quietRelaunch: false },
      get_alerts: [],
      list_benchmarks: [],
      ...(d.agents === null
        ? {}
        : { subscribe_agents: { id: 1, snapshot: d.agents }, unsubscribe_agents: null }),
      ...d.tables,
    };

    const invoke = (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
      if (cmd === 'plugin:event|listen') {
        const event = String(args?.['event']);
        const handler = Number(args?.['handler']);
        listeners.set(event, [...(listeners.get(event) ?? []), handler]);
        if (event === 'vitals://frame') setTimeout(sendFrame, 50);
        return Promise.resolve(handler);
      }
      if (cmd === 'plugin:store|get') {
        const key = String(args?.['key']);
        return Promise.resolve(
          key === 'app' ? [{ locale: d.locale, firstRunDismissed: true }, true] : [null, false],
        );
      }
      if (cmd.startsWith('plugin:')) return Promise.resolve(null);
      if (cmd === 'request_keyframe') {
        sendFrame();
        return Promise.resolve(null);
      }
      if (cmd in answers) return Promise.resolve(answers[cmd]);
      return unsupported(cmd);
    };

    const w = window as unknown as Record<string, unknown>;
    w['isTauri'] = true;
    w['__TAURI_INTERNALS__'] = {
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { windowLabel: 'main', label: 'main' },
      },
      invoke,
      transformCallback: (cb: Handler) => {
        const id = nextId++;
        callbacks.set(id, cb);
        return id;
      },
      unregisterCallback: (id: number) => {
        callbacks.delete(id);
      },
      convertFileSrc: (path: string) => path,
    };
    w['__TAURI_EVENT_PLUGIN_INTERNALS__'] = { unregisterListener: () => undefined };
    // For tests that drive a sequence of snapshots themselves.
    w['__fakeEmit'] = emit;
    setInterval(sendFrame, 1000);
  }, data);
}

/** Pushes an AI-page snapshot through the fake host, as the desktop does every ~2 s. */
export async function pushAgents(page: Page, snapshot: unknown): Promise<void> {
  await page.evaluate((s) => {
    const emit = (window as unknown as Record<string, (e: string, p: unknown) => void>)[
      '__fakeEmit'
    ];
    emit?.('vitals://agents', s);
  }, snapshot);
}
