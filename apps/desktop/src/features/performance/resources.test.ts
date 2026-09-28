import { describe, expect, it } from 'vitest';

import { makeSystem } from '../dashboard/test-fixtures';
import {
  buildResourceList,
  hasThermalReadings,
  hottest,
  resolveSelection,
  setResourceHidden,
  visibleResources,
  type ResourceEntry,
} from './resources';

function ids(entries: readonly ResourceEntry[]): readonly string[] {
  return entries.map((entry) => entry.id);
}

function shownIds(entries: readonly ResourceEntry[]): readonly string[] {
  return ids(visibleResources(entries, false));
}

describe('buildResourceList', () => {
  it('is empty before the first frame', () => {
    expect(buildResourceList(null)).toEqual([]);
  });

  it('always offers CPU and memory first', () => {
    // They are the two resources present on every machine and the two people
    // check first, so they lead the rail rather than being sorted among the
    // devices.
    const entries = buildResourceList(
      // No temperature either, so `thermals` does not appear and the leading
      // pair is the whole list — which is what makes the ordering assertion
      // exact rather than a prefix check.
      makeSystem({ cpu: { temperature: null }, gpus: [], disks: [], networks: [] }),
    );
    expect(ids(entries)).toEqual(['cpu', 'memory']);
  });

  it('puts devices after the two singletons', () => {
    const entries = buildResourceList(makeSystem({ gpus: [{ id: 0 }], disks: [], networks: [] }));
    expect(ids(entries).slice(0, 2)).toEqual(['cpu', 'memory']);
  });

  it('namespaces device ids so a disk and a NIC cannot collide', () => {
    // Both are numbered from zero in the protocol. Without the prefix,
    // selecting disk 0 would also select network 0.
    const entries = buildResourceList(
      makeSystem({ gpus: [{ id: 0 }], disks: [{ id: 0 }], networks: [{ id: 0 }] }),
    );

    expect(ids(entries)).toContain('gpu:0');
    expect(ids(entries)).toContain('disk:0');
    expect(ids(entries)).toContain('network:0');
    expect(new Set(ids(entries)).size).toBe(entries.length);
  });

  it('hides loopback and virtual adapters by default', () => {
    // A developer machine reports a dozen of these and none of them answer
    // "why is my internet slow".
    const system = makeSystem({
      networks: [
        { id: 0, kind: 'ethernet' },
        { id: 1, kind: 'loopback' },
        { id: 2, kind: 'virtual' },
      ],
    });

    expect(shownIds(buildResourceList(system))).toEqual(
      expect.not.arrayContaining(['network:1', 'network:2']),
    );
    expect(shownIds(buildResourceList(system))).toContain('network:0');
  });

  it('still builds hidden entries so "show hidden" can reveal them', () => {
    // When the question IS about a container bridge, they are the only
    // interesting rows.
    const system = makeSystem({
      networks: [
        { id: 0, kind: 'ethernet' },
        { id: 1, kind: 'loopback' },
      ],
    });

    const entries = buildResourceList(system);
    expect(ids(visibleResources(entries, true))).toContain('network:1');
    expect(entries.find((entry) => entry.id === 'network:1')?.hidden).toBe(true);
  });

  it('keeps a VPN that carries traffic visible by default', () => {
    // A VPN is virtual but is frequently the actual cause of slow traffic, so
    // it is not in the background set.
    const system = makeSystem({
      networks: [{ id: 4, kind: 'vpn', connected: true, rxTotal: 4096, txTotal: 0 }],
    });
    expect(shownIds(buildResourceList(system))).toContain('network:4');
  });

  it('hides a tunnel that is down or has never carried a byte', () => {
    // The six WAN miniports, Teredo and 6to4 Windows installs everywhere:
    // measured on a Hyper-V host, fourteen such rows and none in use.
    const system = makeSystem({
      networks: [
        { id: 1, kind: 'vpn', connected: true, rxTotal: 0, txTotal: 0 },
        { id: 2, kind: 'unknown', connected: false, rxTotal: 9000, txTotal: 9000 },
      ],
    });
    expect(shownIds(buildResourceList(system))).toEqual(
      expect.not.arrayContaining(['network:1', 'network:2']),
    );
  });

  it('never hides a physical adapter by default, even an idle one', () => {
    // An unused Wi-Fi card is still hardware the user owns and may plug in.
    const system = makeSystem({
      networks: [{ id: 3, kind: 'wiFi', connected: false, rxTotal: 0, txTotal: 0 }],
    });
    expect(shownIds(buildResourceList(system))).toContain('network:3');
  });

  it('hides a GPU that reports nothing at all', () => {
    // A Parsec virtual display and the Basic Render Driver: real adapters
    // whose panel would be a page of em dashes.
    const system = makeSystem({
      gpus: [
        { id: 0, name: 'RTX', utilization: 20 },
        { id: 1, name: 'Basic Render', utilization: null, engines: [], memoryUsed: null },
      ],
    });
    expect(shownIds(buildResourceList(system))).toEqual(expect.arrayContaining(['gpu:0']));
    expect(shownIds(buildResourceList(system))).not.toContain('gpu:1');
  });

  it('applies the user’s choice in both directions', () => {
    const system = makeSystem({
      disks: [{ id: 7, mount: 'H:' }],
      networks: [{ id: 2, name: 'vEthernet (WSL)', kind: 'virtual' }],
    });

    const entries = buildResourceList(system, {
      visibility: { 'disk:H:': 'hidden', 'network:vEthernet (WSL)': 'shown' },
    });

    expect(shownIds(entries)).not.toContain('disk:7');
    expect(shownIds(entries)).toContain('network:2');
  });

  it('remembers a device by name, not by an id Windows renumbers', () => {
    // An interface index or GPU LUID changes across reboots; hiding by id
    // would forget the choice, or worse, hide a different device.
    const visibility = { 'network:Tailscale': 'hidden' } as const;
    const before = makeSystem({ networks: [{ id: 14, name: 'Tailscale', kind: 'ethernet' }] });
    const after = makeSystem({ networks: [{ id: 51, name: 'Tailscale', kind: 'ethernet' }] });

    expect(shownIds(buildResourceList(before, { visibility }))).not.toContain('network:14');
    expect(shownIds(buildResourceList(after, { visibility }))).not.toContain('network:51');
  });

  it('ranks disks by active time, not throughput', () => {
    // A disk saturated by tiny random reads moves almost no bytes while being
    // completely unusable; a rail scaled on throughput would call it idle.
    const system = makeSystem({ disks: [{ id: 0, activeTime: 97, read: 1000, write: 0 }] });
    const disk = buildResourceList(system).find((entry) => entry.id === 'disk:0');

    expect(disk?.utilization).toBe(97);
  });

  it('gives networks no utilisation rather than inventing one', () => {
    // Link speed is a nominal ceiling Wi-Fi never reaches and many adapters
    // report as null, so a percentage would be fabricated.
    const system = makeSystem({ networks: [{ id: 0, rx: 5_000_000, linkSpeed: null }] });
    const nic = buildResourceList(system).find((entry) => entry.id === 'network:0');

    // Null, not zero: there is no honest percentage for a network adapter,
    // and the rail omits the meter rather than drawing an empty one.
    expect(nic?.utilization).toBeNull();
  });

  it('omits the thermals entry when nothing reports a temperature', () => {
    // Most desktops. A permanently empty tab makes the user think their
    // sensors are broken.
    const system = makeSystem({
      cpu: { temperature: null },
      gpus: [{ id: 0, temperature: null, hotspotTemperature: null }],
      disks: [{ id: 0, temperature: null }],
    });

    expect(ids(buildResourceList(system))).not.toContain('thermals');
  });

  it('includes thermals as soon as anything reports one', () => {
    const system = makeSystem({
      cpu: { temperature: null },
      gpus: [],
      disks: [{ id: 0, temperature: 41 }],
    });

    expect(ids(buildResourceList(system))).toContain('thermals');
  });

  it('rebuilds when a device disappears', () => {
    // Unplugging a drive mid-session. The list must shrink rather than keep a
    // stale entry that resolves to nothing.
    const before = buildResourceList(makeSystem({ disks: [{ id: 0 }, { id: 1 }] }));
    const after = buildResourceList(makeSystem({ disks: [{ id: 0 }] }));

    expect(ids(before)).toContain('disk:1');
    expect(ids(after)).not.toContain('disk:1');
  });
});

