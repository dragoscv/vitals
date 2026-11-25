/**
 * The Projects tab's pure rules: how a set of readings is summarised and
 * ordered, and the one rule that must never be broken — an unmeasured number
 * is an em dash, never a zero.
 */

import { describe, expect, it } from 'vitest';

import {
  formatLatency,
  formatNumber,
  isProblem,
  projectRank,
  severityRank,
  sortIncidents,
  statusRank,
  summary,
  worstStatus,
  type CloudProject,
  type HealthStatus,
  type ProjectIncident,
} from './projectsModel';

function incident(over: Partial<ProjectIncident> = {}): ProjectIncident {
  return {
    id: 'i1',
    severity: 'warning',
    summary: 'slow',
    componentKey: 'http',
    openedAt: '2026-11-25T10:00:00.000Z',
    notifiedAt: null,
    ...over,
  };
}

function project(over: Partial<CloudProject> = {}): CloudProject {
  return {
    id: 'p1',
    slug: 'p1',
    name: 'P1',
    kind: 'service',
    visibility: 'public',
    status: 'up',
    lastCheckedAt: null,
    components: [],
    openIncidents: [],
    ...over,
  };
}

describe('worstStatus', () => {
  it('returns the worst reading, so one down component is not hidden by healthy ones', () => {
    expect(worstStatus(['up', 'up', 'down', 'degraded'])).toBe('down');
    expect(worstStatus(['up', 'degraded'])).toBe('degraded');
    expect(worstStatus(['up', 'unknown'])).toBe('unknown');
  });

  it('treats an empty set as unknown rather than healthy', () => {
    // With nothing measured there is nothing to call healthy.
    expect(worstStatus([] as readonly HealthStatus[])).toBe('unknown');
    expect(isProblem(worstStatus([] as readonly HealthStatus[]))).toBe(false);
  });
});

describe('summary', () => {
  it('counts each status once and every open incident across the projects', () => {
    const totals = summary([
      project({ id: 'a', status: 'up' }),
      project({ id: 'b', status: 'degraded', openIncidents: [incident()] }),
      project({
        id: 'c',
        status: 'down',
        openIncidents: [incident({ id: 'i2', severity: 'critical' })],
      }),
      project({ id: 'd', status: 'unknown' }),
    ]);
    expect(totals).toEqual({ total: 4, up: 1, degraded: 1, down: 1, unknown: 1, openIncidents: 2 });
  });
});

describe('sortIncidents', () => {
  it('puts a critical incident first and then the newest within a severity', () => {
    const older = incident({ id: 'a', severity: 'warning', openedAt: '2026-11-25T08:00:00.000Z' });
    const newer = incident({ id: 'b', severity: 'warning', openedAt: '2026-11-25T09:00:00.000Z' });
    const critical = incident({
      id: 'c',
      severity: 'critical',
      openedAt: '2026-11-24T09:00:00.000Z',
    });
    expect(sortIncidents([older, newer, critical]).map((i) => i.id)).toEqual(['c', 'b', 'a']);
  });
});

describe('formatting an unmeasured figure', () => {
  it('renders a dash for an unmeasured latency or value, but keeps a measured zero', () => {
    expect(formatLatency(null)).toBe('—');
    expect(formatNumber(null)).toBe('—');
    expect(formatLatency(118)).toBe('118 ms');
    // A measured zero is a real zero and must not be turned into a dash.
    expect(formatNumber(0)).toBe('0');
  });
});

describe('ordering ranks', () => {
  it('places a problem before a healthy project and a critical incident before an informational one', () => {
    expect(projectRank(project({ status: 'down' }))).toBeLessThan(
      projectRank(project({ status: 'up' })),
    );
    expect(severityRank('critical')).toBeLessThan(severityRank('info'));
    expect(statusRank('down')).toBeLessThan(statusRank('unknown'));
  });
});
