import { act, cleanup, renderHook } from '@testing-library/react';
import { flushSync } from 'react-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useViewTransition } from './useViewTransition';

// A passthrough spy: the point is to prove the update really is flushed
// synchronously, which cannot be observed any other way from a test — inside
// `act` a plain state update also ends up applied, just later than the browser
// would have taken its snapshot.
vi.mock('react-dom', async (importOriginal) => {
  const actual = await importOriginal<Record<'flushSync', typeof flushSync>>();
  return { ...actual, flushSync: vi.fn(actual.flushSync) };
});

let restoreTransitions: (() => void) | null = null;

afterEach(() => {
  cleanup();
  document.documentElement.removeAttribute('data-reduce-motion');
  restoreTransitions?.();
  restoreTransitions = null;
});

interface FakeTransitions {
  readonly count: () => number;
  readonly finish: () => void;
}

/**
 * A stand-in for `document.startViewTransition`.
 *
 * happy-dom does not implement the API, so without this the "no support" path
 * is the only one any test could reach — and the interesting behaviour is
 * precisely what happens when it *is* there. The fake calls the update callback
 * synchronously, as the real API does, and leaves `finished` pending until the
 * test resolves it, so the name-clearing path can be observed separately from
 * the start.
 */
function installFakeTransitions(): FakeTransitions {
  // Defined on the object, not `Document.prototype`: happy-dom's document is
  // not reliably an instance of the `Document` the environment exposes, so a
  // prototype patch silently does nothing — and a stub that is not installed
  // makes the reduced-motion test pass for the wrong reason.
  const original = Object.getOwnPropertyDescriptor(document, 'startViewTransition');
  let calls = 0;
  let resolveFinished: () => void = () => {};
  const finished = new Promise<void>((resolve) => {
    resolveFinished = resolve;
  });

  Object.defineProperty(document, 'startViewTransition', {
    configurable: true,
    writable: true,
    value: (update: () => void) => {
      calls += 1;
      update();
      return { finished };
    },
  });

  if (typeof (document as { startViewTransition?: unknown }).startViewTransition !== 'function') {
    throw new Error('the view-transition stub is not visible on the document object');
  }

  restoreTransitions = () => {
    if (original === undefined) Reflect.deleteProperty(document, 'startViewTransition');
    else Object.defineProperty(document, 'startViewTransition', original);
  };

  return { count: () => calls, finish: () => resolveFinished() };
}

describe('useViewTransition', () => {
  it('still runs the update when the engine has no view transitions, so a click never does nothing', () => {
    const update = vi.fn();
    const { result } = renderHook(() => useViewTransition());

    act(() => {
      result.current(update);
    });

    expect(
      update,
      'the update was dropped because the API was missing — the button silently does nothing',
    ).toHaveBeenCalledTimes(1);
  });

  it('skips the transition under reduced motion even though the API is available', () => {
    const fake = installFakeTransitions();
    document.documentElement.dataset.reduceMotion = 'true';
    const update = vi.fn();
    const { result } = renderHook(() => useViewTransition());

    act(() => {
      result.current(update);
    });

    expect(update).toHaveBeenCalledTimes(1);
    expect(
      fake.count(),
      'a view transition was started despite the reduced-motion preference — the stylesheet cannot stop this one',
    ).toBe(0);
  });

  it('commits the update through flushSync, so the new DOM exists before the snapshot', () => {
    installFakeTransitions();
    const update = vi.fn();
    const { result } = renderHook(() => useViewTransition());

    act(() => {
      result.current(update);
    });

    expect(update).toHaveBeenCalledTimes(1);
    expect(
      vi.isMockFunction(flushSync),
      'the react-dom mock did not apply, so this test cannot tell a synchronous flush from a deferred update',
    ).toBe(true);
    expect(
      vi.mocked(flushSync),
      'the update was not flushed synchronously: an ordinary React update lands a frame later, after the browser has snapshotted, and the transition then animates the old view into itself',
    ).toHaveBeenCalledTimes(1);
  });

  it('names the shared element before the snapshot and clears the name when the transition ends', async () => {
    const fake = installFakeTransitions();
    const element = document.createElement('div');
    element.id = 'card';
    document.body.append(element);

    const { result } = renderHook(() => useViewTransition());
    act(() => {
      result.current(vi.fn(), { '#card': 'vitals-card' });
    });

    expect(
      element.style.getPropertyValue('view-transition-name'),
      'the name was applied after the call, so the outgoing element is not the one captured',
    ).toBe('vitals-card');

    await act(async () => {
      fake.finish();
      await Promise.resolve();
    });

    expect(
      element.style.getPropertyValue('view-transition-name'),
      'the name was left behind: the next transition aborts on a duplicate, which looks like the animation silently stopping',
    ).toBe('');
    element.remove();
  });
});
