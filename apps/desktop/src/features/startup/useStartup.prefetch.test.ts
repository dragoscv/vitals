/**
 * The first visit to Startup renders the background read — no skeleton, and
 * no second registry-and-SCM walk. Uses the production reader (a mocked
 * `invoke`), because an injected reader deliberately bypasses the prefetch.
 */

import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { clearPrefetched } from '../../lib/prefetch';
import type { StartupSnapshot } from './model';
import { prefetchStartup, useStartup } from './useStartup';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('../../shell/host', () => ({ hasTauriHost: () => true }));

const snapshot = (entries: number): StartupSnapshot =>
  ({
    entries: Array.from({ length: entries }, () => ({})),
    services: [],
  }) as unknown as StartupSnapshot;

beforeEach(() => {
  clearPrefetched();
  invoke.mockReset();
});

describe('useStartup with a background read', () => {
  it('renders the prefetched list on its first render and does not read again', async () => {
    invoke.mockResolvedValue(snapshot(5));
    await prefetchStartup(false);
    expect(invoke).toHaveBeenCalledTimes(1);

    const { result } = renderHook(() => useStartup(false));

    // The first render, before any effect: this is what removes the skeleton.
    expect(result.current.pending).toBe(false);
    expect(result.current.snapshot?.entries).toHaveLength(5);

    await waitFor(() => expect(result.current.refreshing).toBe(false));
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('keeps Startup and Services separate, since only Services asks for start types', async () => {
    invoke.mockResolvedValue(snapshot(2));
    await prefetchStartup(true);

    const { result } = renderHook(() => useStartup(false));

    expect(result.current.pending).toBe(true);
    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(invoke).toHaveBeenLastCalledWith('get_startup', { withServiceConfig: false });
  });

  it('without a background read, loads exactly as before', async () => {
    invoke.mockResolvedValue(snapshot(1));

    const { result } = renderHook(() => useStartup(false));

    expect(result.current.pending).toBe(true);
    await waitFor(() => expect(result.current.snapshot?.entries).toHaveLength(1));
  });
});