describe('hasThermalReadings', () => {
  it('is false before the first frame', () => {
    expect(hasThermalReadings(null)).toBe(false);
  });

  it('counts a battery temperature', () => {
    const system = makeSystem({
      cpu: { temperature: null },
      gpus: [],
      disks: [{ temperature: null }],
      battery: { temperature: 30 },
    });
    expect(hasThermalReadings(system)).toBe(true);
  });
});

describe('hottest', () => {
  it('returns null when nothing is measurable', () => {
    const system = makeSystem({
      cpu: { temperature: null },
      gpus: [],
      disks: [{ temperature: null }],
    });
    expect(hottest(system)).toBeNull();
  });

  it('includes the GPU hotspot, which is what actually throttles', () => {
    // Hotspot runs 10-20 °C above the core reading on modern cards, so taking
    // only the core would under-report the number that matters.
    const system = makeSystem({
      cpu: { temperature: 50 },
      gpus: [{ id: 0, temperature: 60, hotspotTemperature: 88 }],
      disks: [{ temperature: 40 }],
    });

    expect(hottest(system)).toBe(88);
  });
});

describe('resolveSelection', () => {
  const entries = buildResourceList(makeSystem({ disks: [{ id: 0 }, { id: 1 }] }));

  it('returns the requested entry', () => {
    expect(resolveSelection(entries, 'disk:1')?.id).toBe('disk:1');
  });

  it('falls back to the first entry when the device is gone', () => {
    // An empty detail pane after unplugging a drive looks like the page broke.
    expect(resolveSelection(entries, 'disk:99')?.id).toBe('cpu');
  });

  it('falls back when nothing has been chosen yet', () => {
    expect(resolveSelection(entries, null)?.id).toBe('cpu');
  });

  it('returns null only when there is genuinely nothing', () => {
    expect(resolveSelection([], 'cpu')).toBeNull();
  });
});

