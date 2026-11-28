/**
 * The remote-widget guarantees.
 *
 * A widget pointed at another machine is the one path in this app that talks
 * to a computer the user did not necessarily start, over a network they share.
 * So the properties asserted here are the ones that make that safe and honest:
 * it never polls faster than the local sampler, never stacks requests, never
 * loses the line it already drew when a poll fails, and never confuses "your
 * key was refused" with "the machine did not answer" — because the two ask the
 * user for opposite things.
 *
 * The network is a scripted `fetch` on the global, because the hook's own
 * signature is fixed (it takes only the id) and the only seam left is the one
 * it hands to the client. That is also the shipped path: the hook always
 * wraps `globalThis.fetch` to carry its abort signal.
 */

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { VitalsError } from '@vitals/client';

import { makeSystem } from '../dashboard/test-fixtures';
import { EMPTY_WIDGET_INPUT } from './describe';
import { rollUp } from './live';
import {
  REMOTE_POLL_INTERVAL_MS,
  remoteStatusFrom,
  setRemoteConnections,
  useRemoteWidgetInput,
  type RemoteConnection,
} from './remote';
import { makeSummary, scriptedFetch } from './remote.fixtures';

const PC: RemoteConnection = { id: 'pc', baseUrl: 'http://pc:7331', token: 'tok' };

/** Lets a poll's promise chain settle inside the fake clock. */
async function tick(ms = 0): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
    await Promise.resolve();
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  setRemoteConnections([]);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  setRemoteConnections([]);
});

describe('remoteStatusFrom', () => {
  it('reads a refused key as unauthorised, never as a dead network', () => {
    // 401 (unknown token) and 403 (a valid token that may not read, or Windows
    // refused) are both the machine answering. Calling either "unreachable"
    // would send the user to reboot their router over a key that needs
    // re-pairing.
    expect(remoteStatusFrom(new VitalsError('unauthorised', '401'))).toBe('unauthorised');
    expect(remoteStatusFrom(new VitalsError('forbidden', '403'))).toBe('unauthorised');
  });

  it('reads every way a machine can fail to answer as unreachable', () => {
    expect(remoteStatusFrom(new VitalsError('network', 'refused'))).toBe('unreachable');
    expect(remoteStatusFrom(new VitalsError('closed', 'dropped'))).toBe('unreachable');
    expect(remoteStatusFrom(new VitalsError('http', '502'))).toBe('unreachable');
    expect(remoteStatusFrom(new VitalsError('protocol', 'not a summary'))).toBe('unreachable');
  });

  it('falls back to unreachable for a failure that is not a VitalsError at all', () => {
    expect(remoteStatusFrom(new Error('boom'))).toBe('unreachable');
    expect(remoteStatusFrom('a string thrown')).toBe('unreachable');
    expect(remoteStatusFrom(undefined)).toBe('unreachable');
  });
});

