/**
 * Guards the bounded-loading rule.
 *
 * Vitals has already shipped one unbounded loading state — the splash screen
 * that pulsed forever after a throw during boot — and the dashboard skeletons
 * were a second one, found by opening the dev server and watching them never
 * resolve.
 *
 * The rule these tests enforce: `pending` must always end, whether or not any
 * data ever arrives. An app that looks busy is not reported as broken.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createTauriSystemSource, FIRST_FRAME_TIMEOUT_MS, NO_SAMPLER } from './useSystemSnapshot';

/** Controls whether the module under test believes a Tauri host exists. */
const hasHost = vi.hoisted(() => ({ value: false }));

vi.mock('../../shell/host', () => ({
  hasTauriHost: () => hasHost.value,
}));

const subscribeToMetrics = vi.hoisted(() => vi.fn());
const latestMetrics = vi.hoisted(() => vi.fn<() => unknown>(() => null));

vi.mock('@/lib/metrics', () => ({
  subscribeToMetrics,
  latestMetrics,
}));

beforeEach(() => {
  vi.useFakeTimers();
  hasHost.value = false;
  subscribeToMetrics.mockReset();
  latestMetrics.mockReset();
  latestMetrics.mockReturnValue(null);
  // Never resolves by default: the interesting cases are the ones where no
  // frame arrives, so silence is the right default and a test that wants data
  // opts in explicitly.
  subscribeToMetrics.mockReturnValue(new Promise(() => undefined));
});

afterEach(() => {
  vi.useRealTimers();
});

