/**
 * The devcloud panel shows the local half of the picture — run counts and the
 * workspaces marked down — and names the fields it cannot read yet rather than
 * drawing them as zeroes.
 */

import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { DevcloudPanel } from './DevcloudPanel';
import { makeSnapshot } from './fixtures.test-utils';

beforeAll(async () => {
  await initI18n();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('DevcloudPanel', () => {
  it('counts the cloud runs and names every workspace marked down, with its reason', () => {
    const stats = { ...makeSnapshot().stats, cloudRuns: 4, windowHours: 24 };
    const snapshot = makeSnapshot({
      stats,
      cloudDown: [
        { workspace: 'brivio-win', reason: 'maintenance', untilMs: Date.now() + 3_600_000 },
      ],
    });
    render(<DevcloudPanel snapshot={snapshot} />);

    expect(screen.getByText('4 cloud runs in the last 24 h')).toBeTruthy();
    expect(screen.getByText(/brivio-win is unavailable until/)).toBeTruthy();
    expect(screen.getByText('Reason: maintenance')).toBeTruthy();
    // The GCS half is named as not-yet-read, never shown as a zero.
    expect(screen.getByText(/not read yet/)).toBeTruthy();
  });

  it('says no workspace is down when none is marked, and still states what is not read', () => {
    render(<DevcloudPanel snapshot={makeSnapshot()} />);
    expect(screen.getByText('No cloud workspace is marked down.')).toBeTruthy();
    expect(screen.getByText(/not read yet/)).toBeTruthy();
  });
});
