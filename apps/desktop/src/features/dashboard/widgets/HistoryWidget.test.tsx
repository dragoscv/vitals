import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { MachineSample } from '@vitals/protocol';

import { registerDashboardStrings } from '../strings';
import { HistoryWidget } from './HistoryWidget';
import {
  HISTORY_POLL_MS,
  rangeSeconds,
  seriesFromSamples,
  type HistorySource,
} from './useMachineHistory';

beforeAll(async () => {
  await initI18n();
  registerDashboardStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

afterEach(() => {
  vi.useRealTimers();
});

function sample(overrides: Partial<MachineSample> = {}): MachineSample {
  return {
    ts: 1_700_000_000,
    cpuPercent: 40,
    cpuKernelPercent: 10,
    memoryUsed: 8_000_000_000,
    memoryTotal: 16_000_000_000,
    diskReadBps: 0,
    diskWriteBps: 0,
    netRxBps: 0,
    netTxBps: 0,
    gpuPercent: 25,
    cpuTempC: null,
    powerDrawW: null,
    ...overrides,
  };
}

/** A source that answers with `samples` and records what it was asked. */
function fixedSource(samples: readonly MachineSample[]): HistorySource & { asked: number[] } {
  const asked: number[] = [];
  return {
    asked,
    query: (seconds) => {
      asked.push(seconds);
      return Promise.resolve(samples);
    },
  };
}

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

describe('HistoryWidget', () => {
  it('explains that recording is off and opens settings, without touching the store', async () => {
    const source = fixedSource([sample()]);
    const onOpenSettings = vi.fn();
    render(<HistoryWidget recording={false} onOpenSettings={onOpenSettings} source={source} />);
    await flush();

    expect(screen.getByText('History recording is off')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Open Settings' }));
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
    // Reading an empty store to then say "turn it on" would be wasted IPC.
    expect(source.asked).toEqual([]);
  });

  it('shows "No history yet" when recording is on but the store is empty', async () => {
    render(<HistoryWidget recording onOpenSettings={vi.fn()} source={fixedSource([])} />);
    await flush();

    expect(screen.getByText('No history yet')).toBeTruthy();
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('shows an inline error line, not a blank chart, when the query fails', async () => {
    const source: HistorySource = {
      query: () => Promise.reject(new Error('history store: locked')),
    };
    render(<HistoryWidget recording onOpenSettings={vi.fn()} source={source} />);
    await flush();

    expect(screen.getByRole('alert').textContent).toContain('history store: locked');
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('renders the chart for the 24 h range by default and re-queries when the range changes', async () => {
    const source = fixedSource([sample(), sample({ ts: 1_700_000_060 })]);
    render(<HistoryWidget recording onOpenSettings={vi.fn()} source={source} />);
    await flush();

    expect(screen.getByRole('img', { name: 'History' })).toBeTruthy();
    expect(source.asked).toEqual([rangeSeconds.h24]);

    fireEvent.click(screen.getByRole('radio', { name: '7 d' }));
    await flush();
    expect(source.asked).toEqual([rangeSeconds.h24, rangeSeconds.d7]);
  });

  it('turns a null gpu sample into a gap, never a zero', () => {
    const series = seriesFromSamples([
      sample({ gpuPercent: 30 }),
      sample({ gpuPercent: null }),
      sample({ gpuPercent: 50 }),
    ]);

    expect(series.gpu.toArray()).toHaveLength(3);
    expect(series.gpu.at(0)).toBe(30);
    expect(Number.isNaN(series.gpu.at(1))).toBe(true);
    expect(series.gpu.at(2)).toBe(50);
    // A gap must not drag the scale down to zero either.
    expect(series.gpu.extent()).toEqual({ min: 30, max: 50 });
    // Memory is a percentage of total, on the same 0–100 scale as the others.
    expect(series.memory.at(0)).toBe(50);
  });

  it('polls every minute while mounted and does not poll after unmount', async () => {
    vi.useFakeTimers();
    const source = fixedSource([sample()]);
    const view = render(<HistoryWidget recording onOpenSettings={vi.fn()} source={source} />);
    await flush();
    expect(source.asked).toHaveLength(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(HISTORY_POLL_MS);
    });
    expect(source.asked).toHaveLength(2);

    view.unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(HISTORY_POLL_MS * 3);
    });
    expect(source.asked).toHaveLength(2);
  });
});
