/**
 * The polling contract.
 *
 * Two rules this project keeps relearning are enforced here: a loading state
 * must always resolve, and a slow read must not be allowed to queue behind
 * itself. The second matters more than it looks — a backlog of pending reads
 * on a busy machine makes the monitoring tool part of the problem it exists to
 * reveal.
 */

import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { NO_HOST, POLL_INTERVAL_MS, useConnections } from './useConnections';

const hasHost = vi.hoisted(() => ({ value: false }));

vi.mock('../../shell/host', () => ({
  hasTauriHost: () => hasHost.value,
}));

const empty = { connections: [], byProcess: [] };

beforeEach(() => {
  hasHost.value = false;
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useConnections', () => {
  it('reports no host rather than loading forever', async () => {
    // The production path, with no reader injected. There is no IPC to call,
    // so waiting would be waiting for something that cannot arrive.
    const { result } = renderHook(() => useConnections());

    await waitFor(() => {
      expect(result.current.pending).toBe(false);
    });
    expect(result.current.error).toBe(NO_HOST);
  });

  it('lets an injected reader bypass the host check entirely', async () => {
    // An injection seam the production path can veto is not a seam. The first
    // version consulted `hasTauriHost()` first and silently never called the
    // injected reader, which made every screen test fail for a reason
    // unrelated to what it was testing.
    const reader = vi.fn().mockResolvedValue(empty);
    const { result } = renderHook(() => useConnections(reader));

    await waitFor(() => {
      expect(reader).toHaveBeenCalled();
    });
    expect(result.current.error).toBeNull();
  });

  it('records when the data last arrived', async () => {
    const reader = vi.fn().mockResolvedValue(empty);
    const { result } = renderHook(() => useConnections(reader));

    await waitFor(() => {
      expect(result.current.updatedAt).not.toBeNull();
    });
  });

  it('keeps the previous snapshot when a later read fails', async () => {
    // Blanking a list the user is reading is worse than showing data that is
    // labelled stale.
    const snapshot = {
      connections: [],
      byProcess: [
        { pid: 4, total: 1, tcp: 1, udp: 0, active: 1, listening: 0, remoteHosts: 1, public: 0 },
      ],
    };
    const reader = vi.fn().mockResolvedValueOnce(snapshot).mockRejectedValue(new Error('boom'));

    const { result } = renderHook(() => useConnections(reader));
    await waitFor(() => {
      expect(result.current.snapshot).toBe(snapshot);
    });

    act(() => {
      result.current.refresh();
    });

    await waitFor(() => {
      expect(result.current.error).toBe('boom');
    });
    expect(result.current.snapshot).toBe(snapshot);
  });

  it('does not stack reads when one is still in flight', async () => {
    // A queue here lets a slow machine accumulate pending reads until the
    // backlog is the bottleneck — the exact pathology this app exists to make
    // visible, caused by the app itself.
    let release: (() => void) | undefined;
    const reader = vi.fn().mockImplementation(
      () =>
        new Promise((resolve) => {
          release = () => {
            resolve(empty);
          };
        }),
    );

    const { result } = renderHook(() => useConnections(reader));
    await waitFor(() => {
      expect(reader).toHaveBeenCalledTimes(1);
    });

    act(() => {
      result.current.refresh();
      result.current.refresh();
    });

    // Still one: the extra calls were dropped, not queued.
    expect(reader).toHaveBeenCalledTimes(1);

    await act(async () => {
      release?.();
      await Promise.resolve();
    });
  });

  it('polls on an interval', async () => {
    vi.useFakeTimers();
    const reader = vi.fn().mockResolvedValue(empty);

    renderHook(() => useConnections(reader));
    await vi.waitFor(() => {
      expect(reader).toHaveBeenCalledTimes(1);
    });

    await act(async () => {
      vi.advanceTimersByTime(POLL_INTERVAL_MS);
      await Promise.resolve();
    });

    expect(reader.mock.calls.length).toBeGreaterThan(1);
  });

  it('stops polling once unmounted', async () => {
    vi.useFakeTimers();
    const reader = vi.fn().mockResolvedValue(empty);

    const { unmount } = renderHook(() => useConnections(reader));
    await vi.waitFor(() => {
      expect(reader).toHaveBeenCalled();
    });

    unmount();
    const afterUnmount = reader.mock.calls.length;

    await act(async () => {
      vi.advanceTimersByTime(POLL_INTERVAL_MS * 3);
      await Promise.resolve();
    });

    // A timer surviving unmount keeps the whole closure alive and keeps
    // reading socket tables for a screen nobody is looking at.
    expect(reader.mock.calls.length).toBe(afterUnmount);
  });
});
