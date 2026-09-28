import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';
import type { Alert, SystemMetrics } from '@vitals/protocol';

import type { LiveState } from '../lib/live';
import type { Pairing } from '../lib/pairing';
import { registerMobileStrings } from '../strings';
import { MachineCard } from './MachineCard';

const pairing: Pairing = {
  id: 'http://pc',
  baseUrl: 'http://pc',
  token: 't',
  name: 'DESK-01',
  addedAt: 0,
  readOnly: false,
};

function system(overrides: Partial<SystemMetrics>): SystemMetrics {
  return {
    cpu: { total: 12.5, temperature: null } as SystemMetrics['cpu'],
    memory: { used: 4 * 1024 ** 3, total: 16 * 1024 ** 3 } as SystemMetrics['memory'],
    disks: [],
    networks: [],
    gpus: [],
    powerDraw: null,
    battery: null,
    fans: [],
    ...overrides,
  };
}

beforeAll(async () => {
  await initI18n('en');
  registerMobileStrings();
});

describe('MachineCard', () => {
  it('omits the row for hardware the PC does not have, and dashes a reading it has but could not measure', () => {
    // Two different facts, two different renderings. No GPU → no GPU row: a
    // card that is one-third dashes reads as broken. A GPU whose driver hides
    // memory → the row exists and the missing number is a dash, never 0.
    const live: LiveState = {
      state: 'live',
      snapshot: {
        system: system({
          gpus: [{ name: 'GPU', utilization: null } as unknown as SystemMetrics['gpus'][number]],
          networks: [],
        }),
        processes: new Map(),
        seq: 1,
        timestampMs: 0,
      },
      cpuHistory: [10, 12.5],
      memoryHistory: [25, 25],
      alerts: null,
    };
    render(<MachineCard pairing={pairing} live={live} onOpen={() => {}} />);

    // Present-but-unmeasured GPU: a dash.
    expect(screen.getByText('GPU')).toBeTruthy();
    expect(screen.getAllByText('—')).toHaveLength(1);
    expect(screen.queryByText('0%')).toBeNull();
    // Absent network and temperature: no row at all, so no label either.
    expect(screen.queryByText('Network')).toBeNull();
    expect(screen.queryByText('Temperature')).toBeNull();
    expect(screen.getByText('13%')).toBeTruthy();
  });

  it('shows the real card, not the virtual display the kernel enumerates first', () => {
    // The bug this replaced: `gpus[0]` on this machine is a Parsec virtual
    // display with no counters, so the card read "—" while the RTX was at
    // 17 %. Selection is now "busiest measured adapter", shared with the
    // overlay and tested once in @vitals/protocol.
    const live: LiveState = {
      state: 'live',
      snapshot: {
        system: system({
          gpus: [
            { name: 'Display adapter 0x0003', utilization: null },
            { name: 'RTX 3060 Ti', utilization: 17 },
          ] as unknown as SystemMetrics['gpus'],
        }),
        processes: new Map(),
        seq: 1,
        timestampMs: 0,
      },
      cpuHistory: [10, 12.5],
      memoryHistory: [25, 25],
      alerts: null,
    };
    render(<MachineCard pairing={pairing} live={live} onOpen={() => {}} />);
    expect(screen.getByText('17%')).toBeTruthy();
    expect(screen.queryByText('—')).toBeNull();
  });

  it('shows the connection state as text, not only colour', () => {
    const live: LiveState = {
      state: 'reconnecting',
      snapshot: null,
      cpuHistory: [],
      memoryHistory: [],
      alerts: null,
    };
    render(<MachineCard pairing={pairing} live={live} onOpen={() => {}} />);
    expect(screen.getByText('Reconnecting…')).toBeTruthy();
  });
});

describe('MachineCard alerts strip', () => {
  const disconnected: LiveState = {
    state: 'live',
    snapshot: null,
    cpuHistory: [],
    memoryHistory: [],
    alerts: null,
  };

  const diskSpace: Alert = {
    kind: 'diskSpace',
    severity: 'warning',
    subject: 'C:',
    title: 'alert.diskSpace.title',
    cause: 'alert.diskSpace.cause',
    values: { disk: 'C:', percent: 4 },
    route: 'storage',
    sinceSample: 3,
  };

  const cpu: Alert = {
    kind: 'cpuSustained',
    severity: 'critical',
    subject: '',
    title: 'alert.cpuSustained.title',
    cause: 'alert.cpuSustained.cause',
    values: { percent: 97, seconds: 120 },
    route: 'processes',
    sinceSample: 9,
  };

  it('says nothing about alerts until the list has been read once', () => {
    // "Not asked yet" and "nothing wrong" are different facts; an all-clear
    // about a PC that has never answered would be a lie.
    render(<MachineCard pairing={pairing} live={disconnected} onOpen={() => {}} />);
    expect(screen.queryByText('Nothing needs your attention.')).toBeNull();
  });

  it('renders the all-clear line for an empty list', () => {
    render(
      <MachineCard pairing={pairing} live={{ ...disconnected, alerts: [] }} onOpen={() => {}} />,
    );
    expect(screen.getByText('Nothing needs your attention.')).toBeTruthy();
  });

  it('renders each alert with its interpolated title and the severity as text, not only colour', () => {
    render(
      <MachineCard
        pairing={pairing}
        live={{ ...disconnected, alerts: [diskSpace, cpu] }}
        onOpen={() => {}}
      />,
    );
    // `{{disk}}` interpolated from `values`, so the row names the drive.
    expect(screen.getByText('C: is nearly full')).toBeTruthy();
    expect(screen.getByText('Warning')).toBeTruthy();
    expect(screen.getByText('The processor has been busy for a while')).toBeTruthy();
    expect(screen.getByText('Critical')).toBeTruthy();
    expect(screen.queryByText('Nothing needs your attention.')).toBeNull();
    // The key itself must never leak to the screen.
    expect(screen.queryByText(/alert\./)).toBeNull();
  });

  it('interpolates a device name from values into a throttle title', () => {
    const gpu: Alert = {
      ...diskSpace,
      kind: 'gpuThrottled',
      title: 'alert.gpuThrottled.title',
      cause: 'alert.gpuThrottled.thermal',
      values: { gpu: 'RTX 3060 Ti', percent: 42.5 },
      subject: 'RTX 3060 Ti',
    };
    render(
      <MachineCard pairing={pairing} live={{ ...disconnected, alerts: [gpu] }} onOpen={() => {}} />,
    );
    expect(screen.getByText('RTX 3060 Ti is running slower than it can')).toBeTruthy();
  });

  it('opens the processes tab from an alert routed there, and makes nothing else tappable', () => {
    const onOpen = vi.fn();
    render(
      <MachineCard
        pairing={pairing}
        live={{ ...disconnected, alerts: [diskSpace, cpu] }}
        onOpen={onOpen}
      />,
    );
    const rows = screen.getAllByRole('button');
    // The card itself plus the one processes-routed alert; the storage alert is plain text.
    expect(rows).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: /processor has been busy/ }));
    expect(onOpen).toHaveBeenCalledWith(pairing.id);
  });
});
