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

import { defaultPosition, MAX_WIDGETS, type WidgetInstance, type WidgetWindowSpec } from './model';
import type { Monitor } from '@tauri-apps/api/window';

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

/* ------------------------------------------------------------------ */
/* Placement                                                           */
/* ------------------------------------------------------------------ */

// These rules belong in `model.ts` — they are pure, they are about what a
// widget *is*, and that module is where the rest of that lives. They are here
// because `model.ts` is being edited by another writer right now and a second
// pen in that file would cost more than the tidier home buys. Move them across
// when it is free; nothing below depends on this file's bridge.

/** A screen's visible work area, in logical pixels. */
export interface ScreenRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** A position as the placement rules talk about it. */
export interface WidgetPoint {
  readonly x: number;
  readonly y: number;
}

/** A widget's rectangle: where it is and how big it is. */
export interface WidgetRect extends WidgetPoint {
  readonly width: number;
  readonly height: number;
}

/**
 * The grid a dragged widget lands on, in logical pixels.
 *
 * Four, and the number is not a taste: the cascade places a widget at
 * `80 + 28·k` (`defaultPosition`), and four is the greatest common divisor of
 * the origin (80) and the step (28). A coarser grid that did not divide the
 * origin would move every widget still sitting where the cascade put it on the
 * first snap — a write to disk for every user and a stored position that no
 * longer matches what the cascade would produce, so a restart and a fresh add
 * would disagree about the same widget. Four is fine enough that a drag still
 * feels free and coarse enough to take the last couple of pixels of jitter out
 * of two widgets meant to sit in one row.
 */
export const GRID_STEP = 4;

/** One coordinate rounded to the nearest step, or left alone if unusable. */
function snapCoordinate(value: number, step: number): number {
  // A non-finite coordinate is unmeasured, not zero: rounding a NaN would
  // silently place the window at the origin, which is a wrong answer rather
  // than an honest "I do not know where this is".
  return Number.isFinite(value) ? Math.round(value / step) * step : value;
}

/**
 * The nearest grid position to `point`.
 *
 * Idempotent, and it returns the SAME object when nothing moved. Both matter:
 * the store answers with an unchanged array when the snap changes nothing, so
 * the identity is what lets a drag that ends on the grid skip both the disk
 * write and the push to the host instead of scheduling one per frame.
 */
export function snapToGrid(point: WidgetPoint, step: number = GRID_STEP): WidgetPoint {
  if (!Number.isFinite(step) || step <= 0) return point;
  const x = snapCoordinate(point.x, step);
  const y = snapCoordinate(point.y, step);
  // `Object.is`, not `===`: a NaN coordinate is left as it is, and NaN === NaN
  // is false, so `===` would answer with a fresh object every time and defeat
  // the no-op detection above.
  return Object.is(x, point.x) && Object.is(y, point.y) ? point : { x, y };
}

/**
 * The cascade slots a new widget may land in, in the order they fill.
 *
 * Derived from `defaultPosition` rather than a copied constant: the wrap point
 * is a detail of that function, and a hand-written eight here would go stale
 * the day it changes without anything failing. The loop stops at the first
 * slot that repeats one already seen, which is exactly the wrap.
 */
export function cascadeSlots(): readonly WidgetPoint[] {
  const slots: WidgetPoint[] = [];
  for (let index = 0; index < MAX_WIDGETS; index += 1) {
    const position = defaultPosition(index);
    if (slots.some((slot) => slot.x === position.x && slot.y === position.y)) break;
    slots.push(position);
  }
  return slots;
}

/**
 * Where the next widget goes: the first cascade slot, on the grid, that no
 * placed widget already occupies.
 *
 * This is the one existing behaviour the feature changes. `addWidget` in
 * `model.ts` uses `defaultPosition(registry.length)`, so removing the second of
 * three widgets and adding another drops the newcomer on top of the third — the
 * length is two, and slot two was taken. Taking the slot by occupancy keeps the
 * rule the same (a new widget still lands on a cascade step, never dead centre)
 * while making it survive a removal. When every slot is taken the fallback is
 * the old index-based default, so the cap and not this scan decides when to
 * stop adding.
 */
