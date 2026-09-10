/**
 * Keeps the overlay window in step with the `hudVisible` setting, and binds
 * Ctrl+Shift+H to flip it.
 *
 * One hook rather than two so the setting and the shortcut cannot disagree:
 * the shortcut toggles the backend and then writes what the backend reports
 * into the setting, and the setting effect only pushes when the value it
 * holds differs from what it last confirmed. Without that guard the two would
 * ping-pong — shortcut opens, setting sees `true`, pushes `true`, fine; but a
 * failed push would write back `false` and immediately close what the user
 * had just opened.
 */

import { useEffect, useRef } from 'react';

import { useSettings } from '../settings/store';
import { pushHudVisible, toggleHud } from './settingsSync';

/** The key the shortcut listens for, with Ctrl and Shift held. */
const SHORTCUT_KEY = 'h';

/** Whether a keydown is the overlay shortcut. Exported for the test. */
export function isHudShortcut(
  event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'shiftKey' | 'altKey' | 'metaKey'>,
): boolean {
  return (
    event.ctrlKey &&
    event.shiftKey &&
    !event.altKey &&
    !event.metaKey &&
    event.key.toLowerCase() === SHORTCUT_KEY
  );
}

export function useHud(hydrated: boolean): void {
  const hudVisible = useSettings((state) => state.settings.hudVisible);
  const patch = useSettings((state) => state.patch);
  // The last value the backend confirmed. Starts unknown so the first
  // hydrated pass always pushes — that is the "re-open on launch" path.
  const confirmed = useRef<boolean | null>(null);

  useEffect(() => {
    // Before hydration `hudVisible` is the default `false`; pushing it would
    // close an overlay the user left on before the store has said so.
    if (!hydrated) return;
    if (confirmed.current === hudVisible) return;
    let live = true;
    void pushHudVisible(hudVisible).then((actual) => {
      if (!live || actual === null) return;
      confirmed.current = actual;
      // The backend is the source of truth. If it could not create the
      // window, the switch must show that rather than the value we wished for.
      if (actual !== hudVisible) patch({ hudVisible: actual });
    });
    return () => {
      live = false;
    };
  }, [hydrated, hudVisible, patch]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isHudShortcut(event)) return;
      event.preventDefault();
      void toggleHud().then((actual) => {
        if (actual === null) return;
        confirmed.current = actual;
        patch({ hudVisible: actual });
      });
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [patch]);
}
