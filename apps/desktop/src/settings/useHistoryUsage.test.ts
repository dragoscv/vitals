import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useHistoryUsage } from './useHistoryUsage';

const usage = { bytes: 1024, fineRows: 10, flightFrames: 5 };

describe('useHistoryUsage', () => {
  it('resolves to the reported usage', async () => {
    const reader = vi.fn(async () => usage);
    const { result } = renderHook(() => useHistoryUsage(0, reader));

    await waitFor(() => {
      expect(result.current.pending).toBe(false);
    });
    expect(result.current.usage).toEqual(usage);
    expect(reader).toHaveBeenCalledTimes(1);
  });

  it('refetches when the revision changes, and only then', async () => {
    // This is what makes "Delete all history" update the figure beside it.
    const reader = vi.fn(async () => usage);
    const { rerender } = renderHook(({ rev }) => useHistoryUsage(rev, reader), {
      initialProps: { rev: 0 },
    });

    await waitFor(() => {
      expect(reader).toHaveBeenCalledTimes(1);
    });

    rerender({ rev: 0 });
    expect(reader).toHaveBeenCalledTimes(1);

    rerender({ rev: 1 });
    await waitFor(() => {
      expect(reader).toHaveBeenCalledTimes(2);
    });
  });

  it('reports no data rather than an error when the store cannot be read', async () => {
    // The user came here to read a number, not to debug SQLite. A loading
    // state that never resolves is the failure mode this guards against.
    const reader = vi.fn(() => Promise.reject(new Error('locked')));
    const { result } = renderHook(() => useHistoryUsage(0, reader));

    await waitFor(() => {
      expect(result.current.pending).toBe(false);
    });
    expect(result.current.usage).toBeNull();
  });

  it('resolves immediately with no host instead of hanging', async () => {
    const { result } = renderHook(() => useHistoryUsage(0));
    await waitFor(() => {
      expect(result.current.pending).toBe(false);
    });
    expect(result.current.usage).toBeNull();
  });
});
