import { render, screen } from '@testing-library/react';
import { beforeAll, describe, expect, it } from 'vitest';

import { initI18n } from '@vitals/i18n';
import type { SystemMetrics } from '@vitals/protocol';

import type { LiveState } from '../lib/live';
import type { Pairing } from '../lib/pairing';
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
    ...overrides,
  };
}

beforeAll(async () => {
  await initI18n('en');
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
    };
    render(<MachineCard pairing={pairing} live={live} onOpen={() => {}} />);
    expect(screen.getByText('Reconnecting…')).toBeTruthy();
  });
});
