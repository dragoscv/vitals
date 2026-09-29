import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn<(cmd: string, args?: unknown) => Promise<void>>();
const isTauri = vi.fn(() => true);

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  isTauri: () => isTauri(),
}));

const { installGlobalErrorLogging, reportToLog } = await import('./logToFile');

describe('reportToLog', () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
    isTauri.mockReturnValue(true);
  });

  it('sends an Error with its message and stack to the log_webview command', () => {
    const error = new Error('widget exploded');
    error.stack = 'Error: widget exploded\n    at Widget (widget.tsx:1:1)';

    reportToLog('error', error, 'boundary');

    expect(invoke).toHaveBeenCalledWith('log_webview', {
      level: 'error',
      message: 'widget exploded',
      source: 'boundary',
      stack: 'Error: widget exploded\n    at Widget (widget.tsx:1:1)',
    });
  });

  it('stringifies a non-Error throw and sends no stack key at all', () => {
    reportToLog('warn', 42, 'main:startup');

    expect(invoke).toHaveBeenCalledWith('log_webview', {
      level: 'warn',
      message: '42',
      source: 'main:startup',
    });
  });

  it('survives a thrown value whose toString itself throws', () => {
    const hostile = {
      toString(): string {
        throw new Error('no');
      },
    };

    expect(() => reportToLog('error', hostile, 'x')).not.toThrow();
    expect(invoke).toHaveBeenCalledWith(
      'log_webview',
      expect.objectContaining({ message: '[unprintable value thrown]' }),
    );
  });

  it('does not call Tauri when there is no Tauri host', () => {
    isTauri.mockReturnValue(false);

    reportToLog('error', new Error('in a browser'), 'main');

    expect(invoke).not.toHaveBeenCalled();
  });

  it('swallows a rejecting invoke without throwing or reporting again', async () => {
    invoke.mockRejectedValue(new Error('ipc down'));
    const onRejection = vi.fn();
    window.addEventListener('unhandledrejection', onRejection);

    expect(() => reportToLog('error', new Error('first'), 'main')).not.toThrow();
    // Let the rejection settle; if it escaped it would be unhandled here.
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.removeEventListener('unhandledrejection', onRejection);
    expect(onRejection).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('swallows an invoke that throws synchronously', () => {
    invoke.mockImplementation(() => {
      throw new Error('bridge missing');
    });

    expect(() => reportToLog('error', new Error('x'), 'main')).not.toThrow();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});

describe('installGlobalErrorLogging', () => {
  let remove: (() => void) | undefined;

  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
    isTauri.mockReturnValue(true);
  });

  afterEach(() => {
    remove?.();
    remove = undefined;
  });

  function rejection(reason: unknown): Event {
    // happy-dom has no PromiseRejectionEvent constructor; the listener only
    // reads `reason`, so a plain event carrying it is the same to it.
    const event = new Event('unhandledrejection');
    Object.defineProperty(event, 'reason', { value: reason });
    return event;
  }

  it('forwards an unhandled rejection to the log', () => {
    remove = installGlobalErrorLogging('main');

    window.dispatchEvent(rejection(new Error('nobody awaited me')));

    expect(invoke).toHaveBeenCalledWith(
      'log_webview',
      expect.objectContaining({
        level: 'error',
        message: 'nobody awaited me',
        source: 'main:unhandledrejection',
      }),
    );
  });

  it('stops forwarding once removed', () => {
    installGlobalErrorLogging('main')();

    window.dispatchEvent(rejection(new Error('after removal')));

    expect(invoke).not.toHaveBeenCalled();
  });

  it('installs nothing without a Tauri host', () => {
    isTauri.mockReturnValue(false);
    const add = vi.spyOn(window, 'addEventListener');

    remove = installGlobalErrorLogging('main');

    expect(add).not.toHaveBeenCalled();
    add.mockRestore();
  });
});
