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
vi.mock('@/lib/metrics', () => ({ subscribeToMetrics }));

type Listener = { onSnapshot(snapshot: unknown): void };

beforeEach(() => {
  vi.useFakeTimers();
  hasHost.value = true;
  subscribeToMetrics.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('createTauriSnapshotSource', () => {
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

  it('gives up when a host exists but never delivers a frame', () => {
    subscribeToMetrics.mockReturnValue(new Promise(() => undefined));
    const source = createTauriSnapshotSource();
    source.subscribe(() => undefined);
    expect(source.current().pending).toBe(true);

    vi.advanceTimersByTime(FIRST_FRAME_TIMEOUT_MS + 1);

    expect(source.current().pending).toBe(false);
    expect(source.current().error).toBe(NO_SAMPLER);
  });
});
