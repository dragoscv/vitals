/**
 * The devcloud panel draws the local half of the picture — run counts and the
 * workspaces marked down — beside the live reading agentq's own status
 * document carries.
 *
 * What it must never do is invent a figure: a value the document did not
 * measure is an em dash, and a read that failed is a sentence. Both are the
 * states a machine without agentq actually shows, so both are tested.
 */

import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { DevcloudPanel } from './DevcloudPanel';
import { makeSnapshot } from './fixtures.test-utils';
import type { CloudStatusSource, DevcloudStatus } from './useCloudStatus';

beforeAll(async () => {
  await initI18n();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
});

/** A source answering with one document, as `get_devcloud_status` does. */
function source(doc: DevcloudStatus): CloudStatusSource {
  return { read: () => Promise.resolve(doc) };
}

/** One measurable workspace beside one that reported nothing at all. */
const READ: DevcloudStatus = {
  available: true,
  failure: null,
  generatedAt: '2026-11-25T20:00:00Z',
  workspaces: [
    {
      ws: 'codai',
      vm: 'ws-codai',
      project: 'dragos-devcloud',
      zone: 'europe-west4-a',
      state: 'busy',
      heartbeatAgeSec: 12,
      idleSec: 300,
      queueDepth: 2,
      costMilliEur: null,
    },
    {
      ws: 'brivio',
      vm: null,
      project: null,
      zone: null,
      state: 'suspended',
      heartbeatAgeSec: null,
      idleSec: null,
      queueDepth: null,
      costMilliEur: null,
    },
  ],
  unreadable: [{ ws: 'metu', why: 'gcloud listed no instance' }],
};

describe('DevcloudPanel', () => {
  it('counts the cloud runs and names every workspace marked down, with its reason', () => {
    const stats = { ...makeSnapshot().stats, cloudRuns: 4, windowHours: 24 };
    const snapshot = makeSnapshot({
      stats,
      cloudDown: [
        { workspace: 'brivio-win', reason: 'maintenance', untilMs: Date.now() + 3_600_000 },
      ],
    });
    render(<DevcloudPanel snapshot={snapshot} source={source(READ)} />);

    expect(screen.getByText('4 cloud runs in the last 24 h')).toBeTruthy();
    expect(screen.getByText(/brivio-win is unavailable until/)).toBeTruthy();
    expect(screen.getByText('Reason: maintenance')).toBeTruthy();
  });

  it('says no workspace is down when none is marked', () => {
    render(<DevcloudPanel snapshot={makeSnapshot()} source={source(READ)} />);
    expect(screen.getByText('No cloud workspace is marked down.')).toBeTruthy();
  });

  it('draws the figures it read, and an em dash for every figure it did not', async () => {
    render(<DevcloudPanel snapshot={makeSnapshot()} source={source(READ)} />);

    // The read is async, so wait for the row before asserting on its cells.
    await screen.findByText('codai');
    expect(screen.getByText('Heartbeat 12 s ago')).toBeTruthy();
    expect(screen.getByText('Idle for 300 s')).toBeTruthy();
    expect(screen.getByText('Queue 2')).toBeTruthy();
    // brivio reported nothing, so its three measured figures are dashes; the
    // cost is a dash for both rows, because the document carries no cost yet.
    expect(screen.getByText('Heartbeat —')).toBeTruthy();
    expect(screen.getByText('Idle for —')).toBeTruthy();
    expect(screen.getByText('Queue —')).toBeTruthy();
    expect(screen.getAllByText('Cost —')).toHaveLength(2);
    // An unmeasured figure is never rendered as a zero.
    expect(screen.queryByText('Queue 0')).toBeNull();
    expect(screen.getByText(/metu: gcloud listed no instance/)).toBeTruthy();
  });

  it('says the document could not be read instead of drawing zeroes', async () => {
    render(
      <DevcloudPanel
        snapshot={makeSnapshot()}
        source={source({
          available: false,
          failure: { kind: 'noAgentq', detail: 'agentq-cloud.ps1 was not found' },
          generatedAt: null,
          workspaces: [],
          unreadable: [],
        })}
      />,
    );

    expect(
      await screen.findByText('agentq is not installed here, so there is no live status to read.'),
    ).toBeTruthy();
    expect(screen.getByText('agentq-cloud.ps1 was not found')).toBeTruthy();
    expect(screen.queryByText('Cost —')).toBeNull();
  });
});
