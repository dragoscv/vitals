/**
 * The AI page's subscription follows window visibility (ENG2, S21-04): an
 * autostart launch with `lastRoute = agents` mounts the screen in a window
 * hidden in the tray, and must not keep the Rust reader polling for it.
 */

import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { AgentqAction, AgentsSnapshot } from '@vitals/protocol';

import { makeSnapshot } from './fixtures.test-utils';
import { useAgents, type AgentsSource } from './useAgents';

/** A document whose visibility the test controls (see routes.test.tsx). */
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

function source(first: AgentsSnapshot): AgentsSource {
  let next = 7;
  return {
    subscribe: vi.fn(async (id: number | null) => ({ id: id ?? next++, snapshot: first })),
    unsubscribe: vi.fn(async () => undefined),
    listen: vi.fn(async () => () => undefined),
    run: vi.fn(async (_a: AgentqAction) => ({ ok: true, code: 0, output: '' })),
  };
}

describe('useAgents', () => {
  it('does not subscribe while a logon launch keeps the window hidden in the tray', async () => {
    const { doc } = fakeDocument('hidden');
    const src = source(makeSnapshot());
    const { result } = renderHook(() => useAgents(src, doc));
    // give any stray effect a chance to run
    await act(async () => {
      await Promise.resolve();
    });
    expect(src.subscribe).not.toHaveBeenCalled();
    expect(src.listen).not.toHaveBeenCalled();
    expect(result.current.snapshot).toBeNull();
  });

  it('subscribes when the window is shown and unsubscribes when it is hidden again', async () => {
    const { doc, become } = fakeDocument('hidden');
    const src = source(makeSnapshot());
    const { result } = renderHook(() => useAgents(src, doc));
    expect(src.subscribe).not.toHaveBeenCalled();

    act(() => become('visible'));
    await waitFor(() => expect(src.subscribe).toHaveBeenCalledWith(null));
    await waitFor(() => expect(result.current.snapshot).not.toBeNull());

    act(() => become('hidden'));
    await waitFor(() => expect(src.unsubscribe).toHaveBeenCalledWith(7));
    // the last snapshot stays for when the window comes back
    expect(result.current.snapshot).not.toBeNull();
  });

  it('a visible window subscribes at once, as before', async () => {
    const { doc } = fakeDocument('visible');
    const src = source(makeSnapshot());
    renderHook(() => useAgents(src, doc));
    await waitFor(() => expect(src.subscribe).toHaveBeenCalledTimes(1));
  });
});
