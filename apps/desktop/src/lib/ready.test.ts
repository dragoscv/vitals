import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn((_command: string) => Promise.resolve());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (command: string) => invoke(command),
}));

const { resetReadyForTests, signalReady } = await import('./ready');

/** Runs queued animation frames, since jsdom does not paint. */
async function flushFrames(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe('signalReady', () => {
  beforeEach(() => {
    invoke.mockClear();
    invoke.mockImplementation(() => Promise.resolve());
    resetReadyForTests();

    // jsdom's rAF is slow and real; a synchronous stand-in keeps the test
    // deterministic without changing what is under test.
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      setTimeout(() => cb(0), 0);
      return 0;
    });
  });

  it('reveals the window', async () => {
    signalReady();
    await flushFrames();

    expect(invoke).toHaveBeenCalledWith('show_main_window');
  });

  it('reveals it only once however many times it is called', async () => {
    // React StrictMode double-invokes effects in development. Without the
    // guard the window would be shown and focused twice, which steals focus
    // from whatever the user clicked on in between.
    signalReady();
    signalReady();
    signalReady();
    await flushFrames();

    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('does not reject when the command fails', async () => {
    // An unhandled rejection during startup is worse than a delayed reveal:
    // the Rust fallback shows the window a few seconds later regardless.
    invoke.mockImplementation(() => Promise.reject(new Error('no such window')));
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    expect(() => {
      signalReady();
    }).not.toThrow();
    await flushFrames();

    expect(consoleError).toHaveBeenCalled();
    consoleError.mockRestore();
  });

  it('waits for a painted frame rather than firing synchronously', async () => {
    // Showing the window in the same tick reveals an unpainted surface,
    // which is the white flash the hidden window exists to avoid.
    signalReady();
    expect(invoke).not.toHaveBeenCalled();

    await flushFrames();
    expect(invoke).toHaveBeenCalled();
  });
});
