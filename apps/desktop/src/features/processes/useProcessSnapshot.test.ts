/**
 * The Tauri-backed source had no direct test: the screen test drives a manual
 * source, which is exactly why the lifecycle defect below shipped three times
 * (here, in `useSystemSnapshot`, in `useAlerts`) and was found by looking at
 * the running app rather than by CI.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  createTauriSnapshotSource,
  FIRST_FRAME_TIMEOUT_MS,
  NO_SAMPLER,
} from './useProcessSnapshot';

const hasHost = vi.hoisted(() => ({ value: true }));
vi.mock('../../shell/host', () => ({ hasTauriHost: () => hasHost.value }));

const subscribeToMetrics = vi.hoisted(() => vi.fn());
const latestMetrics = vi.hoisted(() => vi.fn<() => unknown>(() => null));
vi.mock('@/lib/metrics', () => ({ subscribeToMetrics, latestMetrics }));

type Listener = { onSnapshot(snapshot: unknown): void };

beforeEach(() => {
  vi.useFakeTimers();
  hasHost.value = true;
  subscribeToMetrics.mockReset();
  latestMetrics.mockReset();
  latestMetrics.mockReturnValue(null);
});

afterEach(() => {
  vi.useRealTimers();
});

describe('createTauriSnapshotSource', () => {
  it('starts from the stream\u2019s current frame instead of an empty table', () => {
    // Found live 2026-09-28: a first visit to Processes showed skeletons and
    // then "116 of 116" on a machine running 780 — each source folded deltas
    // onto nothing. The session-long hub holds the whole map; a new source
    // must start from it.
    const processes = new Map([
      ['4:1', { name: 'a' }],
      ['8:1', { name: 'b' }],
    ]);
    latestMetrics.mockReturnValue({
      system: {},
      processes,
      seq: 9,
      elapsedMs: 1000,
      timestampMs: 1,
    });

    const current = createTauriSnapshotSource().current();

    expect(current.pending).toBe(false);
    expect(current.processes.size).toBe(2);
    // A copy: the hub mutates its map in place on every delta.
    expect(current.processes).not.toBe(processes);
  });

  it('keeps listening after the last subscriber leaves and a new one arrives', async () => {
    // `<Activity mode="hidden">` unmounts a route's effects and remounts them
    // on return; StrictMode does the same on purpose in dev. The second
    // subscription used to inherit `disposed = true` and unlisten itself the
    // moment `subscribeToMetrics` resolved — frames kept flowing over IPC
    // while Processes said no readings were arriving.
    const stops: Array<ReturnType<typeof vi.fn>> = [];
    let listener: Listener | undefined;
    subscribeToMetrics.mockImplementation((candidate: Listener) => {
      listener = candidate;
      const stop = vi.fn();
      stops.push(stop);
      return Promise.resolve(stop);
    });

    const source = createTauriSnapshotSource();
    const unsubscribe = source.subscribe(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    unsubscribe();
    expect(stops[0]).toHaveBeenCalledTimes(1);

    source.subscribe(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    expect(stops).toHaveLength(2);
    expect(stops[1]).not.toHaveBeenCalled();

    listener?.onSnapshot({ system: {}, processes: new Map(), seq: 9, timestampMs: 9000 });
    expect(source.current().seq).toBe(9);
    expect(source.current().error).toBeNull();
  });

  it('a subscription still in flight across a stop and restart is torn down, not leaked', async () => {
    // StrictMode: subscribe, unsubscribe, subscribe again — all before the
    // first `subscribeToMetrics` has resolved. With a boolean `disposed`
    // the restart reset it to false, so the FIRST subscription installed
    // itself as live and was then overwritten by the second; that listener
    // received every frame for the life of the app with no way to stop it.
    const resolvers: Array<(stop: () => void) => void> = [];
    subscribeToMetrics.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          resolvers.push(resolve);
        }),
    );

    const source = createTauriSnapshotSource();
    const unsubscribe = source.subscribe(() => undefined);
    unsubscribe();
    const unsubscribeAgain = source.subscribe(() => undefined);
    expect(resolvers).toHaveLength(2);

    const firstStop = vi.fn();
    const secondStop = vi.fn();
    resolvers[0]?.(firstStop);
    resolvers[1]?.(secondStop);
    await vi.advanceTimersByTimeAsync(0);

    expect(firstStop).toHaveBeenCalledTimes(1);
    expect(secondStop).not.toHaveBeenCalled();

    unsubscribeAgain();
    expect(secondStop).toHaveBeenCalledTimes(1);
  });

  it('a failed listen() surfaces as the error instead of skeletons forever', async () => {
    subscribeToMetrics.mockRejectedValue(new Error('no IPC'));
    const source = createTauriSnapshotSource();
    source.subscribe(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    expect(source.current().pending).toBe(false);
    expect(source.current().error).toBe('no IPC');
  });

  it('gives up when a host exists but never delivers a frame (timeout)', () => {
    subscribeToMetrics.mockReturnValue(new Promise(() => undefined));
    const source = createTauriSnapshotSource();
    source.subscribe(() => undefined);
    expect(source.current().pending).toBe(true);

    vi.advanceTimersByTime(FIRST_FRAME_TIMEOUT_MS + 1);

    expect(source.current().pending).toBe(false);
    expect(source.current().error).toBe(NO_SAMPLER);
  });
});
