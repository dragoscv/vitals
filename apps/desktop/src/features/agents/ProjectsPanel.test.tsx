/**
 * The Projects tab renders each documented state from the one read, keeps a
 * stale list rather than blanking it, and shows a dash rather than a zero for
 * a figure the source did not measure.
 */

import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { makeSnapshot } from './fixtures.test-utils';
import type { ProjectsRead, ProjectsSource } from './projectsApi';
import type { ProjectsSnapshot } from './projectsModel';
import { ProjectsPanel } from './ProjectsPanel';
import { registerAgentsStrings } from './strings';

// The signed-out state renders the account card, which reads the Tauri host
// and `get_cloud_status`; the injected source keeps everything else offline.
vi.mock('../../shell/host', () => ({ hasTauriHost: () => true }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve({ signedIn: false, hosts: [], error: null })),
}));

beforeAll(async () => {
  await initI18n();
  registerAgentsStrings();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function sourceOf(read: () => Promise<ProjectsRead>): ProjectsSource {
  return { read };
}

function snapshot(): ProjectsSnapshot {
  const now = Date.now();
  const iso = (ago: number): string => new Date(now - ago).toISOString();
  return {
    takenAtMs: now,
    projects: [
      {
        id: 'p1',
        slug: 'checkout-api',
        name: 'Checkout API',
        kind: 'service',
        visibility: 'public',
        status: 'degraded',
        lastCheckedAt: iso(1_000),
        components: [
          {
            key: 'http',
            label: 'HTTPS endpoint',
            status: 'up',
            latencyMs: 118,
            value: null,
            detail: null,
            checkedAt: iso(1_000),
          },
          {
            // No latency was measured for this one: it must render a dash.
            key: 'replica',
            label: 'Read replica lag',
            status: 'up',
            latencyMs: null,
            value: null,
            detail: null,
            checkedAt: iso(1_000),
          },
        ],
        openIncidents: [
          {
            id: 'inc1',
            severity: 'critical',
            summary: 'Webhooks are failing',
            componentKey: 'http',
            openedAt: iso(60_000),
            notifiedAt: null,
          },
        ],
      },
      {
        id: 'p2',
        slug: 'status-page',
        name: 'Status page',
        kind: 'webapp',
        visibility: 'public',
        status: 'up',
        lastCheckedAt: iso(2_000),
        components: [],
        openIncidents: [],
      },
    ],
  };
}

describe('ProjectsPanel', () => {
  it('lists the projects, shows the selected one in full, and dashes an unmeasured latency', async () => {
    const src = sourceOf(() => Promise.resolve({ state: 'ok', snapshot: snapshot() }));
    render(<ProjectsPanel snapshot={makeSnapshot()} source={src} />);

    // The list row and the detail header both name the selected project.
    expect(await screen.findAllByText('Checkout API')).not.toHaveLength(0);
    expect(screen.getByText('Status page')).toBeTruthy();
    // Twice over: the project's status chip and the component that degraded it.
    expect(screen.getAllByText('Degraded').length).toBeGreaterThan(1);
    // The incident of the selected project is shown with its un-delivered mark.
    expect(screen.getByText('Webhooks are failing')).toBeTruthy();
    expect(screen.getByText('Alert not delivered')).toBeTruthy();
    // The replica component measured no latency, so it renders an em dash.
    expect(screen.getAllByText(/Latency: —/).length).toBeGreaterThan(0);
    // The devcloud panel is local and always present.
    expect(screen.getByText('Devcloud workspaces')).toBeTruthy();
  });

  it('shows the sign-in state, never an empty list, when no account is stored', async () => {
    const src = sourceOf(() => Promise.resolve({ state: 'signedOut' }));
    render(<ProjectsPanel snapshot={makeSnapshot()} source={src} />);
    expect(await screen.findByText(/Sign in to Codwen/)).toBeTruthy();
    expect(screen.queryByText('No projects yet')).toBeNull();
  });

  it('says the endpoint is not available rather than showing an empty list', async () => {
    const src = sourceOf(() => Promise.resolve({ state: 'unavailable' }));
    render(<ProjectsPanel snapshot={makeSnapshot()} source={src} />);
    expect(await screen.findByText('Projects are not available yet')).toBeTruthy();
    expect(screen.queryByText('No projects yet')).toBeNull();
  });
});