describe('setResourceHidden', () => {
  const system = makeSystem({
    disks: [{ id: 0, mount: 'C:' }],
    networks: [{ id: 1, name: 'vSwitch', kind: 'virtual' }],
  });
  const find = (list: readonly ResourceEntry[], id: string): ResourceEntry => {
    const entry = list.find((candidate) => candidate.id === id);
    if (entry === undefined) throw new Error(`no ${id}`);
    return entry;
  };

  it('stores a hide for a device shown by default', () => {
    const disk = find(buildResourceList(system), 'disk:0');
    expect(setResourceHidden({}, disk, true)).toEqual({ 'disk:C:': 'hidden' });
  });

  it('stores a show for a device hidden by default', () => {
    const nic = find(buildResourceList(system), 'network:1');
    expect(setResourceHidden({}, nic, false)).toEqual({ 'network:vSwitch': 'shown' });
  });

  it('drops the override when the choice returns to the default', () => {
    // Only real overrides are kept, so a better default later still reaches
    // every device the user never touched.
    const entries = buildResourceList(system, { visibility: { 'disk:C:': 'hidden' } });
    expect(setResourceHidden({ 'disk:C:': 'hidden' }, find(entries, 'disk:0'), false)).toEqual({});
  });

  it('cannot hide CPU or memory', () => {
    const cpu = find(buildResourceList(system), 'cpu');
    expect(setResourceHidden({}, cpu, true)).toEqual({});
  });
});
