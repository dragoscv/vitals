/**
 * The overlay's own window controls.
 *
 * Everything here goes through `@tauri-apps/api/window`, which the `hud`
 * capability grants a deliberately narrow slice of: dragging, cursor
 * pass-through, always-on-top, hide and close. No store, no filesystem, no
 * process control — an always-on-top window with no chrome is the easiest
 * thing in the product to mistake for something else, so it is given the
 * least it can work with.
 *
 * Every call swallows its failure. Losing a pin toggle is a cosmetic
 * annoyance; an unhandled rejection in a window with no error surface is a
 * silent dead overlay.
 */

/** The interface the UI depends on, so a test needs no Tauri host. */
export interface HudWindow {
  startDragging(): Promise<void>;
  setIgnoreCursorEvents(ignore: boolean): Promise<void>;
  setAlwaysOnTop(onTop: boolean): Promise<void>;
  close(): Promise<void>;
}

async function current(): Promise<HudWindow> {
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  return getCurrentWindow();
}

/** The real window. Imported lazily so a browser preview of hud.html still boots. */
export const hudWindow: HudWindow = {
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
