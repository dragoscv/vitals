/**
 * The history poll follows window visibility.
 *
 * The widget already stopped reading the store when it was removed from the
 * layout; this covers the other way a user hides it — minimising the window —
 * so a chart nobody can see does not keep opening SQLite 60 times a minute.
 */

import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { HISTORY_POLL_MS, useMachineHistory, type HistorySource } from './useMachineHistory';

/** A document whose visibility the test controls (see useAgents.test.ts). */
function fakeDocument(initial: DocumentVisibilityState) {
  const target = new EventTarget();
  let state = initial;
  Object.defineProperty(target, 'visibilityState', { get: () => state });
  const doc = target as unknown as Document;
  const become = (next: DocumentVisibilityState): void => {
    state = next;
    target.dispatchEvent(new Event('visibilitychange'));
  };
  return { doc, become };
}

/** A source that records every span it was asked for. */
function fixedSource() {
  const asked: number[] = [];
  const source: HistorySource = {
    query: (seconds) => {
      asked.push(seconds);
      return Promise.resolve([]);
    },
  };
  return { source, asked };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('useMachineHistory', () => {
  it('stops reading the store while the window is hidden and resumes when it is shown', async () => {
    vi.useFakeTimers();
    const { source, asked } = fixedSource();
    const { doc, become } = fakeDocument('visible');

    renderHook(() => useMachineHistory('h24', true, source, doc));
    await vi.waitFor(() => {
      expect(asked).toHaveLength(1);
    });

    await act(async () => {
      become('hidden');
      await Promise.resolve();
    });
    const whileHidden = asked.length;

    await act(async () => {
      await vi.advanceTimersByTimeAsync(HISTORY_POLL_MS * 3);
    });
    // A minimised dashboard must not keep querying a store for a chart that
    // is not on screen.
    expect(asked).toHaveLength(whileHidden);

    await act(async () => {
      become('visible');
      await Promise.resolve();
    });
    // Returning reads at once, so the chart is not a minute stale.
    expect(asked).toHaveLength(whileHidden + 1);
  });
});
