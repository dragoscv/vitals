/**
 * The Projects read keeps the last good list across a failed refresh (showing
 * it as out of date rather than blanking it) and reports a signed-out account
 * as a state of its own rather than an empty list.
 */

import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ProjectsRead, ProjectsSource } from './projectsApi';
import type { ProjectsSnapshot } from './projectsModel';
import { useProjects } from './useProjects';

afterEach(() => {
  vi.useRealTimers();
});

const SNAP: ProjectsSnapshot = {
  takenAtMs: 0,
  projects: [
    {
      id: 'p1',
      slug: 'checkout-api',
      name: 'Checkout API',
      kind: 'service',
      visibility: 'public',
      status: 'up',
      lastCheckedAt: null,
      components: [],
      openIncidents: [],
    },
  ],
};

describe('useProjects', () => {
  it('keeps the last list and marks it out of date when a later refresh fails', async () => {
    vi.useFakeTimers();
    let calls = 0;
    const source: ProjectsSource = {
      read: () => {
        calls += 1;
        const answer: ProjectsRead =
          calls === 1 ? { state: 'ok', snapshot: SNAP } : { state: 'unavailable' };
        return calls === 1 ? Promise.resolve(answer) : Promise.reject(new Error('offline'));
      },
    };
    const { result } = renderHook(() => useProjects(source, 30_000));

    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.state.status).toBe('ok');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(result.current.state.status).toBe('error');
    expect(result.current.state.error).toBe('unreachable');
    // The body survives the failed refresh: out of date, not gone.
    expect(result.current.state.data).toBe(SNAP);
  });

  it('answers a stored account as signed out, without inventing an empty list', async () => {
    const source: ProjectsSource = { read: () => Promise.resolve({ state: 'signedOut' }) };
    const { result } = renderHook(() => useProjects(source));
    await waitFor(() => {
      expect(result.current.state.status).toBe('signedOut');
    });
    expect(result.current.state.data).toBeNull();
  });
});
