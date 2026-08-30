import { describe, expect, it } from 'vitest';

import { makeSystem } from '../dashboard/test-fixtures';
import {
  buildResourceList,
  hasThermalReadings,
  hottest,
  resolveSelection,
  type ResourceEntry,
} from './resources';

function ids(entries: readonly ResourceEntry[]): readonly string[] {
  return entries.map((entry) => entry.id);
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

    expect(ids(buildResourceList(system))).toEqual(
      expect.not.arrayContaining(['network:1', 'network:2']),
    );
    expect(ids(buildResourceList(system))).toContain('network:0');
  });

  it('shows them when asked', () => {
    // When the question IS about a container bridge, they are the only
    // interesting rows.
    const system = makeSystem({
      networks: [
        { id: 0, kind: 'ethernet' },
        { id: 1, kind: 'loopback' },
      ],
    });

    const entries = buildResourceList(system, { showVirtualAdapters: true });
    expect(ids(entries)).toContain('network:1');
  });

  it('keeps a VPN visible by default', () => {
    // A VPN is virtual but is frequently the actual cause of slow traffic, so
    // it is not in the background set.
    const system = makeSystem({ networks: [{ id: 4, kind: 'vpn' }] });
    expect(ids(buildResourceList(system))).toContain('network:4');
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

    expect(nic?.utilization).toBe(0);
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
