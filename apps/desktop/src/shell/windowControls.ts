import { hasTauriHost } from './host';

/**
 * The window operations the title bar needs, behind an interface.
 *
 * Two reasons this is not `getCurrentWindow()` inlined at the call site.
 * First, `@tauri-apps/api/window` reaches for the IPC bridge on import, so
 * merely rendering the title bar in a test environment throws. Second, the
 * title bar's real behaviour — tracking maximise state across resizes it did
 * not initiate — is only testable with a substitutable implementation.
 */

export interface WindowControls {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  /** Resolves to an unsubscribe function, matching Tauri's own event API. */
  onResized(handler: () => void): Promise<() => void>;
}

/**
 * Stand-in outside Tauri.
 *
 * The buttons stay present and focusable rather than being hidden, so the
 * layout under test is the layout that ships. A window with no title bar in
 * `vite preview` would hide exactly the regressions this component can have.
 */
const inertControls: WindowControls = {
  minimize: () => Promise.resolve(),
  toggleMaximize: () => Promise.resolve(),
  close: () => Promise.resolve(),
  isMaximized: () => Promise.resolve(false),
  onResized: () => Promise.resolve(() => {}),
};

export function getWindowControls(): WindowControls {
  if (!hasTauriHost()) return inertControls;

  // Deferred: a static import evaluates the Tauri bridge at module load, which
  // is precisely what must not happen when there is no host.
  const load = import('@tauri-apps/api/window').then((module) => module.getCurrentWindow());

  return {
    minimize: () => load.then((win) => win.minimize()),
    toggleMaximize: () => load.then((win) => win.toggleMaximize()),
    close: () => load.then((win) => win.close()),
    isMaximized: () => load.then((win) => win.isMaximized()),
    onResized: (handler) => load.then((win) => win.onResized(() => handler())),
  };
}