describe('useRemoteWidgetInput', () => {
  it('folds a successful poll into the traces, one point per series', async () => {
    const system = makeSystem();
    const net = scriptedFetch({ status: 200, body: JSON.stringify(makeSummary(system)) });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([PC]);

    const { result } = renderHook(() => useRemoteWidgetInput('pc'));
    await tick(1);

    expect(result.current.status).toBe('live');
    expect(result.current.input.system).toEqual(system);
    expect(result.current.input.cpuHistory).toHaveLength(1);
    expect(result.current.input.memoryHistory).toHaveLength(1);
    expect(result.current.input.gpuHistory).toHaveLength(1);
    expect(result.current.input.netHistory).toHaveLength(1);
    expect(result.current.input.diskHistory).toHaveLength(1);
    // Exactly one request for the reading: the mount poll, and no interval one.
    expect(net.calls).toHaveLength(1);
    expect(net.calls[0]).toBe('http://pc:7331/api/v1/summary');
  });

  it('keeps the previous traces — by identity — when a later poll cannot reach the machine', async () => {
    // Blanking the widget on one dropped poll is worse than showing a stale
    // line labelled honestly. The identity assertion matters as much as the
    // contents: a fresh empty object would re-render every consumer and, in a
    // live one, draw nothing.
    const system = makeSystem();
    const net = scriptedFetch({ status: 200, body: JSON.stringify(makeSummary(system)) });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([PC]);

    const { result } = renderHook(() => useRemoteWidgetInput('pc'));
    await tick(1);
    expect(result.current.status).toBe('live');
    const before = result.current.input;

    // The replies are exhausted, so the next poll rejects the way a machine
    // that has left the network does.
    await tick(REMOTE_POLL_INTERVAL_MS);

    expect(result.current.status).toBe('unreachable');
    expect(result.current.input).toBe(before);
    expect(result.current.input.cpuHistory).toHaveLength(1);
  });

  it('maps a 401 to unauthorised and not to unreachable', async () => {
    const net = scriptedFetch({ status: 401, body: '{}' });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([PC]);

    const { result } = renderHook(() => useRemoteWidgetInput('pc'));
    await tick(1);

    expect(result.current.status).toBe('unauthorised');
    expect(result.current.status).not.toBe('unreachable');
  });

  it('maps a 403 to unauthorised too, because a read-only key is not a dead network', async () => {
    const net = scriptedFetch({ status: 403, body: '{}' });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([PC]);

    const { result } = renderHook(() => useRemoteWidgetInput('pc'));
    await tick(1);

    expect(result.current.status).toBe('unauthorised');
  });

  it('issues no request at all when the caller is showing this machine', async () => {
    const net = scriptedFetch({ status: 200, body: '{}' });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([PC]);

    const { result } = renderHook(() => useRemoteWidgetInput(null));
    await tick(REMOTE_POLL_INTERVAL_MS * 5);

    expect(result.current.status).toBe('idle');
    expect(result.current.input).toBe(EMPTY_WIDGET_INPUT);
    expect(net.calls).toHaveLength(0);
  });

  it('produces exactly the traces the local path would for the same reading', async () => {
    // The whole point of folding through `rollUp` rather than beside it: a
    // remote widget and a local one must draw the same line for the same
    // numbers, or the sparkline is a property of which window you opened.
    const system = makeSystem();
    const local = rollUp(EMPTY_WIDGET_INPUT, system);

    const net = scriptedFetch({ status: 200, body: JSON.stringify(makeSummary(system)) });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([PC]);

    const { result } = renderHook(() => useRemoteWidgetInput('pc'));
    await tick(1);

    expect(result.current.input.cpuHistory).toEqual(local.cpuHistory);
    expect(result.current.input.memoryHistory).toEqual(local.memoryHistory);
    expect(result.current.input.gpuHistory).toEqual(local.gpuHistory);
    expect(result.current.input.netHistory).toEqual(local.netHistory);
    expect(result.current.input.diskHistory).toEqual(local.diskHistory);
  });

  it('treats a connection id with no registered machine as unreachable without calling anything', async () => {
    const net = scriptedFetch({ status: 200, body: '{}' });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([]);

    const { result } = renderHook(() => useRemoteWidgetInput('ghost'));
    await tick(REMOTE_POLL_INTERVAL_MS);

    expect(result.current.status).toBe('unreachable');
    expect(net.calls).toHaveLength(0);
  });

  it('never lets a second request overlap the one still in flight', async () => {
    let resolveFirst: ((response: Response) => void) | undefined;
    const pending = new Promise<Response>((resolve) => {
      resolveFirst = resolve;
    });
    const calls: string[] = [];
    const impl: typeof globalThis.fetch = (input) => {
      calls.push(input instanceof Request ? input.url : input.toString());
      return pending;
    };
    vi.stubGlobal('fetch', impl);
    setRemoteConnections([PC]);

    const { result } = renderHook(() => useRemoteWidgetInput('pc'));
    await tick(REMOTE_POLL_INTERVAL_MS * 3);

    // Three interval ticks passed while the first request hung; not one of them
    // started a second request, because a queue here is how a slow machine
    // becomes a backlog this widget caused.
    expect(calls).toHaveLength(1);
    expect(result.current.status).toBe('loading');

    await act(async () => {
      resolveFirst?.(
        new Response(JSON.stringify(makeSummary(makeSystem())), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.status).toBe('live');
  });

  it('clears the remote traces when the caller switches back to this machine', async () => {
    // A line from another machine under this widget's label would be a lie, so
    // deselecting must reset rather than keep what the remote path drew.
    const net = scriptedFetch({ status: 200, body: JSON.stringify(makeSummary(makeSystem())) });
    vi.stubGlobal('fetch', net.impl);
    setRemoteConnections([PC]);

    // Typed here rather than asserted below: an "as" would hide the widening
    // from tsc, and the widening is the point of the test.
    const props: { id: string | null } = { id: 'pc' };
    const { result, rerender } = renderHook(
      ({ id }: { id: string | null }) => useRemoteWidgetInput(id),
      { initialProps: props },
    );
    await tick(1);
    expect(result.current.input.cpuHistory).toHaveLength(1);

    rerender({ id: null });
    await tick(1);

    expect(result.current.status).toBe('idle');
    expect(result.current.input).toBe(EMPTY_WIDGET_INPUT);
  });

  it('starts polling a machine that is registered after the widget mounted', async () => {
    const net = scriptedFetch({ status: 200, body: JSON.stringify(makeSummary(makeSystem())) });
    vi.stubGlobal('fetch', net.impl);

    const { result } = renderHook(() => useRemoteWidgetInput('pc'));
    await tick(1);
    // Nothing registered yet: honest "unreachable", and no request made.
    expect(result.current.status).toBe('unreachable');
    expect(net.calls).toHaveLength(0);

    act(() => {
      setRemoteConnections([PC]);
    });
    await tick(1);

    expect(result.current.status).toBe('live');
    expect(net.calls).toHaveLength(1);
  });
});
