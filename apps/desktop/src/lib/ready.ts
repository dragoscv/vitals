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

import { hasTauriHost } from '../shell/host';

/** Set once so a re-render or a StrictMode double-invoke cannot re-trigger. */
let signalled = false;

/**
 * Whether this instance was launched by the autostart entry.
 *
 * Cached after the first call: the answer cannot change for the lifetime of
 * the process, and the reveal path must not wait on IPC twice.
 */
interface LaunchOptions {
  readonly autostarted: boolean;
  /** Relaunched by the setup after an update installed while the PC was idle. */
  readonly quietRelaunch: boolean;
}

/** As older backends answer: `quietRelaunch` may be missing. */
type LaunchAnswer = Partial<LaunchOptions>;

const userLaunch: LaunchOptions = { autostarted: false, quietRelaunch: false };
let launch: Promise<LaunchOptions> | null = null;

function launchOptions(): Promise<LaunchOptions> {
  if (launch === null) {
    launch = hasTauriHost()
      ? invoke<LaunchAnswer>('get_launch_options')
          .then((o) => ({
            autostarted: o.autostarted === true,
            quietRelaunch: o.quietRelaunch === true,
          }))
          .catch(() => userLaunch)
      : Promise.resolve(userLaunch);
  }
  return launch;
}

export function wasAutostarted(): Promise<boolean> {
  return launchOptions().then((o) => o.autostarted);
}

/**
 * Whether the setup started this instance after installing an update while
 * nobody was at the PC. Such a launch stays in the tray whatever "Start
 * minimised" says: nobody asked for a window, and one appearing on an idle
 * screen is how a person learns something changed behind their back.
 */
export function wasQuietRelaunch(): Promise<boolean> {
  return launchOptions().then((o) => o.quietRelaunch);
}

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
  // In a plain browser (the dev server, the Playwright suite) there is no
  // window to reveal, and invoking anyway logged an error on every launch.
  if (!hasTauriHost()) return;

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
  launch = null;
}
