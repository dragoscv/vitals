/**
 * The bridge to the widget window host.
 *
 * Every call tolerates a missing host and swallows its own failure, the way
 * `hud/lib/window.ts` does: the only contexts without a host are the browser
 * preview and tests, and a feature whose whole job is to show numbers should
 * not be able to throw a rejection into a render path. The one place a failure
 * is worth a line is a failure that leaves the list and the windows disagreeing,
 * so those are logged — see `store.ts`.
 *
 * Command names are written as literals rather than built from a constant or a
 * variable: `scripts/check-drift.ps1` scans this file's text for the command
 * names in order to prove every `#[tauri::command]` is reachable and that every
 * call has a command. A name assembled at runtime is invisible to it, and an
 * unregistered name is a runtime failure in the webview and nothing else.
 */

import { reportToLog } from '@/lib/logToFile';

import type { WidgetInstance, WidgetWindowSpec } from './model';

export interface WidgetFlags {
  readonly alwaysOnTop: boolean;
  readonly clickThrough: boolean;
}

export interface WidgetMove {
  readonly id: string;
  readonly x: number;
  readonly y: number;
}

export interface WidgetEdit {
  readonly id: string;
}

/** Matches the event names in `apps/desktop/src-tauri/src/widgets.rs`. */
export const WIDGET_MOVED_EVENT = 'vitals://widget-moved';
export const WIDGET_CLOSED_EVENT = 'vitals://widget-closed';
export const WIDGET_CHANGED_EVENT = 'vitals://widget-changed';
export const WIDGET_EDIT_EVENT = 'vitals://widget-edit';
export const WIDGET_CONFIG_EVENT = 'vitals://widget-config';

export interface WidgetBridge {
  /**
   * Makes the open windows match `specs` exactly, and returns the ids that are
   * genuinely on screen afterwards. Idempotent on the backend, which is what
   * lets a drag re-send the whole list many times a second for free.
   */
  sync(specs: readonly WidgetWindowSpec[]): Promise<readonly string[]>;
  get(id: string): Promise<WidgetWindowSpec | null>;
  close(id: string): Promise<void>;
  setFlags(id: string, flags: WidgetFlags): Promise<void>;
  /** Brings the main window forward and asks it to open this widget's settings. */
  openSettings(id: string): Promise<void>;
  onMoved(handler: (move: WidgetMove) => void): () => void;
  onClosed(handler: (closed: WidgetEdit) => void): () => void;
  /** A change made inside a widget window (pin, click-through), so the list follows. */
  onChanged(handler: (spec: WidgetWindowSpec) => void): () => void;
  onEdit(handler: (edit: WidgetEdit) => void): () => void;
  /**
   * The backend telling ONE window that its own spec changed.
   *
   * Separate from `onChanged`, which is a global broadcast for the list: a
   * widget window cares only about itself, and `CONFIG_EVENT` is emitted to
   * that window alone so a dozen widgets do not each parse a dozen specs.
   */
  onConfig(handler: (spec: WidgetWindowSpec) => void): () => void;
}

/** Subscribes to a Tauri event, tolerating a window that closes mid-`listen`. */
function tauriEvent<T>(name: string, handler: (payload: T) => void): () => void {
  let unlisten: (() => void) | undefined;
  let live = true;

  void import('@tauri-apps/api/event')
    .then((module) => module.listen<T>(name, (event) => handler(event.payload)))
    .then((dispose) => {
      if (live) unlisten = dispose;
      else dispose();
    })
    .catch(() => {
      // No host, or the event system is unavailable: a widget window with no
      // backend is a window that shows em dashes, which is honest.
    });

  return () => {
    live = false;
    unlisten?.();
  };
}

/**
 * Runs one command, returning `fallback` instead of throwing.
 *
 * A rejected command on this path means the windows and the stored list have
 * diverged, which is worth one log line and never worth an unhandled rejection
 * inside a React effect.
 */
async function tolerate<T>(run: (invoke: Invoke) => Promise<T>, fallback: T): Promise<T> {
  try {
    const module = await import('@tauri-apps/api/core');
    if (!module.isTauri()) return fallback;
    return await run(module.invoke);
  } catch (error: unknown) {
    reportToLog('warn', error, 'widgets');
    return fallback;
  }
}

/** The invoke function the Tauri module exports, named so the callback stays readable. */
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export const tauriWidgetBridge: WidgetBridge = {
  sync(specs) {
    return tolerate((invoke) => invoke<readonly string[]>('sync_widgets', { specs }), []);
  },
  get(id) {
    return tolerate((invoke) => invoke<WidgetWindowSpec | null>('get_widget', { id }), null);
  },
  close(id) {
    return tolerate((invoke) => invoke<void>('close_widget', { id }), undefined);
  },
  setFlags(id, flags) {
    return tolerate(
      (invoke) =>
        invoke<void>('set_widget_flags', {
          id,
          alwaysOnTop: flags.alwaysOnTop,
          clickThrough: flags.clickThrough,
        }),
      undefined,
    );
  },
  openSettings(id) {
    return tolerate((invoke) => invoke<void>('focus_widget_settings', { id }), undefined);
  },
  onMoved(handler) {
    return tauriEvent<WidgetMove>(WIDGET_MOVED_EVENT, handler);
  },
  onClosed(handler) {
    return tauriEvent<WidgetEdit>(WIDGET_CLOSED_EVENT, handler);
  },
  onChanged(handler) {
    return tauriEvent<WidgetWindowSpec>(WIDGET_CHANGED_EVENT, handler);
  },
  onEdit(handler) {
    return tauriEvent<WidgetEdit>(WIDGET_EDIT_EVENT, handler);
  },
  onConfig(handler) {
    return tauriEvent<WidgetWindowSpec>(WIDGET_CONFIG_EVENT, handler);
  },
};

/**
 * Used where there is no host.
 *
 * `sync` answers with the ids it was given rather than an empty list: the
 * caller treats that answer as "these are open", and claiming none are open
 * would make a browser preview delete the user's configuration.
 */
export const noopWidgetBridge: WidgetBridge = {
  sync: (specs) => Promise.resolve(specs.map((spec: WidgetInstance) => spec.id)),
  get: () => Promise.resolve(null),
  close: () => Promise.resolve(),
  setFlags: () => Promise.resolve(),
  openSettings: () => Promise.resolve(),
  onMoved: () => () => undefined,
  onClosed: () => () => undefined,
  onChanged: () => () => undefined,
  onEdit: () => () => undefined,
  onConfig: () => () => undefined,
};
