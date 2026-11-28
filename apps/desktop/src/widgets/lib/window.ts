/**
 * A widget window's own controls.
 *
 * Everything here goes through `@tauri-apps/api/window`, of which the `widgets`
 * capability grants a deliberately narrow slice: dragging, cursor pass-through,
 * always-on-top and close. No store, no filesystem, no position — see
 * `capabilities/widgets.json` for why a widget may not move itself, and the
 * same `hud/lib/window.ts` idiom of swallowing every failure: losing a pin
 * toggle is a cosmetic annoyance, an unhandled rejection inside a window with
 * no error surface is a silently dead widget.
 *
 * # Why a copy
 *
 * A widget window must not import the HUD tree; the two overlays are separate
 * features and neither should be able to break the other by editing a shared
 * helper. The `label` accessor is the one addition: a widget has to find out
 * which widget it is, and its Tauri label is the only identity it is given.
 */

/** The interface the UI depends on, so a test needs no Tauri host. */
export interface WidgetWindow {
  /** This window's Tauri label, or `null` with no host. */
  label(): Promise<string | null>;
  startDragging(): Promise<void>;
  setIgnoreCursorEvents(ignore: boolean): Promise<void>;
  setAlwaysOnTop(onTop: boolean): Promise<void>;
  close(): Promise<void>;
}

/** The slice of Tauri's `Window` this module uses, named so the return type reads. */
type WindowHandle = {
  readonly label: string;
  startDragging(): Promise<void>;
  setIgnoreCursorEvents(ignore: boolean): Promise<void>;
  setAlwaysOnTop(onTop: boolean): Promise<void>;
  close(): Promise<void>;
};

async function current(): Promise<WindowHandle> {
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  return getCurrentWindow();
}

/** The real window. Imported lazily so a browser preview of widget.html still boots. */
export const widgetWindow: WidgetWindow = {
  async label() {
    try {
      return (await current()).label;
    } catch {
      // No host: the caller renders the same "unavailable" panel it renders
      // for an id the backend does not know.
      return null;
    }
  },
  async startDragging() {
    try {
      await (await current()).startDragging();
    } catch {
      // Intentionally ignored; see module doc.
    }
  },
  async setIgnoreCursorEvents(ignore: boolean) {
    try {
      await (await current()).setIgnoreCursorEvents(ignore);
    } catch {
      // Intentionally ignored; see module doc.
    }
  },
  async setAlwaysOnTop(onTop: boolean) {
    try {
      await (await current()).setAlwaysOnTop(onTop);
    } catch {
      // Intentionally ignored; see module doc.
    }
  },
  async close() {
    try {
      await (await current()).close();
    } catch {
      // Intentionally ignored; see module doc.
    }
  },
};