describe('createTauriSystemSource', () => {
  it('is pending before anything subscribes', () => {
    const source = createTauriSystemSource();
    expect(source.current().pending).toBe(true);
  });

  it('starts from the stream\u2019s current frame, so a first visit has data on its first render', () => {
    // The metrics listener lives for the session; a screen created after
    // frames have arrived must not show skeletons while it waits for the
    // next one.
    const system = { cpu: { total: 12 } };
    latestMetrics.mockReturnValue({
      system,
      processes: new Map([['4:1', { name: 'x' }]]),
      seq: 41,
      elapsedMs: 1000,
      timestampMs: 5000,
    });

    const current = createTauriSystemSource().current();

    expect(current.pending).toBe(false);
    expect(current.system).toBe(system);
    expect(current.processes.size).toBe(1);
    expect(current.seq).toBe(41);
  });

  it('gives up immediately when there is no host', () => {
    // In a browser the answer is already known. Making the user wait five
    // seconds to be told something we could say at once is just a slower way
    // to be unhelpful.
    const source = createTauriSystemSource();
    source.subscribe(() => undefined);

    expect(source.current().pending).toBe(false);
    expect(source.current().error).toBe(NO_SAMPLER);
    expect(subscribeToMetrics).not.toHaveBeenCalled();
  });

  it('gives up when a host exists but never delivers a frame', () => {
    // A sampler thread that panicked on startup looks exactly like this from
    // the webview, and is otherwise invisible.
    hasHost.value = true;
    const source = createTauriSystemSource();
    source.subscribe(() => undefined);

    expect(source.current().pending).toBe(true);

    vi.advanceTimersByTime(FIRST_FRAME_TIMEOUT_MS + 1);

    expect(source.current().pending).toBe(false);
    expect(source.current().error).toBe(NO_SAMPLER);
  });

  it('stays pending while the sampler is merely slow', () => {
    // The timeout must not fire on a slow start: the first sample lands within
    // about a second, so five is far outside the normal range.
    hasHost.value = true;
    const source = createTauriSystemSource();
    source.subscribe(() => undefined);

    vi.advanceTimersByTime(FIRST_FRAME_TIMEOUT_MS - 100);
    expect(source.current().pending).toBe(true);
  });

  it('notifies subscribers when it gives up, so the screen re-renders', () => {
    // Setting the state without emitting would leave the skeletons on screen
    // until some unrelated render happened to occur.
    let notified = 0;
    const source = createTauriSystemSource();
    source.subscribe(() => {
      notified += 1;
    });

    expect(notified).toBe(1);
  });

  it('cancels the timeout when the last subscriber leaves', () => {
    hasHost.value = true;
    const source = createTauriSystemSource();
    const unsubscribe = source.subscribe(() => undefined);

    unsubscribe();
    vi.advanceTimersByTime(FIRST_FRAME_TIMEOUT_MS * 2);

    // A timer firing into a disposed source would publish to nobody, but it
    // also keeps the closure — and the process map it captures — alive.
    expect(vi.getTimerCount()).toBe(0);
  });

  it('clears pending on a sampler error, not just the error field', () => {
    // An error before the first frame means the skeletons will never resolve
    // on their own, so leaving `pending` set hides the message underneath them.
    hasHost.value = true;
    let listener: { onError(message: string): void } | undefined;
    subscribeToMetrics.mockImplementation((candidate: typeof listener) => {
      listener = candidate;
      return new Promise(() => undefined);
    });

    const source = createTauriSystemSource();
    source.subscribe(() => undefined);
    listener?.onError('sampler thread died');

    expect(source.current().pending).toBe(false);
    expect(source.current().error).toBe('sampler thread died');
  });

  it('keeps the last good readings when an error arrives after data', () => {
    // Blanking the dashboard on a transient error reads as "your machine
    // stopped", which is far more alarming than the error itself.
    hasHost.value = true;
    let listener:
      { onSnapshot(snapshot: unknown): void; onError(message: string): void } | undefined;
    subscribeToMetrics.mockImplementation((candidate: typeof listener) => {
      listener = candidate;
      return new Promise(() => undefined);
    });

    const source = createTauriSystemSource();
    source.subscribe(() => undefined);

    const system = { cpu: { total: 42 } };
    listener?.onSnapshot({ system, processes: new Map(), seq: 1, timestampMs: 1000 });
    listener?.onError('transient');

    expect(source.current().system).toBe(system);
    expect(source.current().error).toBe('transient');
  });

  it('stops the timeout once a frame lands', () => {
    hasHost.value = true;
    let listener: { onSnapshot(snapshot: unknown): void } | undefined;
    subscribeToMetrics.mockImplementation((candidate: typeof listener) => {
      listener = candidate;
      return new Promise(() => undefined);
    });

    const source = createTauriSystemSource();
    source.subscribe(() => undefined);
    listener?.onSnapshot({ system: {}, processes: new Map(), seq: 1, timestampMs: 1000 });

    vi.advanceTimersByTime(FIRST_FRAME_TIMEOUT_MS * 2);

    // The error must not appear after data has already arrived.
    expect(source.current().error).toBeNull();
    expect(source.current().pending).toBe(false);
  });

  it('keeps listening after the last subscriber leaves and a new one arrives', async () => {
    // `<Activity mode="hidden">` unmounts a route's effects and remounts them
    // when the user comes back; StrictMode does the same on purpose in dev.
    // The second subscription used to inherit `disposed = true` from the
    // first and unlisten itself the moment `subscribeToMetrics` resolved, so
    // frames kept arriving over IPC while the dashboard reported that none
    // were. Found live, not by a test — hence this one.
    hasHost.value = true;
    const stops: Array<ReturnType<typeof vi.fn>> = [];
    let listener: { onSnapshot(snapshot: unknown): void } | undefined;
    subscribeToMetrics.mockImplementation((candidate: typeof listener) => {
      listener = candidate;
      const stop = vi.fn();
      stops.push(stop);
      return Promise.resolve(stop);
    });

    const source = createTauriSystemSource();
    const unsubscribe = source.subscribe(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    unsubscribe();
    expect(stops[0]).toHaveBeenCalledTimes(1);

    source.subscribe(() => undefined);
    await vi.advanceTimersByTimeAsync(0);
    // The fresh subscription must be alive, not torn down on arrival.
    expect(stops).toHaveLength(2);
    expect(stops[1]).not.toHaveBeenCalled();

    listener?.onSnapshot({ system: {}, processes: new Map(), seq: 7, timestampMs: 7000 });
    expect(source.current().seq).toBe(7);
    expect(source.current().error).toBeNull();
  });
});
