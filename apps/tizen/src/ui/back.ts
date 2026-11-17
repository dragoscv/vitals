/**
 * One Back press, one handler. The shell owns the Back key; a screen that has
 * its own steps — the Add flow's address, code and token are a single screen —
 * installs an interceptor while it wants to consume the press, and the shell
 * asks it before leaving the screen.
 *
 * A registry rather than a second `keydown` listener on `window`: with two
 * listeners the winner would depend on the capture phase running before the
 * bubble phase, a property of the event path rather than of this app. Here the
 * order is the call order, which is plain to read and to test.
 */

type Handler = () => boolean;

let current: Handler | null = null;

/** Installs the screen's Back interceptor, or clears it with `null`. */
export function setBackHandler(handler: Handler | null): void {
  current = handler;
}

/** Asks the mounted screen to consume Back. `false` when it has nothing to step back to. */
export function runBackInterceptor(): boolean {
  return current?.() ?? false;
}
