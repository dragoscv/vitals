import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { HealthError } from './api';
import { FULL, fakeHealthApi } from './fixtures.test-utils';
import { useHealth } from './useHealth';

/** A document whose visibility the test controls. */
function fakeDoc(initial: DocumentVisibilityState = 'visible') {
  let state = initial;
  const target = new EventTarget();
  const doc = {
    get visibilityState() {
      return state;
    },
    addEventListener: target.addEventListener.bind(target),
    removeEventListener: target.removeEventListener.bind(target),
  } as unknown as Document;
  const set = (next: DocumentVisibilityState): void => {
    state = next;
    target.dispatchEvent(new Event('visibilitychange'));
  };
  return { doc, set };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('useHealth', () => {
  it('polls while visible, stops entirely while hidden, and resumes on return', async () => {
    vi.useFakeTimers();
    const fetch = vi.fn(() => Promise.resolve(FULL));
    const { doc, set } = fakeDoc('visible');
    renderHook(() => useHealth(fakeHealthApi({ fetch }), 1000, doc));

    await act(async () => {
      await Promise.resolve();
    });
    expect(fetch).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(fetch).toHaveBeenCalledTimes(3);

    act(() => {
      set('hidden');
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(fetch).toHaveBeenCalledTimes(3);

    act(() => {
      set('visible');
    });
    await act(async () => {
      await Promise.resolve();
    });
    // Back on screen: an immediate read, not a wait for the next minute.
    expect(fetch).toHaveBeenCalledTimes(4);
  });

  it('does not read at all when the window starts hidden', async () => {
    vi.useFakeTimers();
    const fetch = vi.fn(() => Promise.resolve(FULL));
    const { doc } = fakeDoc('hidden');
    renderHook(() => useHealth(fakeHealthApi({ fetch }), 1000, doc));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('keeps the last body after vmui becomes unreachable, but drops it when signed out', async () => {
    const kinds = [null, 'unreachable', 'signedOut'] as const;
    let call = 0;
    const fetch = (): Promise<typeof FULL> => {
      const kind = kinds[Math.min(call, kinds.length - 1)] ?? null;
      call += 1;
      return kind === null ? Promise.resolve(FULL) : Promise.reject(new HealthError(kind));
    };
    const { doc } = fakeDoc('visible');
    const { result } = renderHook(() => useHealth(fakeHealthApi({ fetch }), 60_000, doc));

    await waitFor(() => {
      expect(result.current.state.status).toBe('ok');
    });
    act(() => {
      result.current.refresh();
    });
    await waitFor(() => {
      expect(result.current.state.status).toBe('error');
    });
    expect(result.current.state.error).toBe('unreachable');
    expect(result.current.state.data).toBe(FULL);

    act(() => {
      result.current.refresh();
    });
    await waitFor(() => {
      expect(result.current.state.error).toBe('signedOut');
    });
    expect(result.current.state.data).toBeNull();
  });
});
