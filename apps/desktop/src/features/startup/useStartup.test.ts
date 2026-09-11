import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { StartupSnapshot } from './model';
import { useStartup } from './useStartup';

/**
 * A snapshot whose only interesting property is how many entries it has.
 *
 * These tests care about *which* snapshot is on screen at a given moment, not
 * what is in it, so the count serves as an identity marker.
 */
function snapshot(entryCount: number): StartupSnapshot {
  return {
    entries: Array.from({ length: entryCount }, (_, index) => ({
      id: `entry-${index}`,
      name: `Entry ${index}`,
      command: 'C:\\test.exe',
      source: 'runKeyMachine',
      state: 'enabled',
      publisher: null,
      impact: 'unknown',
    })),
    services: [],
    unreadableTasks: 0,
    impactMeasuredAtMs: null,
  } as unknown as StartupSnapshot;
}

describe('useStartup', () => {
  it('reports pending only while there is nothing to show', async () => {
    const reader = vi.fn(async () => snapshot(3));
    const { result } = renderHook(() => useStartup(false, reader));

    expect(result.current.pending).toBe(true);

    await waitFor(() => expect(result.current.snapshot).not.toBeNull());
    expect(result.current.pending).toBe(false);
    expect(result.current.refreshing).toBe(false);
  });

  it('keeps the old list on screen while refreshing, rather than blanking it', async () => {
    // This is the whole point of the change. Screens now stay mounted between
    // navigations, so returning to one re-reads while its list is already
    // drawn. Reporting that as `pending` swapped a good list for a skeleton
    // every time the user came back.
    let release: (value: StartupSnapshot) => void = () => undefined;

    const reader = vi
      .fn<(withServiceConfig: boolean) => Promise<StartupSnapshot>>()
      .mockResolvedValueOnce(snapshot(3))
      .mockImplementationOnce(
        async () =>
          new Promise<StartupSnapshot>((resolve) => {
            release = resolve;
          }),
      );

    const { result } = renderHook(() => useStartup(false, reader));
    await waitFor(() => expect(result.current.snapshot?.entries).toHaveLength(3));

    act(() => {
      result.current.refresh();
    });

    await waitFor(() => expect(result.current.refreshing).toBe(true));

    // The critical assertion: mid-refresh the previous data is still there,
    // and the screen is not told it is pending.
    expect(result.current.pending).toBe(false);
    expect(result.current.snapshot?.entries).toHaveLength(3);

    await act(async () => {
      release(snapshot(5));
    });

    await waitFor(() => expect(result.current.snapshot?.entries).toHaveLength(5));
    expect(result.current.refreshing).toBe(false);
  });

  it('a failed refresh leaves the previous list intact', async () => {
    const reader = vi
      .fn<(withServiceConfig: boolean) => Promise<StartupSnapshot>>()
      .mockResolvedValueOnce(snapshot(4))
      .mockRejectedValueOnce(new Error('SCM unavailable'));

    const { result } = renderHook(() => useStartup(false, reader));
    await waitFor(() => expect(result.current.snapshot?.entries).toHaveLength(4));

    act(() => {
      result.current.refresh();
    });

    await waitFor(() => expect(result.current.error).toBe('SCM unavailable'));

    // Blanking a list the user is reading because a background re-read failed
    // would be worse than showing data a few seconds old.
    expect(result.current.snapshot?.entries).toHaveLength(4);
    expect(result.current.pending).toBe(false);
  });

  it('does not stack concurrent reads', async () => {
    // Returning to a screen while its previous read is still in flight must
    // not queue a second walk of the registry and the SCM.
    const reader = vi.fn(async () => snapshot(2));
    const { result } = renderHook(() => useStartup(false, reader));

    await waitFor(() => expect(result.current.snapshot).not.toBeNull());

    act(() => {
      result.current.refresh();
      result.current.refresh();
      result.current.refresh();
    });

    await waitFor(() => expect(result.current.refreshing).toBe(false));

    // One initial read plus one refresh; the two extra calls were rejected by
    // the in-flight guard.
    expect(reader).toHaveBeenCalledTimes(2);
  });
});