export function nextCascadePosition(
  occupied: readonly WidgetPoint[],
  step: number = GRID_STEP,
): WidgetPoint {
  for (const slot of cascadeSlots()) {
    const candidate = snapToGrid(slot, step);
    if (!occupied.some((point) => point.x === candidate.x && point.y === candidate.y)) {
      return candidate;
    }
  }
  return snapToGrid(defaultPosition(occupied.length), step);
}

/** Whether a point lies inside a screen's work area. */
function containsPoint(screen: ScreenRect, point: WidgetPoint): boolean {
  return (
    point.x >= screen.x &&
    point.x < screen.x + screen.width &&
    point.y >= screen.y &&
    point.y < screen.y + screen.height
  );
}

/** The distance from a point to a screen, zero when the point is inside it. */
function pointToScreenDistance(point: WidgetPoint, screen: ScreenRect): number {
  const dx = Math.max(screen.x - point.x, 0, point.x - (screen.x + screen.width));
  const dy = Math.max(screen.y - point.y, 0, point.y - (screen.y + screen.height));
  return Math.hypot(dx, dy);
}

/**
 * The screen a widget belongs on: the one holding its top-left corner, or the
 * nearest one when no screen does.
 *
 * The corner rather than the centre, because the stored position IS the
 * corner: a widget dragged past the right edge of a monitor has a corner that
 * is off every screen, and bringing it back by the corner lands it on the
 * monitor the user dragged it towards rather than on whichever screen happens
 * to be largest.
 */
function screenFor(point: WidgetPoint, screens: readonly ScreenRect[]): ScreenRect | null {
  let nearest: ScreenRect | null = null;
  let nearestDistance = Number.POSITIVE_INFINITY;
  for (const screen of screens) {
    if (containsPoint(screen, point)) return screen;
    const distance = pointToScreenDistance(point, screen);
    if (distance < nearestDistance) {
      nearestDistance = distance;
      nearest = screen;
    }
  }
  return nearest;
}

/** A value kept inside a range whose ends may cross when the widget is too big. */
function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

/**
 * Moves a widget back onto a screen, or leaves it exactly where it is.
 *
 * The failing case: a widget stored on a monitor that has since been unplugged,
 * or at an address the current arrangement no longer covers — the resolution
 * shrank, the desktop area moved. Windows will happily place a window at a
 * coordinate nothing is showing, and the result is a widget the user can
 * neither see nor reach, while the settings screen keeps offering it. That is
 * a silently lost widget, and it is the one thing this function exists to stop.
 *
 * Within the chosen screen the widget is clamped so its WHOLE rectangle sits in
 * the work area; when it is wider or taller than the screen it is pinned to the
 * work area's top-left instead of being pushed to a negative coordinate off the
 * edge. A widget half on screen can be grabbed; one just off it cannot.
 *
 * Returns the SAME object when nothing needs to move, so a caller that persists
 * on identity — the store does — writes nothing when every widget is already
 * where it should be.
 *
 * An empty screen list leaves the widget untouched on purpose. No screens
 * learned means no trustworthy answer, and a move made on a guess about a
 * coordinate system we could not read is worse than no move: it would take a
 * widget that is merely on another monitor today and fling it somewhere else.
 */
export function bringOnScreen(rect: WidgetRect, screens: readonly ScreenRect[]): WidgetPoint {
  if (screens.length === 0) return rect;

  const home = screenFor(rect, screens);
  if (home === null) return rect;

  const maxX = home.x + home.width - rect.width;
  const maxY = home.y + home.height - rect.height;
  const x = maxX < home.x ? home.x : clamp(rect.x, home.x, maxX);
  const y = maxY < home.y ? home.y : clamp(rect.y, home.y, maxY);

  return Object.is(x, rect.x) && Object.is(y, rect.y) ? rect : { x, y };
}

