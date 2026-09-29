import { invoke, isTauri } from '@tauri-apps/api/core';

/**
 * Sends webview errors to the backend's capped log file.
 *
 * A release build has no devtools: `console.error` there writes to a console
 * nobody can open, so every error thrown in the field was lost. The backend
 * already keeps `vitals.log` for bug reports; this puts the webview's errors
 * in the same file (`log_webview`, target `webview`), beside the backend
 * lines that explain them.
 *
 * Deliberately *not* used from `src/mobile`: the phone page runs in a browser
 * with no Tauri host, and its errors belong to that browser, not to this
 * machine's log.
 */

export type LogLevel = 'error' | 'warn' | 'info';

interface Serialised {
  readonly message: string;
  readonly stack: string | undefined;
}

/**
 * Anything can be thrown in JavaScript — strings, `undefined`, objects whose
 * `toString` itself throws. Every path here is guarded, because a reporter
 * that throws while describing an error replaces the original error with its
 * own and hides the one that mattered.
 */
function serialise(error: unknown): Serialised {
  try {
    if (error instanceof Error) {
      const message = error.name === 'Error' ? error.message : `${error.name}: ${error.message}`;
      return { message, stack: error.stack };
    }
    return { message: String(error), stack: undefined };
  } catch {
    return { message: '[unprintable value thrown]', stack: undefined };
  }
}

/**
 * Set while a report is being handed to Tauri. If the act of reporting raises
 * something — a synchronous throw in `invoke`, a listener catching our own
 * failure — the second report is dropped instead of looping.
 */
let reporting = false;

/**
 * Records `error` in the log file. Never throws and never rejects.
 *
 * `context` is appended to the stack — the boundary uses it for React's
 * component stack, which says *where* in the tree the render failed.
 */
export function reportToLog(
  level: LogLevel,
  error: unknown,
  source: string,
  context?: string,
): void {
  if (reporting) return;
  reporting = true;
  try {
    if (!isTauri()) return;
    const { message, stack: errorStack } = serialise(error);
    const stack =
      context === undefined ? errorStack : `${errorStack ?? ''}\n\n${context}`.trimStart();
    // The rejection is swallowed on purpose. A log that cannot be written is
    // not worth an error of its own, and an unhandled rejection here would be
    // caught by the global listener below and sent straight back to this
    // function: a loop, one IPC call per turn.
    invoke<void>('log_webview', {
      level,
      message,
      source,
      ...(stack !== undefined && { stack }),
    }).catch(() => undefined);
  } catch {
    // `invoke` throws synchronously when the IPC bridge is missing or broken.
    // Same reasoning as the rejection: never let reporting fail loudly.
  } finally {
    reporting = false;
  }
}

/**
 * Forwards uncaught errors and unhandled rejections to the log.
 *
 * These are the errors no `try` and no boundary sees: a throw in an event
 * handler, a timer, or an un-awaited promise. Returns a remover, for tests
 * and for anything that wants to scope the listeners.
 *
 * Installs nothing without a Tauri host — the listeners would only ever
 * decide not to report, and in the browser dev server and the test suite
 * they would still count as user error handlers.
 */
export function installGlobalErrorLogging(source = 'window'): () => void {
  if (!isTauri()) return () => undefined;

  const onError = (event: ErrorEvent): void => {
    // `error` is null for a cross-origin script error, where the browser
    // withholds everything but the message; the location is still worth
    // keeping.
    const where = event.filename ? ` (${event.filename}:${event.lineno}:${event.colno})` : '';
    reportToLog('error', event.error ?? `${event.message}${where}`, `${source}:error`);
  };
  const onRejection = (event: PromiseRejectionEvent): void => {
    reportToLog('error', event.reason, `${source}:unhandledrejection`);
  };

  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);
  return () => {
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
  };
}
