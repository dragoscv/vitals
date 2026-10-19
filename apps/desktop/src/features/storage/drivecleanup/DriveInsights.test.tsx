import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { resetSettingsForTests, useSettings } from '../../../settings/store';
import { registerStorageStrings } from '../strings';
import { DriveInsights } from './DriveInsights';
import type { DriveHistory, DriveHistorySource } from './history';

const GB = 1024 ** 3;
const DAY = 86_400;
const NOW = 1_792_411_200;
const VOLUME = '\\\\?\\Volume{1234}\\';

beforeAll(async () => {
  await initI18n();
  registerStorageStrings();
});

beforeEach(() => {
  resetSettingsForTests();
});

afterEach(async () => {
  cleanup();
  await i18n.changeLanguage('en');
});

function history(overrides: Partial<DriveHistory> = {}): DriveHistory {
  return {
    volume: VOLUME,
    series: [
      { ts: NOW - 7 * DAY, used: 100 * GB, free: 400 * GB },
      { ts: NOW, used: 114 * GB, free: 386 * GB },
    ],
    growth: { day: 2 * GB, week: 2 * GB, month: null },
    latestAnalysis: NOW,
    previousAnalysis: NOW - 7 * DAY,
    changes: [
      { kind: 'tempOld', label: null, then: 4 * GB, now: GB },
      { kind: 'dockerDisk', label: 'docker_data.vhdx', then: 100 * GB, now: 142 * GB },
    ],
    runs: [],
    ...overrides,
  };
}

function source(value: DriveHistory): DriveHistorySource {
  return { load: vi.fn<DriveHistorySource['load']>().mockResolvedValue(value) };
}

async function shown(value: DriveHistory = history()) {
  const src = source(value);
  render(<DriveInsights mount="C:\" locale="en" source={src} />);
  await screen.findByText('Why is this drive growing?');
  return src;
}

describe('drive insights', () => {
  it('shows growth per day where the readings allow it and says so where they do not', async () => {
    const src = await shown();
    expect(src.load).toHaveBeenCalledWith('C:\\');
    expect(screen.getAllByText('+2.00 GB a day')).toHaveLength(2);
    expect(screen.getByText('Not enough readings yet')).toBeTruthy();
    expect(
      screen.getByRole('img', { name: 'Space used on C:\\ over the last 30 days' }),
    ).toBeTruthy();
  });

  it('lists what grew since the earlier analysis, largest first, naming the docker disk', async () => {
    await shown();
    const items = screen.getAllByRole('listitem').map((li) => li.textContent);
    expect(items[0]).toContain('Docker disk file (docker_data.vhdx)');
    expect(items[0]).toContain('+42.0 GB');
    expect(items[1]).toContain('Old temporary files');
    expect(items[1]).toContain('−3.00 GB');
  });

  it('says this is the first analysis rather than showing an empty list', async () => {
    await shown(history({ previousAnalysis: null, changes: [] }));
    expect(screen.getByText(/first analysis of this drive/)).toBeTruthy();
  });

  it('turns the weekly clean-up on under the volume key and lists only the safe choices', async () => {
    useSettings.getState().patch({
      driveCleanupProfiles: { 'C:': ['tempOld', 'compactVhd', 'dockerNamedVolume', 'npmCache'] },
    });
    await shown();
    fireEvent.click(screen.getByRole('switch', { name: 'Clean up C: every week' }));
    const saved = useSettings.getState().settings.driveCleanupSchedules[VOLUME];
    expect(saved).toMatchObject({ mount: 'C:', weekday: 6, minute: 180 });
    expect(saved?.enabledAt).toBeGreaterThan(0);
    expect(screen.getByText('It will clean: Old temporary files, npm cache')).toBeTruthy();
  });

  it('says plainly when none of the saved choices can run unattended', async () => {
    useSettings.getState().patch({ driveCleanupProfiles: { 'C:': ['compactVhd', 'rustTarget'] } });
    await shown();
    fireEvent.click(screen.getByRole('switch', { name: 'Clean up C: every week' }));
    expect(screen.getByText(/none of your saved choices/i)).toBeTruthy();
  });

  it('turning it off removes the schedule and keeps other drives', async () => {
    useSettings.getState().patch({
      driveCleanupSchedules: {
        [VOLUME]: { mount: 'C:', weekday: 2, minute: 600, enabledAt: 5 },
        other: { mount: 'E:', weekday: 0, minute: 0, enabledAt: 5 },
      },
    });
    await shown();
    fireEvent.click(screen.getByRole('switch', { name: 'Clean up C: every week' }));
    expect(Object.keys(useSettings.getState().settings.driveCleanupSchedules)).toEqual(['other']);
  });

  it('shows recent runs with what they freed, an unmeasured gain, and failures', async () => {
    await shown(
      history({
        runs: [
          { ts: NOW, scheduled: true, freed: 3 * GB, items: 2, failed: 1, errors: ['docker down'] },
          { ts: NOW - DAY, scheduled: false, freed: null, items: 1, failed: 0, errors: [] },
        ],
      }),
    );
    expect(screen.getByText('Weekly')).toBeTruthy();
    expect(screen.getByText('By you')).toBeTruthy();
    expect(screen.getByText(/freed 3.00 GB/)).toBeTruthy();
    expect(screen.getByText(/gain not measured/)).toBeTruthy();
    expect(screen.getByText(/failures: 1/)).toBeTruthy();
    expect(screen.getByText('docker down')).toBeTruthy();
  });

  it('reports a history that could not be read instead of showing nothing', async () => {
    const src: DriveHistorySource = {
      load: vi.fn<DriveHistorySource['load']>().mockRejectedValue(new Error('locked')),
    };
    render(<DriveInsights mount="C:\" locale="en" source={src} />);
    expect(await screen.findByText(/could not be read\. locked/)).toBeTruthy();
  });
});