/** A source of the screens a widget may be brought back onto. */
export interface ScreenSource {
  screens(): Promise<readonly ScreenRect[]>;
}

/** A rectangle only if it is one, so a nonsense monitor is dropped not shown. */
function positiveRect(x: number, y: number, width: number, height: number): ScreenRect | null {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
  if (!Number.isFinite(width) || !Number.isFinite(height)) return null;
  if (width <= 0 || height <= 0) return null;
  return { x, y, width, height };
}

/**
 * A monitor's work area in logical pixels.
 *
 * Tauri reports physical pixels and a scale factor; the stored positions and
 * `set_position` are logical, so the same division `widgets.rs` does on a move
 * is done here on a read.
 */
function monitorWorkArea(monitor: Monitor): ScreenRect | null {
  const scale = monitor.scaleFactor;
  if (!Number.isFinite(scale) || scale <= 0) return null;
  const origin = monitor.workArea.position.toLogical(scale);
  const size = monitor.workArea.size.toLogical(scale);
  return positiveRect(origin.x, origin.y, size.width, size.height);
}

/**
 * Every screen the host knows about, or an empty list.
 *
 * Never throws: a capability file that does not grant the window permission
 * makes `availableMonitors` reject rather than answer, which is the failure
 * this catch exists for. The rejection is logged once because a capability
 * that quietly stopped granting monitor access is the kind of thing nobody
 * notices until widgets pile up on one monitor.
 */
async function hostScreens(): Promise<readonly ScreenRect[]> {
  try {
    const { availableMonitors } = await import('@tauri-apps/api/window');
    const monitors = await availableMonitors();
    const screens: ScreenRect[] = [];
    for (const monitor of monitors) {
      const screen = monitorWorkArea(monitor);
      if (screen !== null) screens.push(screen);
    }
    return screens;
  } catch (error: unknown) {
    reportToLog('warn', error, 'widgets');
    return [];
  }
}

/** The browser's one screen, as a last resort, or `null` if it cannot say. */
function browserScreen(): ScreenRect | null {
  const screen = typeof window === 'undefined' ? undefined : window.screen;
  if (screen === undefined || screen === null) return null;
  // The work area, not the resolution: a widget placed at the bottom of a
  // maximised screen would otherwise sit under the taskbar. `availTop` and
  // `availLeft` are non-standard — only Firefox reports them, and they are not
  // on the DOM `Screen` type — so they are read through an explicit optional
  // view and default to the origin, which is the honest assumption for an
  // engine that will not say where the work area starts.
  const exotic = screen as Screen & {
    readonly availLeft?: number;
    readonly availTop?: number;
  };
  const x = typeof exotic.availLeft === 'number' ? exotic.availLeft : 0;
  const y = typeof exotic.availTop === 'number' ? exotic.availTop : 0;
  return positiveRect(x, y, screen.availWidth, screen.availHeight);
}

/**
 * The screens this machine actually has, best effort, and never a throw.
 *
 * The host is asked first because only it knows about a second monitor, the
 * arrangement, and where the taskbar leaves the work area. The browser's single
 * `screen` is the fallback: it is always there, it knows the work area, and for
 * the common one-monitor case it is the same answer. An empty list — no host, a
 * denied capability and no browser object — is the honest "I could not learn
 * any", which the placement rules treat as "do not move anything".
 */
export async function readScreens(): Promise<readonly ScreenRect[]> {
  const fromHost = await hostScreens();
  if (fromHost.length > 0) return fromHost;
  const fromBrowser = browserScreen();
  return fromBrowser === null ? [] : [fromBrowser];
}

/**
 * A screen source backed by `read`, so a test can supply screens without a
 * host and without a browser, and the store can be handed a different one.
 */
export function createScreenSource(
  read: () => Promise<readonly ScreenRect[]> = readScreens,
): ScreenSource {
  return { screens: read };
}
