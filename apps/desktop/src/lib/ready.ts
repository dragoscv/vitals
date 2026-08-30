/**
 * Signals that the UI has painted and the window may be shown.
 *
 * The main window is created hidden so the user never sees an empty white
 * rectangle while the webview boots — that flash is the single clearest tell
 * that an app is web-based rather than native.
 *
 * The cost of that choice is that something must actually reveal the window.
 * Rust has a five-second fallback so a broken frontend cannot leave an
 * invisible process running, but the fallback shows a blank window; this path
 * is the one that shows a *painted* window.
 */

import { invoke } from '@tauri-apps/api/core';

/** Set once so a re-render or a StrictMode double-invoke cannot re-trigger. */
let signalled = false;

/**
 * Reveals the window on the next frame after paint.
 *
 * Two rAFs, not one: the first fires *before* the browser has painted the
 * current frame, so showing the window there still risks revealing an
 * unpainted surface. The second runs after that paint has completed.
 */
export function signalReady(): void {
  if (signalled) return;
  signalled = true;

  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void invoke('show_main_window').catch((error: unknown) => {
        // Never rethrow. A failure here means the Rust fallback shows the
        // window a few seconds later; an unhandled rejection during startup
        // would be a worse outcome than a slightly delayed reveal.
        console.error('failed to reveal the main window', error);
      });
    });
  });
}

/** Test seam: lets a test start from a clean state. */
export function resetReadyForTests(): void {
  signalled = false;
}
